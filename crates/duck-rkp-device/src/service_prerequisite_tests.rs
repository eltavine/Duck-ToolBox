use super::*;

#[tokio::test]
async fn provision_refuses_without_the_acknowledgement() {
    let (_dir, ctx) = working_dir("rkp-device-noack");
    let ta = FakeTa::new();
    let server = seeded_server();

    let error = provision(
        &ctx,
        false,
        Options::default(),
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap_err();

    assert_eq!(crate::error::code_of(&error), "acknowledgement_required");
    assert!(
        ta.request.borrow().is_empty(),
        "KeyMint must not be contacted"
    );
    assert!(server.submitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn provision_reports_missing_prerequisites() {
    let (_dir, ctx) = working_dir("rkp-device-blocked");
    let ta = FakeTa::new();
    let server = seeded_server();

    let error = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &ta,
        &server,
        &BlockedEnvironment(vec!["root_required", "qualcomm_soc_required"]),
    )
    .await
    .unwrap_err();

    assert_eq!(crate::error::code_of(&error), "unsupported_device");
    assert!(error.to_string().contains("root_required"));
    assert!(ta.request.borrow().is_empty());
}

#[tokio::test]
async fn provision_honors_an_explicit_fingerprint() {
    let (_dir, ctx) = working_dir("rkp-device-fingerprint");
    let ta = FakeTa::new();
    let server = seeded_server();

    let _ = provision(
        &ctx,
        true,
        Options {
            protocol: crate::issuance::Protocol::Auto,
            fingerprint: Some("custom/fingerprint".into()),
            include_fingerprint: false,
            output: None,
        },
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await;

    assert_eq!(
        server.fingerprint.lock().unwrap().as_deref(),
        Some("custom/fingerprint")
    );
}

#[tokio::test]
async fn the_detected_device_fingerprint_is_the_default() {
    let (_dir, ctx) = working_dir("rkp-device-detected-fingerprint");
    let server = seeded_server();
    let data = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &FakeTa::new(),
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();
    assert_eq!(data.fingerprint, FINGERPRINT);
    assert_eq!(
        server.fingerprint.lock().unwrap().as_deref(),
        Some(FINGERPRINT)
    );
}

#[tokio::test]
async fn an_explicit_fingerprint_satisfies_only_the_fingerprint_prerequisite() {
    let (_dir, ctx) = working_dir("rkp-device-missing-fingerprint");
    let server = seeded_server();
    for blockers in [
        vec!["fingerprint_missing"],
        vec!["fingerprint_missing", "root_required"],
    ] {
        let expected_to_pass = blockers.len() == 1;
        let result = provision(
            &ctx,
            true,
            Options {
                protocol: crate::issuance::Protocol::Auto,
                fingerprint: Some("  custom/fingerprint  ".into()),
                include_fingerprint: false,
                output: None,
            },
            &FixedKeys,
            &FakeTa::new(),
            &server,
            &BlockedEnvironment(blockers),
        )
        .await;
        if expected_to_pass {
            let data = result.unwrap();
            assert_eq!(data.fingerprint, "custom/fingerprint");
            assert!(data.prerequisites.prerequisites_met);
            assert!(data.prerequisites.blockers.is_empty());
        } else {
            let error = result.unwrap_err();
            assert_eq!(crate::error::code_of(&error), "unsupported_device");
            assert!(error.to_string().contains("root_required"));
        }
    }
}

#[test]
fn production_environment_agrees_with_its_own_blockers() {
    let (_dir, ctx) = working_dir("rkp-device-env");
    let status = DeviceEnvironment.capabilities(&ctx);
    assert_eq!(status.prerequisites_met, status.blockers.is_empty());
}
