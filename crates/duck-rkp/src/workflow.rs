use anyhow::{Result, ensure};
use ciborium::value::Value;

use crate::{
    cbor::{as_array, as_bytes, decode, encode},
    certificates::select_chain_for_p256,
    cose::{EcKeyPair, generate_ec_keypair},
    http::{fetch_challenge, rkp_client, submit_csr, validate_endpoint},
    keybox_xml::ParsedCertificate,
    verify::{VerifyReport, verify_csr},
};

pub(crate) struct IssuedKey {
    pub pair: EcKeyPair,
    pub chain: Vec<ParsedCertificate>,
}

pub(crate) struct Issued {
    pub csr: Vec<u8>,
    pub challenge: Vec<u8>,
    pub report: VerifyReport,
    pub keys: Vec<IssuedKey>,
}

pub(crate) fn validate_request_binding(
    csr: &[u8],
    challenge: &[u8],
    public_keys: &[Value],
) -> Result<VerifyReport> {
    let report = verify_csr(csr)?;
    ensure!(report.signature_valid, "request signature is invalid");
    let value = decode(csr)?;
    let request = as_array(&value, "request")?;
    let signed = as_array(&request[3], "signed data")?;
    let payload = decode(as_bytes(&signed[2], "signed payload")?)?;
    let items = as_array(&payload, "signed payload")?;
    ensure!(
        as_bytes(&items[0], "challenge")? == challenge,
        "request challenge mismatch"
    );
    let payload = decode(as_bytes(&items[1], "CSR payload")?)?;
    let items = as_array(&payload, "CSR payload")?;
    ensure!(
        encode(&items[3])? == encode(&Value::Array(public_keys.to_vec()))?,
        "request public keys mismatch"
    );
    Ok(report)
}

pub(crate) async fn issue(
    fingerprint: &str,
    include_fingerprint: bool,
    server_url: &str,
    count: u32,
    build: impl Fn(&[u8], &[Value]) -> Result<Vec<u8>>,
) -> Result<Issued> {
    ensure!(
        !fingerprint.trim().is_empty(),
        "explicit fingerprint is required"
    );
    validate_endpoint(server_url)?;
    ensure!(
        (1..=16).contains(&count),
        "request must contain between 1 and 16 keys"
    );
    let pairs = (0..count)
        .map(|_| generate_ec_keypair())
        .collect::<Result<Vec<_>>>()?;
    let public_keys = pairs
        .iter()
        .map(|key| key.cose_public.clone())
        .collect::<Vec<_>>();
    let build = |challenge: &[u8], public: &[Value]| {
        crate::request::with_fingerprint(
            &build(challenge, public)?,
            include_fingerprint.then_some(fingerprint),
        )
    };
    // validate local material and configuration before contacting the server
    validate_request_binding(&build(&[0; 64], &public_keys)?, &[0; 64], &public_keys)?;
    let client = rkp_client()?;
    let challenge = fetch_challenge(&client, fingerprint, server_url).await?;
    let csr = build(&challenge, &public_keys)?;
    let report = validate_request_binding(&csr, &challenge, &public_keys)?;
    let response = submit_csr(&client, &csr, &challenge, server_url).await?;
    let mut keys = Vec::new();
    for pair in pairs {
        let expected = pair.secret_key.public_key().to_sec1_bytes();
        let chain = select_chain_for_p256(&response, expected.as_ref())
            .map_err(|error| crate::error::RkpError::CertificateValidation(format!("{error:#}")))?;
        keys.push(IssuedKey { pair, chain });
    }
    Ok(Issued {
        csr,
        challenge,
        report,
        keys,
    })
}

#[cfg(test)]
pub(crate) mod mock_server;
#[cfg(test)]
mod tests;
