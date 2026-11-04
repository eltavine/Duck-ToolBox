//! explicit offline signing material attached to an existing, verified DICE chain

use crate::{
    cbor::{MAX_INPUT_BYTES, as_map, decode},
    profile::DeviceInfo,
    verify::{DiceChainReport, verify_dice_chain},
};
use anyhow::{Context, Result, ensure};
use ciborium::value::Value;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};
use zeroize::Zeroizing;

mod error;
mod json;
mod signing;
pub use error::{MaterialError, error_code};
use json::StrictDevice;
use signing::{Signer, SigningInput};

#[derive(Debug, Serialize)]
pub struct MaterialReport {
    pub material_type: String,
    pub algorithm: &'static str,
    pub dice_entries: usize,
    pub chain_signatures_valid: bool,
    pub key_matches: bool,
    pub source_trust: &'static str,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuanceConfig {
    pub fingerprint: String,
    #[serde(default)]
    pub include_fingerprint: bool,
    pub server_url: String,
    pub protocol: String,
}

pub struct ImportedMaterial {
    pub(crate) chain_bytes: Vec<u8>,
    pub(crate) uds_certs_bytes: Vec<u8>,
    pub(crate) signer: Signer,
    chain: DiceChainReport,
    report: MaterialReport,
    device: Option<DeviceInfo>,
    issuance: Option<IssuanceConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    version: u32,
    dice_chain_hex: String,
    signing: SigningInput,
    device: Option<StrictDevice>,
    issuance: Option<IssuanceConfig>,
    uds_certs_hex: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    device: Option<StrictDevice>,
    issuance: Option<IssuanceConfig>,
    uds_certs_hex: Option<String>,
}

pub fn load(
    bundle: Option<&Path>,
    chain: Option<&Path>,
    signing: Option<&Path>,
    config: Option<&Path>,
) -> Result<ImportedMaterial> {
    let (chain, signing, config) = match bundle {
        Some(path) => {
            ensure!(
                chain.is_none() && signing.is_none(),
                MaterialError::Configuration
            );
            let bundle: Bundle = parse_file(path)?;
            ensure!(bundle.version == 1, MaterialError::Unsupported);
            let chain =
                hex::decode(&bundle.dice_chain_hex).map_err(|_| MaterialError::Structure)?;
            let source = Config {
                device: bundle.device,
                issuance: bundle.issuance,
                uds_certs_hex: bundle.uds_certs_hex,
            };
            let supplemental = config.map(parse_file).transpose()?.unwrap_or_default();
            (chain, bundle.signing, source.supplement(supplemental)?)
        }
        None => {
            let chain = read_file(chain.context(MaterialError::Structure)?)?.to_vec();
            let signing: SigningInput = parse_file(signing.context(MaterialError::Structure)?)?;
            let config = config.map(parse_file).transpose()?.unwrap_or_default();
            (chain, signing, config)
        }
    };
    let chain_report = verify_dice_chain(&chain)
        .map_err(|error| MaterialError::InvalidChain(error.to_string()))?;
    let (signer, material_type) = signing::resolve(&signing, &chain_report)?;
    let device = config.device.map(DeviceInfo::from);
    if let Some(device) = device.as_ref() {
        crate::validate::validate_device_info_for_request(device)
            .map_err(|_| MaterialError::Configuration)?;
    }
    if let Some(issuance) = config.issuance.as_ref() {
        issuance.validate()?;
    }
    let uds_certs_bytes = match config.uds_certs_hex {
        Some(value) => hex::decode(value).map_err(|_| MaterialError::Structure)?,
        None => vec![0xa0],
    };
    let uds = decode(&uds_certs_bytes).map_err(|_| MaterialError::Structure)?;
    if !as_map(&uds, "UDS certificates")
        .map_err(|_| MaterialError::Structure)?
        .is_empty()
    {
        crate::certificates::validate_uds(
            &uds,
            &chain_report.root_public_key,
            chain_report.root_algorithm,
        )
        .map_err(|error| MaterialError::InvalidChain(error.to_string()))?;
    }
    let report = MaterialReport {
        material_type,
        algorithm: match signer.algorithm() {
            -8 => "ed25519",
            -7 => "p256",
            -35 => "p384",
            _ => unreachable!(),
        },
        dice_entries: chain_report.dice_entries,
        chain_signatures_valid: true,
        key_matches: true,
        source_trust: "unverified",
    };
    Ok(ImportedMaterial {
        chain_bytes: chain,
        uds_certs_bytes,
        signer,
        chain: chain_report,
        report,
        device,
        issuance: config.issuance,
    })
}

impl ImportedMaterial {
    pub fn report(&self) -> &MaterialReport {
        &self.report
    }
    pub fn device(&self) -> Option<&DeviceInfo> {
        self.device.as_ref()
    }
    pub fn issuance(&self) -> Option<&IssuanceConfig> {
        self.issuance.as_ref()
    }
    pub fn root_public_key(&self) -> &[u8] {
        &self.chain.root_public_key
    }
    pub fn root_algorithm(&self) -> i128 {
        self.chain.root_algorithm
    }
    pub fn algorithm(&self) -> i128 {
        self.signer.algorithm()
    }
    pub fn build_csr(&self, challenge: &[u8], keys: &[Value]) -> Result<Vec<u8>> {
        let device = self.device().context(MaterialError::Configuration)?;
        crate::cose::imported::build(self, device, challenge, keys)
    }
}

impl IssuanceConfig {
    fn validate(&self) -> Result<()> {
        crate::http::validate_endpoint(&self.server_url)
            .map_err(|_| MaterialError::Configuration)?;
        ensure!(
            self.protocol == "v3" && !self.fingerprint.trim().is_empty(),
            MaterialError::Configuration
        );
        Ok(())
    }
}

impl Config {
    fn supplement(self, other: Self) -> Result<Self> {
        ensure!(
            !(self.device.is_some() && other.device.is_some())
                && !(self.issuance.is_some() && other.issuance.is_some())
                && !(self.uds_certs_hex.is_some() && other.uds_certs_hex.is_some()),
            MaterialError::Configuration
        );
        Ok(Self {
            device: self.device.or(other.device),
            issuance: self.issuance.or(other.issuance),
            uds_certs_hex: self.uds_certs_hex.or(other.uds_certs_hex),
        })
    }
}

fn parse_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    json::parse(&read_file(path)?).map_err(|_| MaterialError::Structure.into())
}

fn read_file(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    let read = || -> Result<Zeroizing<Vec<u8>>> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file: File = options.open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_INPUT_BYTES as u64,
            MaterialError::Structure
        );
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_INPUT_BYTES,
            MaterialError::Structure
        );
        Ok(bytes)
    };
    read().map_err(|_| MaterialError::Structure.into())
}

#[cfg(test)]
mod tests;
