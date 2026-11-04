use super::*;
use crate::v1::{
    assemble,
    tests::{public_key, response as evidence},
};
use duck_rkp::cbor::{bytes, decode};

fn eek() -> Vec<u8> {
    // One COSE_Sign1 certificate; signature/root acceptance belongs to the device.
    vec![0x81, 0x84, 0x40, 0xa0, 0x40, 0x40]
}

#[test]
fn native_request_binds_the_public_key_and_exact_production_fields() {
    let request = build_request(&[3; 32], &public_key(), &eek(), &[7; 32]).unwrap();
    // Independent HMAC-SHA256 vector over the RFC 9052 MAC0 structure.
    assert_eq!(
        hex::encode(request.mac_tag),
        "45cae113e3ef7c803fade8ba9931427f6a22b97eebf053bc7c5dce4952592205"
    );
    let payload = protocol::ta_payload(&request.bytes).unwrap();
    assert_eq!(&request.bytes[..4], COMMAND.to_le_bytes());
    let value = decode(payload).unwrap();
    let fields = as_map(&value, "request").unwrap();
    assert_eq!(fields.len(), 5);
    assert_eq!(required(fields, 55).unwrap(), &int(0));
    assert_eq!(required(fields, 56).unwrap(), &bytes(vec![7; 32]));
    assert_eq!(
        required(fields, 57).unwrap(),
        &bytes(request.mac_tag.to_vec())
    );
    assert_eq!(required(fields, 58).unwrap(), &bytes(eek()));
    let other = build_request(&[3; 32], &public_key(), &eek(), &[8; 32]).unwrap();
    assert_ne!(request.mac_tag, other.mac_tag);
}

#[test]
fn request_boundary_rejects_wrong_mode_lengths_unknown_fields_and_malformed_eek() {
    let request = build_request(&[3; 32], &public_key(), &eek(), &[7; 32]).unwrap();
    let value = decode(&request.bytes[4..]).unwrap();
    let fields = as_map(&value, "request").unwrap();
    for (label, invalid) in [
        (55, int(1)),
        (55, Value::Bool(false)),
        (4, bytes(vec![])),
        (4, bytes(vec![0; 65])),
        (56, bytes(vec![0; 31])),
        (57, bytes(vec![0; 33])),
        (58, bytes(vec![])),
        (58, bytes(vec![0x80])),
        (58, bytes(vec![0x81, 0xa0])),
        (58, bytes(vec![0x80, 0])),
    ] {
        let mut changed = fields.to_vec();
        changed
            .iter_mut()
            .find(|(key, _)| key == &int(label))
            .unwrap()
            .1 = invalid;
        assert!(validate_request(&encode(&Value::Map(changed)).unwrap()).is_err());
    }
    let mut extra = fields.to_vec();
    extra.push((int(99), int(0)));
    assert!(validate_request(&encode(&Value::Map(extra)).unwrap()).is_err());
    let mut trailing = request.bytes[4..].to_vec();
    trailing.push(0);
    assert!(validate_request(&trailing).is_err());
    assert!(validate_request(&vec![0; MAX_REQUEST_SIZE]).is_err());
    assert!(build_request(&[3; 32], &Value::Map(vec![]), &eek(), &[7; 32]).is_err());
}

#[test]
fn native_reply_reuses_the_aidl_csr_layout_and_preserves_exact_evidence() {
    let request = build_request(&[3; 32], &public_key(), &eek(), &[7; 32]).unwrap();
    let evidence = evidence();
    let payload = encode(&Value::Map(vec![
        (int(59), bytes(evidence.device_info.clone())),
        (int(60), bytes(evidence.protected_data.clone())),
    ]))
    .unwrap();
    let parsed = response(&payload, &request.mac_tag).unwrap();
    assert_eq!(parsed.device_info, evidence.device_info);
    assert_eq!(parsed.protected_data, evidence.protected_data);
    let csr = assemble(parsed, &[3; 32], &public_key(), None).unwrap();
    let Value::Array(fields) = decode(&csr).unwrap() else {
        panic!("CSR array")
    };
    let device = as_array(&fields[0], "[verified, unverified]").unwrap();
    assert_eq!(device.len(), 2);
    assert_eq!(device[1], Value::Map(vec![]));
    let maced = as_array(&fields[3], "MacedKeysToSign").unwrap();
    assert_eq!(maced[3], bytes(request.mac_tag.to_vec()));
    assert_eq!(
        decode(as_bytes(&maced[2], "keys").unwrap()).unwrap(),
        Value::Array(vec![public_key()])
    );
}

#[test]
fn malformed_native_replies_are_never_accepted_as_csr_evidence() {
    for payload in [
        vec![],
        vec![0xa0],
        vec![0xa0, 0],
        vec![0; BUFFER_SIZE + 1],
        encode(&Value::Map(vec![
            (int(59), bytes(vec![])),
            (int(61), bytes(vec![])),
        ]))
        .unwrap(),
    ] {
        assert!(response(&payload, &[7; 32]).is_err());
    }
    let payload = encode(&Value::Map(vec![
        (int(59), bytes(vec![0xa0])),
        (int(60), bytes(vec![0x80])),
    ]))
    .unwrap();
    let parsed = response(&payload, &[7; 32]).unwrap();
    assert!(assemble(parsed, &[3; 32], &public_key(), None).is_err());
}
