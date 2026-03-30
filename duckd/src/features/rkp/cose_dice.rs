use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use anyhow::{Context, Result, anyhow};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hkdf::hmac::{Hmac, Mac};
use p256::{
    PublicKey as P256PublicKey, SecretKey as P256SecretKey, ecdh::diffie_hellman,
    ecdsa::SigningKey as P256SigningKey, elliptic_curve::sec1::ToEncodedPoint,
};
use serde::Serialize;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use crate::runtime::{errors::AppError, profile::DeviceInfo};

use super::cbor::{bytes, empty_map, encode, int, text};

pub const ALG_EDDSA: i128 = -8;
pub const ALG_ES256: i128 = -7;
pub const ALG_A256GCM: i128 = 3;
pub const ALG_HMAC_256: i128 = 5;
pub const ALG_ECDH_ES_HKDF_256: i128 = -25;
pub const CWT_ISSUER: i128 = 1;
pub const CWT_SUBJECT: i128 = 2;
pub const DICE_PROFILE_NAME: i128 = -4_670_554;
pub const DICE_SUBJECT_PUB_KEY: i128 = -4_670_552;
pub const DICE_KEY_USAGE: i128 = -4_670_553;
pub const RPC_CURVE_P256: i128 = 1;
pub const RPC_CURVE_25519: i128 = 2;
const ANDROID_DICE_PROFILE_VERSION: &str = "android.15";

#[derive(Debug, Clone)]
pub struct DeviceKeys {
    seed: [u8; 32],
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

#[derive(Debug, Clone, Serialize)]
pub struct EcKeyPair {
    #[serde(skip)]
    pub secret_key: P256SecretKey,
    #[serde(skip)]
    pub signing_key: P256SigningKey,
    #[serde(skip)]
    pub cose_public: ciborium::value::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtectedDataBundle {
    pub protected_data_bytes: Vec<u8>,
    pub protected_data_len: usize,
    pub ephemeral_public_hex: String,
    pub nonce_hex: String,
    #[serde(skip)]
    pub dice: ciborium::value::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CsrBundle {
    pub csr_bytes: Vec<u8>,
    pub protected_data_bytes: Vec<u8>,
    pub protected_data_len: usize,
}

impl DeviceKeys {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        Self {
            seed,
            signing_key,
            verifying_key,
        }
    }

    pub fn seed_hex(&self) -> String {
        hex::encode(self.seed)
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    pub fn sign(&self, payload: &[u8]) -> Vec<u8> {
        self.signing_key.sign(payload).to_bytes().to_vec()
    }

    pub fn cose_key(&self) -> ciborium::value::Value {
        ciborium::value::Value::Map(vec![
            (int(1), int(1)),
            (int(3), int(ALG_EDDSA)),
            (int(-1), int(6)),
            (int(-2), bytes(self.public_key_bytes().to_vec())),
        ])
    }
}

pub fn generate_ec_keypair() -> Result<EcKeyPair> {
    loop {
        let mut secret_bytes = [0_u8; 32];
        fill_random(&mut secret_bytes)?;

        if let Ok(secret_key) = P256SecretKey::from_slice(&secret_bytes) {
            let signing_key = P256SigningKey::from(secret_key.clone());
            let verifying_key = signing_key.verifying_key();
            let encoded = verifying_key.to_encoded_point(false);
            let x = encoded.x().context("missing P-256 x coordinate")?;
            let y = encoded.y().context("missing P-256 y coordinate")?;

            return Ok(EcKeyPair {
                secret_key,
                signing_key,
                cose_public: ciborium::value::Value::Map(vec![
                    (int(1), int(2)),
                    (int(3), int(ALG_ES256)),
                    (int(-1), int(1)),
                    (int(-2), bytes(x.to_vec())),
                    (int(-3), bytes(y.to_vec())),
                ]),
            });
        }
    }
}

pub fn build_csr(
    keys: &DeviceKeys,
    challenge: &[u8],
    keys_to_sign: &[ciborium::value::Value],
    eek_pub_bytes: &[u8],
    eek_key_id: &[u8],
    device_info: &DeviceInfo,
    eek_curve: i128,
) -> Result<CsrBundle> {
    match eek_curve {
        RPC_CURVE_25519 | RPC_CURVE_P256 => {}
        other => return Err(AppError::UnsupportedEekCurve(other).into()),
    }

    let csr_payload = encode(&ciborium::value::Value::Array(vec![
        int(3),
        text("keymint"),
        device_info_to_cbor(device_info)?,
        ciborium::value::Value::Array(keys_to_sign.to_vec()),
    ]))?;

    let protected = build_protected_data(
        keys,
        challenge,
        keys_to_sign,
        eek_pub_bytes,
        eek_key_id,
        device_info,
        eek_curve,
    )?;
    let signed_payload = encode(&ciborium::value::Value::Array(vec![
        bytes(challenge.to_vec()),
        bytes(csr_payload),
    ]))?;
    let signed_data = cose_sign1(
        |payload| Ok(keys.sign(payload)),
        vec![(int(1), int(ALG_EDDSA))],
        signed_payload,
    )?;

    let csr = ciborium::value::Value::Array(vec![int(1), empty_map(), protected.dice, signed_data]);
    let csr_bytes = encode(&csr)?;

    Ok(CsrBundle {
        csr_bytes,
        protected_data_bytes: protected.protected_data_bytes,
        protected_data_len: protected.protected_data_len,
    })
}

pub fn build_sig_structure(context: &str, protected: &[u8], payload: &[u8]) -> Result<Vec<u8>> {
    build_cose_structure(context, protected, &[], payload)
}

fn build_cose_structure(
    context: &str,
    protected: &[u8],
    external_aad: &[u8],
    payload: &[u8],
) -> Result<Vec<u8>> {
    encode(&ciborium::value::Value::Array(vec![
        text(context),
        bytes(protected.to_vec()),
        bytes(external_aad.to_vec()),
        bytes(payload.to_vec()),
    ]))
}

pub fn device_info_to_cbor(device_info: &DeviceInfo) -> Result<ciborium::value::Value> {
    let mut entries = vec![
        (text("brand"), text(device_info.brand.clone())),
        (text("manufacturer"), text(device_info.manufacturer.clone())),
        (text("product"), text(device_info.product.clone())),
        (text("model"), text(device_info.model.clone())),
        (text("device"), text(device_info.device.clone())),
        (text("vb_state"), text(device_info.vb_state.clone())),
        (
            text("bootloader_state"),
            text(device_info.bootloader_state.clone()),
        ),
        (
            text("vbmeta_digest"),
            bytes(hex::decode(
                device_info
                    .vbmeta_digest
                    .as_deref()
                    .ok_or(AppError::MissingDeviceField("vbmeta_digest"))?,
            )?),
        ),
        (
            text("system_patch_level"),
            int(i128::from(device_info.system_patch_level)),
        ),
        (
            text("boot_patch_level"),
            int(i128::from(device_info.boot_patch_level)),
        ),
        (
            text("vendor_patch_level"),
            int(i128::from(device_info.vendor_patch_level)),
        ),
        (
            text("security_level"),
            text(device_info.security_level.clone()),
        ),
        (text("fused"), int(i128::from(device_info.fused))),
    ];

    if !device_info.os_version.trim().is_empty() {
        entries.push((text("os_version"), text(device_info.os_version.clone())));
    }

    Ok(ciborium::value::Value::Map(entries))
}

fn build_dice_chain(keys: &DeviceKeys, device_info: &DeviceInfo) -> Result<ciborium::value::Value> {
    Ok(ciborium::value::Value::Array(vec![
        keys.cose_key(),
        build_dice_entry(keys, device_info)?,
    ]))
}

fn build_dice_entry(keys: &DeviceKeys, device_info: &DeviceInfo) -> Result<ciborium::value::Value> {
    let payload = encode(&ciborium::value::Value::Map(vec![
        (int(CWT_ISSUER), text(device_info.dice_issuer.clone())),
        (int(CWT_SUBJECT), text(device_info.dice_subject.clone())),
        (int(DICE_PROFILE_NAME), text(ANDROID_DICE_PROFILE_VERSION)),
        (int(DICE_SUBJECT_PUB_KEY), bytes(encode(&keys.cose_key())?)),
        (int(DICE_KEY_USAGE), bytes(vec![0x20])),
    ]))?;

    cose_sign1(
        |bytes| Ok(keys.sign(bytes)),
        vec![(int(1), int(ALG_EDDSA))],
        payload,
    )
}

fn build_protected_data(
    keys: &DeviceKeys,
    challenge: &[u8],
    keys_to_sign: &[ciborium::value::Value],
    eek_pub_bytes: &[u8],
    eek_key_id: &[u8],
    device_info: &DeviceInfo,
    eek_curve: i128,
) -> Result<ProtectedDataBundle> {
    let dice = build_dice_chain(keys, device_info)?;
    let keys_to_sign_cbor = encode(&ciborium::value::Value::Array(keys_to_sign.to_vec()))?;
    let mac_key = random_bytes_array::<32>()?;
    let keys_to_sign_mac = build_keys_to_sign_mac(&mac_key, &keys_to_sign_cbor)?;
    let signed_mac_protected =
        encode(&ciborium::value::Value::Map(vec![(int(1), int(ALG_EDDSA))]))?;
    let signed_mac_aad = encode(&ciborium::value::Value::Array(vec![
        bytes(challenge.to_vec()),
        device_info_to_cbor(device_info)?,
        bytes(keys_to_sign_mac),
    ]))?;
    let signed_mac_input = build_cose_structure(
        "Signature1",
        &signed_mac_protected,
        &signed_mac_aad,
        &mac_key,
    )?;
    let signed_mac = ciborium::value::Value::Array(vec![
        bytes(signed_mac_protected),
        empty_map(),
        bytes(mac_key.to_vec()),
        bytes(keys.sign(&signed_mac_input)),
    ]);
    let plaintext = encode(&ciborium::value::Value::Array(vec![
        signed_mac,
        dice.clone(),
    ]))?;

    if eek_key_id.is_empty() {
        return Err(anyhow!("EEK key identifier must not be empty"));
    }

    let (ephemeral_public, sender_cose_key, aes_key) =
        derive_transport_key(eek_curve, eek_pub_bytes)?;

    let mut nonce = [0_u8; 12];
    fill_random(&mut nonce)?;

    let protected = encode(&ciborium::value::Value::Map(vec![(
        int(1),
        int(ALG_A256GCM),
    )]))?;
    let aad = encode(&ciborium::value::Value::Array(vec![
        text("Encrypt"),
        bytes(protected.clone()),
        bytes(Vec::new()),
    ]))?;
    let cipher = Aes256Gcm::new_from_slice(&aes_key).context("create AES-256-GCM")?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow!("encrypt protected data"))?;

    let recipient_protected = encode(&ciborium::value::Value::Map(vec![(
        int(1),
        int(ALG_ECDH_ES_HKDF_256),
    )]))?;
    let recipient = ciborium::value::Value::Array(vec![
        bytes(recipient_protected),
        ciborium::value::Value::Map(vec![
            (int(-1), sender_cose_key),
            (int(4), bytes(eek_key_id.to_vec())),
        ]),
        ciborium::value::Value::Null,
    ]);
    let encrypt = ciborium::value::Value::Array(vec![
        bytes(protected),
        ciborium::value::Value::Map(vec![(int(5), bytes(nonce.to_vec()))]),
        bytes(ciphertext),
        ciborium::value::Value::Array(vec![recipient]),
    ]);
    let protected_data_bytes = encode(&encrypt)?;

    Ok(ProtectedDataBundle {
        protected_data_len: protected_data_bytes.len(),
        protected_data_bytes,
        ephemeral_public_hex: hex::encode(&ephemeral_public),
        nonce_hex: hex::encode(nonce),
        dice,
    })
}

fn cose_sign1<F>(
    sign: F,
    protected_map: Vec<(ciborium::value::Value, ciborium::value::Value)>,
    payload: Vec<u8>,
) -> Result<ciborium::value::Value>
where
    F: Fn(&[u8]) -> Result<Vec<u8>>,
{
    let protected = encode(&ciborium::value::Value::Map(protected_map))?;
    let signature_input = build_sig_structure("Signature1", &protected, &payload)?;

    Ok(ciborium::value::Value::Array(vec![
        bytes(protected),
        empty_map(),
        bytes(payload),
        bytes(sign(&signature_input)?),
    ]))
}

fn derive_transport_key(
    eek_curve: i128,
    eek_pub_bytes: &[u8],
) -> Result<(Vec<u8>, ciborium::value::Value, [u8; 32])> {
    match eek_curve {
        RPC_CURVE_25519 => derive_x25519_transport_key(eek_pub_bytes),
        RPC_CURVE_P256 => derive_p256_transport_key(eek_pub_bytes),
        other => Err(AppError::UnsupportedEekCurve(other).into()),
    }
}

fn derive_x25519_transport_key(
    server_pub_bytes: &[u8],
) -> Result<(Vec<u8>, ciborium::value::Value, [u8; 32])> {
    let mut secret_bytes = [0_u8; 32];
    fill_random(&mut secret_bytes)?;
    let ephemeral_secret = StaticSecret::from(secret_bytes);
    let ephemeral_public = X25519PublicKey::from(&ephemeral_secret);
    let client_pub_bytes = ephemeral_public.as_bytes();

    let server_pub = X25519PublicKey::from(
        <[u8; 32]>::try_from(server_pub_bytes)
            .map_err(|_| anyhow!("EEK public key must be 32 bytes"))?,
    );
    let shared = ephemeral_secret.diffie_hellman(&server_pub);
    let context = encode_kdf_context(client_pub_bytes, server_pub_bytes)?;
    let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut output = [0_u8; 32];
    hkdf.expand(&context, &mut output)
        .map_err(|_| anyhow!("expand HKDF"))?;
    Ok((
        client_pub_bytes.to_vec(),
        ciborium::value::Value::Map(vec![
            (int(1), int(1)),
            (int(-1), int(4)),
            (int(-2), bytes(client_pub_bytes.to_vec())),
        ]),
        output,
    ))
}

fn derive_p256_transport_key(
    server_pub_bytes: &[u8],
) -> Result<(Vec<u8>, ciborium::value::Value, [u8; 32])> {
    let ephemeral_secret = generate_p256_secret_key()?;
    let ephemeral_public = ephemeral_secret.public_key();
    let encoded = ephemeral_public.to_encoded_point(false);
    let x = encoded
        .x()
        .context("missing ephemeral P-256 x coordinate")?;
    let y = encoded
        .y()
        .context("missing ephemeral P-256 y coordinate")?;

    let mut client_pub_bytes = Vec::with_capacity(64);
    client_pub_bytes.extend_from_slice(x);
    client_pub_bytes.extend_from_slice(y);

    let server_public = parse_p256_public_key(server_pub_bytes)?;
    let shared = diffie_hellman(
        ephemeral_secret.to_nonzero_scalar(),
        server_public.as_affine(),
    );
    let context = encode_kdf_context(&client_pub_bytes, server_pub_bytes)?;
    let hkdf = Hkdf::<Sha256>::new(None, shared.raw_secret_bytes());
    let mut output = [0_u8; 32];
    hkdf.expand(&context, &mut output)
        .map_err(|_| anyhow!("expand HKDF"))?;

    Ok((
        client_pub_bytes,
        ciborium::value::Value::Map(vec![
            (int(1), int(2)),
            (int(-1), int(1)),
            (int(-2), bytes(x.to_vec())),
            (int(-3), bytes(y.to_vec())),
        ]),
        output,
    ))
}

fn encode_kdf_context(client_pub_bytes: &[u8], server_pub_bytes: &[u8]) -> Result<Vec<u8>> {
    encode(&ciborium::value::Value::Array(vec![
        int(ALG_A256GCM),
        ciborium::value::Value::Array(vec![
            text("client"),
            bytes(Vec::new()),
            bytes(client_pub_bytes.to_vec()),
        ]),
        ciborium::value::Value::Array(vec![
            text("server"),
            bytes(Vec::new()),
            bytes(server_pub_bytes.to_vec()),
        ]),
        ciborium::value::Value::Array(vec![int(256), bytes(Vec::new())]),
    ]))
}

fn generate_p256_secret_key() -> Result<P256SecretKey> {
    loop {
        let mut secret_bytes = [0_u8; 32];
        fill_random(&mut secret_bytes)?;

        if let Ok(secret_key) = P256SecretKey::from_slice(&secret_bytes) {
            return Ok(secret_key);
        }
    }
}

fn parse_p256_public_key(public_key_bytes: &[u8]) -> Result<P256PublicKey> {
    if public_key_bytes.len() != 64 {
        return Err(anyhow!(
            "P-256 EEK public key must be 64 bytes of x||y coordinates"
        ));
    }

    let mut sec1 = Vec::with_capacity(65);
    sec1.push(0x04);
    sec1.extend_from_slice(public_key_bytes);

    P256PublicKey::from_sec1_bytes(&sec1).map_err(|_| anyhow!("parse P-256 EEK public key"))
}

fn fill_random(bytes: &mut [u8]) -> Result<()> {
    getrandom::fill(bytes).map_err(|error| anyhow!("fill random bytes from OS RNG: {error}"))
}

fn random_bytes_array<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    fill_random(&mut bytes)?;
    Ok(bytes)
}

fn build_keys_to_sign_mac(mac_key: &[u8; 32], keys_to_sign_cbor: &[u8]) -> Result<Vec<u8>> {
    let mac_protected = encode(&ciborium::value::Value::Map(vec![(
        int(1),
        int(ALG_HMAC_256),
    )]))?;
    let mac_structure = build_cose_structure("MAC0", &mac_protected, &[], keys_to_sign_cbor)?;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(mac_key).context("create HMAC-SHA256")?;
    mac.update(&mac_structure);
    Ok(mac.finalize().into_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::{
        DICE_PROFILE_NAME, DeviceKeys, RPC_CURVE_25519, RPC_CURVE_P256, build_csr,
        build_sig_structure, device_info_to_cbor, generate_ec_keypair,
    };
    use crate::runtime::profile::DeviceInfo;
    use p256::elliptic_curve::sec1::ToEncodedPoint;

    use super::super::cbor::{encode, map_get_text};
    use ciborium::value::Value;

    fn valid_device_info() -> DeviceInfo {
        DeviceInfo {
            brand: "google".into(),
            model: "Pixel".into(),
            device: "pixel".into(),
            product: "pixel".into(),
            manufacturer: "Google".into(),
            fused: 1,
            vb_state: "green".into(),
            os_version: "13".into(),
            security_level: "tee".into(),
            bootloader_state: "locked".into(),
            boot_patch_level: 20260101,
            system_patch_level: 202601,
            vendor_patch_level: 20260101,
            vbmeta_digest: Some("22".repeat(32)),
            dice_issuer: "CN=Android".into(),
            dice_subject: "CN=Android".into(),
        }
    }

    #[test]
    fn generated_p256_key_is_valid_cose_key() {
        let key = generate_ec_keypair().unwrap();
        let map = match key.cose_public {
            ciborium::value::Value::Map(map) => map,
            other => panic!("unexpected COSE key value: {other:?}"),
        };
        assert_eq!(map.len(), 5);
    }

    #[test]
    fn device_info_cbor_includes_expected_fields() {
        let value = device_info_to_cbor(&valid_device_info()).unwrap();
        let bytes = encode(&value).unwrap();
        assert!(!bytes.is_empty());
        let Value::Map(entries) = value else {
            panic!("device info must encode as a map");
        };
        assert!(map_get_text(&entries, "vbmeta_digest").is_some());
        assert!(map_get_text(&entries, "version").is_none());
    }

    #[test]
    fn device_info_cbor_omits_blank_os_version_for_strongbox() {
        let mut device = valid_device_info();
        device.security_level = "strongbox".into();
        device.os_version.clear();

        let value = device_info_to_cbor(&device).unwrap();
        let Value::Map(entries) = value else {
            panic!("device info must encode as a map");
        };
        assert!(
            !entries
                .iter()
                .any(|(key, _)| matches!(key, Value::Text(text) if text == "os_version"))
        );
    }

    #[test]
    fn built_csr_produces_bytes() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec = generate_ec_keypair().unwrap();
        let challenge = [0x22; 32];
        let bundle = build_csr(
            &keys,
            &challenge,
            &[ec.cose_public],
            &[0x33; 32],
            &[0x44; 8],
            &valid_device_info(),
            RPC_CURVE_25519,
        )
        .unwrap();
        assert!(!bundle.csr_bytes.is_empty());
        assert!(bundle.protected_data_len > 0);
    }

    #[test]
    fn build_csr_supports_p256_eek() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec = generate_ec_keypair().unwrap();
        let challenge = [0x22; 32];

        let server = generate_ec_keypair().unwrap();
        let encoded = server.secret_key.public_key().to_encoded_point(false);
        let mut server_pub = encoded.x().unwrap().to_vec();
        server_pub.extend_from_slice(encoded.y().unwrap());

        let bundle = build_csr(
            &keys,
            &challenge,
            &[ec.cose_public],
            &server_pub,
            &[0x55; 8],
            &valid_device_info(),
            RPC_CURVE_P256,
        )
        .unwrap();

        assert!(!bundle.csr_bytes.is_empty());
        assert!(bundle.protected_data_len > 0);
    }

    #[test]
    fn build_csr_rejects_unknown_eek_curve() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let ec = generate_ec_keypair().unwrap();
        let challenge = [0x22; 32];

        let error = build_csr(
            &keys,
            &challenge,
            &[ec.cose_public],
            &[0x33; 32],
            &[0x44; 8],
            &valid_device_info(),
            99,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unsupported EEK curve"));
    }

    #[test]
    fn signature_structure_matches_cose_shape() {
        let encoded = build_sig_structure("Signature1", &[1, 2], &[3, 4]).unwrap();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn dice_entry_payload_includes_android_profile_name() {
        let keys = DeviceKeys::from_seed([0x11; 32]);
        let device = valid_device_info();
        let Value::Array(chain) = super::build_dice_chain(&keys, &device).unwrap() else {
            panic!("DICE chain must be an array");
        };
        let Value::Array(entry) = &chain[1] else {
            panic!("DICE entry must be a COSE_Sign1 array");
        };
        let payload = match &entry[2] {
            Value::Bytes(bytes) => bytes,
            other => panic!("unexpected DICE payload wrapper: {other:?}"),
        };
        let decoded = super::super::cbor::decode(payload).unwrap();
        let Value::Map(entries) = decoded else {
            panic!("DICE payload must decode as a map");
        };
        let profile =
            super::super::cbor::map_get(&entries, DICE_PROFILE_NAME).and_then(
                |value| match value {
                    Value::Text(text) => Some(text.as_str()),
                    _ => None,
                },
            );

        assert_eq!(profile, Some("android.15"));
    }
}
