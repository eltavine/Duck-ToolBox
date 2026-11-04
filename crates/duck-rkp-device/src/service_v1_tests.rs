use super::*;
use crate::{error::DeviceRkpError, issuance::Protocol, v1};

struct LegacyTa {
    v2_error: i32,
    calls: RefCell<Vec<&'static str>>,
    malformed: bool,
}

impl LegacyTa {
    fn new(error: i32) -> Self {
        Self {
            v2_error: error,
            calls: RefCell::new(vec![]),
            malformed: false,
        }
    }
}

#[tokio::test]
async fn aidl_and_v2_fallback_share_the_v1_fingerprint_policy_on_both_vendors() {
    for include in [false, true] {
        for environment in [&FakeEnvironment as &dyn Environment, &MediaTekEnvironment] {
            let (_dir, ctx) = working_dir("rkp-aidl-metadata");
            let server = seeded_server();
            let ta = LegacyTa::new(-40);
            let data = provision(
                &ctx,
                true,
                Options {
                    include_fingerprint: include,
                    ..Options::default()
                },
                &FixedKeys,
                &ta,
                &server,
                environment,
            )
            .await
            .unwrap();
            assert_fingerprint_metadata(&data, &server, include, FINGERPRINT);
            assert!(data.local_verify.is_none());
        }
    }
}

impl Exchange for LegacyTa {
    fn csr(&self, request: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.calls
            .borrow_mut()
            .push(if request[..4] == v1::native::COMMAND.to_le_bytes() {
                "v1-native"
            } else {
                "v2"
            });
        Err(anyhow::Error::new(DeviceRkpError::Rejected(self.v2_error)).context("vendor reply"))
    }
    fn csr_v1(&self, request: &v1::Request) -> anyhow::Result<v1::Response> {
        self.calls.borrow_mut().push("v1");
        assert_eq!(request.challenge, CHALLENGE);
        let key = duck_rkp::cbor::decode(&request.maced_key).unwrap();
        let fields = duck_rkp::cbor::as_array(&key, "MacedPublicKey").unwrap();
        let payload = duck_rkp::cbor::as_bytes(&fields[2], "key").unwrap();
        assert_eq!(
            duck_rkp::cbor::decode(payload).unwrap(),
            cose_public_of(&test_key())
        );
        let mut response = v1::tests::response();
        if self.malformed {
            response.mac.clear();
        }
        Ok(response)
    }
}

struct Availability {
    v1: bool,
    v2: bool,
}

struct MediaTekEnvironment;

impl Environment for MediaTekEnvironment {
    fn capabilities(&self, ctx: &Context) -> DeviceStatus {
        let mut status = FakeEnvironment.capabilities(ctx);
        status.qualcomm_soc = false;
        status.mediatek_soc = true;
        status.soc_manufacturer = "MediaTek".into();
        status.soc_detection = crate::soc::detect(
            &[("ro.soc.manufacturer".into(), "MediaTek".into())].into(),
            &ctx.sysroot,
        );
        status.v2_available = false;
        status.v1_native_available = false;
        status
    }
}

#[tokio::test]
async fn mediatek_never_calls_the_qualcomm_worker_or_falls_back_to_it() {
    for malformed in [false, true] {
        let (_dir, ctx) = working_dir("rkp-mediatek");
        let mut ta = LegacyTa::new(-40);
        ta.malformed = malformed;
        let server = seeded_server();
        let result = provision(
            &ctx,
            true,
            Options::default(),
            &FixedKeys,
            &ta,
            &server,
            &MediaTekEnvironment,
        )
        .await;
        assert_eq!(result.is_ok(), !malformed);
        assert_eq!(*ta.calls.borrow(), ["v1"]);
    }
    for protocol in [Protocol::V2, Protocol::V1Native] {
        let (_dir, ctx) = working_dir("rkp-mediatek-native");
        let ta = LegacyTa::new(-40);
        let server = seeded_server();
        let error = provision(
            &ctx,
            true,
            Options {
                protocol,
                ..Options::default()
            },
            &FixedKeys,
            &ta,
            &server,
            &MediaTekEnvironment,
        )
        .await
        .unwrap_err();
        assert_eq!(crate::error::code_of(&error), "unsupported_device");
        assert!(ta.calls.borrow().is_empty());
        assert!(server.fingerprint.lock().unwrap().is_none());
    }
}
impl Environment for Availability {
    fn capabilities(&self, ctx: &Context) -> DeviceStatus {
        let mut status = FakeEnvironment.capabilities(ctx);
        status.v1_available = self.v1;
        status.v2_available = self.v2;
        status.v1_native_available = self.v2;
        status
    }
}

#[tokio::test]
async fn legacy_round_trip_exports_a_keybox_without_claiming_local_signature_verification() {
    for (protocol, v2, expected_calls) in [
        (Protocol::Auto, true, vec!["v2", "v1"]),
        (Protocol::Auto, false, vec!["v1"]),
        (Protocol::V1, true, vec!["v1"]),
    ] {
        let (_dir, ctx) = working_dir("rkp-v1-round-trip");
        let ta = LegacyTa::new(-40);
        let server = seeded_server();
        let data = provision(
            &ctx,
            true,
            Options {
                protocol,
                ..Options::default()
            },
            &FixedKeys,
            &ta,
            &server,
            &Availability { v1: true, v2 },
        )
        .await
        .unwrap();
        assert_eq!(*ta.calls.borrow(), expected_calls);
        assert_eq!(data.csr_protocol, Protocol::V1);
        assert!(data.local_verify.is_none());
        assert_eq!(server.submitted.lock().unwrap().len(), 1);
        let json = serde_json::to_value(&data).unwrap();
        assert!(json["local_verify"].is_null());
        assert_eq!(json["csr_protocol"], "v1");
        let summary = duck_tricky_store::keybox::validate::validate(&data.keybox_xml).unwrap();
        assert_eq!(summary.chain_lengths, vec![2]);
        assert!(
            fs::read_to_string(&data.artifacts.private_key_path)
                .unwrap()
                .starts_with("-----BEGIN EC PRIVATE KEY-----")
        );
    }
}

#[tokio::test]
async fn explicit_v2_failure_does_not_switch_to_another_method() {
    let (_dir, ctx) = working_dir("rkp-v2-explicit");
    let ta = LegacyTa::new(-40);
    let server = seeded_server();
    let error = provision(
        &ctx,
        true,
        Options {
            protocol: Protocol::V2,
            ..Options::default()
        },
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap_err();
    assert_eq!(crate::error::code_of(&error), "ta_rejected");
    assert_eq!(*ta.calls.borrow(), ["v2"]);
    assert!(server.submitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn explicit_protocol_gates_run_before_the_network_or_device_is_contacted() {
    for protocol in [Protocol::V1, Protocol::V2, Protocol::V1Native] {
        let (_dir, ctx) = working_dir("rkp-v1-unavailable");
        let ta = LegacyTa::new(-40);
        let server = seeded_server();
        let result = provision(
            &ctx,
            true,
            Options {
                protocol,
                ..Options::default()
            },
            &FixedKeys,
            &ta,
            &server,
            &Availability {
                v1: protocol != Protocol::V1,
                v2: !matches!(protocol, Protocol::V2 | Protocol::V1Native),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(crate::error::code_of(&result), "unsupported_device");
        assert!(ta.calls.borrow().is_empty());
        assert!(server.fingerprint.lock().unwrap().is_none());
    }
}

#[tokio::test]
async fn explicit_protocols_preserve_shared_and_unknown_prerequisites() {
    for protocol in [Protocol::V1, Protocol::V2, Protocol::V1Native] {
        let (_dir, ctx) = working_dir("rkp-shared-prerequisites");
        let ta = LegacyTa::new(-40);
        let server = seeded_server();
        let failure = provision(
            &ctx,
            true,
            Options {
                protocol,
                ..Options::default()
            },
            &FixedKeys,
            &ta,
            &server,
            &BlockedEnvironment(vec![
                "root_required",
                "soc_vendor_unknown",
                "future_prerequisite",
            ]),
        )
        .await
        .unwrap_err();
        assert_eq!(crate::error::code_of(&failure), "unsupported_device");
        for blocker in ["root_required", "soc_vendor_unknown", "future_prerequisite"] {
            assert!(failure.to_string().contains(blocker));
        }
        assert!(ta.calls.borrow().is_empty());
        assert!(server.fingerprint.lock().unwrap().is_none());
    }
}

#[tokio::test]
async fn invalid_legacy_evidence_is_never_submitted_and_writes_no_artifacts() {
    let (_dir, ctx) = working_dir("rkp-v1-invalid");
    let mut ta = LegacyTa::new(-40);
    ta.malformed = true;
    let server = seeded_server();
    let failure = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap_err();
    assert_eq!(crate::error::code_of(&failure), "all_methods_failed");
    assert!(failure.to_string().contains("v1 [invalid_device_csr]"));
    assert_eq!(*ta.calls.borrow(), ["v2", "v1", "v1-native"]);
    assert!(server.submitted.lock().unwrap().is_empty());
    assert_eq!(fs::read_dir(&ctx.paths.outputs_dir).unwrap().count(), 0);
}
