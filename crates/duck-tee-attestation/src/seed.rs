use anyhow::{Result, ensure};
use ciborium::value::Value;
use clap::ValueEnum;
use duck_rkp::{
    cbor::{as_array, as_bytes, as_map, decode, encode, int, map_get},
    cose::{DICE_SUBJECT_PUB_KEY, DeviceKeys},
    profile::DiceCurve,
    verify::verify_csr,
};
use p256::elliptic_curve::sec1::ToSec1Point;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{error::MaterialError, files};

#[derive(Debug, Clone, Copy, Default, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Derivation {
    #[default]
    Ed25519Seed,
    P256Seed,
    P256Scalar,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    #[default]
    Raw,
    Hex,
}

#[derive(Serialize)]
pub(crate) struct Report {
    pub derivation: &'static str,
    pub curve: &'static str,
    pub identity_matches: bool,
    pub csr_signature_valid: bool,
    pub csr_sha256: String,
    pub public_key_sha256: String,
    pub trust_verified: bool,
}

pub(crate) fn candidate(
    bytes: &[u8],
    format: Format,
    offset: Option<usize>,
) -> Result<Zeroizing<[u8; 32]>> {
    let invalid = || {
        MaterialError::Identity(
            "expected 32 raw bytes, an explicit 32-byte slice, or 64 hexadecimal digits",
        )
    };
    let mut result = Zeroizing::new([0_u8; 32]);
    match format {
        Format::Raw => {
            let bytes = if let Some(offset) = offset {
                bytes
                    .get(offset..offset.checked_add(32).ok_or_else(invalid)?)
                    .ok_or_else(invalid)?
            } else {
                bytes
            };
            if bytes.len() != 32 {
                return Err(invalid().into());
            }
            result.copy_from_slice(bytes);
        }
        Format::Hex => {
            if offset.is_some() {
                return Err(
                    MaterialError::Identity("offset is only available for raw input").into(),
                );
            }
            let text = std::str::from_utf8(bytes).map_err(|_| invalid())?.trim();
            hex::decode_to_slice(text, result.as_mut()).map_err(|_| invalid())?;
        }
    }
    Ok(result)
}

pub(crate) fn verify(seed: &[u8; 32], csr: &[u8], derivation: Derivation) -> Result<Report> {
    let verify = || -> Result<Report> {
        let request = decode(csr)?;
        let report = verify_csr(csr)?;
        ensure!(report.signature_valid && report.cert_type == "keymint");
        let request = as_array(&request, "CSR")?;
        let dice = as_array(
            request.get(2).ok_or_else(|| anyhow::anyhow!("no DICE"))?,
            "DICE",
        )?;
        let leaf = as_array(
            dice.last().ok_or_else(|| anyhow::anyhow!("no leaf"))?,
            "leaf",
        )?;
        let payload = decode(as_bytes(
            leaf.get(2).ok_or_else(|| anyhow::anyhow!("no payload"))?,
            "payload",
        )?)?;
        let payload = as_map(&payload, "leaf payload")?;
        let subject = decode(as_bytes(
            map_get(payload, DICE_SUBJECT_PUB_KEY)
                .ok_or_else(|| anyhow::anyhow!("no subject key"))?,
            "subject key",
        )?)?;
        let (key, curve, method) = match derivation {
            Derivation::Ed25519Seed => (
                DeviceKeys::from_seed(*seed).cose_key(),
                "ed25519",
                "ed25519-seed",
            ),
            Derivation::P256Seed => (
                DeviceKeys::from_seed_with_curve(*seed, DiceCurve::P256).cose_key(),
                "p256",
                "p256-seed",
            ),
            Derivation::P256Scalar => {
                let secret = p256::SecretKey::from_slice(seed)?;
                let point = secret.public_key().to_sec1_point(false);
                let key = Value::Map(vec![
                    (int(1), int(2)),
                    (int(3), int(-7)),
                    (int(-1), int(1)),
                    (int(-2), Value::Bytes(point.x().unwrap().to_vec())),
                    (int(-3), Value::Bytes(point.y().unwrap().to_vec())),
                ]);
                (key, "p256", "p256-scalar")
            }
        };
        let public = encode(&key)?;
        Ok(Report {
            derivation: method,
            curve,
            identity_matches: public == encode(&subject)?,
            csr_signature_valid: true,
            csr_sha256: files::digest(csr),
            public_key_sha256: files::digest(&public),
            trust_verified: false,
        })
    };
    verify().map_err(|_| {
        MaterialError::Identity(
        "CSR must have a valid KeyMint signature and DICE chain; candidate derivation must be valid"
    ).into()
    })
}
