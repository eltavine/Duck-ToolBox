use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use rustls::{ClientConfig, RootCertStore};
use serde::Serialize;
use serde_json::{Value, json};

use super::args::{
    ArtifactCommands, Commands, KeyboxArgs, ProfileCommands, ProvisionArgs, RkpCommands,
    SharedRunArgs, VerifyArgs,
};
use super::keybox_output::{KeyboxData, resolve_keybox_output_path};
use duckd::{
    features::rkp::{
        cose_dice::{DeviceKeys, build_csr, generate_ec_keypair},
        crypto_kdf::resolve_seed,
        http::{fetch_eek, submit_csr},
        keybox_xml::{
            CertificateChainSummary, build_keybox_xml, parse_der_cert_chain, summarize_chain,
        },
        verify::{VerifyReport, verify_csr},
    },
    runtime::{
        errors::AppError,
        files::{create_unique_dir, list_files_recursive, write_bytes_atomic},
        json_api,
        paths::AppPaths,
        profile::{
            ProfileData, ResolvedProfile, RunOverrides, clear_profile, resolve_profile,
            save_profile, show_profile,
        },
    },
};

type HandlerFailure = (anyhow::Error, Option<Value>);
pub type CommandFailure = (&'static str, anyhow::Error, Option<Value>);
pub type CommandResult = std::result::Result<Value, CommandFailure>;

const HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
const HTTP_REQUEST_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Serialize)]
struct InfoData {
    mode: String,
    seed_hex: String,
    ed25519_pubkey_hex: String,
    device: duckd::runtime::profile::DeviceInfo,
    fingerprint: String,
    server_url: String,
    num_keys: u32,
    output_path: String,
}

#[derive(Debug, Serialize)]
struct ArtifactFile {
    name: String,
    path: String,
    size: u64,
    modified_unix: u64,
}

#[derive(Debug, Serialize)]
struct ArtifactsData {
    outputs: Vec<ArtifactFile>,
    profile_path: String,
    profile_secrets_path: String,
    log_path: String,
}

#[derive(Debug, Serialize)]
struct ProvisionChain {
    index: usize,
    path: String,
    summary: CertificateChainSummary,
}

#[derive(Debug, Serialize)]
struct ProvisionData {
    mode: String,
    cdi_leaf_pubkey_hex: String,
    challenge_hex: String,
    csr_path: String,
    csr_len: usize,
    protected_data_len: usize,
    local_verify: VerifyReport,
    cert_chains: Vec<ProvisionChain>,
}

pub async fn dispatch(command: Commands, paths: &AppPaths) -> CommandResult {
    match command {
        Commands::Artifacts { command } => handle_artifacts_command(paths, command),
        Commands::Rkp { command } => handle_rkp_command(paths, command).await,
    }
}

fn handle_artifacts_command(paths: &AppPaths, command: ArtifactCommands) -> CommandResult {
    match command {
        ArtifactCommands::List(_args) => handle_artifacts(paths)
            .map(|data| json_api::success("artifacts.list", data))
            .map_err(|error| ("artifacts.list", error, None)),
    }
}

async fn handle_rkp_command(paths: &AppPaths, command: RkpCommands) -> CommandResult {
    match command {
        RkpCommands::Profile { command } => handle_profile(paths, command),
        RkpCommands::Info(args) => handle_info(paths, &args.shared)
            .map(|data| json_api::success("rkp.info", data))
            .map_err(|error| ("rkp.info", error, None)),
        RkpCommands::Provision(args) => handle_provision(paths, &args)
            .await
            .map(|data| json_api::success("rkp.provision", data))
            .map_err(|(error, details)| ("rkp.provision", error, details)),
        RkpCommands::Keybox(args) => handle_keybox(paths, &args)
            .await
            .map(|data| json_api::success("rkp.keybox", data))
            .map_err(|(error, details)| ("rkp.keybox", error, details)),
        RkpCommands::Verify(args) => handle_verify(paths, &args)
            .map(|data| json_api::success("rkp.verify", data))
            .map_err(|error| ("rkp.verify", error, None)),
    }
}

fn handle_profile(paths: &AppPaths, command: ProfileCommands) -> CommandResult {
    match command {
        ProfileCommands::Show(args) => show_profile(paths, args.profile.as_deref())
            .map(|profile| {
                json_api::success(
                    "rkp.profile.show",
                    json!({
                        "profile": profile,
                        "paths": paths,
                    }),
                )
            })
            .map_err(|error| ("rkp.profile.show", error, None)),
        ProfileCommands::Save(args) => {
            handle_profile_save(paths, args.profile.as_deref(), args.stdin_json)
                .map(|profile| {
                    json_api::success(
                        "rkp.profile.save",
                        json!({
                            "profile": profile,
                            "paths": paths,
                        }),
                    )
                })
                .map_err(|error| ("rkp.profile.save", error, None))
        }
        ProfileCommands::Clear(args) => clear_profile(paths, args.profile.as_deref())
            .map(|()| {
                json_api::success(
                    "rkp.profile.clear",
                    json!({
                        "cleared": true,
                        "paths": paths,
                    }),
                )
            })
            .map_err(|error| ("rkp.profile.clear", error, None)),
    }
}

fn handle_profile_save(
    paths: &AppPaths,
    profile_name: Option<&str>,
    stdin_json: bool,
) -> Result<ProfileData> {
    if !stdin_json {
        anyhow::bail!("profile save requires `--stdin-json`");
    }

    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("read profile JSON from stdin")?;

    let profile = serde_json::from_str::<ProfileData>(&input).context("parse profile JSON")?;
    save_profile(paths, profile_name, &profile)
}

fn handle_artifacts(paths: &AppPaths) -> Result<ArtifactsData> {
    paths.ensure_runtime_dirs()?;
    let mut outputs = Vec::new();

    for path in list_files_recursive(&paths.outputs_dir)? {
        let metadata = fs::metadata(&path).with_context(|| format!("read {}", path.display()))?;
        outputs.push(ArtifactFile {
            name: path
                .strip_prefix(&paths.outputs_dir)
                .unwrap_or(&path)
                .display()
                .to_string(),
            path: path.display().to_string(),
            size: metadata.len(),
            modified_unix: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0),
        });
    }

    outputs.sort_by(|left, right| right.modified_unix.cmp(&left.modified_unix));

    Ok(ArtifactsData {
        outputs,
        profile_path: paths.profile_path.display().to_string(),
        profile_secrets_path: paths.profile_secrets_path.display().to_string(),
        log_path: paths.log_path.display().to_string(),
    })
}

fn handle_info(paths: &AppPaths, args: &SharedRunArgs) -> Result<InfoData> {
    let resolved = resolve_runtime(paths, args, None, None)?;
    let seed = resolve_seed(&resolved.profile.key_source)?;
    let keys = DeviceKeys::from_seed(seed);

    Ok(InfoData {
        mode: resolved.profile.key_source.mode_label().into(),
        seed_hex: keys.seed_hex(),
        ed25519_pubkey_hex: keys.public_key_hex(),
        device: resolved.profile.device.clone(),
        fingerprint: resolved.profile.fingerprint.value.clone(),
        server_url: resolved.profile.server_url.clone(),
        num_keys: resolved.profile.num_keys,
        output_path: resolved.output_path.display().to_string(),
    })
}

async fn handle_provision(
    paths: &AppPaths,
    args: &ProvisionArgs,
) -> std::result::Result<ProvisionData, HandlerFailure> {
    let resolved =
        resolve_runtime(paths, &args.shared, args.num_keys, None).map_err(|error| (error, None))?;
    let server_url =
        ensure_rkp_request_context(&resolved.profile).map_err(|error| (error, None))?;
    let seed = resolve_seed(&resolved.profile.key_source).map_err(|error| (error, None))?;
    let keys = DeviceKeys::from_seed(seed);
    let client = build_http_client().map_err(|error| (error, None))?;

    let mut cose_pubs = Vec::new();
    for _ in 0..resolved.profile.num_keys {
        let pair = generate_ec_keypair().map_err(|error| (error, None))?;
        cose_pubs.push(pair.cose_public);
    }

    let eek = fetch_eek(&client, &resolved.profile.fingerprint.value, &server_url)
        .await
        .map_err(|error| (error, None))?;

    let csr = build_csr(
        &keys,
        &eek.challenge,
        &cose_pubs,
        &eek.eek_public,
        &eek.eek_id,
        &resolved.profile.device,
        eek.eek_curve,
    )
    .map_err(|error| (error, None))?;

    let run_dir = unique_output_dir(paths, "rkp-provision").map_err(|error| (error, None))?;
    let csr_path = run_dir.join("csr_output.cbor");
    write_bytes(
        &csr_path,
        &csr.csr_bytes,
        Some(json!({
            "csr_path": csr_path.display().to_string(),
        })),
    )?;

    let local_verify = verify_csr(&csr.csr_bytes).map_err(|error| {
        (
            error,
            Some(json!({
                "csr_path": csr_path.display().to_string(),
            })),
        )
    })?;

    let cert_chains = submit_csr(&client, &csr.csr_bytes, &eek.challenge, &server_url)
        .await
        .map_err(|error| {
            (
                error,
                Some(json!({
                    "csr_path": csr_path.display().to_string(),
                })),
            )
        })?;

    let mut chain_data = Vec::new();
    for (index, chain) in cert_chains.iter().enumerate() {
        let chain_path = run_dir.join(format!("cert_chain_{index}.der"));
        write_bytes(&chain_path, chain, None)?;
        let parsed = parse_der_cert_chain(chain).map_err(|error| (error, None))?;
        chain_data.push(ProvisionChain {
            index,
            path: chain_path.display().to_string(),
            summary: summarize_chain(&parsed),
        });
    }

    Ok(ProvisionData {
        mode: resolved.profile.key_source.mode_label().into(),
        cdi_leaf_pubkey_hex: keys.public_key_hex(),
        challenge_hex: eek.challenge_hex,
        csr_path: csr_path.display().to_string(),
        csr_len: csr.csr_bytes.len(),
        protected_data_len: csr.protected_data_len,
        local_verify,
        cert_chains: chain_data,
    })
}

async fn handle_keybox(
    paths: &AppPaths,
    args: &KeyboxArgs,
) -> std::result::Result<KeyboxData, HandlerFailure> {
    let resolved = resolve_runtime(paths, &args.shared, Some(1), args.output.clone())
        .map_err(|error| (error, None))?;
    let server_url =
        ensure_rkp_request_context(&resolved.profile).map_err(|error| (error, None))?;
    let seed = resolve_seed(&resolved.profile.key_source).map_err(|error| (error, None))?;
    let keys = DeviceKeys::from_seed(seed);
    let client = build_http_client().map_err(|error| (error, None))?;
    let ec_key = generate_ec_keypair().map_err(|error| (error, None))?;

    let eek = fetch_eek(&client, &resolved.profile.fingerprint.value, &server_url)
        .await
        .map_err(|error| (error, None))?;

    let csr = build_csr(
        &keys,
        &eek.challenge,
        &[ec_key.cose_public.clone()],
        &eek.eek_public,
        &eek.eek_id,
        &resolved.profile.device,
        eek.eek_curve,
    )
    .map_err(|error| (error, None))?;

    let keybox_path =
        resolve_keybox_output_path(paths, &resolved, args.output.is_some()).map_err(|error| {
            (
                error,
                Some(json!({
                    "output_path": resolved.output_path.display().to_string(),
                })),
            )
        })?;

    if let Some(parent) = keybox_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))
            .map_err(|error| (error, None))?;
    }

    let csr_path = keybox_path.with_extension("cbor");
    write_bytes(
        &csr_path,
        &csr.csr_bytes,
        Some(json!({
            "csr_path": csr_path.display().to_string(),
        })),
    )?;

    let cert_chains = submit_csr(&client, &csr.csr_bytes, &eek.challenge, &server_url)
        .await
        .map_err(|error| {
            (
                error,
                Some(json!({
                    "csr_path": csr_path.display().to_string(),
                })),
            )
        })?;

    let first_chain = cert_chains
        .first()
        .ok_or_else(|| anyhow!("RKP server returned no certificate chains"))
        .map_err(|error| {
            (
                error,
                Some(json!({
                    "csr_path": csr_path.display().to_string(),
                })),
            )
        })?;

    let parsed = parse_der_cert_chain(first_chain).map_err(|error| (error, None))?;
    let device_id =
        random_device_id(&resolved.profile.device.manufacturer).map_err(|error| (error, None))?;
    let xml =
        build_keybox_xml(&ec_key.secret_key, &parsed, &device_id).map_err(|error| (error, None))?;

    write_bytes(&keybox_path, xml.as_bytes(), None)?;

    Ok(KeyboxData {
        mode: resolved.profile.key_source.mode_label().into(),
        cdi_leaf_pubkey_hex: keys.public_key_hex(),
        challenge_hex: eek.challenge_hex,
        csr_path: csr_path.display().to_string(),
        keybox_path: keybox_path.display().to_string(),
        keybox_xml: xml,
        device_id,
        chain_summary: summarize_chain(&parsed),
    })
}

fn handle_verify(paths: &AppPaths, args: &VerifyArgs) -> Result<Value> {
    let file = paths.resolve_in_root(&args.csr_file);
    let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
    let report = verify_csr(&bytes)?;

    Ok(json!({
        "path": file.display().to_string(),
        "report": report,
    }))
}

fn resolve_runtime(
    paths: &AppPaths,
    shared: &SharedRunArgs,
    num_keys: Option<u32>,
    output_path: Option<String>,
) -> Result<ResolvedProfile> {
    resolve_profile(
        paths,
        &RunOverrides {
            profile_name: shared.profile.clone(),
            seed_hex: shared.seed.clone(),
            hw_key_hex: shared.hw_key.clone(),
            kdf_label: shared.kdf_label.clone(),
            server_url: shared.server_url.clone(),
            num_keys,
            output_path,
        },
    )
}

fn build_http_client() -> Result<Client> {
    let builder = Client::builder()
        .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS))
        .user_agent(format!("duck-toolbox/{}", env!("CARGO_PKG_VERSION")));

    let builder = if cfg!(target_os = "android") {
        builder.tls_backend_preconfigured(embedded_rustls_client_config())
    } else {
        builder
    };

    builder.build().context("build HTTP client")
}

fn embedded_rustls_client_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    config
}

fn ensure_rkp_request_context(profile: &ProfileData) -> Result<String> {
    if profile.fingerprint.value.trim().is_empty() {
        return Err(AppError::MissingFingerprint.into());
    }

    validate_device_info_for_request(&profile.device)?;

    Ok(profile.server_url.trim().to_owned())
}

fn random_device_id(manufacturer: &str) -> Result<String> {
    let mut suffix = [0_u8; 6];
    getrandom::fill(&mut suffix)
        .map_err(|error| anyhow!("fill device id random bytes: {error}"))?;
    let manufacturer = manufacturer.trim();
    let manufacturer = if manufacturer.is_empty() {
        "generic"
    } else {
        manufacturer
    };
    Ok(format!("{manufacturer}-{}", hex::encode(suffix)))
}

fn write_bytes(
    path: &Path,
    bytes: &[u8],
    details: Option<Value>,
) -> std::result::Result<(), HandlerFailure> {
    write_bytes_atomic(path, bytes).map_err(|error| (error, details))
}

fn unique_output_dir(paths: &AppPaths, prefix: &str) -> Result<PathBuf> {
    create_unique_dir(&paths.outputs_dir, prefix)
}

fn validate_device_info_for_request(device: &duckd::runtime::profile::DeviceInfo) -> Result<()> {
    for (field, value) in [
        ("brand", device.brand.as_str()),
        ("model", device.model.as_str()),
        ("device", device.device.as_str()),
        ("product", device.product.as_str()),
        ("manufacturer", device.manufacturer.as_str()),
        ("vb_state", device.vb_state.as_str()),
        ("security_level", device.security_level.as_str()),
        ("bootloader_state", device.bootloader_state.as_str()),
        ("dice_issuer", device.dice_issuer.as_str()),
        ("dice_subject", device.dice_subject.as_str()),
    ] {
        require_device_text(field, value)?;
    }

    validate_device_choice(
        "security_level",
        &device.security_level,
        &["tee", "strongbox"],
    )?;
    validate_device_choice("vb_state", &device.vb_state, &["green", "yellow", "orange"])?;
    validate_device_choice(
        "bootloader_state",
        &device.bootloader_state,
        &["locked", "unlocked"],
    )?;
    validate_os_version(device)?;

    if device.fused > 1 {
        return Err(AppError::InvalidDeviceField {
            field: "fused",
            reason: "expected 0 or 1".into(),
        }
        .into());
    }

    validate_patch_level("boot_patch_level", device.boot_patch_level, 8)?;
    validate_patch_level("system_patch_level", device.system_patch_level, 6)?;
    validate_patch_level("vendor_patch_level", device.vendor_patch_level, 8)?;

    let vbmeta_digest = device
        .vbmeta_digest
        .as_deref()
        .ok_or(AppError::MissingDeviceField("vbmeta_digest"))?;
    let decoded = hex::decode(vbmeta_digest).map_err(|_| AppError::InvalidDeviceField {
        field: "vbmeta_digest",
        reason: "expected 32-byte hexadecimal data".into(),
    })?;
    if decoded.len() != 32 {
        return Err(AppError::InvalidDeviceField {
            field: "vbmeta_digest",
            reason: "expected 32-byte hexadecimal data".into(),
        }
        .into());
    }

    Ok(())
}

fn require_device_text(field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::MissingDeviceField(field).into());
    }

    Ok(())
}

fn validate_device_choice(field: &'static str, value: &str, allowed: &[&str]) -> Result<()> {
    if allowed.iter().any(|candidate| *candidate == value) {
        return Ok(());
    }

    Err(AppError::InvalidDeviceField {
        field,
        reason: format!("expected one of {}", allowed.join(", ")),
    }
    .into())
}

fn validate_patch_level(field: &'static str, value: u32, _digits: usize) -> Result<()> {
    let encoded = value.to_string();
    if value > 0 && matches!(encoded.len(), 6 | 8) && is_valid_patch_level_date(&encoded) {
        return Ok(());
    }

    Err(AppError::InvalidDeviceField {
        field,
        reason: format!(
            "expected a valid patch level in YYYYMM or YYYYMMDD form; Android currently accepts both"
        ),
    }
    .into())
}

fn is_valid_patch_level_date(value: &str) -> bool {
    let normalized = match value.len() {
        6 => format!("{value}01"),
        8 => value.to_owned(),
        _ => return false,
    };

    let year = match normalized[0..4].parse::<u32>() {
        Ok(year) => year,
        Err(_) => return false,
    };
    let month = match normalized[4..6].parse::<u32>() {
        Ok(month @ 1..=12) => month,
        _ => return false,
    };
    let day = match normalized[6..8].parse::<u32>() {
        Ok(day) => day,
        Err(_) => return false,
    };

    day >= 1 && day <= days_in_month(year, month)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn validate_os_version(device: &duckd::runtime::profile::DeviceInfo) -> Result<()> {
    let value = device.os_version.trim();
    if value.is_empty() {
        if device.security_level == "strongbox" {
            return Ok(());
        }
        return Err(AppError::MissingDeviceField("os_version").into());
    }

    Ok(())
}

#[cfg(test)]
mod tls_tests {
    use super::{embedded_rustls_client_config, validate_device_info_for_request};
    use duckd::runtime::profile::DeviceInfo;

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
            vbmeta_digest: Some("11".repeat(32)),
            dice_issuer: "CN=Android".into(),
            dice_subject: "CN=Android".into(),
        }
    }

    #[test]
    fn embedded_rustls_config_sets_application_protocols() {
        let config = embedded_rustls_client_config();

        assert_eq!(
            config.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );
    }

    #[test]
    fn validate_device_info_rejects_red_vb_state() {
        let mut device = valid_device_info();
        device.vb_state = "red".into();

        let error = validate_device_info_for_request(&device).unwrap_err();
        assert!(error.to_string().contains("vb_state"));
    }

    #[test]
    fn validate_device_info_requires_vbmeta_digest() {
        let mut device = valid_device_info();
        device.vbmeta_digest = None;

        let error = validate_device_info_for_request(&device).unwrap_err();
        assert!(error.to_string().contains("vbmeta_digest"));
    }

    #[test]
    fn validate_device_info_requires_os_version_for_tee() {
        let mut device = valid_device_info();
        device.os_version.clear();

        let error = validate_device_info_for_request(&device).unwrap_err();
        assert!(error.to_string().contains("os_version"));
    }

    #[test]
    fn validate_device_info_allows_blank_os_version_for_strongbox() {
        let mut device = valid_device_info();
        device.security_level = "strongbox".into();
        device.os_version.clear();

        validate_device_info_for_request(&device).unwrap();
    }

    #[test]
    fn validate_device_info_accepts_six_digit_boot_patch_level() {
        let mut device = valid_device_info();
        device.boot_patch_level = 202601;

        validate_device_info_for_request(&device).unwrap();
    }

    #[test]
    fn validate_device_info_rejects_invalid_patch_level_date() {
        let mut device = valid_device_info();
        device.vendor_patch_level = 20261301;

        let error = validate_device_info_for_request(&device).unwrap_err();
        assert!(error.to_string().contains("vendor_patch_level"));
    }
}
