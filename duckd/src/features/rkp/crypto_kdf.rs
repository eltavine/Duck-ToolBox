use aes_gcm::aes::Aes128;
use anyhow::{Context, Result};
use cmac::{Cmac, Mac};
use hex::FromHex;

use crate::runtime::{errors::AppError, profile::KeySource};

#[derive(Debug, Clone)]
pub struct HardwareKdf {
    key: [u8; 16],
}

impl HardwareKdf {
    pub fn new(aes_key: [u8; 16]) -> Self {
        Self { key: aes_key }
    }

    pub fn derive(&self, label: &[u8], length: usize) -> Result<Vec<u8>> {
        let mut output = Vec::with_capacity(length);
        let blocks = length.div_ceil(16);

        for counter in 1..=blocks {
            let mut mac = Cmac::<Aes128>::new_from_slice(&self.key).context("create AES-CMAC")?;
            mac.update(&(counter as u32).to_be_bytes());
            mac.update(label);
            output.extend_from_slice(&mac.finalize().into_bytes());
        }

        output.truncate(length);
        Ok(output)
    }
}

pub fn resolve_seed(key_source: &KeySource) -> Result<[u8; 32]> {
    match key_source {
        KeySource::Seed { seed_hex } => parse_fixed_hex(seed_hex).map_err(Into::into),
        KeySource::HwKey {
            hw_key_hex,
            kdf_label,
        } => {
            let key: [u8; 16] = parse_fixed_hex(hw_key_hex)?;
            let derived = HardwareKdf::new(key).derive(kdf_label.as_bytes(), 32)?;
            let mut seed = [0_u8; 32];
            seed.copy_from_slice(&derived);
            Ok(seed)
        }
        KeySource::Unset => Err(AppError::MissingKeySource.into()),
    }
}

pub fn parse_fixed_hex<const N: usize>(value: &str) -> Result<[u8; N]> {
    let decoded = <Vec<u8>>::from_hex(value.trim()).context("decode hex")?;
    if decoded.len() != N {
        return Err(match N {
            32 => AppError::InvalidSeedLength(decoded.len()).into(),
            16 => AppError::InvalidHardwareKeyLength(decoded.len()).into(),
            _ => anyhow::anyhow!("expected {N} decoded bytes, got {}", decoded.len()),
        });
    }

    let mut bytes = [0_u8; N];
    bytes.copy_from_slice(&decoded);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{HardwareKdf, parse_fixed_hex, resolve_seed};
    use crate::runtime::profile::KeySource;

    #[test]
    fn parse_hex_requires_exact_length() {
        let parsed = parse_fixed_hex::<4>("00112233").unwrap();
        assert_eq!(parsed, [0x00, 0x11, 0x22, 0x33]);
    }

    #[test]
    fn hardware_kdf_matches_nist_counter_mode_shape() {
        let kdf = HardwareKdf::new([0x11; 16]);
        let out = kdf.derive(b"rkp_bcc_km", 32).unwrap();
        assert_eq!(out.len(), 32);
        assert_ne!(&out[..16], &out[16..]);
    }

    #[test]
    fn resolve_seed_supports_direct_seed() {
        let seed = resolve_seed(&KeySource::Seed {
            seed_hex: "11".repeat(32),
        })
        .unwrap();
        assert_eq!(seed, [0x11; 32]);
    }
}
