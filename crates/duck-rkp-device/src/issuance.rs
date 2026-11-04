//! Selects one device protocol while preserving each protocol's validation semantics.
use crate::{error::DeviceRkpError, protocol, status::DeviceStatus, v1};
use anyhow::Result;
use clap::ValueEnum;
use duck_rkp::{cose::EcKeyPair, error::RkpError, http::EekResponse, verify::VerifyReport};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    #[default]
    Auto,
    V1,
    V2,
    /// Select the legacy vendor command directly.
    V1Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Backend {
    Aidl,
    Native,
}

/// Device adapters preserve each protocol's distinct request and response contract.
pub(crate) trait Exchange {
    fn csr(&self, request: &[u8]) -> Result<Vec<u8>>;
    fn csr_v1(&self, _request: &v1::Request) -> Result<v1::Response> {
        Err(DeviceRkpError::UnsupportedDevice("V1 adapter is unavailable".into()).into())
    }
}

pub(crate) fn blockers_for(status: &DeviceStatus, protocol: Protocol) -> Vec<&'static str> {
    let mut blockers = status.blockers.clone();
    // The status report describes both alternatives; an explicit protocol must not
    // require the other adapter's resources. All shared and unknown blockers remain.
    blockers.retain(|blocker| match protocol {
        Protocol::V1 => !matches!(
            *blocker,
            "keymint_library_missing"
                | "smcinvoke_missing"
                | "v2_unavailable"
                | "v1_native_unavailable"
        ),
        Protocol::V2 => !matches!(*blocker, "v1_unavailable" | "v1_native_unavailable"),
        Protocol::V1Native => !matches!(*blocker, "v1_unavailable" | "v2_unavailable"),
        Protocol::Auto => true,
    });
    let missing = match protocol {
        Protocol::V1 if !status.v1_available => Some("v1_unavailable"),
        Protocol::V2 if !status.v2_available => Some("v2_unavailable"),
        Protocol::V1Native if !status.v1_native_available => Some("v1_native_unavailable"),
        _ => None,
    };
    if let Some(missing) = missing.filter(|missing| !blockers.contains(missing)) {
        blockers.push(missing);
    }
    blockers
}

pub(crate) struct SignedCsr {
    pub bytes: Vec<u8>,
    pub protocol: Protocol,
    pub backend: Backend,
    /// V1 evidence is encrypted for the server, so only V2 has a local signature report.
    pub local_verify: Option<VerifyReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Method {
    V2,
    V1,
    V1Native,
}

impl Method {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::V2 => "v2",
            Self::V1 => "v1",
            Self::V1Native => "v1-native",
        }
    }
}

pub(crate) fn candidates(selected: Protocol, status: &DeviceStatus) -> Vec<Method> {
    match selected {
        Protocol::Auto => [
            (Method::V2, status.v2_available),
            (Method::V1, status.v1_available),
            (Method::V1Native, status.v1_native_available),
        ]
        .into_iter()
        .filter_map(|(method, available)| available.then_some(method))
        .collect(),
        Protocol::V2 => vec![Method::V2],
        Protocol::V1 => vec![Method::V1],
        Protocol::V1Native => vec![Method::V1Native],
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct AttemptFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Attempt {
    pub method: Method,
    pub error: Option<AttemptFailure>,
}

impl Attempt {
    pub(crate) fn failed(method: Method, error: &anyhow::Error) -> Self {
        Self {
            method,
            error: Some(AttemptFailure {
                code: crate::error::code_of(error),
                message: format!("{error:#}"),
            }),
        }
    }
}

/// Issue exactly one CSR. The provisioning owner decides whether another method is needed.
pub(crate) fn issue(
    method: Method,
    exchange: &dyn Exchange,
    key: &EcKeyPair,
    eek: &EekResponse,
    fingerprint: Option<&str>,
) -> Result<SignedCsr> {
    match method {
        Method::V1 => return issue_v1(exchange, key, eek, fingerprint),
        Method::V1Native => return issue_native_v1(exchange, key, eek, fingerprint),
        Method::V2 => {}
    }
    let request = protocol::build_request(&eek.challenge, &key.cose_public)?;
    let signed = exchange.csr(&request)?;
    let bytes = protocol::unwrap_csr(&signed)?;
    let bytes = duck_rkp::request::with_fingerprint(&bytes, fingerprint)
        .map_err(|error| DeviceRkpError::InvalidCsr(format!("V2 metadata: {error:#}")))?;
    if bytes.len() > protocol::BUFFER_SIZE {
        return Err(
            DeviceRkpError::InvalidCsr("device CSR exceeds the response limit".into()).into(),
        );
    }
    let report = protocol::validate_csr(&bytes, &eek.challenge, &key.cose_public)?;
    Ok(SignedCsr {
        bytes,
        protocol: Protocol::V2,
        backend: Backend::Native,
        local_verify: Some(report),
    })
}

fn issue_v1(
    exchange: &dyn Exchange,
    key: &EcKeyPair,
    eek: &EekResponse,
    fingerprint: Option<&str>,
) -> Result<SignedCsr> {
    let request = v1::request(&eek.challenge, &key.cose_public, &eek.eek_chains)?;
    let response = exchange.csr_v1(&request)?;
    finish_v1(response, Backend::Aidl, key, eek, fingerprint)
}

fn issue_native_v1(
    exchange: &dyn Exchange,
    key: &EcKeyPair,
    eek: &EekResponse,
    fingerprint: Option<&str>,
) -> Result<SignedCsr> {
    let chain = eek
        .eek_chains
        .iter()
        .find(|chain| chain.curve == eek.eek_curve)
        .ok_or_else(|| {
            RkpError::InvalidRkpResponse("no complete EEK chain for native V1".into())
        })?;
    let request = v1::native::request(&eek.challenge, &key.cose_public, &chain.encoded)
        .map_err(|error| DeviceRkpError::InvalidCsr(format!("native V1 request: {error:#}")))?;
    let payload = exchange.csr(&request.bytes)?;
    let response = v1::native::response(&payload, &request.mac_tag)
        .map_err(|error| DeviceRkpError::InvalidResponse(format!("native V1: {error:#}")))?;
    finish_v1(response, Backend::Native, key, eek, fingerprint)
}

fn finish_v1(
    response: v1::Response,
    backend: Backend,
    key: &EcKeyPair,
    eek: &EekResponse,
    fingerprint: Option<&str>,
) -> Result<SignedCsr> {
    let bytes = v1::assemble(response, &eek.challenge, &key.cose_public, fingerprint)
        .map_err(|error| DeviceRkpError::InvalidCsr(format!("V1: {error:#}")))?;
    Ok(SignedCsr {
        bytes,
        protocol: Protocol::V1,
        backend,
        local_verify: None,
    })
}
