//! Server-facing metadata outside the authenticated KeyMint request.

use anyhow::{Result, ensure};
use ciborium::value::Value;

use crate::cbor::{
    MAX_INPUT_BYTES, array_items_raw, as_array, as_i128, as_map, as_text, decode, encode, text,
};

pub fn unverified_device_info(fingerprint: Option<&str>) -> Result<Value> {
    let entries = match fingerprint {
        Some(value) => {
            ensure!(
                !value.trim().is_empty(),
                "build fingerprint must not be empty"
            );
            vec![(text("fingerprint"), text(value))]
        }
        None => Vec::new(),
    };
    Ok(Value::Map(entries))
}

pub(crate) fn validate_unverified_device_info(value: &Value) -> Result<()> {
    for (key, value) in as_map(value, "unverified device info")? {
        ensure!(
            as_text(key, "unverified device info key")? == "fingerprint",
            "unsupported unverified device info field"
        );
        ensure!(
            !as_text(value, "fingerprint")?.trim().is_empty(),
            "build fingerprint must not be empty"
        );
    }
    Ok(())
}

/// Attaches the optional fifth field to an AuthenticatedRequest, preserving all four
/// original items byte for byte. A pre-existing metadata field is never overwritten.
pub fn with_fingerprint(csr: &[u8], fingerprint: Option<&str>) -> Result<Vec<u8>> {
    let value = decode(csr)?;
    let items = as_array(&value, "AuthenticatedRequest")?;
    ensure!(
        items.len() == 4,
        "expected a four-item AuthenticatedRequest"
    );
    ensure!(
        as_i128(&items[0], "AuthenticatedRequest version")? == 1,
        "unsupported AuthenticatedRequest version"
    );
    let Some(fingerprint) = fingerprint else {
        return Ok(csr.to_vec());
    };
    let metadata = encode(&unverified_device_info(Some(fingerprint))?)?;
    let mut result = vec![0x85];
    for item in array_items_raw(csr)? {
        result.extend_from_slice(item);
    }
    result.extend_from_slice(&metadata);
    ensure!(
        result.len() <= MAX_INPUT_BYTES,
        "CSR exceeds the input limit"
    );
    Ok(result)
}

#[cfg(test)]
mod tests;
