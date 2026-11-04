use super::*;
use crate::{
    cbor::{bytes, empty_map, int},
    cose::{DeviceKeys, build_experimental_csr_v3, generate_ec_keypair},
    profile::{DeviceInfo, DiceCurve},
    verify::verify_csr,
    workflow::validate_request_binding,
};

const FINGERPRINT: &str = "google/husky/husky:15/AP3A/1:user/release-keys";

#[test]
fn requests_from_the_pinned_upstream_implementation_round_trip_exactly() {
    // Upstream build_csr receives DeviceInfo in the canonical order required by
    // the existing verifier. No upstream signing or serialization code is changed.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/upstream-fingerprint.json")).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let encoded = hex::decode(vector["csr_hex"].as_str().unwrap()).unwrap();
        let fingerprint = vector["fingerprint"].as_str().unwrap();
        let report = verify_csr(&encoded).unwrap();
        assert!(report.signature_valid);
        assert_eq!(report.unverified_fingerprint.as_deref(), Some(fingerprint));
        let items = array_items_raw(&encoded).unwrap();
        let mut bare = vec![0x84];
        for item in &items[..4] {
            bare.extend_from_slice(item);
        }
        assert!(verify_csr(&bare).unwrap().unverified_fingerprint.is_none());
        assert_eq!(with_fingerprint(&bare, Some(fingerprint)).unwrap(), encoded);
    }
}

#[test]
fn metadata_matches_the_upstream_outer_fifth_field_for_both_signing_curves() {
    for curve in [DiceCurve::Ed25519, DiceCurve::P256] {
        let keys = DeviceKeys::from_seed_with_curve([7; 32], curve);
        let public = [generate_ec_keypair().unwrap().cose_public];
        let raw =
            build_experimental_csr_v3(&keys, &[8; 32], &public, &DeviceInfo::default()).unwrap();
        assert_eq!(with_fingerprint(&raw, None).unwrap(), raw);
        let sent = with_fingerprint(&raw, Some(FINGERPRINT)).unwrap();
        let original_items = array_items_raw(&raw).unwrap();
        let sent_items = array_items_raw(&sent).unwrap();
        assert_eq!(&sent_items[..4], original_items.as_slice());
        let parsed = decode(&sent).unwrap();
        let fields = as_array(&parsed, "CSR").unwrap();
        assert_eq!(fields.len(), 5);
        assert_eq!(
            fields[4],
            Value::Map(vec![(text("fingerprint"), text(FINGERPRINT))])
        );
        let report = validate_request_binding(&sent, &[8; 32], &public).unwrap();
        assert!(report.signature_valid);
        assert_eq!(report.unverified_fingerprint.as_deref(), Some(FINGERPRINT));
        assert!(validate_request_binding(&sent, &[9; 32], &public).is_err());
        assert!(
            validate_request_binding(
                &sent,
                &[8; 32],
                &[generate_ec_keypair().unwrap().cose_public]
            )
            .is_err()
        );

        // Changing host metadata cannot establish or invalidate the signed identity.
        let mut changed = fields.to_vec();
        changed[4] = unverified_device_info(Some("another/build")).unwrap();
        let report = verify_csr(&encode(&Value::Array(changed)).unwrap()).unwrap();
        assert!(report.signature_valid);
        assert_eq!(
            report.unverified_fingerprint.as_deref(),
            Some("another/build")
        );
    }
}

#[test]
fn wrapping_preserves_noncanonical_and_indefinite_original_items() {
    // Non-minimal integers and unsorted maps are deliberately preserved.
    let items = [0x18, 1, 0xa2, 2, 0, 1, 0, 0x9f, 0x18, 0, 0xff, 0x40];
    for header in [
        vec![0x84],
        vec![0x98, 4],
        vec![0x99, 0, 4],
        vec![0x9a, 0, 0, 0, 4],
        vec![0x9b, 0, 0, 0, 0, 0, 0, 0, 4],
        vec![0x9f],
    ] {
        let mut raw = header.clone();
        raw.extend_from_slice(&items);
        if header == [0x9f] {
            raw.push(0xff);
        }
        assert_eq!(with_fingerprint(&raw, None).unwrap(), raw);
        let sent = with_fingerprint(&raw, Some(FINGERPRINT)).unwrap();
        assert_eq!(&sent[1..1 + items.len()], &items);
        assert_eq!(array_items_raw(&sent).unwrap().len(), 5);
    }
}

#[test]
fn signed_upstream_requests_preserve_alternate_container_encodings() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/upstream-fingerprint.json")).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let original = hex::decode(vector["csr_hex"].as_str().unwrap()).unwrap();
        let parts = array_items_raw(&original).unwrap();
        assert_eq!(parts[1], [0xa0]);
        assert_eq!(parts[2][0], 0x82);
        assert_eq!(parts[3][0], 0x84);
        for indefinite_dice in [false, true] {
            for indefinite_signed in [false, true] {
                for uds in [vec![0xa0], vec![0xb8, 0]] {
                    let mut contents = parts[0].to_vec();
                    contents.extend_from_slice(&uds);
                    contents.push(if indefinite_dice { 0x9f } else { 0x98 });
                    if !indefinite_dice {
                        contents.push(2);
                    }
                    contents.extend_from_slice(&parts[2][1..]);
                    if indefinite_dice {
                        contents.push(0xff);
                    }
                    contents.push(if indefinite_signed { 0x9f } else { 0x98 });
                    if !indefinite_signed {
                        contents.push(4);
                    }
                    contents.extend_from_slice(&parts[3][1..]);
                    if indefinite_signed {
                        contents.push(0xff);
                    }
                    for header in [vec![0x84], vec![0x98, 4], vec![0x9f]] {
                        let mut raw = header.clone();
                        raw.extend_from_slice(&contents);
                        if header == [0x9f] {
                            raw.push(0xff);
                        }
                        assert!(verify_csr(&raw).unwrap().signature_valid);
                        assert_eq!(with_fingerprint(&raw, None).unwrap(), raw);
                        let sent = with_fingerprint(&raw, Some(FINGERPRINT)).unwrap();
                        assert_eq!(&sent[1..1 + contents.len()], contents.as_slice());
                        assert_eq!(
                            verify_csr(&sent).unwrap().unverified_fingerprint.as_deref(),
                            Some(FINGERPRINT)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_or_already_wrapped_requests_are_rejected() {
    for input in [
        vec![],
        vec![0x84, 1],
        vec![0x84, 1, 0xa0, 0x80, 0x80, 0],
        vec![0x83, 1, 0xa0, 0x80],
        vec![0x84, 2, 0xa0, 0x80, 0x80],
        vec![0x85, 1, 0xa0, 0x80, 0x80, 0xa0],
    ] {
        assert!(with_fingerprint(&input, Some(FINGERPRINT)).is_err());
    }
    let raw = [0x84, 1, 0xa0, 0x80, 0x80];
    assert!(with_fingerprint(&raw, Some("  ")).is_err());
    let oversized = "x".repeat(MAX_INPUT_BYTES);
    assert!(with_fingerprint(&raw, Some(&oversized)).is_err());
}

#[test]
fn utf8_fingerprint_respects_the_exact_encoded_request_size_limit() {
    let keys = DeviceKeys::from_seed([7; 32]);
    let public = [generate_ec_keypair().unwrap().cose_public];
    let raw = build_experimental_csr_v3(&keys, &[8; 32], &public, &DeviceInfo::default()).unwrap();
    // One map header, the encoded "fingerprint" key, and a five-byte text header.
    let fingerprint_bytes = MAX_INPUT_BYTES - raw.len() - 18;
    assert!(fingerprint_bytes > 65535);
    let mut fingerprint = "界".repeat(fingerprint_bytes / 3);
    fingerprint.push_str(&"x".repeat(fingerprint_bytes % 3));
    let submitted = with_fingerprint(&raw, Some(&fingerprint)).unwrap();
    assert_eq!(submitted.len(), MAX_INPUT_BYTES);
    let report = validate_request_binding(&submitted, &[8; 32], &public).unwrap();
    assert_eq!(
        report.unverified_fingerprint.as_deref(),
        Some(fingerprint.as_str())
    );
    fingerprint.push('x');
    assert!(with_fingerprint(&raw, Some(&fingerprint)).is_err());
    assert_eq!(with_fingerprint(&raw, None).unwrap(), raw);
}

#[test]
fn optional_metadata_does_not_relax_request_or_signature_validation() {
    let keys = DeviceKeys::from_seed([7; 32]);
    let public = [generate_ec_keypair().unwrap().cose_public];
    let raw = build_experimental_csr_v3(&keys, &[8; 32], &public, &DeviceInfo::default()).unwrap();
    let mut value = as_array(&decode(&raw).unwrap(), "CSR").unwrap().to_vec();
    for metadata in [
        int(1),
        bytes(vec![]),
        Value::Map(vec![(text("fingerprint"), int(1))]),
        Value::Map(vec![(text("fingerprint"), text(" "))]),
        Value::Map(vec![(text("other"), text("value"))]),
        Value::Map(vec![
            (text("fingerprint"), text("a")),
            (text("fingerprint"), text("b")),
        ]),
    ] {
        value.truncate(4);
        value.push(metadata);
        assert!(verify_csr(&encode(&Value::Array(value.clone())).unwrap()).is_err());
    }
    value.truncate(4);
    value.push(empty_map());
    assert!(
        verify_csr(&encode(&Value::Array(value.clone())).unwrap())
            .unwrap()
            .unverified_fingerprint
            .is_none()
    );
    value.push(empty_map());
    assert!(verify_csr(&encode(&Value::Array(value)).unwrap()).is_err());
    let sent = with_fingerprint(&raw, Some(FINGERPRINT)).unwrap();
    let mut value = as_array(&decode(&sent).unwrap(), "CSR").unwrap().to_vec();
    let Value::Array(signed) = &mut value[3] else {
        unreachable!()
    };
    let Value::Bytes(signature) = &mut signed[3] else {
        unreachable!()
    };
    signature[0] ^= 1;
    assert!(verify_csr(&encode(&Value::Array(value)).unwrap()).is_err());
}
