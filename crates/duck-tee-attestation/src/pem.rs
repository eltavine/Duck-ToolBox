use anyhow::Result;
use base64ct::{Base64, Encoding};
use zeroize::Zeroizing;

use crate::error::MaterialError;

pub(crate) struct Block {
    pub label: String,
    pub der: Zeroizing<Vec<u8>>,
}

/// PEM blocks may be surrounded by capture headers or binary data. Every discovered
/// block must be complete; malformed or unsupported blocks are never silently skipped.
pub(crate) fn blocks(bytes: &[u8]) -> Result<Vec<Block>> {
    let mut remaining = bytes;
    let mut blocks = Vec::new();
    while let Some(start) = find(remaining, b"-----BEGIN ") {
        remaining = &remaining[start + 11..];
        let label_end =
            find(remaining, b"-----").ok_or(MaterialError::Keybox("incomplete PEM header"))?;
        let label = std::str::from_utf8(&remaining[..label_end])
            .map_err(|_| MaterialError::Keybox("invalid PEM label"))?;
        if !matches!(
            label,
            "CERTIFICATE" | "PRIVATE KEY" | "EC PRIVATE KEY" | "RSA PRIVATE KEY"
        ) {
            return Err(MaterialError::Keybox("unsupported PEM block type").into());
        }
        remaining = &remaining[label_end + 5..];
        let end_marker = format!("-----END {label}-----");
        let end = find(remaining, end_marker.as_bytes())
            .ok_or(MaterialError::Keybox("incomplete PEM block"))?;
        let encoded = Zeroizing::new(
            remaining[..end]
                .iter()
                .filter(|byte| !byte.is_ascii_whitespace())
                .copied()
                .collect::<Vec<_>>(),
        );
        let text = std::str::from_utf8(&encoded)
            .map_err(|_| MaterialError::Keybox("invalid PEM encoding"))?;
        let der =
            Base64::decode_vec(text).map_err(|_| MaterialError::Keybox("invalid PEM base64"))?;
        blocks.push(Block {
            label: label.to_owned(),
            der: Zeroizing::new(der),
        });
        if blocks.len() > 128 {
            return Err(MaterialError::Keybox("too many PEM blocks").into());
        }
        remaining = &remaining[end + end_marker.len()..];
    }
    if blocks.is_empty() {
        return Err(MaterialError::Keybox("no PEM material found").into());
    }
    Ok(blocks)
}

pub(crate) fn wrap(label: &str, der: &[u8]) -> Zeroizing<String> {
    let encoded = Zeroizing::new(Base64::encode_string(der));
    let body = encoded
        .as_bytes()
        .chunks(64)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    Zeroizing::new(format!(
        "-----BEGIN {label}-----\n{body}\n-----END {label}-----"
    ))
}

pub(crate) fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}
