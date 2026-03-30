use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};

mod abx;

#[cfg(test)]
mod tests;

pub fn read_xmlish_text(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    decode_xmlish_bytes(&bytes).with_context(|| format!("decode {}", path.display()))
}

pub fn decode_xmlish_bytes(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&abx::MAGIC) {
        return abx::decode(bytes);
    }

    decode_text_xml(bytes)
}

pub fn parse_boolish(value: &str) -> Option<bool> {
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        value
            if value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on") =>
        {
            Some(true)
        }
        value
            if value.eq_ignore_ascii_case("false")
                || value.eq_ignore_ascii_case("no")
                || value.eq_ignore_ascii_case("off") =>
        {
            Some(false)
        }
        _ => None,
    }
}

pub fn parse_i64ish(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (negative, digits) = if let Some(rest) = trimmed.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        (false, rest)
    } else {
        (false, trimmed)
    };

    let parsed = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()
    } else if digits
        .bytes()
        .any(|byte| matches!(byte, b'a'..=b'f' | b'A'..=b'F'))
    {
        i64::from_str_radix(digits, 16).ok()
    } else {
        digits.parse::<i64>().ok()
    }?;

    Some(if negative { -parsed } else { parsed })
}

fn decode_text_xml(bytes: &[u8]) -> Result<String> {
    if let Some(bytes) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes.to_vec()).context("decode UTF-8 XML with BOM");
    }

    if let Some(bytes) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16_xml(bytes, true);
    }

    if let Some(bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16_xml(bytes, false);
    }

    if let Some(little_endian) = sniff_utf16_xml_without_bom(bytes) {
        return decode_utf16_xml(bytes, little_endian);
    }

    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => Ok(text),
        Err(_) => decode_modified_utf8(bytes),
    }
}

fn sniff_utf16_xml_without_bom(bytes: &[u8]) -> Option<bool> {
    match bytes.get(..4)? {
        [b'<', 0x00, _, 0x00] => Some(true),
        [0x00, b'<', 0x00, _] => Some(false),
        _ => None,
    }
}

fn decode_utf16_xml(bytes: &[u8], little_endian: bool) -> Result<String> {
    if bytes.len() % 2 != 0 {
        bail!("UTF-16 XML byte length is not even");
    }

    let code_units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();

    String::from_utf16(&code_units).context("decode UTF-16 XML")
}

fn decode_modified_utf8(bytes: &[u8]) -> Result<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_owned());
    }

    let mut offset = 0;
    let mut out = String::new();
    let mut high_surrogate = None;

    while offset < bytes.len() {
        match decode_modified_utf8_unit(bytes, &mut offset)? {
            ModifiedUnit::Scalar(ch) => {
                if high_surrogate.is_some() {
                    bail!("dangling modified UTF-8 high surrogate before scalar");
                }
                out.push(ch);
            }
            ModifiedUnit::CodeUnit(code_unit) => {
                push_code_unit(&mut out, &mut high_surrogate, code_unit)?
            }
        }
    }

    if high_surrogate.is_some() {
        bail!("dangling modified UTF-8 high surrogate");
    }

    Ok(out)
}

enum ModifiedUnit {
    Scalar(char),
    CodeUnit(u16),
}

fn decode_modified_utf8_unit(bytes: &[u8], offset: &mut usize) -> Result<ModifiedUnit> {
    let first = *bytes
        .get(*offset)
        .ok_or_else(|| anyhow!("unexpected end of modified UTF-8"))?;

    if first <= 0x7F {
        *offset += 1;
        return Ok(ModifiedUnit::CodeUnit(u16::from(first)));
    }

    if first & 0xE0 == 0xC0 {
        let second = read_continuation(bytes, *offset + 1)?;
        *offset += 2;
        let code_unit = (u16::from(first & 0x1F) << 6) | u16::from(second & 0x3F);
        return Ok(ModifiedUnit::CodeUnit(code_unit));
    }

    if first & 0xF0 == 0xE0 {
        let second = read_continuation(bytes, *offset + 1)?;
        let third = read_continuation(bytes, *offset + 2)?;
        *offset += 3;
        let code_unit = (u16::from(first & 0x0F) << 12)
            | (u16::from(second & 0x3F) << 6)
            | u16::from(third & 0x3F);
        return Ok(ModifiedUnit::CodeUnit(code_unit));
    }

    if first & 0xF8 == 0xF0 {
        let second = read_continuation(bytes, *offset + 1)?;
        let third = read_continuation(bytes, *offset + 2)?;
        let fourth = read_continuation(bytes, *offset + 3)?;
        *offset += 4;

        let scalar = (u32::from(first & 0x07) << 18)
            | (u32::from(second & 0x3F) << 12)
            | (u32::from(third & 0x3F) << 6)
            | u32::from(fourth & 0x3F);
        let ch = char::from_u32(scalar)
            .ok_or_else(|| anyhow!("invalid modified UTF-8 scalar U+{scalar:04X}"))?;
        return Ok(ModifiedUnit::Scalar(ch));
    }

    bail!("unsupported modified UTF-8 lead byte 0x{first:02x}")
}

fn push_code_unit(
    out: &mut String,
    high_surrogate: &mut Option<u16>,
    code_unit: u16,
) -> Result<()> {
    match code_unit {
        0xD800..=0xDBFF => {
            if high_surrogate.replace(code_unit).is_some() {
                bail!("consecutive modified UTF-8 high surrogates");
            }
        }
        0xDC00..=0xDFFF => {
            let high = high_surrogate.take().ok_or_else(|| {
                anyhow!("modified UTF-8 low surrogate without leading high surrogate")
            })?;
            let scalar =
                0x10000 + (((u32::from(high) - 0xD800) << 10) | (u32::from(code_unit) - 0xDC00));
            let ch = char::from_u32(scalar)
                .ok_or_else(|| anyhow!("invalid modified UTF-8 surrogate pair U+{scalar:04X}"))?;
            out.push(ch);
        }
        scalar => {
            if high_surrogate.is_some() {
                bail!("dangling modified UTF-8 high surrogate");
            }
            let ch = char::from_u32(u32::from(scalar))
                .ok_or_else(|| anyhow!("invalid modified UTF-8 code unit U+{scalar:04X}"))?;
            out.push(ch);
        }
    }

    Ok(())
}

fn read_continuation(bytes: &[u8], index: usize) -> Result<u8> {
    let byte = *bytes
        .get(index)
        .ok_or_else(|| anyhow!("unexpected end of modified UTF-8"))?;
    if byte & 0xC0 != 0x80 {
        bail!("invalid modified UTF-8 continuation byte 0x{byte:02x}");
    }
    Ok(byte)
}
