use std::{cell::RefCell, fs, path::PathBuf, sync::Mutex};

use ciborium::value::Value;
use duck_core::{AppPaths, BoxFuture, Context, Sysroot};
use duck_rkp::{
    cbor::{bytes, encode, int},
    cose::{DeviceKeys, RPC_CURVE_25519, build_csr},
    http::EekResponse,
};
use p256::SecretKey as P256SecretKey;

use p256::elliptic_curve::sec1::ToSec1Point as _;
use rcgen::{
    CertificateParams, KeyPair as RcgenKeyPair, PKCS_ECDSA_P256_SHA256, PublicKeyData,
    SignatureAlgorithm,
};
use sha2::{Digest as _, Sha256};

use super::{DeviceEnvironment, Environment, Exchange, KeySource, Options, Server, provision};
use crate::{protocol, status::DeviceStatus};

const CHALLENGE: [u8; 32] = [0x5A; 32];
const FINGERPRINT: &str = "google/husky/husky:15/AP3A/1:user/release-keys";

/// A device that claims every prerequisite, so the pipeline itself is what is tested.
struct FakeEnvironment;

impl Environment for FakeEnvironment {
    fn capabilities(&self, ctx: &Context) -> DeviceStatus {
        DeviceStatus {
            prerequisites_met: true,
            android_arm64: true,
            root: true,
            library_present: true,
            device_present: true,
            qualcomm_soc: true,
            library_path: "/vendor/lib64/libkeymasterdeviceutils.so",
            device_path: "/dev/smcinvoke",
            fingerprint: FINGERPRINT.into(),
            soc_manufacturer: "QTI".into(),
            soc_detection: crate::soc::detect(
                &[("ro.soc.manufacturer".into(), "QTI".into())].into(),
                &ctx.sysroot,
            ),
            limitations: super::LIMITATIONS,
            blockers: Vec::new(),
        }
    }
}

struct BlockedEnvironment(Vec<&'static str>);

impl Environment for BlockedEnvironment {
    fn capabilities(&self, ctx: &Context) -> DeviceStatus {
        DeviceStatus {
            prerequisites_met: false,
            android_arm64: true,
            root: false,
            library_present: false,
            device_present: false,
            qualcomm_soc: false,
            library_path: "/vendor/lib64/libkeymasterdeviceutils.so",
            device_path: "/dev/smcinvoke",
            fingerprint: FINGERPRINT.into(),
            soc_manufacturer: "Google".into(),
            soc_detection: crate::soc::detect(
                &[("ro.soc.manufacturer".into(), "Google".into())].into(),
                &ctx.sysroot,
            ),
            limitations: super::LIMITATIONS,
            blockers: self.0.clone(),
        }
    }
}

/// A stand-in TA: it reads the key and challenge out of the request exactly as the real
/// TA does, and answers with a real CSR over that key.
struct FakeTa {
    device_keys: DeviceKeys,
    request: RefCell<Vec<u8>>,
    /// When set, the mocked TA signs another key than the one it was asked about.
    substitute: Option<Value>,
}

impl FakeTa {
    fn new() -> Self {
        Self {
            device_keys: DeviceKeys::from_seed([0x22; 32]),
            request: RefCell::new(Vec::new()),
            substitute: None,
        }
    }

    fn csr(&self) -> Vec<u8> {
        let (challenge, requested) = requested_fields(&self.request.borrow());
        let signed = self.substitute.clone().unwrap_or(requested);
        build_csr(
            &self.device_keys,
            &challenge,
            &[signed],
            &[0x33; 32],
            &[0x44; 4],
            &duck_rkp::profile::detect::detect().device,
            RPC_CURVE_25519,
        )
        .unwrap()
        .csr_bytes
    }
}

impl Exchange for FakeTa {
    fn csr(&self, request: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.request.replace(request.to_vec());
        encode(&Value::Map(vec![(int(61), bytes(self.csr()))]))
    }
}

/// Decodes `{4: challenge, 57: bstr([COSE_Key])}` as the TA reads it.
fn requested_fields(request: &[u8]) -> (Vec<u8>, Value) {
    let payload = protocol::ta_payload(request).expect("a request the TA accepts");
    let decoded = duck_rkp::cbor::decode(payload).unwrap();
    let Value::Map(entries) = decoded else {
        panic!("request payload must be a CBOR map");
    };
    let challenge = duck_rkp::cbor::map_get(&entries, 4).unwrap().clone();
    let Value::Bytes(challenge) = challenge else {
        panic!("challenge must be bytes");
    };
    let keys = duck_rkp::cbor::map_get(&entries, 57).unwrap();
    let Value::Bytes(keys) = keys else {
        panic!("keysToSign must be bytes");
    };
    let Value::Array(keys) = duck_rkp::cbor::decode(keys).unwrap() else {
        panic!("keysToSign must decode as an array");
    };
    (challenge, keys[0].clone())
}

/// Signs rcgen certificates with a P-256 key this crate already generated.
struct TestSigner {
    key: p256::ecdsa::SigningKey,
    subject: Vec<u8>,
}

impl PublicKeyData for TestSigner {
    fn der_bytes(&self) -> &[u8] {
        &self.subject
    }

    fn algorithm(&self) -> &'static SignatureAlgorithm {
        &rcgen::PKCS_ECDSA_P256_SHA256
    }
}

impl rcgen::SigningKey for TestSigner {
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, rcgen::Error> {
        use p256::ecdsa::signature::Signer as _;

        let signature: p256::ecdsa::Signature = self.key.sign(&Sha256::digest(msg));
        Ok(signature.to_bytes().to_vec())
    }
}

/// A leaf for `signer` issued by a generated root, DER concatenated leaf first.
fn issued_chain(signer: &TestSigner) -> Vec<u8> {
    let root = rcgen::CertifiedIssuer::self_signed(
        named_params("duck-test-root"),
        RcgenKeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap(),
    )
    .unwrap();
    let leaf = named_params("duck-test-leaf")
        .signed_by(signer, &root)
        .unwrap();
    let mut chain = leaf.der().to_vec();
    chain.extend_from_slice(root.der());
    chain
}

/// Parameters with a real subject name, so an issuer and its subject differ.
fn named_params(common_name: &str) -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut name = rcgen::DistinguishedName::new();
    name.push(rcgen::DnType::CommonName, common_name);
    params.distinguished_name = name;
    params
}

/// A stand-in server: one fixed challenge and a fresh certificate chain over the key the
/// CSR asked to sign.
struct FakeServer {
    submitted: Mutex<Vec<Vec<u8>>>,
    fingerprint: Mutex<Option<String>>,
    challenge: [u8; 32],
}

impl FakeServer {
    fn new() -> Self {
        Self {
            submitted: Mutex::new(Vec::new()),
            fingerprint: Mutex::new(None),
            challenge: CHALLENGE,
        }
    }
}

impl Server for FakeServer {
    fn challenge<'a>(&'a self, fingerprint: &'a str) -> BoxFuture<'a, anyhow::Result<EekResponse>> {
        Box::pin(async move {
            *self.fingerprint.lock().unwrap() = Some(fingerprint.to_owned());
            Ok(EekResponse {
                challenge_hex: hex::encode(self.challenge),
                challenge: self.challenge.to_vec(),
                eek_public_hex: hex::encode([0x55; 32]),
                eek_public: vec![0x55; 32],
                eek_id: vec![0x66; 4],
                eek_curve: RPC_CURVE_25519,
            })
        })
    }

    fn certificates<'a>(
        &'a self,
        csr: &'a [u8],
        challenge: &'a [u8],
    ) -> BoxFuture<'a, anyhow::Result<Vec<Vec<u8>>>> {
        Box::pin(async move {
            assert_eq!(challenge, self.challenge);
            self.submitted.lock().unwrap().push(csr.to_vec());

            // A working pipeline generates the key, so the certificate must cover that
            // key. The harness cannot recover a private key from a public point, so the
            // test keeps the key pair; a run with any other key makes rcgen's key and the
            // CSR key disagree and the assertion below fails.
            let requested = csr_keys_to_sign(csr);
            let signer = TEST_KEY.with(|cell| {
                let secret = cell.borrow().clone().expect("test seeds the key");
                assert_eq!(
                    cose_public_of(&secret),
                    requested,
                    "CSR key must match the server key"
                );
                TestSigner {
                    key: p256::ecdsa::SigningKey::from(secret),
                    subject: requested_point(&requested),
                }
            });
            // Google answers with a leaf plus its issuers, not a bare self-signed leaf, so
            // the mocked server builds the same shape.
            Ok(vec![issued_chain(&signer)])
        })
    }
}

thread_local! {
    /// The key the mocked server issues for; `SEED_KEY` fills it per test.
    static TEST_KEY: RefCell<Option<P256SecretKey>> = const { RefCell::new(None) };
}

fn seed_server_key() {
    TEST_KEY.with(|cell| *cell.borrow_mut() = Some(P256SecretKey::from_slice(&SEED_KEY).unwrap()));
}

const SEED_KEY: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn cose_public_of(secret: &P256SecretKey) -> Value {
    let point = secret.public_key().to_sec1_point(false);
    Value::Map(vec![
        (int(1), int(2)),
        (int(3), int(-7)),
        (int(-1), int(1)),
        (int(-2), bytes(point.x().unwrap().to_vec())),
        (int(-3), bytes(point.y().unwrap().to_vec())),
    ])
}

fn requested_point(cose_key: &Value) -> Vec<u8> {
    let Value::Map(entries) = cose_key else {
        panic!("keysToSign entry must be a COSE key map");
    };
    let mut point = vec![4];
    for label in [-2, -3] {
        let coordinate = duck_rkp::cbor::map_get(entries, label).unwrap();
        point.extend_from_slice(duck_rkp::cbor::as_bytes(coordinate, "coordinate").unwrap());
    }
    point
}

/// Reads `keysToSign[0]` out of the CSR the TA produced.
fn csr_keys_to_sign(csr: &[u8]) -> Value {
    let decoded = duck_rkp::cbor::decode(csr).unwrap();
    let Value::Array(request) = decoded else {
        panic!("CSR must be a CBOR array");
    };
    let Value::Array(signed) = &request[3] else {
        panic!("SignedData must be a CBOR array");
    };
    let Value::Bytes(payload) = &signed[2] else {
        panic!("SignedData payload must be bytes");
    };
    let Value::Array(signed_payload) = duck_rkp::cbor::decode(payload).unwrap() else {
        panic!("signed payload must be a CBOR array");
    };
    let Value::Bytes(contents) = &signed_payload[1] else {
        panic!("CSR payload must be bytes");
    };
    let Value::Array(contents) = duck_rkp::cbor::decode(contents).unwrap() else {
        panic!("CSR payload must be a CBOR array");
    };
    let Value::Array(keys) = &contents[3] else {
        panic!("keysToSign must be a CBOR array");
    };
    keys[0].clone()
}

/// The key pair the mocked server and this test agree on.
fn test_key() -> P256SecretKey {
    P256SecretKey::from_slice(&SEED_KEY).unwrap()
}

/// Hands the pipeline the key the mocked server knows how to sign for.
struct FixedKeys;

impl KeySource for FixedKeys {
    fn generate(&self) -> anyhow::Result<duck_rkp::cose::EcKeyPair> {
        Ok(duck_rkp::cose::EcKeyPair {
            secret_key: test_key(),
            signing_key: p256::ecdsa::SigningKey::from(test_key()),
            cose_public: cose_public_of(&test_key()),
        })
    }
}

/// A server that will issue for `test_key`, with the thread-local seed filled in.
fn seeded_server() -> FakeServer {
    seed_server_key();
    FakeServer::new()
}

fn context(root: &std::path::Path) -> Context {
    Context {
        paths: AppPaths::for_root(root, root),
        sysroot: Sysroot::default(),
    }
}

fn working_dir(name: &str) -> (tempfile::TempDir, Context) {
    let dir = tempfile::Builder::new().prefix(name).tempdir().unwrap();
    let ctx = context(dir.path());
    fs::create_dir_all(&ctx.paths.outputs_dir).unwrap();
    (dir, ctx)
}

#[tokio::test]
async fn provision_completes_the_round_trip_and_writes_artifacts() {
    let (_dir, ctx) = working_dir("rkp-device-ok");
    let ta = FakeTa::new();
    let server = seeded_server();

    let data = provision(
        &ctx,
        true,
        Options {
            fingerprint: Some(FINGERPRINT.into()),
            output: None,
        },
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();

    assert!(data.acknowledged_limitations);
    assert_eq!(data.limitations, super::LIMITATIONS);
    assert_eq!(data.mode, "device-ta-oracle");
    assert_eq!(data.fingerprint, FINGERPRINT);
    assert_eq!(data.challenge_hex, hex::encode(CHALLENGE));
    assert!(data.local_verify.signature_valid);
    assert_eq!(data.local_verify.keys_to_sign, 1);
    assert_eq!(data.chain.certificates, 2);
    assert_eq!(
        data.chain.subjects,
        ["CN=duck-test-leaf", "CN=duck-test-root"]
    );
    assert!(data.device_id.starts_with("QTI-"));
    // The whole chain reaches the keybox, and the daemon validator accepts it.
    let summary = duck_tricky_store::keybox::validate::validate(&data.keybox_xml).unwrap();
    assert_eq!(summary.chain_lengths, vec![2]);
    assert!(summary.counts_declared_correctly);

    // The TA was asked to sign exactly the key the server later certified.
    assert_eq!(server.submitted.lock().unwrap().len(), 1);
    assert_eq!(server.submitted.lock().unwrap()[0], ta.csr());

    let request = ta.request.borrow();
    assert_eq!(
        request.get(..4),
        Some(protocol::CSR_COMMAND.to_le_bytes().as_slice())
    );
    assert_eq!(&request[4..], protocol::ta_payload(&request).unwrap());

    for path in [
        &data.artifacts.keybox_path,
        &data.artifacts.csr_path,
        &data.artifacts.cert_chain_path,
        &data.artifacts.private_key_path,
    ] {
        assert!(PathBuf::from(path).is_file(), "{path} must exist");
    }
}

#[tokio::test]
async fn provision_rejects_a_csr_for_another_key() {
    let (_dir, ctx) = working_dir("rkp-device-substitute");
    let mut ta = FakeTa::new();
    ta.substitute = Some(cose_public_of(
        &P256SecretKey::from_slice(&[0x77; 32]).unwrap(),
    ));
    let server = FakeServer::new();

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

    assert_eq!(crate::error::code_of(&error), "invalid_device_csr");
    assert!(
        server.submitted.lock().unwrap().is_empty(),
        "a bad CSR must not be submitted"
    );
}

#[tokio::test]
async fn provision_mirrors_the_keybox_to_an_explicit_output() {
    let (dir, ctx) = working_dir("rkp-device-output");
    let ta = FakeTa::new();
    let server = seeded_server();
    let output = dir.path().join("var/outputs/custom.xml");

    let data = provision(
        &ctx,
        true,
        Options {
            fingerprint: None,
            output: Some(output.clone()),
        },
        &FixedKeys,
        &ta,
        &server,
        &FakeEnvironment,
    )
    .await
    .unwrap();

    assert_eq!(data.artifacts.keybox_path, output.display().to_string());
    assert_eq!(fs::read_to_string(&output).unwrap(), data.keybox_xml);
    // The run directory keeps its own copy, so the report never points at a stale file.
    assert!(PathBuf::from(&data.artifacts.run_dir).is_dir());
}

#[test]
fn google_accepts_a_server_override() {
    let server = super::Google::new(Some("https://example.invalid/v1".into())).unwrap();
    assert!(format!("{server:?}").contains("example.invalid"));
}

#[test]
fn google_validates_and_normalizes_the_server_endpoint() {
    for url in ["", "  ", "file:///tmp/rkp", "not a URL"] {
        let error = super::Google::new(Some(url.into())).unwrap_err();
        assert_eq!(crate::error::code_of(&error), "invalid_server_url");
    }
    let server = super::Google::new(Some(" https://example.invalid/v1/  ".into())).unwrap();
    assert_eq!(server.server_url, "https://example.invalid/v1");
}

#[test]
fn exactly_one_nonempty_bounded_certificate_chain_is_required() {
    for chains in [
        vec![],
        vec![vec![]],
        vec![vec![1], vec![2]],
        vec![vec![0; super::MAX_CERT_CHAIN_LEN + 1]],
    ] {
        let error = super::single_chain(&chains).unwrap_err();
        assert_eq!(crate::error::code_of(&error), "invalid_rkp_response");
    }
    assert_eq!(super::single_chain(&[vec![1, 2, 3]]).unwrap(), [1, 2, 3]);
}

#[path = "service_prerequisite_tests.rs"]
mod prerequisites;
