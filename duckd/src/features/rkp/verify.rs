use std::collections::HashSet;

use anyhow::{Context, Result, anyhow};
use ciborium::{ser::into_writer, value::Value};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256VerifyingKey};
use p384::ecdsa::{Signature as P384Signature, VerifyingKey as P384VerifyingKey};
use serde::Serialize;

use super::{
    cbor::{as_array, as_bytes, as_i128, as_map, as_text, decode, encode, map_get, map_get_text},
    cose_dice::{
        ALG_EDDSA, ALG_ES256, CWT_ISSUER, CWT_SUBJECT, DICE_KEY_USAGE, DICE_PROFILE_NAME,
        DICE_SUBJECT_PUB_KEY, build_sig_structure,
    },
};

const AUTHENTICATED_REQUEST_VERSION: i128 = 1;
const CSR_PAYLOAD_VERSION: i128 = 3;
const ALG_ES384: i128 = -35;
const ED25519_CURVE: i128 = 6;
const P256_CURVE: i128 = 1;
const P384_CURVE: i128 = 2;
const P256_COORD_LEN: usize = 32;
const P384_COORD_LEN: usize = 48;
const MAX_CHALLENGE_LEN: usize = 64;
const TEST_KEY_LABEL: i128 = -70000;
const ANDROID_DICE_PROFILE_VERSION: &str = "android.15";

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub version: i128,
    pub dice_entries: usize,
    pub uds_pub_hex: String,
    pub signature_valid: bool,
    pub csr_version: i128,
    pub cert_type: String,
    pub brand: Option<String>,
    pub keys_to_sign: usize,
}

pub fn verify_csr(bytes: &[u8]) -> Result<VerifyReport> {
    let value = decode(bytes)?;
    let csr = as_array(&value, "CSR")?;
    ensure_array_len(csr, 4, "CSR")?;

    let version = as_i128(
        csr.first().ok_or_else(|| anyhow!("missing CSR version"))?,
        "CSR version",
    )?;
    if version != AUTHENTICATED_REQUEST_VERSION {
        return Err(anyhow!(
            "CSR version must be {AUTHENTICATED_REQUEST_VERSION}, got {version}"
        ));
    }

    validate_uds_certs(csr.get(1).ok_or_else(|| anyhow!("missing UDS certs"))?)?;

    let dice_chain = as_array(
        csr.get(2).ok_or_else(|| anyhow!("missing DICE chain"))?,
        "DICE chain",
    )?;
    if dice_chain.len() < 2 {
        return Err(anyhow!(
            "DICE chain must contain a UDS key and at least one DICE entry"
        ));
    }
    let uds = extract_uds_key(dice_chain)?;
    let leaf = verify_dice_chain_and_extract_leaf_key(dice_chain)?;

    let signed_data = parse_cose_sign1(
        as_array(
            csr.get(3).ok_or_else(|| anyhow!("missing signed data"))?,
            "signed data",
        )?,
        "signed data",
        Some(&leaf),
    )?;
    let signature_input =
        build_sig_structure("Signature1", signed_data.protected, signed_data.payload)?;

    let signature_valid =
        verify_signed_data_signature(&leaf, signed_data.signature, &signature_input)?;

    let signed_payload = decode(signed_data.payload)?;
    let signed_items = as_array(&signed_payload, "signed payload")?;
    ensure_array_len(signed_items, 2, "signed payload")?;
    let challenge = as_bytes(
        signed_items
            .first()
            .ok_or_else(|| anyhow!("signed payload is missing challenge"))?,
        "challenge",
    )?;
    validate_challenge(challenge, "challenge")?;
    let csr_payload = decode(as_bytes(
        signed_items
            .get(1)
            .ok_or_else(|| anyhow!("signed payload is missing CSR body"))?,
        "CSR payload bytes",
    )?)?;
    let csr_payload_items = as_array(&csr_payload, "CSR payload")?;
    ensure_array_len(csr_payload_items, 4, "CSR payload")?;
    let csr_version = as_i128(
        csr_payload_items
            .first()
            .ok_or_else(|| anyhow!("missing CSR payload version"))?,
        "CSR payload version",
    )?;
    if csr_version != CSR_PAYLOAD_VERSION {
        return Err(anyhow!(
            "CSR payload version must be {CSR_PAYLOAD_VERSION}, got {csr_version}"
        ));
    }
    let cert_type = as_text(
        csr_payload_items
            .get(1)
            .ok_or_else(|| anyhow!("missing cert type"))?,
        "cert type",
    )?
    .to_owned();
    if cert_type.trim().is_empty() {
        return Err(anyhow!("cert type must not be empty"));
    }

    let brand = validate_device_info(
        csr_payload_items
            .get(2)
            .ok_or_else(|| anyhow!("missing device info"))?,
    )?;
    let keys_to_sign = as_array(
        csr_payload_items
            .get(3)
            .ok_or_else(|| anyhow!("missing keysToSign"))?,
        "keysToSign",
    )?;
    validate_keys_to_sign(keys_to_sign)?;

    Ok(VerifyReport {
        version,
        dice_entries: dice_chain.len(),
        uds_pub_hex: hex::encode(&uds.public_key_hex_bytes),
        signature_valid,
        csr_version,
        cert_type,
        brand: Some(brand),
        keys_to_sign: keys_to_sign.len(),
    })
}

struct CosePublicKeyInfo {
    public_key_hex_bytes: Vec<u8>,
    algorithm: i128,
    verifier: CoseVerifier,
}

enum CoseVerifier {
    Ed25519(VerifyingKey),
    P256(P256VerifyingKey),
    P384(P384VerifyingKey),
}

fn extract_uds_key(dice_chain: &[Value]) -> Result<CosePublicKeyInfo> {
    let uds_key = dice_chain
        .first()
        .ok_or_else(|| anyhow!("DICE chain is empty"))?;
    let uds_entries = as_map(uds_key, "UDS COSE key")?;
    parse_cose_public_key(uds_entries, "UDS COSE key")
}

fn verify_dice_chain_and_extract_leaf_key(dice_chain: &[Value]) -> Result<CosePublicKeyInfo> {
    let mut signer = extract_uds_key(dice_chain)?;

    for (index, item) in dice_chain.iter().enumerate().skip(1) {
        let label = format!("DICE chain entry {index}");
        let entry = parse_cose_sign1(as_array(item, "DICE chain entry")?, &label, Some(&signer))?;
        let signature_input = build_sig_structure("Signature1", entry.protected, entry.payload)?;
        if !verify_signed_data_signature(&signer, entry.signature, &signature_input)? {
            return Err(anyhow!("{label} signature is invalid"));
        }

        let payload_value = decode(entry.payload)?;
        signer = parse_dice_entry_payload(&payload_value, &label)?;
    }

    Ok(signer)
}

fn parse_cose_public_key(entries: &[(Value, Value)], label: &str) -> Result<CosePublicKeyInfo> {
    ensure_unique_integer_keys(entries, label)?;
    let key_type = as_i128(
        map_get(entries, 1).ok_or_else(|| anyhow!("{label} is missing `1`"))?,
        &format!("{label} key type"),
    )?;
    let algorithm = as_i128(
        map_get(entries, 3).ok_or_else(|| anyhow!("{label} is missing `3`"))?,
        &format!("{label} algorithm"),
    )?;
    let curve = as_i128(
        map_get(entries, -1).ok_or_else(|| anyhow!("{label} is missing `-1`"))?,
        &format!("{label} curve"),
    )?;

    match (key_type, algorithm, curve) {
        (1, ALG_EDDSA, ED25519_CURVE) => {
            ensure_allowed_integer_keys(entries, &[1, 3, -1, -2], label)?;
            let public_key = as_bytes(
                map_get(entries, -2).ok_or_else(|| anyhow!("{label} is missing `-2`"))?,
                &format!("{label} public key"),
            )?
            .to_vec();
            let verifying_key = VerifyingKey::from_bytes(
                &<[u8; 32]>::try_from(public_key.as_slice())
                    .map_err(|_| anyhow!("{label} public key must be 32 bytes"))?,
            )
            .context("build Ed25519 verifying key")?;

            Ok(CosePublicKeyInfo {
                public_key_hex_bytes: public_key,
                algorithm: ALG_EDDSA,
                verifier: CoseVerifier::Ed25519(verifying_key),
            })
        }
        (2, ALG_ES256, P256_CURVE) => {
            ensure_allowed_integer_keys(entries, &[1, 3, -1, -2, -3], label)?;
            let x = as_bytes(
                map_get(entries, -2).ok_or_else(|| anyhow!("{label} is missing `-2`"))?,
                &format!("{label} x coordinate"),
            )?;
            let y = as_bytes(
                map_get(entries, -3).ok_or_else(|| anyhow!("{label} is missing `-3`"))?,
                &format!("{label} y coordinate"),
            )?;
            if x.len() != P256_COORD_LEN || y.len() != P256_COORD_LEN {
                return Err(anyhow!(
                    "{label} P-256 coordinates must each be {P256_COORD_LEN} bytes"
                ));
            }

            let mut sec1 = Vec::with_capacity(65);
            sec1.push(0x04);
            sec1.extend_from_slice(x);
            sec1.extend_from_slice(y);

            let verifying_key =
                P256VerifyingKey::from_sec1_bytes(&sec1).context("build ES256 verifying key")?;
            let mut public_key = x.to_vec();
            public_key.extend_from_slice(y);

            Ok(CosePublicKeyInfo {
                public_key_hex_bytes: public_key,
                algorithm: ALG_ES256,
                verifier: CoseVerifier::P256(verifying_key),
            })
        }
        (2, ALG_ES384, P384_CURVE) => {
            ensure_allowed_integer_keys(entries, &[1, 3, -1, -2, -3], label)?;
            let x = as_bytes(
                map_get(entries, -2).ok_or_else(|| anyhow!("{label} is missing `-2`"))?,
                &format!("{label} x coordinate"),
            )?;
            let y = as_bytes(
                map_get(entries, -3).ok_or_else(|| anyhow!("{label} is missing `-3`"))?,
                &format!("{label} y coordinate"),
            )?;
            if x.len() != P384_COORD_LEN || y.len() != P384_COORD_LEN {
                return Err(anyhow!(
                    "{label} P-384 coordinates must each be {P384_COORD_LEN} bytes"
                ));
            }

            let mut sec1 = Vec::with_capacity(97);
            sec1.push(0x04);
            sec1.extend_from_slice(x);
            sec1.extend_from_slice(y);

            let verifying_key =
                P384VerifyingKey::from_sec1_bytes(&sec1).context("build ES384 verifying key")?;
            let mut public_key = x.to_vec();
            public_key.extend_from_slice(y);

            Ok(CosePublicKeyInfo {
                public_key_hex_bytes: public_key,
                algorithm: ALG_ES384,
                verifier: CoseVerifier::P384(verifying_key),
            })
        }
        _ => Err(anyhow!(
            "unsupported {label} parameters: kty={key_type}, alg={algorithm}, crv={curve}"
        )),
    }
}

fn verify_signed_data_signature(
    key: &CosePublicKeyInfo,
    signature_bytes: &[u8],
    signature_input: &[u8],
) -> Result<bool> {
    Ok(match &key.verifier {
        CoseVerifier::Ed25519(verifying_key) => {
            let signature =
                Signature::from_slice(signature_bytes).context("parse Ed25519 signature")?;
            verifying_key.verify(signature_input, &signature).is_ok()
        }
        CoseVerifier::P256(verifying_key) => {
            let signature =
                P256Signature::from_slice(signature_bytes).context("parse ES256 signature")?;
            verifying_key.verify(signature_input, &signature).is_ok()
        }
        CoseVerifier::P384(verifying_key) => {
            let signature =
                P384Signature::from_slice(signature_bytes).context("parse ES384 signature")?;
            verifying_key.verify(signature_input, &signature).is_ok()
        }
    })
}

struct ParsedCoseSign1<'a> {
    protected: &'a [u8],
    payload: &'a [u8],
    signature: &'a [u8],
}

fn parse_cose_sign1<'a>(
    sign1: &'a [Value],
    label: &str,
    signer: Option<&CosePublicKeyInfo>,
) -> Result<ParsedCoseSign1<'a>> {
    if sign1.len() != 4 {
        return Err(anyhow!("{label} must contain 4 array entries"));
    }

    let protected = as_bytes(&sign1[0], "COSE_Sign1 protected headers")?;
    ensure_empty_map(&sign1[1], &format!("{label} unprotected headers"))?;
    let protected_algorithm = parse_protected_algorithm(protected, label)?;
    if let Some(signer) = signer
        && signer.algorithm != protected_algorithm
    {
        return Err(anyhow!(
            "{label} protected algorithm {protected_algorithm} does not match signer algorithm {}",
            signer.algorithm
        ));
    }
    let payload = as_bytes(&sign1[2], "COSE_Sign1 payload")?;
    let signature = as_bytes(&sign1[3], "COSE_Sign1 signature")?;
    if signature.is_empty() {
        return Err(anyhow!("{label} signature must not be empty"));
    }

    Ok(ParsedCoseSign1 {
        protected,
        payload,
        signature,
    })
}

fn validate_uds_certs(value: &Value) -> Result<()> {
    let entries = as_map(value, "UDS certs")?;
    let mut seen = HashSet::new();

    for (signer_name, chain) in entries {
        let signer_name = as_text(signer_name, "UDS cert signer")?;
        if signer_name.is_empty() {
            return Err(anyhow!("UDS cert signer name must not be empty"));
        }
        if !seen.insert(signer_name.to_owned()) {
            return Err(anyhow!("duplicate UDS cert signer `{signer_name}`"));
        }

        let label = format!("UDS cert chain `{signer_name}`");
        let certs = as_array(chain, &label)?;
        if certs.is_empty() {
            return Err(anyhow!(
                "UDS cert chain `{signer_name}` must contain at least one certificate"
            ));
        }

        for (index, cert) in certs.iter().enumerate() {
            let cert = as_bytes(cert, &format!("UDS cert `{signer_name}` entry {index}"))?;
            if cert.is_empty() {
                return Err(anyhow!(
                    "UDS cert `{signer_name}` entry {index} must not be empty"
                ));
            }
        }
    }

    Ok(())
}

fn parse_dice_entry_payload(value: &Value, label: &str) -> Result<CosePublicKeyInfo> {
    let payload_entries = as_map(value, &format!("{label} payload"))?;
    ensure_unique_integer_keys(payload_entries, &format!("{label} payload"))?;

    let issuer = required_int_text(payload_entries, CWT_ISSUER, label, "issuer")?;
    if issuer.is_empty() {
        return Err(anyhow!("{label} issuer must not be empty"));
    }

    let subject = required_int_text(payload_entries, CWT_SUBJECT, label, "subject")?;
    if subject.is_empty() {
        return Err(anyhow!("{label} subject must not be empty"));
    }

    let profile_name =
        required_int_text(payload_entries, DICE_PROFILE_NAME, label, "profile name")?;
    if profile_name != ANDROID_DICE_PROFILE_VERSION {
        return Err(anyhow!(
            "{label} profile name must be `{ANDROID_DICE_PROFILE_VERSION}`, got `{profile_name}`"
        ));
    }

    let key_usage = required_int_bytes(payload_entries, DICE_KEY_USAGE, label, "key usage")?;
    if key_usage.is_empty() {
        return Err(anyhow!("{label} key usage must not be empty"));
    }

    for (field, field_label) in [
        (-4_670_545, "code hash"),
        (-4_670_546, "code descriptor"),
        (-4_670_547, "configuration hash"),
        (-4_670_549, "authority hash"),
        (-4_670_550, "authority descriptor"),
        (-4_670_551, "mode"),
    ] {
        if let Some(value) = map_get(payload_entries, field) {
            let bytes = as_bytes(value, &format!("{label} {field_label}"))?;
            if bytes.is_empty() {
                return Err(anyhow!(
                    "{label} {field_label} must not be empty when present"
                ));
            }
        }
    }

    if let Some(configuration) = map_get(payload_entries, -4_670_548) {
        let configuration = decode(as_bytes(
            configuration,
            &format!("{label} configuration descriptor"),
        )?)?;
        let _ = as_map(&configuration, &format!("{label} configuration descriptor"))?;
    }

    let subject_key_bytes = required_int_bytes(
        payload_entries,
        DICE_SUBJECT_PUB_KEY,
        label,
        "subject public key",
    )?;
    let subject_key = decode(subject_key_bytes)?;
    let subject_entries = as_map(&subject_key, "DICE chain subject COSE key")?;
    parse_cose_public_key(subject_entries, "DICE chain subject COSE key")
}

fn validate_device_info(value: &Value) -> Result<String> {
    validate_canonical_order(value, "DeviceInfo")?;

    let entries = as_map(value, "DeviceInfo")?;
    ensure_unique_text_keys(entries, "DeviceInfo")?;

    const ALLOWED_KEYS: &[&str] = &[
        "brand",
        "manufacturer",
        "product",
        "model",
        "device",
        "vb_state",
        "bootloader_state",
        "vbmeta_digest",
        "os_version",
        "system_patch_level",
        "boot_patch_level",
        "vendor_patch_level",
        "security_level",
        "fused",
    ];

    for (key, _) in entries {
        let key = as_text(key, "DeviceInfo key")?;
        if !ALLOWED_KEYS.contains(&key) {
            return Err(anyhow!("DeviceInfo contains unrecognized key `{key}`"));
        }
    }

    let brand = required_text_field(entries, "brand")?.to_owned();
    let manufacturer = required_text_field(entries, "manufacturer")?;
    let product = required_text_field(entries, "product")?;
    let model = required_text_field(entries, "model")?;
    let device = required_text_field(entries, "device")?;
    let vb_state = required_text_field(entries, "vb_state")?;
    let bootloader_state = required_text_field(entries, "bootloader_state")?;
    let security_level = required_text_field(entries, "security_level")?;

    for (field, value) in [
        ("brand", brand.as_str()),
        ("manufacturer", manufacturer),
        ("product", product),
        ("model", model),
        ("device", device),
    ] {
        if value.is_empty() {
            return Err(anyhow!("DeviceInfo field `{field}` must not be empty"));
        }
    }

    validate_choice("vb_state", vb_state, &["green", "yellow", "orange"])?;
    validate_choice(
        "bootloader_state",
        bootloader_state,
        &["locked", "unlocked"],
    )?;
    validate_choice("security_level", security_level, &["tee", "strongbox"])?;

    let vbmeta_digest = required_bytes_field(entries, "vbmeta_digest")?;
    if vbmeta_digest.len() != 32 {
        return Err(anyhow!(
            "DeviceInfo field `vbmeta_digest` must be 32 bytes, got {}",
            vbmeta_digest.len()
        ));
    }

    let os_version = map_get_text(entries, "os_version")
        .map(|value| as_text(value, "os_version"))
        .transpose()?;
    if security_level == "tee" && os_version.is_none() {
        return Err(anyhow!("DeviceInfo field `os_version` is required for TEE"));
    }
    if let Some(os_version) = os_version
        && os_version.is_empty()
    {
        return Err(anyhow!("DeviceInfo field `os_version` must not be empty"));
    }

    let system_patch_level = required_uint_field(entries, "system_patch_level")?;
    validate_patch_level("system_patch_level", system_patch_level)?;
    let boot_patch_level = required_uint_field(entries, "boot_patch_level")?;
    validate_patch_level("boot_patch_level", boot_patch_level)?;
    let vendor_patch_level = required_uint_field(entries, "vendor_patch_level")?;
    validate_patch_level("vendor_patch_level", vendor_patch_level)?;

    let fused = required_uint_field(entries, "fused")?;
    if fused > 1 {
        return Err(anyhow!(
            "DeviceInfo field `fused` must be 0 or 1, got {fused}"
        ));
    }

    let expected_len = if security_level == "tee" {
        entries.len() == 14
    } else {
        matches!(entries.len(), 13 | 14)
    };
    if !expected_len {
        return Err(anyhow!(
            "DeviceInfo has an unexpected field count of {} for security_level `{security_level}`",
            entries.len()
        ));
    }

    Ok(brand)
}

fn validate_keys_to_sign(keys_to_sign: &[Value]) -> Result<()> {
    for (index, item) in keys_to_sign.iter().enumerate() {
        let label = format!("keysToSign entry {index}");
        let entries = as_map(item, &label)?;
        ensure_unique_integer_keys(entries, &label)?;
        ensure_allowed_integer_keys(entries, &[1, 3, -1, -2, -3, TEST_KEY_LABEL], &label)?;

        let key_type = as_i128(
            map_get(entries, 1).ok_or_else(|| anyhow!("{label} is missing `1`"))?,
            &format!("{label} key type"),
        )?;
        if key_type != 2 {
            return Err(anyhow!("{label} key type must be 2, got {key_type}"));
        }

        let algorithm = as_i128(
            map_get(entries, 3).ok_or_else(|| anyhow!("{label} is missing `3`"))?,
            &format!("{label} algorithm"),
        )?;
        if algorithm != ALG_ES256 {
            return Err(anyhow!(
                "{label} algorithm must be {ALG_ES256}, got {algorithm}"
            ));
        }

        let curve = as_i128(
            map_get(entries, -1).ok_or_else(|| anyhow!("{label} is missing `-1`"))?,
            &format!("{label} curve"),
        )?;
        if curve != P256_CURVE {
            return Err(anyhow!("{label} curve must be {P256_CURVE}, got {curve}"));
        }

        let x = as_bytes(
            map_get(entries, -2).ok_or_else(|| anyhow!("{label} is missing `-2`"))?,
            &format!("{label} x coordinate"),
        )?;
        if x.len() != P256_COORD_LEN {
            return Err(anyhow!(
                "{label} x coordinate must be {P256_COORD_LEN} bytes, got {}",
                x.len()
            ));
        }

        let y = as_bytes(
            map_get(entries, -3).ok_or_else(|| anyhow!("{label} is missing `-3`"))?,
            &format!("{label} y coordinate"),
        )?;
        if y.len() != P256_COORD_LEN {
            return Err(anyhow!(
                "{label} y coordinate must be {P256_COORD_LEN} bytes, got {}",
                y.len()
            ));
        }

        if let Some(test_key) = map_get(entries, TEST_KEY_LABEL)
            && !matches!(test_key, Value::Null)
        {
            return Err(anyhow!("{label} test-key marker `-70000` must be null"));
        }
    }

    Ok(())
}

fn ensure_array_len(items: &[Value], expected: usize, label: &str) -> Result<()> {
    if items.len() == expected {
        return Ok(());
    }

    Err(anyhow!(
        "{label} must contain {expected} entries, got {}",
        items.len()
    ))
}

fn ensure_empty_map(value: &Value, label: &str) -> Result<()> {
    let entries = as_map(value, label)?;
    if entries.is_empty() {
        return Ok(());
    }

    Err(anyhow!("{label} must be an empty map"))
}

fn parse_protected_algorithm(protected: &[u8], label: &str) -> Result<i128> {
    let protected_value = decode(protected)?;
    let headers = as_map(&protected_value, &format!("{label} protected headers"))?;
    ensure_unique_integer_keys(headers, &format!("{label} protected headers"))?;
    if headers.len() != 1 {
        return Err(anyhow!(
            "{label} protected headers must contain only the algorithm label"
        ));
    }

    let algorithm = as_i128(
        map_get(headers, 1).ok_or_else(|| anyhow!("{label} protected headers are missing `1`"))?,
        &format!("{label} algorithm"),
    )?;
    if matches!(algorithm, ALG_EDDSA | ALG_ES256 | ALG_ES384) {
        return Ok(algorithm);
    }

    Err(anyhow!(
        "{label} uses unsupported protected algorithm {algorithm}"
    ))
}

fn required_int_text<'a>(
    entries: &'a [(Value, Value)],
    key: i128,
    label: &str,
    field: &str,
) -> Result<&'a str> {
    as_text(
        map_get(entries, key).ok_or_else(|| anyhow!("{label} is missing {field}"))?,
        &format!("{label} {field}"),
    )
}

fn required_int_bytes<'a>(
    entries: &'a [(Value, Value)],
    key: i128,
    label: &str,
    field: &str,
) -> Result<&'a [u8]> {
    as_bytes(
        map_get(entries, key).ok_or_else(|| anyhow!("{label} is missing {field}"))?,
        &format!("{label} {field}"),
    )
}

fn required_text_field<'a>(entries: &'a [(Value, Value)], key: &str) -> Result<&'a str> {
    as_text(
        map_get_text(entries, key).ok_or_else(|| anyhow!("DeviceInfo field `{key}` is missing"))?,
        key,
    )
}

fn required_bytes_field<'a>(entries: &'a [(Value, Value)], key: &str) -> Result<&'a [u8]> {
    as_bytes(
        map_get_text(entries, key).ok_or_else(|| anyhow!("DeviceInfo field `{key}` is missing"))?,
        key,
    )
}

fn required_uint_field(entries: &[(Value, Value)], key: &str) -> Result<u32> {
    let value = as_i128(
        map_get_text(entries, key).ok_or_else(|| anyhow!("DeviceInfo field `{key}` is missing"))?,
        key,
    )?;
    if value < 0 {
        return Err(anyhow!(
            "DeviceInfo field `{key}` must be an unsigned integer"
        ));
    }

    u32::try_from(value).map_err(|_| anyhow!("DeviceInfo field `{key}` is out of range"))
}

fn validate_choice(field: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if allowed.iter().any(|candidate| *candidate == value) {
        return Ok(());
    }

    Err(anyhow!(
        "DeviceInfo field `{field}` must be one of {}, got `{value}`",
        allowed.join(", ")
    ))
}

fn validate_challenge(challenge: &[u8], label: &str) -> Result<()> {
    if challenge.len() <= MAX_CHALLENGE_LEN {
        return Ok(());
    }

    Err(anyhow!(
        "{label} must be at most {MAX_CHALLENGE_LEN} bytes, got {}",
        challenge.len()
    ))
}

fn validate_patch_level(field: &str, value: u32) -> Result<()> {
    let encoded = value.to_string();
    if value > 0 && matches!(encoded.len(), 6 | 8) && is_valid_patch_level_date(&encoded) {
        return Ok(());
    }

    Err(anyhow!(
        "DeviceInfo field `{field}` must be a valid patch level in YYYYMM or YYYYMMDD form"
    ))
}

fn is_valid_patch_level_date(value: &str) -> bool {
    let normalized = match value.len() {
        6 => format!("{value}01"),
        8 => value.to_owned(),
        _ => return false,
    };

    let year = match normalized[0..4].parse::<u32>() {
        Ok(year) => year,
        Err(_) => return false,
    };
    let month = match normalized[4..6].parse::<u32>() {
        Ok(month @ 1..=12) => month,
        _ => return false,
    };
    let day = match normalized[6..8].parse::<u32>() {
        Ok(day) => day,
        Err(_) => return false,
    };

    day >= 1 && day <= days_in_month(year, month)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn validate_canonical_order(value: &Value, label: &str) -> Result<()> {
    let original = encode_preserving_order(value)?;
    let canonical = encode(value)?;
    if original == canonical {
        return Ok(());
    }

    Err(anyhow!("{label} ordering is non-canonical"))
}

fn encode_preserving_order(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    into_writer(value, &mut bytes).context("encode CBOR without canonical reordering")?;
    Ok(bytes)
}

fn ensure_unique_integer_keys(entries: &[(Value, Value)], label: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for (key, _) in entries {
        let key = as_i128(key, &format!("{label} key"))?;
        if !seen.insert(key) {
            return Err(anyhow!("{label} contains duplicate key `{key}`"));
        }
    }
    Ok(())
}

fn ensure_unique_text_keys(entries: &[(Value, Value)], label: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for (key, _) in entries {
        let key = as_text(key, &format!("{label} key"))?;
        if !seen.insert(key.to_owned()) {
            return Err(anyhow!("{label} contains duplicate key `{key}`"));
        }
    }
    Ok(())
}

fn ensure_allowed_integer_keys(
    entries: &[(Value, Value)],
    allowed: &[i128],
    label: &str,
) -> Result<()> {
    for (key, _) in entries {
        let key = as_i128(key, &format!("{label} key"))?;
        if !allowed.contains(&key) {
            return Err(anyhow!("{label} contains unsupported key `{key}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::verify_csr;
    use crate::runtime::profile::DeviceInfo;

    use super::super::{
        cbor::{bytes, decode, encode, int},
        cose_dice::{DeviceKeys, build_csr, generate_ec_keypair},
    };
    use ciborium::ser::into_writer;
    use ciborium::value::Value;
    use p384::{SecretKey as P384SecretKey, elliptic_curve::sec1::ToEncodedPoint};

    fn valid_device_info() -> DeviceInfo {
        DeviceInfo {
            brand: "google".into(),
            model: "Pixel".into(),
            device: "pixel".into(),
            product: "pixel".into(),
            manufacturer: "Google".into(),
            fused: 1,
            vb_state: "green".into(),
            os_version: "13".into(),
            security_level: "tee".into(),
            bootloader_state: "locked".into(),
            boot_patch_level: 20260101,
            system_patch_level: 202601,
            vendor_patch_level: 20260101,
            vbmeta_digest: Some("11".repeat(32)),
            dice_issuer: "CN=Android".into(),
            dice_subject: "CN=Android".into(),
        }
    }

    #[test]
    fn verify_reports_valid_signature() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec_key = generate_ec_keypair().unwrap();
        let csr = build_csr(
            &keys,
            &[0x44; 32],
            &[ec_key.cose_public],
            &[0x55; 32],
            &[0x66; 8],
            &valid_device_info(),
            2,
        )
        .unwrap();

        let report = verify_csr(&csr.csr_bytes).unwrap();
        assert!(report.signature_valid);
        assert_eq!(report.csr_version, 3);
        assert_eq!(report.cert_type, "keymint");
    }

    #[test]
    fn verify_rejects_wrong_authenticated_request_version() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec_key = generate_ec_keypair().unwrap();
        let csr = build_csr(
            &keys,
            &[0x44; 32],
            &[ec_key.cose_public],
            &[0x55; 32],
            &[0x66; 8],
            &valid_device_info(),
            2,
        )
        .unwrap();

        let mut decoded = decode(&csr.csr_bytes).unwrap();
        let Value::Array(csr_items) = &mut decoded else {
            panic!("CSR must decode as an array");
        };
        csr_items[0] = int(9);

        let tampered = encode(&decoded).unwrap();
        let error = verify_csr(&tampered).unwrap_err();
        assert!(error.to_string().contains("CSR version"));
    }

    #[test]
    fn verify_rejects_oversized_signed_challenge() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec_key = generate_ec_keypair().unwrap();
        let csr = build_csr(
            &keys,
            &[0x44; 32],
            &[ec_key.cose_public],
            &[0x55; 32],
            &[0x66; 8],
            &valid_device_info(),
            2,
        )
        .unwrap();

        let mut decoded = decode(&csr.csr_bytes).unwrap();
        let Value::Array(csr_items) = &mut decoded else {
            panic!("CSR must decode as an array");
        };
        let Value::Array(signed_data) = &mut csr_items[3] else {
            panic!("signed data must decode as a COSE_Sign1 array");
        };
        let Value::Bytes(payload_bytes) = &mut signed_data[2] else {
            panic!("signed payload must be bytes");
        };
        let mut signed_payload = decode(payload_bytes).unwrap();
        let Value::Array(signed_items) = &mut signed_payload else {
            panic!("signed payload must decode as an array");
        };
        signed_items[0] = bytes(vec![0xAA; 65]);
        *payload_bytes = encode(&signed_payload).unwrap();

        let tampered = encode(&decoded).unwrap();
        let error = verify_csr(&tampered).unwrap_err();
        assert!(error.to_string().contains("challenge"));
    }

    #[test]
    fn verify_rejects_noncanonical_device_info_order() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec_key = generate_ec_keypair().unwrap();
        let csr = build_csr(
            &keys,
            &[0x44; 32],
            &[ec_key.cose_public],
            &[0x55; 32],
            &[0x66; 8],
            &valid_device_info(),
            2,
        )
        .unwrap();

        let mut decoded = decode(&csr.csr_bytes).unwrap();
        let Value::Array(csr_items) = &mut decoded else {
            panic!("CSR must decode as an array");
        };
        let Value::Array(signed_data) = &mut csr_items[3] else {
            panic!("signed data must decode as a COSE_Sign1 array");
        };
        let Value::Bytes(payload_bytes) = &mut signed_data[2] else {
            panic!("signed payload must be bytes");
        };
        let mut signed_payload = decode(payload_bytes).unwrap();
        let Value::Array(signed_items) = &mut signed_payload else {
            panic!("signed payload must decode as an array");
        };
        let Value::Bytes(csr_payload_bytes) = &mut signed_items[1] else {
            panic!("CSR payload must be bytes");
        };
        let mut csr_payload = decode(csr_payload_bytes).unwrap();
        let Value::Array(csr_payload_items) = &mut csr_payload else {
            panic!("CSR payload must decode as an array");
        };
        let Value::Map(device_info) = &mut csr_payload_items[2] else {
            panic!("device info must decode as a map");
        };
        device_info.reverse();

        *csr_payload_bytes = encode_preserving_order_for_test(&csr_payload);
        *payload_bytes = encode(&signed_payload).unwrap();

        let tampered = encode(&decoded).unwrap();
        let error = verify_csr(&tampered).unwrap_err();
        assert!(error.to_string().contains("non-canonical"));
    }

    #[test]
    fn verify_rejects_invalid_keys_to_sign_shape() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec_key = generate_ec_keypair().unwrap();
        let csr = build_csr(
            &keys,
            &[0x44; 32],
            &[ec_key.cose_public],
            &[0x55; 32],
            &[0x66; 8],
            &valid_device_info(),
            2,
        )
        .unwrap();

        let mut decoded = decode(&csr.csr_bytes).unwrap();
        let Value::Array(csr_items) = &mut decoded else {
            panic!("CSR must decode as an array");
        };
        let Value::Array(signed_data) = &mut csr_items[3] else {
            panic!("signed data must decode as a COSE_Sign1 array");
        };
        let Value::Bytes(payload_bytes) = &mut signed_data[2] else {
            panic!("signed payload must be bytes");
        };
        let mut signed_payload = decode(payload_bytes).unwrap();
        let Value::Array(signed_items) = &mut signed_payload else {
            panic!("signed payload must decode as an array");
        };
        let Value::Bytes(csr_payload_bytes) = &mut signed_items[1] else {
            panic!("CSR payload must be bytes");
        };
        let mut csr_payload = decode(csr_payload_bytes).unwrap();
        let Value::Array(csr_payload_items) = &mut csr_payload else {
            panic!("CSR payload must decode as an array");
        };
        let Value::Array(keys_to_sign) = &mut csr_payload_items[3] else {
            panic!("keysToSign must decode as an array");
        };
        let Value::Map(entries) = &mut keys_to_sign[0] else {
            panic!("keysToSign entry must decode as a map");
        };
        entries.retain(
            |(key, _)| !matches!(key, Value::Integer(number) if i128::try_from(*number).ok() == Some(-3)),
        );

        *csr_payload_bytes = encode(&csr_payload).unwrap();
        *payload_bytes = encode(&signed_payload).unwrap();

        let tampered = encode(&decoded).unwrap();
        let error = verify_csr(&tampered).unwrap_err();
        assert!(error.to_string().contains("keysToSign entry 0"));
    }

    #[test]
    fn verify_rejects_tampered_dice_chain_signature() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec_key = generate_ec_keypair().unwrap();
        let csr = build_csr(
            &keys,
            &[0x44; 32],
            &[ec_key.cose_public],
            &[0x55; 32],
            &[0x66; 8],
            &valid_device_info(),
            2,
        )
        .unwrap();

        let mut decoded = decode(&csr.csr_bytes).unwrap();
        let Value::Array(csr_items) = &mut decoded else {
            panic!("CSR must decode as an array");
        };
        let Value::Array(dice_chain) = &mut csr_items[2] else {
            panic!("DICE chain must decode as an array");
        };
        let Value::Array(dice_entry) = &mut dice_chain[1] else {
            panic!("DICE entry must decode as a COSE_Sign1 array");
        };
        dice_entry[3] = bytes(vec![0; 64]);

        let tampered = encode(&decoded).unwrap();
        let error = verify_csr(&tampered).unwrap_err();
        assert!(error.to_string().contains("DICE chain entry 1 signature"));
    }

    #[test]
    fn parse_cose_public_key_accepts_p384() {
        let secret_key = P384SecretKey::from_slice(&[0x11; 48]).unwrap();
        let public_key = secret_key.public_key();
        let encoded = public_key.to_encoded_point(false);
        let x = encoded.x().unwrap().to_vec();
        let y = encoded.y().unwrap().to_vec();

        let entries = vec![
            (int(1), int(2)),
            (int(3), int(super::ALG_ES384)),
            (int(-1), int(2)),
            (int(-2), bytes(x.clone())),
            (int(-3), bytes(y.clone())),
        ];

        let parsed = super::parse_cose_public_key(&entries, "P-384 key").unwrap();
        assert_eq!(parsed.public_key_hex_bytes.len(), x.len() + y.len());
    }

    fn encode_preserving_order_for_test(value: &Value) -> Vec<u8> {
        let mut bytes = Vec::new();
        into_writer(value, &mut bytes).unwrap();
        bytes
    }
}
