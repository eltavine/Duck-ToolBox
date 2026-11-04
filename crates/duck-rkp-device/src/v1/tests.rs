use super::*;
use duck_core::Sysroot;
use std::io::{Cursor, Read};

pub(crate) fn public_key() -> Value {
    Value::Map(vec![
        (int(1), int(2)),
        (int(3), int(-7)),
        (int(-1), int(1)),
        (
            int(-2),
            bytes(
                hex::decode("6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296")
                    .unwrap(),
            ),
        ),
        (
            int(-3),
            bytes(
                hex::decode("4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5")
                    .unwrap(),
            ),
        ),
    ])
}

pub(crate) fn response() -> Response {
    Response {
        mac: vec![7; 32],
        device_info: encode(&Value::Map(vec![(Value::Text("version".into()), int(2))])).unwrap(),
        protected_data: encode(&Value::Array(vec![
            bytes(encode(&Value::Map(vec![(int(1), int(3))])).unwrap()),
            Value::Map(vec![(int(5), bytes(vec![1; 12]))]),
            bytes(vec![2; 32]),
            Value::Array(vec![Value::Array(vec![
                bytes(encode(&Value::Map(vec![(int(1), int(-25))])).unwrap()),
                Value::Map(vec![]),
                Value::Null,
            ])]),
        ]))
        .unwrap(),
    }
}

#[test]
fn mac_matches_an_independent_hkdf_hmac_vector() {
    let request = request(&[3; 32], &public_key(), &[]).unwrap();
    let maced = decode_exact(&request.maced_key).unwrap();
    let fields = as_array(&maced, "MacedPublicKey").unwrap();
    assert_eq!(
        hex::encode(as_bytes(&fields[3], "MAC").unwrap()),
        "983c51a5b56d85510943322f6cd3542b7f41505b75025a31b5a975c354cc8e4d"
    );
    assert_eq!(
        decode_exact(as_bytes(&fields[2], "key").unwrap()).unwrap(),
        public_key()
    );
}

#[test]
fn legacy_csr_preserves_server_evidence_challenge_and_requested_key() {
    let csr = assemble(response(), &[3; 32], &public_key(), None).unwrap();
    let csr = decode_exact(&csr).unwrap();
    let fields = as_array(&csr, "CSR").unwrap();
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[1], bytes(vec![3; 32]));
    let maced = as_array(&fields[3], "MacedKeys").unwrap();
    assert_eq!(
        decode_exact(as_bytes(&maced[2], "keys").unwrap()).unwrap(),
        Value::Array(vec![public_key()])
    );
    assert_eq!(maced[3], bytes(vec![7; 32]));
    // The V2 verifier must not be presented as verifying an encrypted legacy CSR.
    assert!(duck_rkp::verify::verify_csr(&encode(&csr).unwrap()).is_err());
}

#[test]
fn legacy_csr_keeps_the_exact_hal_encodings() {
    let mut result = response();
    let device_info = Value::Map(vec![
        (Value::Text("version".into()), int(2)),
        (Value::Text("brand".into()), Value::Text("qti".into())),
    ]);
    result.device_info.clear();
    ciborium::ser::into_writer(&device_info, &mut result.device_info).unwrap();
    let mut protected = decode_exact(&result.protected_data).unwrap();
    let Value::Array(fields) = &mut protected else {
        unreachable!()
    };
    // A legal unprotected header in a different map order must remain opaque too.
    let Value::Map(headers) = &mut fields[1] else {
        unreachable!()
    };
    headers.insert(0, (int(6), bytes(vec![9])));
    result.protected_data.clear();
    ciborium::ser::into_writer(&protected, &mut result.protected_data).unwrap();
    let original_device = result.device_info.clone();
    let original_protected = result.protected_data.clone();
    let challenge = [3; 32];
    for fingerprint in [None, Some("source/device/build")] {
        let result = Response {
            mac: result.mac.clone(),
            device_info: result.device_info.clone(),
            protected_data: result.protected_data.clone(),
        };
        let csr = assemble(result, &challenge, &public_key(), fingerprint).unwrap();
        let device_start = 2; // CSR array and [verified, unverified] DeviceInfo array.
        assert_eq!(
            &csr[device_start..device_start + original_device.len()],
            original_device
        );
        let protected_start = device_start
            + original_device.len()
            + encode(&duck_rkp::request::unverified_device_info(fingerprint).unwrap())
                .unwrap()
                .len()
            + encode(&bytes(challenge.to_vec())).unwrap().len();
        assert_eq!(
            &csr[protected_start..protected_start + original_protected.len()],
            original_protected
        );
        assert!(decode_exact(&csr).is_ok());
    }
}

#[test]
fn invalid_legacy_evidence_is_rejected_before_submission() {
    let mut variants = Vec::new();
    for mac in [vec![], vec![0; 31], vec![0; 33]] {
        variants.push(Response { mac, ..response() });
    }
    for device_info in [vec![0xa0], vec![0x80], vec![0xa0, 0]] {
        variants.push(Response {
            device_info,
            ..response()
        });
    }
    for protected_data in [vec![], vec![0x80], vec![0x84, 0x40, 0xa0, 0x40, 0x80]] {
        variants.push(Response {
            protected_data,
            ..response()
        });
    }
    for value in variants {
        assert!(assemble(value, &[3; 32], &public_key(), None).is_err());
    }
    assert!(request(&[], &public_key(), &[]).is_err());
    assert!(request(&[0; 65], &public_key(), &[]).is_err());
    assert!(request(&[0; 32], &Value::Map(vec![]), &[]).is_err());
}

#[test]
fn malformed_recipient_structures_are_rejected() {
    for recipient in [
        int(1),
        Value::Array(vec![bytes(vec![]), Value::Map(vec![])]),
        Value::Array(vec![bytes(vec![]), Value::Map(vec![]), bytes(vec![])]),
        Value::Array(vec![bytes(vec![0xa0, 0]), Value::Map(vec![]), Value::Null]),
    ] {
        let mut value = response();
        let mut encrypted = decode_exact(&value.protected_data).unwrap();
        let Value::Array(fields) = &mut encrypted else {
            unreachable!()
        };
        fields[3] = Value::Array(vec![recipient]);
        value.protected_data = encode(&encrypted).unwrap();
        assert!(assemble(value, &[3; 32], &public_key(), None).is_err());
    }
}

fn append(output: &mut Vec<u8>, field: &[u8]) {
    output.extend_from_slice(&(field.len() as u32).to_be_bytes());
    output.extend_from_slice(field);
}

#[test]
fn bridge_framing_is_bounded_exact_and_preserves_aidl_errors() {
    let response = response();
    let mut encoded = 0u32.to_be_bytes().to_vec();
    for field in [
        &response.mac,
        &response.device_info,
        &response.protected_data,
    ] {
        append(&mut encoded, field);
    }
    assert_eq!(wire::decode_response(&encoded).unwrap().mac, response.mac);
    for cut in 0..encoded.len() {
        assert!(wire::decode_response(&encoded[..cut]).is_err());
    }
    encoded.push(0);
    assert!(wire::decode_response(&encoded).is_err());
    let mut oversized = vec![0; 4];
    oversized.extend_from_slice(&((BUFFER_SIZE + 1) as u32).to_be_bytes());
    assert!(wire::decode_response(&oversized).is_err());
    for code in [2i32, 6, -40] {
        let mut error = 1u32.to_be_bytes().to_vec();
        error.extend_from_slice(&code.to_be_bytes());
        append(&mut error, b"unsupported firmware");
        let error = wire::decode_response(&error).unwrap_err();
        assert_eq!(crate::error::code_of(&error), "ta_rejected");
        assert!(error.chain().any(
            |cause| matches!(cause.downcast_ref::<crate::error::DeviceRkpError>(),
            Some(crate::error::DeviceRkpError::Rejected(value)) if *value == code)
        ));
    }
}

#[test]
fn bridge_keeps_transport_and_invalid_response_errors_distinct() {
    for (status, expected_code) in [(2u32, "ta_transport_error"), (3, "invalid_ta_response")] {
        let mut reply = status.to_be_bytes().to_vec();
        append(&mut reply, b"invalid DeviceInfo length");
        let error = wire::decode_response(&reply).unwrap_err();
        assert_eq!(crate::error::code_of(&error), expected_code);
        assert!(error.to_string().contains("invalid DeviceInfo length"));
        reply.push(0);
        assert!(wire::decode_response(&reply).is_err());
    }
}

#[test]
fn bridge_receives_both_eek_curves_and_no_private_key() {
    let mut request = request(&[3; 32], &public_key(), &[]).unwrap();
    request.eek_chains = vec![
        EekChain {
            curve: 2,
            encoded: vec![2],
        },
        EekChain {
            curve: 1,
            encoded: vec![1],
        },
    ];
    let output = wire::encode_request(&request).unwrap();
    let mut input = Cursor::new(output.as_slice());
    for expected in [&request.challenge, &request.maced_key, &vec![1], &vec![2]] {
        let mut size = [0; 4];
        input.read_exact(&mut size).unwrap();
        let mut actual = vec![0; u32::from_be_bytes(size) as usize];
        input.read_exact(&mut actual).unwrap();
        assert_eq!(&actual, expected);
    }
    assert_eq!(input.position(), output.len() as u64);
}

#[test]
fn discovers_only_system_aidl_apks() {
    let root = tempfile::tempdir().unwrap();
    let sysroot = Sysroot::new(root.path());
    assert!(find_aidl_apk(&sysroot).is_none());
    let apk = root
        .path()
        .join("apex/com.android.rkpd/priv-app/rkpdapp@123/rkpdapp.apk");
    std::fs::create_dir_all(apk.parent().unwrap()).unwrap();
    std::fs::write(&apk, []).unwrap();
    assert_eq!(find_aidl_apk(&sysroot), Some(apk));
}
