use anyhow::{Context, Result, anyhow};
use base64ct::{Base64Url, Encoding};
use ciborium::value::Value;
use reqwest::{Client, StatusCode};
use serde::Serialize;

use crate::runtime::errors::AppError;

use super::cbor::{as_array, as_bytes, as_i128, as_map, decode, encode, int, map_get};
use super::cose_dice::{RPC_CURVE_25519, RPC_CURVE_P256};

pub const RKP_SERVER_URL: &str = "https://remoteprovisioning.googleapis.com/v1";

#[derive(Debug, Clone, Serialize)]
pub struct EekResponse {
    pub challenge_hex: String,
    pub challenge: Vec<u8>,
    pub eek_public_hex: String,
    pub eek_public: Vec<u8>,
    #[serde(skip)]
    pub eek_id: Vec<u8>,
    pub eek_curve: i128,
}

#[derive(Debug, Clone)]
struct SelectedEek {
    public_key: Vec<u8>,
    key_id: Vec<u8>,
    curve: i128,
}

const MAX_RKP_CHALLENGE_LEN: usize = 64;
const COSE_KEY_TYPE_OKP: i128 = 1;
const COSE_KEY_TYPE_EC2: i128 = 2;
const COSE_KEY_ID_LABEL: i128 = 2;
const COSE_ALGORITHM_ECDH_ES_HKDF_256: i128 = -25;
const COSE_CURVE_P256: i128 = 1;
const COSE_CURVE_X25519: i128 = 4;
const P256_COORD_LEN: usize = 32;
const X25519_PUBLIC_KEY_LEN: usize = 32;

pub async fn fetch_eek(
    client: &Client,
    fingerprint: &str,
    server_url: &str,
) -> Result<EekResponse> {
    let url = format!("{server_url}:fetchEekChain");
    let payload = encode(&Value::Map(vec![
        (
            Value::Text("fingerprint".into()),
            Value::Text(fingerprint.into()),
        ),
        (Value::Text("id".into()), int(42)),
    ]))?;

    let response = client
        .post(url)
        .header("Content-Type", "application/cbor")
        .header("Accept", "application/cbor")
        .body(payload)
        .send()
        .await
        .context("send fetchEekChain request")?;

    let status = response.status();
    let body = response.bytes().await.context("read fetchEekChain body")?;
    if !status.is_success() {
        return Err(map_http_error(status, &body).into());
    }

    parse_fetch_eek_response(&body)
}

pub async fn submit_csr(
    client: &Client,
    csr_bytes: &[u8],
    challenge: &[u8],
    server_url: &str,
) -> Result<Vec<Vec<u8>>> {
    let url = format!(
        "{server_url}:signCertificates?challenge={}",
        Base64Url::encode_string(challenge)
    );

    let response = client
        .post(url)
        .header("Content-Type", "application/cbor")
        .header("Accept", "application/cbor")
        .body(csr_bytes.to_vec())
        .send()
        .await
        .context("send signCertificates request")?;

    let status = response.status();
    let body = response
        .bytes()
        .await
        .context("read signCertificates body")?;
    if !status.is_success() {
        return Err(map_http_error(status, &body).into());
    }

    parse_sign_certificates_response(&body)
}

fn map_http_error(status: StatusCode, body: &[u8]) -> AppError {
    let message = String::from_utf8_lossy(body).trim().to_owned();
    if status.as_u16() == 444 {
        return AppError::DeviceNotRegistered(message);
    }
    if status.is_client_error() {
        return AppError::RkpClient(format!("HTTP {}: {message}", status.as_u16()));
    }
    if status.is_server_error() {
        return AppError::RkpServer(format!("HTTP {}: {message}", status.as_u16()));
    }

    AppError::RkpServer(format!("HTTP {}: {message}", status.as_u16()))
}

fn select_eek_material(chains_value: &Value) -> Result<SelectedEek> {
    let chains = as_array(chains_value, "EEK chains")?;
    let mut last_error = None;

    for preferred in [RPC_CURVE_25519, RPC_CURVE_P256] {
        for chain_entry in chains {
            let chain_entry_items = as_array(chain_entry, "EEK chain entry")?;
            let curve = as_i128(
                chain_entry_items
                    .first()
                    .ok_or_else(|| anyhow!("EEK chain entry missing curve"))?,
                "curve",
            )?;
            if curve != preferred {
                continue;
            }

            match extract_eek_material(chain_entry_items, curve) {
                Ok(selected) => return Ok(selected),
                Err(error) => last_error = Some(error),
            }
        }
    }

    if let Some(error) = last_error {
        return Err(error);
    }

    Err(anyhow!("RKP server did not return a usable EEK public key"))
}

fn extract_eek_material(chain_entry_items: &[Value], curve: i128) -> Result<SelectedEek> {
    let chain = as_array(
        chain_entry_items
            .get(1)
            .ok_or_else(|| anyhow!("EEK chain entry missing certificates"))?,
        "EEK certificate chain",
    )?;
    let last_cert = chain.last().ok_or_else(|| anyhow!("EEK chain is empty"))?;
    let cert_items = as_array(last_cert, "EEK certificate")?;
    let payload_bytes = as_bytes(
        cert_items
            .get(2)
            .ok_or_else(|| anyhow!("EEK certificate missing payload"))?,
        "EEK payload",
    )?;
    let payload = decode(payload_bytes)?;
    let payload_entries = as_map(&payload, "EEK payload")?;
    parse_eek_cose_key(payload_entries, curve)
}

fn parse_eek_cose_key(entries: &[(Value, Value)], eek_curve: i128) -> Result<SelectedEek> {
    let key_type = required_cose_int(entries, 1, "EEK key type")?;
    let algorithm = required_cose_int(entries, 3, "EEK algorithm")?;
    let curve_id = required_cose_int(entries, -1, "EEK curve id")?;
    let key_id = required_cose_bytes(entries, COSE_KEY_ID_LABEL, "EEK key identifier")?.to_vec();
    if key_id.is_empty() {
        return Err(anyhow!("EEK key identifier must not be empty"));
    }
    if algorithm != COSE_ALGORITHM_ECDH_ES_HKDF_256 {
        return Err(anyhow!(
            "EEK algorithm must be {COSE_ALGORITHM_ECDH_ES_HKDF_256}, got {algorithm}"
        ));
    }

    match eek_curve {
        RPC_CURVE_25519 => {
            if key_type != COSE_KEY_TYPE_OKP {
                return Err(anyhow!("X25519 EEK key type must be {COSE_KEY_TYPE_OKP}"));
            }
            if curve_id != COSE_CURVE_X25519 {
                return Err(anyhow!("X25519 EEK curve id must be {COSE_CURVE_X25519}"));
            }

            let public_key = required_cose_bytes(entries, -2, "X25519 EEK public key")?.to_vec();
            if public_key.len() != X25519_PUBLIC_KEY_LEN {
                return Err(anyhow!(
                    "X25519 EEK public key must be {X25519_PUBLIC_KEY_LEN} bytes"
                ));
            }

            Ok(SelectedEek {
                public_key,
                key_id,
                curve: eek_curve,
            })
        }
        RPC_CURVE_P256 => {
            if key_type != COSE_KEY_TYPE_EC2 {
                return Err(anyhow!("P-256 EEK key type must be {COSE_KEY_TYPE_EC2}"));
            }
            if curve_id != COSE_CURVE_P256 {
                return Err(anyhow!("P-256 EEK curve id must be {COSE_CURVE_P256}"));
            }

            let x = required_cose_bytes(entries, -2, "P-256 EEK x coordinate")?;
            let y = required_cose_bytes(entries, -3, "P-256 EEK y coordinate")?;
            if x.len() != P256_COORD_LEN || y.len() != P256_COORD_LEN {
                return Err(anyhow!(
                    "P-256 EEK coordinates must each be {P256_COORD_LEN} bytes"
                ));
            }

            let mut public_key = Vec::with_capacity(P256_COORD_LEN * 2);
            public_key.extend_from_slice(x);
            public_key.extend_from_slice(y);

            Ok(SelectedEek {
                public_key,
                key_id,
                curve: eek_curve,
            })
        }
        other => Err(anyhow!("unsupported EEK curve {other} in response")),
    }
}

fn required_cose_int(entries: &[(Value, Value)], key: i128, label: &str) -> Result<i128> {
    as_i128(
        map_get(entries, key).ok_or_else(|| anyhow!("{label} is missing"))?,
        label,
    )
}

fn required_cose_bytes<'a>(
    entries: &'a [(Value, Value)],
    key: i128,
    label: &str,
) -> Result<&'a [u8]> {
    as_bytes(
        map_get(entries, key).ok_or_else(|| anyhow!("{label} is missing"))?,
        label,
    )
}

fn parse_fetch_eek_response(body: &[u8]) -> Result<EekResponse> {
    let value = invalid_rkp_response(decode(body))?;
    let items = invalid_rkp_response(as_array(&value, "fetchEekChain response"))?;
    let eek_chains = items.first().ok_or_else(|| {
        AppError::InvalidRkpResponse("fetchEekChain response is missing EEK chains".into())
    })?;
    let challenge = invalid_rkp_response(as_bytes(
        items.get(1).ok_or_else(|| {
            AppError::InvalidRkpResponse("fetchEekChain response is missing challenge".into())
        })?,
        "challenge",
    ))?
    .to_vec();
    validate_challenge(&challenge)?;

    let selected = invalid_rkp_response(select_eek_material(eek_chains))?;
    Ok(EekResponse {
        challenge_hex: hex::encode(&challenge),
        challenge,
        eek_public_hex: hex::encode(&selected.public_key),
        eek_public: selected.public_key,
        eek_id: selected.key_id,
        eek_curve: selected.curve,
    })
}

fn parse_sign_certificates_response(body: &[u8]) -> Result<Vec<Vec<u8>>> {
    let value = invalid_rkp_response(decode(body))?;
    let top = invalid_rkp_response(as_array(&value, "signCertificates response"))?;
    if top.is_empty() {
        return Err(
            AppError::InvalidRkpResponse("signCertificates response is empty".into()).into(),
        );
    }

    let inner = match &top[0] {
        Value::Array(inner) => inner.as_slice(),
        _ => top,
    };

    if inner.len() < 2 {
        return Err(AppError::InvalidRkpResponse(
            "signCertificates response is missing certificate chain data".into(),
        )
        .into());
    }

    let shared = invalid_rkp_response(as_bytes(&inner[0], "shared certificate chain"))?.to_vec();
    let unique_list = invalid_rkp_response(as_array(&inner[1], "unique certificate chains"))?;

    let mut chains = Vec::new();
    for item in unique_list {
        let unique = invalid_rkp_response(as_bytes(item, "unique certificate chain"))?;
        let mut chain = shared.clone();
        chain.extend_from_slice(unique);
        chains.push(chain);
    }

    if chains.is_empty() {
        return Err(anyhow!("RKP server returned no certificate chains"));
    }

    Ok(chains)
}

fn validate_challenge(challenge: &[u8]) -> Result<()> {
    if challenge.len() <= MAX_RKP_CHALLENGE_LEN {
        return Ok(());
    }

    Err(AppError::InvalidRkpResponse(format!(
        "fetchEekChain challenge must be at most {MAX_RKP_CHALLENGE_LEN} bytes, got {}",
        challenge.len()
    ))
    .into())
}

fn invalid_rkp_response<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| AppError::InvalidRkpResponse(error.to_string()).into())
}

#[cfg(test)]
mod tests {
    use super::{
        COSE_ALGORITHM_ECDH_ES_HKDF_256, COSE_CURVE_P256, COSE_CURVE_X25519, MAX_RKP_CHALLENGE_LEN,
        parse_fetch_eek_response, validate_challenge,
    };
    use crate::features::rkp::{
        cbor::{bytes, encode, int},
        cose_dice::{RPC_CURVE_25519, RPC_CURVE_P256},
    };
    use ciborium::value::Value;

    fn eek_payload_x25519(key_id: &[u8], public_key: &[u8]) -> Value {
        Value::Map(vec![
            (int(1), int(1)),
            (int(2), bytes(key_id.to_vec())),
            (int(3), int(COSE_ALGORITHM_ECDH_ES_HKDF_256)),
            (int(-1), int(COSE_CURVE_X25519)),
            (int(-2), bytes(public_key.to_vec())),
        ])
    }

    fn eek_payload_p256(key_id: &[u8], x: &[u8], y: &[u8]) -> Value {
        Value::Map(vec![
            (int(1), int(2)),
            (int(2), bytes(key_id.to_vec())),
            (int(3), int(COSE_ALGORITHM_ECDH_ES_HKDF_256)),
            (int(-1), int(COSE_CURVE_P256)),
            (int(-2), bytes(x.to_vec())),
            (int(-3), bytes(y.to_vec())),
        ])
    }

    fn eek_chain_entry(curve: i128, payload: Value) -> Value {
        Value::Array(vec![
            int(curve),
            Value::Array(vec![Value::Array(vec![
                Value::Null,
                Value::Null,
                bytes(encode(&payload).unwrap()),
            ])]),
        ])
    }

    fn fetch_eek_response(chains: Vec<Value>, challenge: &[u8]) -> Vec<u8> {
        encode(&Value::Array(vec![
            Value::Array(chains),
            bytes(challenge.to_vec()),
        ]))
        .unwrap()
    }

    #[test]
    fn validate_challenge_rejects_oversized_values() {
        let error = validate_challenge(&vec![0_u8; MAX_RKP_CHALLENGE_LEN + 1]).unwrap_err();
        assert!(error.to_string().contains("challenge"));
    }

    #[test]
    fn parse_fetch_eek_response_prefers_x25519_over_p256() {
        let challenge = [0xAA; 32];
        let x25519_key_id = [0x11; 4];
        let p256_key_id = [0x22; 4];
        let x = [0x44; 32];
        let y = [0x55; 32];

        let body = fetch_eek_response(
            vec![
                eek_chain_entry(RPC_CURVE_P256, eek_payload_p256(&p256_key_id, &x, &y)),
                eek_chain_entry(
                    RPC_CURVE_25519,
                    eek_payload_x25519(&x25519_key_id, &[0x33; 32]),
                ),
            ],
            &challenge,
        );

        let parsed = parse_fetch_eek_response(&body).unwrap();
        assert_eq!(parsed.eek_curve, RPC_CURVE_25519);
        assert_eq!(parsed.eek_public, vec![0x33; 32]);
        assert_eq!(parsed.eek_id, x25519_key_id);
    }

    #[test]
    fn parse_fetch_eek_response_accepts_p256_coordinates() {
        let challenge = [0xAB; 32];
        let key_id = [0x77; 4];
        let x = [0x10; 32];
        let y = [0x20; 32];

        let body = fetch_eek_response(
            vec![eek_chain_entry(
                RPC_CURVE_P256,
                eek_payload_p256(&key_id, &x, &y),
            )],
            &challenge,
        );

        let parsed = parse_fetch_eek_response(&body).unwrap();
        let mut expected_public = Vec::from(x);
        expected_public.extend_from_slice(&y);

        assert_eq!(parsed.eek_curve, RPC_CURVE_P256);
        assert_eq!(parsed.eek_public, expected_public);
        assert_eq!(parsed.eek_id, key_id);
        assert_eq!(parsed.challenge, challenge);
    }

    #[test]
    fn parse_fetch_eek_response_skips_malformed_preferred_chain() {
        let challenge = [0xAC; 32];
        let key_id = [0x88; 4];
        let x = [0x21; 32];
        let y = [0x43; 32];

        let body = fetch_eek_response(
            vec![
                eek_chain_entry(RPC_CURVE_25519, eek_payload_x25519(&[0x99; 4], &[0x31; 31])),
                eek_chain_entry(RPC_CURVE_P256, eek_payload_p256(&key_id, &x, &y)),
            ],
            &challenge,
        );

        let parsed = parse_fetch_eek_response(&body).unwrap();
        assert_eq!(parsed.eek_curve, RPC_CURVE_P256);
        assert_eq!(parsed.eek_id, key_id);
        assert_eq!(parsed.eek_public.len(), 64);
    }
}
