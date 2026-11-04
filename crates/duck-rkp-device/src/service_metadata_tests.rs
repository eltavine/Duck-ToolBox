use super::*;

pub(super) fn assert_fingerprint_metadata(
    data: &crate::service::ProvisionData,
    server: &FakeServer,
    include: bool,
    fingerprint: &str,
) {
    let submitted = server.submitted.lock().unwrap();
    assert_eq!(submitted.len(), 1);
    assert_eq!(fs::read(&data.artifacts.csr_path).unwrap(), submitted[0]);
    assert_eq!(data.include_fingerprint, include);
    assert_eq!(
        server.fingerprint.lock().unwrap().as_deref(),
        Some(fingerprint)
    );
    let value = duck_rkp::cbor::decode(&submitted[0]).unwrap();
    let fields = duck_rkp::cbor::as_array(&value, "CSR").unwrap();
    let metadata = match data.csr_protocol {
        crate::issuance::Protocol::V2 => {
            assert_eq!(fields.len(), if include { 5 } else { 4 });
            if !include {
                return;
            }
            &fields[4]
        }
        crate::issuance::Protocol::V1 => {
            assert_eq!(fields.len(), 4);
            &fields[0].as_array().unwrap()[1]
        }
        _ => unreachable!(),
    };
    assert_eq!(
        metadata,
        &duck_rkp::request::unverified_device_info(include.then_some(fingerprint)).unwrap()
    );
}

#[tokio::test]
async fn v2_submits_the_effective_fingerprint_only_when_enabled() {
    for include in [false, true] {
        let (_dir, ctx) = working_dir("rkp-v2-metadata");
        let ta = FakeTa::new();
        let server = seeded_server();
        let data = provision(
            &ctx,
            true,
            Options {
                protocol: crate::issuance::Protocol::V2,
                fingerprint: Some("  explicit/build  ".into()),
                include_fingerprint: include,
                output: None,
            },
            &FixedKeys,
            &ta,
            &server,
            &FakeEnvironment,
        )
        .await
        .unwrap();
        assert_fingerprint_metadata(&data, &server, include, "explicit/build");
        let report = data.local_verify.unwrap();
        assert_eq!(
            report.unverified_fingerprint.as_deref(),
            include.then_some("explicit/build")
        );
        let raw = ta.csr();
        let submitted = server.submitted.lock().unwrap();
        if include {
            assert_eq!(&submitted[0][1..raw.len()], &raw[1..]);
        } else {
            assert_eq!(submitted[0], raw);
        }
    }
}

#[tokio::test]
async fn impossible_metadata_size_is_rejected_before_network_or_ta_access() {
    for selected in [
        crate::issuance::Protocol::Auto,
        crate::issuance::Protocol::V1,
        crate::issuance::Protocol::V2,
        crate::issuance::Protocol::V1Native,
    ] {
        let (_dir, ctx) = working_dir("rkp-device-metadata-limit");
        let ta = FakeTa::new();
        let server = seeded_server();
        let error = provision(
            &ctx,
            true,
            Options {
                protocol: selected,
                include_fingerprint: true,
                fingerprint: Some("x".repeat(protocol::BUFFER_SIZE)),
                output: None,
            },
            &FixedKeys,
            &ta,
            &server,
            &FakeEnvironment,
        )
        .await
        .unwrap_err();
        assert_eq!(crate::error::code_of(&error), "invalid_device_csr");
        assert!(server.fingerprint.lock().unwrap().is_none());
        assert!(server.submitted.lock().unwrap().is_empty());
        assert!(ta.request.borrow().is_empty());
        assert_eq!(fs::read_dir(&ctx.paths.outputs_dir).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn disabled_metadata_does_not_restrict_the_challenge_fingerprint_to_the_csr_buffer() {
    let (_dir, ctx) = working_dir("rkp-device-no-metadata-limit");
    let ta = FakeTa::new();
    let server = seeded_server();
    let data = provision(
        &ctx,
        true,
        Options {
            protocol: crate::issuance::Protocol::V2,
            include_fingerprint: false,
            fingerprint: Some("x".repeat(protocol::BUFFER_SIZE)),
            output: None,
        },
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();
    assert!(!data.include_fingerprint);
    assert!(data.local_verify.unwrap().unverified_fingerprint.is_none());
    assert_eq!(server.submitted.lock().unwrap().len(), 1);
}
