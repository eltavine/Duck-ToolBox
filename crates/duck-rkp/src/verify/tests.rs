use ciborium::{ser::into_writer, value::Value};
use p384::{SecretKey as P384SecretKey, elliptic_curve::sec1::ToSec1Point};

use super::{ALG_ES384, cose_key::parse_cose_public_key, verify_csr};
use crate::{
    cbor::{as_bytes, bytes, decode, encode, int, text},
    cose::{DeviceKeys, RPC_CURVE_25519, build_csr, build_sig_structure, generate_ec_keypair},
    profile::{DeviceInfo, DiceCurve},
};

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

fn csr_bytes(keys: &DeviceKeys, device: &DeviceInfo) -> Vec<u8> {
    let ec_key = generate_ec_keypair().unwrap();
    build_csr(
        keys,
        &[0x44; 32],
        &[ec_key.cose_public],
        &[0x55; 32],
        &[0x66; 8],
        device,
        RPC_CURVE_25519,
    )
    .unwrap()
    .csr_bytes
}

fn valid_csr() -> Vec<u8> {
    csr_bytes(&DeviceKeys::from_seed([0x11; 32]), &valid_device_info())
}

fn csr_with_dice_fields(fields: &[Vec<(Value, Value)>]) -> Vec<u8> {
    let keys = DeviceKeys::from_seed([0x11; 32]);
    let mut csr = decode(&csr_bytes(&keys, &valid_device_info())).unwrap();
    let chain = array(&mut array(&mut csr)[2]);
    let template = array(&mut chain[1]).clone();
    let Value::Map(payload) = decode(as_bytes(&template[2], "DICE payload").unwrap()).unwrap()
    else {
        panic!("DICE payload must be a map");
    };
    chain.truncate(1);

    for fields in fields {
        let mut entry = template.clone();
        let mut payload = payload.clone();
        payload.extend_from_slice(fields);
        let payload = encode(&Value::Map(payload)).unwrap();
        let signature_input = build_sig_structure(
            "Signature1",
            as_bytes(&entry[0], "protected header").unwrap(),
            &payload,
        )
        .unwrap();
        entry[2] = bytes(payload);
        entry[3] = bytes(keys.sign(&signature_input));
        chain.push(Value::Array(entry));
    }

    encode(&csr).unwrap()
}

fn dice_fields(profile: Option<&str>, mode: Value) -> Vec<(Value, Value)> {
    let mut fields = vec![(int(-4_670_551), mode)];
    if let Some(profile) = profile {
        fields.push((int(-4_670_554), text(profile)));
    }
    fields
}

fn android_16_fields(configuration: Vec<(Value, Value)>) -> Vec<(Value, Value)> {
    let mut fields = dice_fields(Some("android.16"), bytes(vec![1]));
    fields.push((
        int(-4_670_548),
        bytes(encode(&Value::Map(configuration)).unwrap()),
    ));
    fields
}

fn array(value: &mut Value) -> &mut Vec<Value> {
    match value {
        Value::Array(items) => items,
        other => panic!("expected CBOR array, got {other:?}"),
    }
}

fn byte_string(value: &mut Value) -> &mut Vec<u8> {
    match value {
        Value::Bytes(bytes) => bytes,
        other => panic!("expected CBOR bytes, got {other:?}"),
    }
}

fn sign_entry(entry: &mut [Value], keys: &DeviceKeys) {
    let input = build_sig_structure(
        "Signature1",
        as_bytes(&entry[0], "header").unwrap(),
        as_bytes(&entry[2], "payload").unwrap(),
    )
    .unwrap();
    entry[3] = bytes(keys.sign(&input));
}

fn encode_preserving_order(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    into_writer(value, &mut bytes).unwrap();
    bytes
}

/// Decodes the signed payload inside the CSR, lets `edit` change it, and re-encodes the CSR.
fn tamper_signed_payload(csr: &[u8], edit: impl FnOnce(&mut Vec<Value>)) -> Vec<u8> {
    let mut decoded = decode(csr).unwrap();
    let signed_data = array(&mut array(&mut decoded)[3]);
    let payload_bytes = byte_string(&mut signed_data[2]);
    let mut signed_payload = decode(payload_bytes).unwrap();
    edit(array(&mut signed_payload));
    *payload_bytes = encode(&signed_payload).unwrap();
    encode(&decoded).unwrap()
}

/// Like [`tamper_signed_payload`] but edits the inner CSR payload array.
fn tamper_csr_payload(csr: &[u8], edit: impl FnOnce(&mut Vec<Value>), canonical: bool) -> Vec<u8> {
    tamper_signed_payload(csr, |signed_items| {
        let csr_payload_bytes = byte_string(&mut signed_items[1]);
        let mut csr_payload = decode(csr_payload_bytes).unwrap();
        edit(array(&mut csr_payload));
        *csr_payload_bytes = if canonical {
            encode(&csr_payload).unwrap()
        } else {
            encode_preserving_order(&csr_payload)
        };
    })
}

#[test]
fn verify_reports_valid_signature() {
    let report = verify_csr(&valid_csr()).unwrap();
    assert!(report.signature_valid);
    assert_eq!(report.csr_version, 3);
    assert_eq!(report.cert_type, "keymint");
}

#[test]
fn verify_rejects_signed_payloads_and_headers_with_trailing_cbor() {
    let keys = DeviceKeys::from_seed([0x11; 32]);
    for (index, nested) in [(0, false), (2, false), (2, true)] {
        let mut decoded = decode(&valid_csr()).unwrap();
        let signed_data = array(&mut array(&mut decoded)[3]);
        if nested {
            let mut payload = decode(byte_string(&mut signed_data[2])).unwrap();
            byte_string(&mut array(&mut payload)[1]).push(0);
            signed_data[2] = bytes(encode(&payload).unwrap());
        } else {
            byte_string(&mut signed_data[index]).push(0);
        }
        sign_entry(signed_data, &keys);
        assert!(verify_csr(&encode(&decoded).unwrap()).is_err());
    }
}

#[test]
fn verify_rejects_trailing_cbor_inside_signed_dice_entries() {
    let keys = DeviceKeys::from_seed([0x11; 32]);
    for (index, nested) in [(0, false), (2, false), (2, true)] {
        let mut decoded = decode(&valid_csr()).unwrap();
        let chain = array(&mut array(&mut decoded)[2]);
        let entry = array(&mut chain[1]);
        if nested {
            let mut payload = decode(byte_string(&mut entry[2])).unwrap();
            let Value::Map(fields) = &mut payload else {
                panic!("DICE payload must be a map");
            };
            let (_, subject) = fields
                .iter_mut()
                .find(|(key, _)| *key == int(-4_670_552))
                .unwrap();
            byte_string(subject).push(0);
            entry[2] = bytes(encode(&payload).unwrap());
        } else {
            byte_string(&mut entry[index]).push(0);
        }
        sign_entry(entry, &keys);
        assert!(verify_csr(&encode(&decoded).unwrap()).is_err());
    }

    let mut configuration = encode(&Value::Map(vec![(int(-70_005), int(7))])).unwrap();
    configuration.push(0);
    let csr = csr_with_dice_fields(&[vec![
        (int(-4_670_554), text("android.16")),
        (int(-4_670_548), bytes(configuration)),
    ]]);
    assert!(verify_csr(&csr).is_err());
}

#[test]
fn verify_rejects_wrong_authenticated_request_version() {
    let mut decoded = decode(&valid_csr()).unwrap();
    array(&mut decoded)[0] = int(9);

    let error = verify_csr(&encode(&decoded).unwrap()).unwrap_err();
    assert!(error.to_string().contains("CSR version"));
}

#[test]
fn verify_rejects_oversized_signed_challenge() {
    let tampered = tamper_signed_payload(&valid_csr(), |items| {
        items[0] = bytes(vec![0xAA; 65]);
    });

    let error = verify_csr(&tampered).unwrap_err();
    assert!(error.to_string().contains("challenge"));
}

#[test]
fn verify_rejects_noncanonical_device_info_order() {
    let tampered = tamper_csr_payload(
        &valid_csr(),
        |items| {
            let Value::Map(device_info) = &mut items[2] else {
                panic!("device info must decode as a map");
            };
            device_info.reverse();
        },
        false,
    );

    let error = verify_csr(&tampered).unwrap_err();
    assert!(error.to_string().contains("non-canonical"));
}

#[test]
fn verify_allows_missing_vbmeta_digest() {
    let mut device = valid_device_info();
    device.vbmeta_digest = None;

    let report = verify_csr(&csr_bytes(&DeviceKeys::from_seed([0x11; 32]), &device)).unwrap();
    assert!(report.signature_valid);
}

#[test]
fn verify_accepts_p256_dice_chain() {
    let keys = DeviceKeys::from_seed_with_curve([0x11; 32], DiceCurve::P256);
    let report = verify_csr(&csr_bytes(&keys, &valid_device_info())).unwrap();
    assert!(report.signature_valid);
}

#[test]
fn verify_rejects_invalid_keys_to_sign_shape() {
    let tampered = tamper_csr_payload(
        &valid_csr(),
        |items| {
            let Value::Map(entries) = &mut array(&mut items[3])[0] else {
                panic!("keysToSign entry must decode as a map");
            };
            entries.retain(
                |(key, _)| !matches!(key, Value::Integer(number) if i128::from(*number) == -3),
            );
        },
        true,
    );

    let error = verify_csr(&tampered).unwrap_err();
    assert!(error.to_string().contains("keysToSign entry 0"));
}

#[test]
fn verify_rejects_tampered_dice_chain_signature() {
    let mut decoded = decode(&valid_csr()).unwrap();
    let dice_chain = array(&mut array(&mut decoded)[2]);
    array(&mut dice_chain[1])[3] = bytes(vec![0; 64]);

    let error = verify_csr(&encode(&decoded).unwrap()).unwrap_err();
    assert!(error.to_string().contains("DICE chain entry 1 signature"));
}

#[test]
fn verify_accepts_profile_specific_dice_mode_encodings() {
    for profile in [
        None,
        Some("android.14"),
        Some("android.15"),
        Some("android.16"),
    ] {
        let report = verify_csr(&csr_with_dice_fields(&[dice_fields(
            profile,
            bytes(vec![1]),
        )]))
        .unwrap();
        assert!(report.signature_valid, "profile: {profile:?}");
    }
    for profile in [None, Some("android.14")] {
        let report = verify_csr(&csr_with_dice_fields(&[dice_fields(profile, int(1))])).unwrap();
        assert!(report.signature_valid, "profile: {profile:?}");
    }
}

#[test]
fn verify_rejects_integer_modes_in_newer_dice_profiles() {
    for profile in ["android.15", "android.16"] {
        let error =
            verify_csr(&csr_with_dice_fields(&[dice_fields(Some(profile), int(1))])).unwrap_err();
        assert!(
            error.to_string().contains("DICE chain entry 1 mode"),
            "{error}"
        );
    }
}

#[test]
fn verify_rejects_invalid_dice_modes_and_profiles() {
    for mode in [
        bytes(vec![]),
        bytes(vec![1, 2]),
        text("normal"),
        Value::Null,
    ] {
        let error = verify_csr(&csr_with_dice_fields(&[dice_fields(None, mode)])).unwrap_err();
        assert!(
            error.to_string().contains("DICE chain entry 1 mode"),
            "{error}"
        );
    }
    for profile in [text("android.99"), int(14)] {
        let error =
            verify_csr(&csr_with_dice_fields(&[vec![(int(-4_670_554), profile)]])).unwrap_err();
        assert!(error.to_string().contains("profile name"), "{error}");
    }
}

#[test]
fn verify_accepts_dice_entries_without_a_mode_field() {
    // `dice_fields` always sets `mode`; an entry that omits the field entirely must still
    // verify, for the strict profile as well as the defaulted one.
    for entry in [vec![], vec![(int(-4_670_554), text("android.15"))]] {
        let report = verify_csr(&csr_with_dice_fields(&[entry])).unwrap();
        assert!(report.signature_valid);
    }
}

#[test]
fn verify_rejects_android_16_configuration_without_unsigned_security_version() {
    for configuration in [
        vec![],
        vec![(int(-70_005), int(-1))],
        vec![(int(-70_005), text("1"))],
    ] {
        let error =
            verify_csr(&csr_with_dice_fields(&[android_16_fields(configuration)])).unwrap_err();
        assert!(error.to_string().contains("security version"), "{error}");
    }
}

#[test]
fn verify_accepts_android_16_unsigned_security_version() {
    for version in [0, 1, i128::from(u64::MAX)] {
        let fields = android_16_fields(vec![(int(-70_005), int(version))]);
        assert!(
            verify_csr(&csr_with_dice_fields(&[fields]))
                .unwrap()
                .signature_valid
        );
    }
}

#[test]
fn verify_rejects_duplicate_android_16_security_versions() {
    let fields = android_16_fields(vec![(int(-70_005), int(1)), (int(-70_005), int(2))]);
    let error = verify_csr(&csr_with_dice_fields(&[fields])).unwrap_err();
    assert!(
        error.to_string().contains("duplicate key `-70005`"),
        "{error}"
    );
}

#[test]
fn verify_preserves_older_dice_configuration_compatibility() {
    for profile in [None, Some("android.14"), Some("android.15")] {
        let mut fields = dice_fields(profile, bytes(vec![1]));
        fields.push((int(-4_670_548), bytes(encode(&Value::Map(vec![])).unwrap())));
        assert!(
            verify_csr(&csr_with_dice_fields(&[fields]))
                .unwrap()
                .signature_valid
        );
    }
}

#[test]
fn verify_rejects_dice_profile_downgrades() {
    for (first, second) in [
        (Some("android.15"), Some("android.14")),
        (Some("android.15"), None),
        (Some("android.16"), Some("android.15")),
        (Some("android.16"), Some("android.14")),
        (Some("android.16"), None),
    ] {
        let first_fields = if first == Some("android.16") {
            android_16_fields(vec![(int(-70_005), int(1))])
        } else {
            dice_fields(first, bytes(vec![1]))
        };
        let csr = csr_with_dice_fields(&[first_fields, dice_fields(second, bytes(vec![1]))]);
        let error = verify_csr(&csr).unwrap_err();
        assert!(
            error.to_string().contains("DICE chain entry 2 profile"),
            "{error}"
        );
    }
}

#[test]
fn verify_accepts_dice_profile_upgrades_and_equal_versions() {
    let csr = csr_with_dice_fields(&[
        dice_fields(None, int(1)),
        dice_fields(Some("android.14"), int(1)),
        dice_fields(Some("android.15"), bytes(vec![1])),
        dice_fields(Some("android.15"), bytes(vec![1])),
        android_16_fields(vec![(int(-70_005), int(1))]),
        android_16_fields(vec![(int(-70_005), int(1))]),
    ]);
    let report = verify_csr(&csr).unwrap();
    assert!(report.signature_valid);
    assert_eq!(report.dice_entries, 7);
}

#[test]
fn parse_cose_public_key_accepts_p384() {
    let secret_key = P384SecretKey::from_slice(&[0x11; 48]).unwrap();
    let encoded = secret_key.public_key().to_sec1_point(false);
    let x = encoded.x().unwrap().to_vec();
    let y = encoded.y().unwrap().to_vec();

    let entries = vec![
        (int(1), int(2)),
        (int(3), int(ALG_ES384)),
        (int(-1), int(2)),
        (int(-2), bytes(x.clone())),
        (int(-3), bytes(y.clone())),
    ];

    let parsed = parse_cose_public_key(&entries, "P-384 key").unwrap();
    assert_eq!(parsed.public_key_bytes.len(), x.len() + y.len());
}
