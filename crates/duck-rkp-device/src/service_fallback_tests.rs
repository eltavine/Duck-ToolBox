use super::*;
use crate::{
    error::DeviceRkpError,
    issuance::{Backend, Method, Protocol},
    v1,
};
use duck_rkp::{
    cbor::{as_bytes, as_map, map_get},
    error::RkpError,
};

#[derive(Clone, Copy)]
enum ResultKind {
    Good,
    Reject,
    Timeout,
    Transport,
    Malformed,
    WrongKey,
}

struct Methods {
    outcomes: [ResultKind; 3],
    calls: RefCell<Vec<Method>>,
    v2: FakeTa,
    outputs: PathBuf,
}

impl Methods {
    fn new(ctx: &Context, outcomes: [ResultKind; 3]) -> Self {
        let mut v2 = FakeTa::new();
        if matches!(outcomes[0], ResultKind::WrongKey) {
            v2.substitute = Some(cose_public_of(
                &P256SecretKey::from_slice(&[0x77; 32]).unwrap(),
            ));
        }
        Self {
            outcomes,
            calls: RefCell::new(vec![]),
            v2,
            outputs: ctx.paths.outputs_dir.clone(),
        }
    }

    fn begin(&self, method: Method, kind: ResultKind) -> anyhow::Result<()> {
        assert_eq!(
            fs::read_dir(&self.outputs).unwrap().count(),
            0,
            "failed attempts must not leave files"
        );
        self.calls.borrow_mut().push(method);
        match kind {
            ResultKind::Reject => Err(DeviceRkpError::Rejected(-18).into()),
            ResultKind::Timeout => Err(DeviceRkpError::Timeout.into()),
            ResultKind::Transport => {
                Err(DeviceRkpError::Transport("adapter unavailable".into()).into())
            }
            _ => Ok(()),
        }
    }
}

impl Exchange for Methods {
    fn csr(&self, request: &[u8]) -> anyhow::Result<Vec<u8>> {
        if request[..4] == protocol::CSR_COMMAND.to_le_bytes() {
            self.begin(Method::V2, self.outcomes[0])?;
            if matches!(self.outcomes[0], ResultKind::Malformed) {
                return Ok(vec![0xa0, 0]);
            }
            return Exchange::csr(&self.v2, request);
        }
        self.begin(Method::V1Native, self.outcomes[2])?;
        assert_eq!(request[..4], v1::native::COMMAND.to_le_bytes());
        if matches!(self.outcomes[2], ResultKind::Malformed) {
            return Ok(vec![0xa0, 0]);
        }
        let value = duck_rkp::cbor::decode(protocol::ta_payload(request).unwrap()).unwrap();
        let fields = as_map(&value, "native request").unwrap();
        let key: [u8; 32] = as_bytes(map_get(fields, 56).unwrap(), "MAC key")
            .unwrap()
            .try_into()
            .unwrap();
        let tag = duck_rkp::cose::key_mac_tag(&key, &[cose_public_of(&test_key())]).unwrap();
        assert_eq!(map_get(fields, 57), Some(&bytes(tag.to_vec())));
        let response = v1::tests::response();
        encode(&Value::Map(vec![
            (int(59), bytes(response.device_info)),
            (int(60), bytes(response.protected_data)),
        ]))
    }

    fn csr_v1(&self, _request: &v1::Request) -> anyhow::Result<v1::Response> {
        self.begin(Method::V1, self.outcomes[1])?;
        let mut response = v1::tests::response();
        if matches!(self.outcomes[1], ResultKind::Malformed) {
            response.mac.clear();
        }
        Ok(response)
    }
}

#[derive(Clone, Copy)]
enum ServerResult {
    Good,
    Reject,
    InvalidChain,
    WrongKey,
    ChallengeFailure,
    EmptyChallenge,
}

struct FreshServer {
    inner: FakeServer,
    challenges: Mutex<Vec<Vec<u8>>>,
    stages: Vec<ServerResult>,
    fingerprints: Mutex<Vec<String>>,
    submissions: Mutex<Vec<Vec<u8>>>,
}

impl FreshServer {
    fn new(stages: Vec<ServerResult>) -> Self {
        Self {
            inner: seeded_server(),
            challenges: Mutex::new(vec![]),
            stages,
            fingerprints: Mutex::new(vec![]),
            submissions: Mutex::new(vec![]),
        }
    }
}

impl Server for FreshServer {
    fn challenge<'a>(&'a self, fingerprint: &'a str) -> BoxFuture<'a, anyhow::Result<EekResponse>> {
        Box::pin(async move {
            self.fingerprints
                .lock()
                .unwrap()
                .push(fingerprint.to_owned());
            let challenge = {
                let mut challenges = self.challenges.lock().unwrap();
                let challenge = vec![(challenges.len() + 1) as u8; 32];
                challenges.push(challenge.clone());
                challenge
            };
            if matches!(
                self.stages.get(challenge[0] as usize - 1),
                Some(ServerResult::ChallengeFailure)
            ) {
                return Err(RkpError::RkpServer("challenge unavailable".into()).into());
            }
            let mut eek = self.inner.challenge(fingerprint).await?;
            eek.challenge_hex = hex::encode(&challenge);
            eek.challenge = challenge;
            if matches!(
                self.stages.get(eek.challenge[0] as usize - 1),
                Some(ServerResult::EmptyChallenge)
            ) {
                eek.challenge.clear();
                eek.challenge_hex.clear();
            }
            Ok(eek)
        })
    }

    fn certificates<'a>(
        &'a self,
        csr: &'a [u8],
        challenge: &'a [u8],
    ) -> BoxFuture<'a, anyhow::Result<Vec<Vec<u8>>>> {
        Box::pin(async move {
            self.submissions.lock().unwrap().push(csr.to_vec());
            let index = {
                let challenges = self.challenges.lock().unwrap();
                assert_eq!(challenge, challenges.last().unwrap());
                challenges.len() - 1
            };
            match self
                .stages
                .get(index)
                .copied()
                .unwrap_or(ServerResult::Good)
            {
                ServerResult::Reject => {
                    return Err(RkpError::RkpClient("CSR rejected".into()).into());
                }
                ServerResult::InvalidChain => return Ok(vec![vec![1, 2]]),
                ServerResult::WrongKey => {
                    let key = P256SecretKey::from_slice(&[0x77; 32]).unwrap();
                    let signer = TestSigner {
                        subject: requested_point(&cose_public_of(&key)),
                        key: p256::ecdsa::SigningKey::from(key),
                    };
                    return Ok(vec![issued_chain(&signer)]);
                }
                _ => {}
            }
            self.inner.certificates(csr, &self.inner.challenge).await
        })
    }
}

#[tokio::test]
async fn server_rejections_do_not_change_fingerprint_policy_during_protocol_fallback() {
    for include in [false, true] {
        let (_dir, ctx) = working_dir("rkp-fallback-fingerprint");
        let methods = Methods::new(&ctx, [ResultKind::Good; 3]);
        let server = FreshServer::new(vec![
            ServerResult::Reject,
            ServerResult::Reject,
            ServerResult::Good,
        ]);
        let fingerprint = "explicit/source/build";
        let data = provision(
            &ctx,
            true,
            Options {
                include_fingerprint: include,
                fingerprint: Some(fingerprint.into()),
                ..Options::default()
            },
            &FixedKeys,
            &methods,
            &server,
            &FakeEnvironment,
        )
        .await
        .unwrap();
        assert_eq!(
            *methods.calls.borrow(),
            [Method::V2, Method::V1, Method::V1Native]
        );
        assert_eq!(
            *server.fingerprints.lock().unwrap(),
            vec![fingerprint.to_owned(); 3]
        );
        let submissions = server.submissions.lock().unwrap();
        assert_eq!(submissions.len(), 3);
        for (index, csr) in submissions.iter().enumerate() {
            let value = duck_rkp::cbor::decode(csr).unwrap();
            let fields = value.as_array().unwrap();
            let expected =
                duck_rkp::request::unverified_device_info(include.then_some(fingerprint)).unwrap();
            if index == 0 {
                let report =
                    protocol::validate_csr(csr, &[1; 32], &cose_public_of(&test_key())).unwrap();
                assert_eq!(
                    report.unverified_fingerprint.as_deref(),
                    include.then_some(fingerprint)
                );
                assert_eq!(fields.len(), if include { 5 } else { 4 });
                if include {
                    assert_eq!(fields[4], expected);
                }
            } else {
                assert_eq!(fields[0].as_array().unwrap()[1], expected);
                assert_eq!(fields[1], bytes(vec![(index + 1) as u8; 32]));
            }
        }
        assert_eq!(fs::read(&data.artifacts.csr_path).unwrap(), submissions[2]);
        assert_eq!(data.include_fingerprint, include);
    }
}

#[tokio::test]
async fn automatic_mode_retries_every_kind_of_v2_failure_and_stops_after_aidl_success() {
    for (failure, code) in [
        (ResultKind::Reject, "ta_rejected"),
        (ResultKind::Timeout, "ta_timeout"),
        (ResultKind::Transport, "ta_transport_error"),
        (ResultKind::Malformed, "invalid_ta_response"),
        (ResultKind::WrongKey, "invalid_device_csr"),
    ] {
        let (_dir, ctx) = working_dir("rkp-auto-second");
        let methods = Methods::new(&ctx, [failure, ResultKind::Good, ResultKind::Reject]);
        let server = FreshServer::new(vec![]);
        let data = provision(
            &ctx,
            true,
            Options::default(),
            &FixedKeys,
            &methods,
            &server,
            &FakeEnvironment,
        )
        .await
        .unwrap();
        assert_eq!(*methods.calls.borrow(), [Method::V2, Method::V1]);
        assert_eq!(data.csr_backend, Backend::Aidl);
        assert_eq!(data.attempts.len(), 2);
        assert_eq!(data.attempts[0].error.as_ref().unwrap().code, code);
        assert!(data.attempts[1].error.is_none());
        assert_eq!(server.challenges.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn native_v1_is_attempted_after_both_previous_methods_fail() {
    let (_dir, ctx) = working_dir("rkp-auto-third");
    let methods = Methods::new(
        &ctx,
        [ResultKind::Reject, ResultKind::Timeout, ResultKind::Good],
    );
    let server = FreshServer::new(vec![]);
    let data = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &methods,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();
    assert_eq!(
        *methods.calls.borrow(),
        [Method::V2, Method::V1, Method::V1Native]
    );
    assert_eq!(data.csr_protocol, Protocol::V1);
    assert_eq!(data.csr_backend, Backend::Native);
    assert!(data.local_verify.is_none());
    assert_eq!(data.attempts.len(), 3);
    assert_eq!(
        *server.challenges.lock().unwrap(),
        [vec![1; 32], vec![2; 32], vec![3; 32]]
    );
    assert_eq!(fs::read_dir(&ctx.paths.outputs_dir).unwrap().count(), 1);
}

#[tokio::test]
async fn all_method_failures_keep_their_names_codes_and_leave_no_artifacts() {
    let (_dir, ctx) = working_dir("rkp-auto-all-fail");
    let methods = Methods::new(
        &ctx,
        [
            ResultKind::Transport,
            ResultKind::Timeout,
            ResultKind::Reject,
        ],
    );
    let server = FreshServer::new(vec![]);
    let error = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &methods,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap_err();
    assert_eq!(crate::error::code_of(&error), "all_methods_failed");
    for expected in [
        "v2 [ta_transport_error]",
        "v1 [ta_timeout]",
        "v1-native [ta_rejected]",
    ] {
        assert!(error.to_string().contains(expected));
    }
    assert_eq!(methods.calls.borrow().len(), 3);
    assert_eq!(fs::read_dir(&ctx.paths.outputs_dir).unwrap().count(), 0);
}

#[tokio::test]
async fn server_rejection_and_invalid_certificates_retry_with_fresh_challenges() {
    for stages in [
        vec![ServerResult::Reject, ServerResult::InvalidChain],
        vec![ServerResult::WrongKey, ServerResult::Reject],
    ] {
        let (_dir, ctx) = working_dir("rkp-auto-server-retry");
        let methods = Methods::new(&ctx, [ResultKind::Good; 3]);
        let server = FreshServer::new(stages);
        let data = provision(
            &ctx,
            true,
            Options::default(),
            &FixedKeys,
            &methods,
            &server,
            &FakeEnvironment,
        )
        .await
        .unwrap();
        assert_eq!(data.attempts.len(), 3);
        assert_eq!(data.csr_backend, Backend::Native);
        assert_eq!(data.challenge_hex, hex::encode([3; 32]));
        assert_eq!(
            data.attempts[1].error.as_ref().map(|e| e.code),
            Some(if matches!(server.stages[1], ServerResult::Reject) {
                "rkp_client_error"
            } else {
                "invalid_device_csr"
            })
        );
    }
}

#[tokio::test]
async fn challenge_failure_moves_to_the_next_method_without_calling_the_first_adapter() {
    let (_dir, ctx) = working_dir("rkp-auto-challenge-retry");
    let methods = Methods::new(&ctx, [ResultKind::Good; 3]);
    let server = FreshServer::new(vec![ServerResult::ChallengeFailure]);
    let data = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &methods,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();
    assert_eq!(*methods.calls.borrow(), [Method::V1]);
    assert_eq!(
        data.attempts[0].error.as_ref().unwrap().code,
        "rkp_server_error"
    );
    assert_eq!(data.challenge_hex, hex::encode([2; 32]));
}

#[tokio::test]
async fn all_rejected_or_invalid_server_results_leave_no_files_and_keep_each_cause() {
    let (_dir, ctx) = working_dir("rkp-all-server-results-fail");
    let methods = Methods::new(&ctx, [ResultKind::Good; 3]);
    let server = FreshServer::new(vec![
        ServerResult::Reject,
        ServerResult::WrongKey,
        ServerResult::InvalidChain,
    ]);
    let error = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &methods,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap_err();
    assert_eq!(crate::error::code_of(&error), "all_methods_failed");
    for expected in [
        "v2 [rkp_client_error]",
        "v1 [certificate_key_mismatch]",
        "v1-native [invalid_device_csr]",
    ] {
        assert!(error.to_string().contains(expected));
    }
    assert_eq!(methods.calls.borrow().len(), 3);
    assert_eq!(fs::read_dir(&ctx.paths.outputs_dir).unwrap().count(), 0);
}

#[tokio::test]
async fn saving_failure_does_not_repeat_a_successful_issuance() {
    let (_dir, ctx) = working_dir("rkp-saving-failure");
    fs::remove_dir(&ctx.paths.outputs_dir).unwrap();
    fs::write(&ctx.paths.outputs_dir, "occupied").unwrap();
    let ta = FakeTa::new();
    let server = seeded_server();
    let error = provision(
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
    assert_ne!(crate::error::code_of(&error), "all_methods_failed");
    assert_eq!(server.submitted.lock().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(&ctx.paths.outputs_dir).unwrap(),
        "occupied"
    );
}

#[test]
fn candidate_order_skips_only_the_unavailable_methods() {
    let (_dir, ctx) = working_dir("rkp-candidate-order");
    let mut status = FakeEnvironment.capabilities(&ctx);
    status.v1_available = false;
    assert_eq!(
        crate::issuance::candidates(Protocol::Auto, &status),
        [Method::V2, Method::V1Native]
    );
    status.v2_available = false;
    assert_eq!(
        crate::issuance::candidates(Protocol::Auto, &status),
        [Method::V1Native]
    );
    assert_eq!(
        crate::issuance::candidates(Protocol::V1, &status),
        [Method::V1]
    );
}

#[tokio::test]
async fn malformed_server_challenge_is_recorded_before_trying_the_next_method() {
    let (_dir, ctx) = working_dir("rkp-empty-challenge");
    let methods = Methods::new(&ctx, [ResultKind::Good; 3]);
    let server = FreshServer::new(vec![ServerResult::EmptyChallenge]);
    let data = provision(
        &ctx,
        true,
        Options::default(),
        &FixedKeys,
        &methods,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();
    assert_eq!(*methods.calls.borrow(), [Method::V1]);
    assert_eq!(
        data.attempts[0].error.as_ref().unwrap().code,
        "invalid_rkp_response"
    );
}
