//! Legacy RKP signing adapters and shared server CSR assembly.
//! The zero-HBK PureSoft AIDL path adapts LazyBonesLZY/rkp-v1-oracle, revision b28480c.
//! The upstream MIT notice is distributed in module/licenses/rkp-v1-oracle.txt.

use anyhow::{Result, bail};
use ciborium::value::Value;
use duck_rkp::{
    cbor::{as_array, as_bytes, as_map, bytes, encode, int, map_get},
    http::EekChain,
};
use hkdf::{
    Hkdf,
    hmac::{Hmac, Mac, digest::KeyInit},
};
use sha2::Sha256;

use crate::protocol::{self, BUFFER_SIZE, decode_exact};

mod bridge;
mod discovery;
pub(crate) mod native;
mod wire;
pub(crate) use bridge::sign;
pub(crate) use discovery::{APP_PROCESS, find_aidl_apk};

pub(crate) struct Request {
    pub challenge: Vec<u8>,
    pub maced_key: Vec<u8>,
    pub eek_chains: Vec<EekChain>,
}

#[derive(Debug)]
pub(crate) struct Response {
    pub mac: Vec<u8>,
    pub device_info: Vec<u8>,
    pub protected_data: Vec<u8>,
}

/// Only the public key crosses the AIDL bridge. The private key stays in the caller.
pub(crate) fn request(
    challenge: &[u8],
    public_key: &Value,
    chains: &[EekChain],
) -> Result<Request> {
    protocol::validate_inputs(challenge, public_key)?;
    let payload = encode(public_key)?;
    let protected = encode(&Value::Map(vec![(int(1), int(5))]))?;
    let mut mac_key = [0; 32];
    Hkdf::<Sha256>::new(None, &[0; 32])
        .expand(b"Key to MAC public keys", &mut mac_key)
        .map_err(|_| anyhow::anyhow!("derive PureSoft MAC key"))?;
    let structure = encode(&Value::Array(vec![
        Value::Text("MAC0".into()),
        bytes(protected.clone()),
        bytes(vec![]),
        bytes(payload.clone()),
    ]))?;
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&mac_key)?;
    mac.update(&structure);
    let tag = mac.finalize().into_bytes().to_vec();
    let maced_key = encode(&Value::Array(vec![
        bytes(protected),
        Value::Map(vec![]),
        bytes(payload),
        bytes(tag),
    ]))?;
    Ok(Request {
        challenge: challenge.to_vec(),
        maced_key,
        eek_chains: chains.to_vec(),
    })
}

/// V1 encrypts the DICE chain and signing evidence for the server. These checks establish
/// well-formedness only; no local signature-valid claim can be made from this response.
pub(crate) fn assemble(
    response: Response,
    challenge: &[u8],
    public_key: &Value,
    fingerprint: Option<&str>,
) -> Result<Vec<u8>> {
    protocol::validate_inputs(challenge, public_key)?;
    if response.mac.len() != 32 {
        bail!("V1 keysToSign MAC must contain 32 bytes");
    }
    if response.device_info.len() > BUFFER_SIZE || response.protected_data.len() > BUFFER_SIZE {
        bail!("V1 response field exceeds the response limit");
    }
    let device = decode_exact(&response.device_info)?;
    if as_map(&device, "V1 DeviceInfo")?.is_empty() {
        bail!("V1 DeviceInfo must not be empty");
    }
    let protected_data = decode_exact(&response.protected_data)?;
    let encrypted = as_array(&protected_data, "V1 ProtectedData")?;
    if encrypted.len() != 4 {
        bail!("V1 ProtectedData must be COSE_Encrypt with four fields");
    }
    let headers = decode_exact(as_bytes(&encrypted[0], "ProtectedData headers")?)?;
    if map_get(as_map(&headers, "ProtectedData headers")?, 1) != Some(&int(3)) {
        bail!("V1 ProtectedData must use A256GCM");
    }
    let unprotected = as_map(&encrypted[1], "ProtectedData unprotected headers")?;
    let iv = map_get(unprotected, 5).ok_or_else(|| anyhow::anyhow!("ProtectedData IV missing"))?;
    if as_bytes(iv, "ProtectedData IV")?.len() != 12
        || as_bytes(&encrypted[2], "ProtectedData ciphertext")?.len() < 16
        || as_array(&encrypted[3], "ProtectedData recipients")?.len() != 1
    {
        bail!("invalid V1 ProtectedData encryption fields");
    }
    let recipients = as_array(&encrypted[3], "ProtectedData recipients")?;
    let recipient = as_array(&recipients[0], "ProtectedData recipient")?;
    if recipient.len() != 3 || recipient[2] != Value::Null {
        bail!("invalid V1 COSE recipient");
    }
    let recipient_headers = as_bytes(&recipient[0], "recipient protected headers")?;
    if !recipient_headers.is_empty() {
        as_map(
            &decode_exact(recipient_headers)?,
            "recipient protected headers",
        )?;
    }
    as_map(&recipient[1], "recipient unprotected headers")?;
    let tag: [u8; 32] = response.mac.as_slice().try_into()?;
    let maced_keys = duck_rkp::cose::cose_mac0(std::slice::from_ref(public_key), &tag)?;
    // DeviceInfo participates in the HAL's SignedMac AAD. Validation must not rewrite
    // its encoding, nor any opaque ProtectedData returned by the HAL.
    let mut csr = vec![0x84, 0x82]; // [ [verified DeviceInfo, unverified DeviceInfo], ... ]
    csr.extend_from_slice(&response.device_info);
    csr.extend_from_slice(&encode(&duck_rkp::request::unverified_device_info(
        fingerprint,
    )?)?);
    csr.extend_from_slice(&encode(&bytes(challenge.to_vec()))?);
    csr.extend_from_slice(&response.protected_data);
    csr.extend_from_slice(&maced_keys);
    if csr.len() > BUFFER_SIZE {
        bail!("V1 CSR exceeds the response limit");
    }
    Ok(csr)
}

#[cfg(test)]
pub(crate) mod tests;
