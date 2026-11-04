//! The device-backed provisioning round trip.
//!
//! Unlike the offline workbench, this path never touches a seed: the challenge and the
//! certificate chain come from Google, the CSR is signed by the device's own KeyMint TA,
//! and only the private key stays on the host. Both sides of the exchange are traits so
//! the whole flow can be exercised without a device or a network.

use std::path::PathBuf;

use anyhow::Result;
use duck_core::BoxFuture;
use duck_core::{AppPaths, Context};
use duck_platform::net::{http_client, is_web_url};
use duck_rkp::{
    cose::{EcKeyPair, generate_ec_keypair},
    error::RkpError,
    http::{EekResponse, fetch_eek, submit_csr},
    profile::DEFAULT_SERVER_URL,
    verify::VerifyReport,
};
use reqwest::Client;
use serde::Serialize;

use crate::{
    artifacts::{self, ArtifactPaths, Artifacts, ChainSummary},
    error::DeviceRkpError,
    issuance::{self, Exchange, Protocol},
    protocol::MAX_CERT_CHAIN_LEN,
    status::{self, DeviceStatus, LIMITATIONS},
    v1, worker,
};

/// Everything the caller can still adjust without touching KeyMint.
#[derive(Debug, Clone, Default)]
pub(crate) struct Options {
    pub protocol: Protocol,
    /// Override the build fingerprint used for this issuance.
    pub fingerprint: Option<String>,
    pub include_fingerprint: bool,
    /// Also write `keybox.xml` here, instead of only inside the timestamped run directory.
    pub output: Option<PathBuf>,
}

/// Where the key pair that ends up in `keysToSign` comes from. Production uses the OS
/// RNG; a test substitutes a fixed pair so it can issue a certificate for it.
pub(crate) trait KeySource {
    fn generate(&self) -> Result<EcKeyPair>;
}

/// The production key source.
pub(crate) struct RandomKeys;

impl KeySource for RandomKeys {
    fn generate(&self) -> Result<EcKeyPair> {
        generate_ec_keypair()
    }
}

/// The capability report the run is gated on. Only tests replace it; the device answer
/// comes from `status::detect`.
pub(crate) trait Environment {
    fn capabilities(&self, ctx: &Context) -> DeviceStatus;
}

/// Everything the running device actually offers.
pub(crate) struct DeviceEnvironment;

impl Environment for DeviceEnvironment {
    fn capabilities(&self, ctx: &Context) -> DeviceStatus {
        status::detect(ctx)
    }
}

/// The Google half: the anti-replay challenge and the issued certificate chain. The
/// futures are boxed so the trait stays usable behind a reference.
pub(crate) trait Server: Sync {
    fn challenge<'a>(&'a self, fingerprint: &'a str) -> BoxFuture<'a, Result<EekResponse>>;
    fn certificates<'a>(
        &'a self,
        csr: &'a [u8],
        challenge: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<Vec<u8>>>>;
}

/// The real vendor path. It runs inside a child process started from this binary.
pub(crate) struct Device<'a> {
    pub paths: &'a AppPaths,
    pub sysroot: &'a duck_core::Sysroot,
}

impl Exchange for Device<'_> {
    fn csr(&self, request: &[u8]) -> Result<Vec<u8>> {
        worker::sign(self.paths, request)
    }
    fn csr_v1(&self, request: &v1::Request) -> Result<v1::Response> {
        let apk = v1::find_aidl_apk(self.sysroot).ok_or_else(|| {
            DeviceRkpError::UnsupportedDevice("V1 AIDL APK is unavailable".into())
        })?;
        v1::sign(self.paths, &apk, request)
    }
}

/// The real server path, through the shared HTTP client.
#[derive(Debug)]
pub(crate) struct Google {
    client: Client,
    server_url: String,
}

impl Google {
    pub(crate) fn new(server_url: Option<String>) -> Result<Self> {
        let server_url = server_url.unwrap_or_else(|| DEFAULT_SERVER_URL.to_owned());
        let server_url = server_url.trim().trim_end_matches('/').to_owned();
        if !is_web_url(&server_url) {
            return Err(RkpError::InvalidServerUrl(server_url).into());
        }
        Ok(Self {
            client: http_client()?,
            server_url,
        })
    }
}

impl Server for Google {
    fn challenge<'a>(&'a self, fingerprint: &'a str) -> BoxFuture<'a, Result<EekResponse>> {
        Box::pin(async move { fetch_eek(&self.client, fingerprint, &self.server_url).await })
    }

    fn certificates<'a>(
        &'a self,
        csr: &'a [u8],
        challenge: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<Vec<u8>>>> {
        Box::pin(async move { submit_csr(&self.client, csr, challenge, &self.server_url).await })
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ProvisionData {
    /// The run only happened because the caller acknowledged the limitations.
    pub acknowledged_limitations: bool,
    /// Stable identifiers of the limitations the user acknowledged.
    pub limitations: [&'static str; 2],
    /// How the CSR was produced, so a report can tell a device run from an offline one.
    pub mode: &'static str,
    pub csr_protocol: Protocol,
    pub csr_backend: issuance::Backend,
    pub attempts: Vec<issuance::Attempt>,
    pub fingerprint: String,
    pub include_fingerprint: bool,
    pub challenge_hex: String,
    pub csr_len: usize,
    pub local_verify: Option<VerifyReport>,
    pub chain: ChainSummary,
    pub device_id: String,
    pub keybox_xml: String,
    pub artifacts: ArtifactPaths,
    pub prerequisites: DeviceStatus,
}

/// Runs one complete provisioning round trip and writes every artifact.
pub(crate) async fn provision(
    ctx: &Context,
    acknowledged: bool,
    options: Options,
    key_source: &dyn KeySource,
    exchange: &dyn Exchange,
    server: &dyn Server,
    environment: &dyn Environment,
) -> Result<ProvisionData> {
    if !acknowledged {
        return Err(DeviceRkpError::AcknowledgementRequired.into());
    }

    let mut prerequisites = environment.capabilities(ctx);
    let explicit_fingerprint = options
        .fingerprint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if explicit_fingerprint.is_some() {
        prerequisites
            .blockers
            .retain(|blocker| *blocker != "fingerprint_missing");
        prerequisites.prerequisites_met = prerequisites.blockers.is_empty();
    }
    let blockers = issuance::blockers_for(&prerequisites, options.protocol);
    if !blockers.is_empty() {
        return Err(DeviceRkpError::UnsupportedDevice(blockers.join(", ")).into());
    }

    let fingerprint = explicit_fingerprint
        .unwrap_or(prerequisites.fingerprint.trim())
        .to_owned();
    let key = key_source.generate()?;
    let mut attempts = Vec::new();
    let mut issued = None;
    for method in issuance::candidates(options.protocol, &prerequisites) {
        match attempt(
            method,
            &fingerprint,
            options.include_fingerprint,
            &prerequisites.soc_manufacturer,
            &key,
            exchange,
            server,
        )
        .await
        {
            Ok(result) => {
                attempts.push(issuance::Attempt {
                    method,
                    error: None,
                });
                issued = Some(result);
                break;
            }
            Err(error) if options.protocol == Protocol::Auto => {
                attempts.push(issuance::Attempt::failed(method, &error));
            }
            Err(error) => return Err(error),
        }
    }
    let Some(Issued {
        eek,
        signed,
        prepared,
    }) = issued
    else {
        let failures = attempts
            .iter()
            .filter_map(|attempt| {
                attempt.error.as_ref().map(|error| {
                    format!(
                        "{} [{}]: {}",
                        attempt.method.name(),
                        error.code,
                        error.message
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(DeviceRkpError::AllMethodsFailed(failures).into());
    };
    let mut artifacts = prepared.write(&ctx.paths, &signed.bytes)?;
    if let Some(path) = options.output {
        artifacts::write_keybox(&path, &artifacts.keybox_xml)?;
        artifacts.paths.keybox_path = path.display().to_string();
    }

    Ok(report(
        &fingerprint,
        options.include_fingerprint,
        &eek,
        signed,
        artifacts,
        prerequisites,
        attempts,
    ))
}

struct Issued {
    eek: EekResponse,
    signed: issuance::SignedCsr,
    prepared: artifacts::Prepared,
}

async fn attempt(
    method: issuance::Method,
    fingerprint: &str,
    include_fingerprint: bool,
    manufacturer: &str,
    key: &EcKeyPair,
    exchange: &dyn Exchange,
    server: &dyn Server,
) -> Result<Issued> {
    // Each method owns a fresh challenge, including after a previous server rejection.
    let eek = server.challenge(fingerprint).await?;
    if eek.challenge.is_empty() || eek.challenge.len() > 64 {
        return Err(RkpError::InvalidRkpResponse(
            "device challenge must contain 1..64 bytes".into(),
        )
        .into());
    }
    let signed = issuance::issue(
        method,
        exchange,
        key,
        &eek,
        include_fingerprint.then_some(fingerprint),
    )?;
    let chains = server.certificates(&signed.bytes, &eek.challenge).await?;
    let chain_der = single_chain(&chains)?;
    let prepared = artifacts::prepare(&key.secret_key, chain_der, manufacturer)?;
    Ok(Issued {
        eek,
        signed,
        prepared,
    })
}

fn report(
    fingerprint: &str,
    include_fingerprint: bool,
    eek: &EekResponse,
    signed: issuance::SignedCsr,
    artifacts: Artifacts,
    prerequisites: DeviceStatus,
    attempts: Vec<issuance::Attempt>,
) -> ProvisionData {
    ProvisionData {
        acknowledged_limitations: true,
        limitations: LIMITATIONS,
        mode: "device-ta-oracle",
        csr_protocol: signed.protocol,
        csr_backend: signed.backend,
        attempts,
        fingerprint: fingerprint.to_owned(),
        include_fingerprint,
        challenge_hex: eek.challenge_hex.clone(),
        csr_len: signed.bytes.len(),
        local_verify: signed.local_verify,
        chain: ChainSummary::of(&artifacts.chain),
        device_id: artifacts.device_id.clone(),
        keybox_xml: artifacts.keybox_xml.clone(),
        artifacts: artifacts.paths.clone(),
        prerequisites,
    }
}

fn single_chain(chains: &[Vec<u8>]) -> Result<Vec<u8>> {
    let [chain] = chains else {
        return Err(RkpError::InvalidRkpResponse(format!(
            "expected one certificate chain for one requested key, got {}",
            chains.len()
        ))
        .into());
    };
    if chain.is_empty() || chain.len() > MAX_CERT_CHAIN_LEN {
        return Err(RkpError::InvalidRkpResponse(format!(
            "issued certificate chain must contain 1..{MAX_CERT_CHAIN_LEN} bytes"
        ))
        .into());
    }
    Ok(chain.to_vec())
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
