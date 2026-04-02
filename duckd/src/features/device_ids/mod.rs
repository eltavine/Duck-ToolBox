use std::{
    ffi::CString,
    os::raw::{c_char, c_int, c_void},
    ptr,
    process::Command,
    slice,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use libloading::Library;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runtime::{
    errors::AppError,
    files::{create_unique_dir, write_bytes_atomic},
    paths::AppPaths,
};

const KM_TAG_ATTESTATION_ID_BRAND: u32 = 0x9000_02C6;
const KM_TAG_ATTESTATION_ID_DEVICE: u32 = 0x9000_02C7;
const KM_TAG_ATTESTATION_ID_PRODUCT: u32 = 0x9000_02C8;
const KM_TAG_ATTESTATION_ID_SERIAL: u32 = 0x9000_02C9;
const KM_TAG_ATTESTATION_ID_IMEI: u32 = 0x9000_02CA;
const KM_TAG_ATTESTATION_ID_MEID: u32 = 0x9000_02CB;
const KM_TAG_ATTESTATION_ID_MANUFACTURER: u32 = 0x9000_02CC;
const KM_TAG_ATTESTATION_ID_MODEL: u32 = 0x9000_02CD;

const CMD_GET_VERSION: u32 = 0x0200;
const CMD_SET_VERSION: u32 = 0x0207;
const CMD_PROVISION_DEVICE_IDS: u32 = 0x220A;
const CMD_SET_PROVISIONING_DEVICE_ID_SUCCESS: u32 = 0x2218;

const SHARED_BUF_SIZE: usize = 0xA000;
const DEFAULT_LIB_PATH: &str = "/vendor/lib64/libQSEEComAPI.so";
const DEFAULT_LIB_PATH_ALT: &str = "/vendor/lib64/hw/libQSEEComAPI.so";
const DEFAULT_TA_NAME: &str = "keymaster64";
const DEFAULT_TA_PATH: &str = "/vendor/firmware_mnt/image";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdsProfile {
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub product: String,
    #[serde(default)]
    pub serial: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub imei: String,
    #[serde(default)]
    pub imei2: String,
    #[serde(default)]
    pub meid: String,
    #[serde(default)]
    pub meid2: String,
    #[serde(default = "default_ta_name")]
    pub ta_name: String,
    #[serde(default = "default_ta_path")]
    pub ta_path: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvisionedId {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceIdsProvisionResult {
    pub count: usize,
    pub ids: Vec<ProvisionedId>,
    pub dry_run: bool,
    pub ta_name: String,
    pub ta_path: String,
    pub loaded_library: Option<String>,
    pub ta_api_version: Option<String>,
    pub ta_version: Option<String>,
    pub report_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct DeviceIdsReport {
    profile: DeviceIdsProfile,
    ids: Vec<ProvisionedId>,
    dry_run: bool,
    loaded_library: Option<String>,
    ta_api_version: Option<String>,
    ta_version: Option<String>,
    response_hex: Option<String>,
    command_hex: String,
}

#[derive(Clone, Copy)]
struct DeviceIdSpec {
    tag: u32,
    label: &'static str,
}

#[derive(Debug, Clone)]
struct SelectedId {
    tag: u32,
    label: &'static str,
    value: String,
}

#[derive(Debug, Clone, Copy)]
struct KmVersion {
    ta_api_major: u32,
    ta_api_minor: u32,
    ta_major: u32,
    ta_minor: u32,
}

#[derive(Debug)]
struct KmResponse {
    status: i32,
    data: Vec<u8>,
}

#[repr(C)]
struct QseeComHandle {
    ion_sbuffer: *mut u8,
}

type StartApp = unsafe extern "C" fn(
    *mut *mut QseeComHandle,
    *const c_char,
    *const c_char,
    u32,
) -> c_int;
type SendCmd =
    unsafe extern "C" fn(*mut QseeComHandle, *mut c_void, u32, *mut c_void, u32) -> c_int;
type ShutdownApp = unsafe extern "C" fn(*mut *mut QseeComHandle) -> c_int;

const REQUIRED_IDS: [DeviceIdSpec; 6] = [
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_BRAND,
        label: "BRAND",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_DEVICE,
        label: "DEVICE",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_PRODUCT,
        label: "PRODUCT",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_SERIAL,
        label: "SERIAL",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_MANUFACTURER,
        label: "MANUFACTURER",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_MODEL,
        label: "MODEL",
    },
];

const OPTIONAL_IDS: [DeviceIdSpec; 4] = [
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_IMEI,
        label: "IMEI",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_IMEI,
        label: "IMEI2",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_MEID,
        label: "MEID",
    },
    DeviceIdSpec {
        tag: KM_TAG_ATTESTATION_ID_MEID,
        label: "MEID2",
    },
];

pub fn detect_defaults() -> DeviceIdsProfile {
    let mut profile = DeviceIdsProfile::default();
    profile.brand = first_prop(&["ro.product.brand", "ro.product.vendor.brand"]);
    profile.device = first_prop(&["ro.product.device", "ro.product.vendor.device"]);
    profile.product = first_prop(&["ro.product.name", "ro.product.vendor.name"]);
    profile.serial = first_prop(&["ro.serialno", "ro.boot.serialno"]);
    profile.manufacturer = first_prop(&[
        "ro.product.manufacturer",
        "ro.product.vendor.manufacturer",
    ]);
    profile.model = first_prop(&["ro.product.model", "ro.product.vendor.model"]);
    profile.imei = first_prop(&[
        "persist.vendor.radio.imei",
        "persist.radio.imei",
        "vendor.ril.imei",
        "ril.gsm.imei",
        "ro.ril.oem.imei",
        "ro.ril.oem.imei1",
    ]);
    profile.imei2 = first_prop(&[
        "persist.vendor.radio.imei2",
        "persist.radio.imei2",
        "vendor.ril.imei2",
        "ril.gsm.imei2",
        "ro.ril.oem.imei2",
    ]);
    profile.meid = first_prop(&[
        "persist.vendor.radio.meid",
        "persist.radio.meid",
        "vendor.ril.meid",
        "ro.ril.oem.meid",
    ]);
    profile.meid2 = first_prop(&[
        "persist.vendor.radio.meid2",
        "persist.radio.meid2",
        "vendor.ril.meid2",
        "ro.ril.oem.meid2",
    ]);
    profile
}

pub fn provision(paths: &AppPaths, profile: DeviceIdsProfile) -> Result<DeviceIdsProvisionResult> {
    let resolved = merge_missing_from_system(profile);
    let ids = collect_ids(&resolved)?;
    let command = build_command(&ids)?;

    let mut loaded_library = None;
    let mut ta_api_version = None;
    let mut ta_version = None;
    let mut response_hex = None;

    if !resolved.dry_run {
        if !cfg!(target_os = "android") {
            bail!("device ID provisioning is only supported when Duck ToolBox runs on Android");
        }

        let api = QseecomApi::load()?;
        loaded_library = Some(api.loaded_path.clone());
        wait_listeners();

        let session = api.start_session(&resolved.ta_path, &resolved.ta_name)?;
        let version = session.get_version()?;
        ta_api_version = Some(format!("{}.{}", version.ta_api_major, version.ta_api_minor));
        ta_version = Some(format!("{}.{}", version.ta_major, version.ta_minor));
        session.set_version()?;

        let response = session.provision_device_ids(&command)?;
        if response.status != 0 {
            bail!("PROVISION_DEVICE_IDS failed with status {}", response.status);
        }
        if !response.data.is_empty() {
            response_hex = Some(hex::encode(&response.data));
        }

        session.set_success_marker()?;
    }

    let report = DeviceIdsReport {
        profile: resolved.clone(),
        ids: ids
            .iter()
            .map(|entry| ProvisionedId {
                label: entry.label.into(),
                value: entry.value.clone(),
            })
            .collect(),
        dry_run: resolved.dry_run,
        loaded_library: loaded_library.clone(),
        ta_api_version: ta_api_version.clone(),
        ta_version: ta_version.clone(),
        response_hex,
        command_hex: hex::encode(&command),
    };
    let report_path = write_report(paths, &report)?;

    Ok(DeviceIdsProvisionResult {
        count: ids.len(),
        ids: report.ids,
        dry_run: resolved.dry_run,
        ta_name: resolved.ta_name,
        ta_path: resolved.ta_path,
        loaded_library,
        ta_api_version,
        ta_version,
        report_path,
    })
}

impl Default for DeviceIdsProfile {
    fn default() -> Self {
        Self {
            brand: String::new(),
            device: String::new(),
            product: String::new(),
            serial: String::new(),
            manufacturer: String::new(),
            model: String::new(),
            imei: String::new(),
            imei2: String::new(),
            meid: String::new(),
            meid2: String::new(),
            ta_name: default_ta_name(),
            ta_path: default_ta_path(),
            dry_run: false,
        }
    }
}

fn default_ta_name() -> String {
    DEFAULT_TA_NAME.into()
}

fn default_ta_path() -> String {
    DEFAULT_TA_PATH.into()
}

fn merge_missing_from_system(mut profile: DeviceIdsProfile) -> DeviceIdsProfile {
    let detected = detect_defaults();

    fill_if_blank(&mut profile.brand, &detected.brand);
    fill_if_blank(&mut profile.device, &detected.device);
    fill_if_blank(&mut profile.product, &detected.product);
    fill_if_blank(&mut profile.serial, &detected.serial);
    fill_if_blank(&mut profile.manufacturer, &detected.manufacturer);
    fill_if_blank(&mut profile.model, &detected.model);
    fill_if_blank(&mut profile.imei, &detected.imei);
    fill_if_blank(&mut profile.imei2, &detected.imei2);
    fill_if_blank(&mut profile.meid, &detected.meid);
    fill_if_blank(&mut profile.meid2, &detected.meid2);
    fill_if_blank(&mut profile.ta_name, &detected.ta_name);
    fill_if_blank(&mut profile.ta_path, &detected.ta_path);

    profile
}

fn fill_if_blank(target: &mut String, fallback: &str) {
    if target.trim().is_empty() {
        *target = fallback.to_owned();
    }
}

fn collect_ids(profile: &DeviceIdsProfile) -> Result<Vec<SelectedId>> {
    let mut ids = Vec::new();

    for spec in REQUIRED_IDS {
        let value = match spec.label {
            "BRAND" => &profile.brand,
            "DEVICE" => &profile.device,
            "PRODUCT" => &profile.product,
            "SERIAL" => &profile.serial,
            "MANUFACTURER" => &profile.manufacturer,
            "MODEL" => &profile.model,
            _ => "",
        };

        let trimmed = value.trim();
        if trimmed.is_empty() {
            let field = match spec.label {
                "BRAND" => "brand",
                "DEVICE" => "device",
                "PRODUCT" => "product",
                "SERIAL" => "serial",
                "MANUFACTURER" => "manufacturer",
                "MODEL" => "model",
                _ => "device_id",
            };
            return Err(AppError::MissingDeviceField(field).into());
        }

        ids.push(SelectedId {
            tag: spec.tag,
            label: spec.label,
            value: trimmed.to_owned(),
        });
    }

    for (spec, value) in OPTIONAL_IDS.into_iter().zip([
        profile.imei.as_str(),
        profile.imei2.as_str(),
        profile.meid.as_str(),
        profile.meid2.as_str(),
    ]) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        ids.push(SelectedId {
            tag: spec.tag,
            label: spec.label,
            value: trimmed.to_owned(),
        });
    }

    Ok(ids)
}

fn build_command(ids: &[SelectedId]) -> Result<Vec<u8>> {
    if ids.is_empty() {
        bail!("device ID provisioning requires at least one ID");
    }

    let mut payload = Vec::with_capacity(1024);
    encode_map_header(&mut payload, ids.len() + 1);
    encode_uint(&mut payload, 22);
    encode_uint(&mut payload, ids.len() as u32);

    for entry in ids {
        encode_tag_key(&mut payload, entry.tag);
        encode_bstr(&mut payload, entry.value.as_bytes())?;
    }

    let mut command = Vec::with_capacity(4 + payload.len());
    command.extend_from_slice(&CMD_PROVISION_DEVICE_IDS.to_ne_bytes());
    command.extend_from_slice(&payload);
    Ok(command)
}

fn encode_map_header(buf: &mut Vec<u8>, count: usize) {
    if count <= 23 {
        buf.push(0xA0 | count as u8);
    } else if count <= 0xFF {
        buf.push(0xB8);
        buf.push(count as u8);
    } else {
        buf.push(0xB9);
        buf.extend_from_slice(&(count as u16).to_be_bytes());
    }
}

fn encode_uint(buf: &mut Vec<u8>, value: u32) {
    if value <= 23 {
        buf.push(value as u8);
    } else if value <= 0xFF {
        buf.extend_from_slice(&[0x18, value as u8]);
    } else if value <= 0xFFFF {
        buf.push(0x19);
        buf.extend_from_slice(&(value as u16).to_be_bytes());
    } else {
        buf.push(0x1A);
        buf.extend_from_slice(&value.to_be_bytes());
    }
}

fn encode_nint(buf: &mut Vec<u8>, value: u32) {
    if value <= 23 {
        buf.push(0x20 | value as u8);
    } else if value <= 0xFF {
        buf.extend_from_slice(&[0x38, value as u8]);
    } else if value <= 0xFFFF {
        buf.push(0x39);
        buf.extend_from_slice(&(value as u16).to_be_bytes());
    } else {
        buf.push(0x3A);
        buf.extend_from_slice(&value.to_be_bytes());
    }
}

fn encode_tag_key(buf: &mut Vec<u8>, tag: u32) {
    let signed = tag as i32;
    if signed >= 0 {
        encode_uint(buf, signed as u32);
    } else {
        encode_nint(buf, (-1_i64 - signed as i64) as u32);
    }
}

fn encode_bstr(buf: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let len = bytes.len();
    if len <= 23 {
        buf.push(0x40 | len as u8);
    } else if len <= 0xFF {
        buf.extend_from_slice(&[0x58, len as u8]);
    } else if len <= 0xFFFF {
        buf.push(0x59);
        buf.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        return Err(anyhow!("device ID value is too large for Keymaster CBOR payload"));
    }
    buf.extend_from_slice(bytes);
    Ok(())
}

fn write_report(paths: &AppPaths, report: &DeviceIdsReport) -> Result<String> {
    let run_dir = create_unique_dir(&paths.outputs_dir, "device-ids")
        .with_context(|| format!("create report dir under {}", paths.outputs_dir.display()))?;
    let report_path = run_dir.join("device_ids_report.json");
    let bytes = serde_json::to_vec_pretty(&json!(report)).context("serialize device ID report")?;
    write_bytes_atomic(&report_path, &bytes)
        .with_context(|| format!("write {}", report_path.display()))?;
    Ok(report_path.display().to_string())
}

fn first_prop(keys: &[&str]) -> String {
    for key in keys {
        let value = get_prop(key);
        if !value.trim().is_empty() {
            return value;
        }
    }

    String::new()
}

fn get_prop(name: &str) -> String {
    let direct_value = command_stdout("getprop", [name]).unwrap_or_default();
    if !direct_value.trim().is_empty() || !cfg!(target_os = "android") {
        return direct_value;
    }

    if cfg!(target_os = "android") {
        let command = format!("getprop {name}");
        for su in ["su", "/system/bin/su"] {
            if let Some(value) = command_stdout(su, ["-c", command.as_str()]) {
                if !value.trim().is_empty() {
                    return value;
                }
            }
        }
    }

    direct_value
}

fn command_stdout<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Some(text)
}

fn wait_listeners() {
    for _ in 0..50 {
        if get_prop("vendor.sys.listeners.registered") == "true" {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

struct QseecomApi {
    _library: Library,
    start_app: StartApp,
    send_cmd: SendCmd,
    shutdown_app: ShutdownApp,
    loaded_path: String,
}

impl QseecomApi {
    fn load() -> Result<Self> {
        for candidate in [DEFAULT_LIB_PATH, DEFAULT_LIB_PATH_ALT] {
            let library = match unsafe { Library::new(candidate) } {
                Ok(library) => library,
                Err(_) => continue,
            };

            let start_app = unsafe {
                *library
                    .get::<StartApp>(b"QSEECom_start_app\0")
                    .context("resolve QSEECom_start_app")?
            };
            let send_cmd = unsafe {
                *library
                    .get::<SendCmd>(b"QSEECom_send_cmd\0")
                    .context("resolve QSEECom_send_cmd")?
            };
            let shutdown_app = unsafe {
                *library
                    .get::<ShutdownApp>(b"QSEECom_shutdown_app\0")
                    .context("resolve QSEECom_shutdown_app")?
            };

            return Ok(Self {
                _library: library,
                start_app,
                send_cmd,
                shutdown_app,
                loaded_path: candidate.into(),
            });
        }

        bail!(
            "failed to load QSEEComAPI from {} or {}",
            DEFAULT_LIB_PATH,
            DEFAULT_LIB_PATH_ALT
        )
    }

    fn start_session(&self, ta_path: &str, ta_name: &str) -> Result<QseecomSession<'_>> {
        let handle = self.try_start(ta_path, ta_name).or_else(|error| {
            if ta_name.trim() == DEFAULT_TA_NAME {
                self.try_start(ta_path, "keymaster").with_context(|| {
                    format!("fallback to keymaster after starting {ta_name}: {error}")
                })
            } else {
                Err(error)
            }
        })?;

        Ok(QseecomSession { api: self, handle })
    }

    fn try_start(&self, ta_path: &str, ta_name: &str) -> Result<*mut QseeComHandle> {
        let path = CString::new(ta_path.trim()).context("TA path contains NUL byte")?;
        let name = CString::new(ta_name.trim()).context("TA name contains NUL byte")?;
        let mut handle = ptr::null_mut();
        let status = unsafe {
            (self.start_app)(
                &mut handle,
                path.as_ptr(),
                name.as_ptr(),
                SHARED_BUF_SIZE as u32,
            )
        };
        if status != 0 || handle.is_null() {
            bail!("QSEECom_start_app failed with status {status}");
        }

        Ok(handle)
    }
}

struct QseecomSession<'a> {
    api: &'a QseecomApi,
    handle: *mut QseeComHandle,
}

impl QseecomSession<'_> {
    fn get_version(&self) -> Result<KmVersion> {
        let response = self.send_raw(&CMD_GET_VERSION.to_ne_bytes())?;
        if response.len() < 20 {
            bail!("GET_VERSION returned too little data");
        }

        let status = read_i32(&response, 0)?;
        if status != 0 {
            bail!("GET_VERSION failed with status {status}");
        }

        Ok(KmVersion {
            ta_api_major: read_u32(&response, 4)?,
            ta_api_minor: read_u32(&response, 8)?,
            ta_major: read_u32(&response, 12)?,
            ta_minor: read_u32(&response, 16)?,
        })
    }

    fn set_version(&self) -> Result<()> {
        let mut request = Vec::with_capacity(24);
        for value in [CMD_SET_VERSION, 4, 5, 4, 5, 0] {
            request.extend_from_slice(&value.to_ne_bytes());
        }
        let response = self.send_raw(&request)?;
        let status = read_i32(&response, 0)?;
        if status != 0 {
            bail!("SET_VERSION failed with status {status}");
        }
        Ok(())
    }

    fn provision_device_ids(&self, request: &[u8]) -> Result<KmResponse> {
        let response = self.send_raw(request)?;
        if response.len() < 8 {
            bail!("PROVISION_DEVICE_IDS returned too little data");
        }

        let status = read_i32(&response, 0)?;
        let data_len = read_u32(&response, 4)? as usize;
        let available = response.len().saturating_sub(8);
        let data = response[8..8 + data_len.min(available)].to_vec();
        Ok(KmResponse { status, data })
    }

    fn set_success_marker(&self) -> Result<()> {
        let response = self.send_raw(&CMD_SET_PROVISIONING_DEVICE_ID_SUCCESS.to_ne_bytes())?;
        let status = read_i32(&response, 0)?;
        if status != 0 {
            bail!("SET_PROVISIONING_DEVICE_ID_SUCCESS failed with status {status}");
        }
        Ok(())
    }

    fn send_raw(&self, request: &[u8]) -> Result<Vec<u8>> {
        let rsp_offset = align4(request.len());
        if rsp_offset >= SHARED_BUF_SIZE {
            bail!("request is too large for QSEECom shared buffer");
        }

        let buffer = unsafe {
            let sbuffer = (*self.handle).ion_sbuffer;
            if sbuffer.is_null() {
                bail!("QSEECom shared buffer is unavailable");
            }
            slice::from_raw_parts_mut(sbuffer, SHARED_BUF_SIZE)
        };
        buffer.fill(0);
        buffer[..request.len()].copy_from_slice(request);

        let response_len = (SHARED_BUF_SIZE - rsp_offset) as u32;
        let status = unsafe {
            (self.api.send_cmd)(
                self.handle,
                buffer.as_mut_ptr().cast(),
                request.len() as u32,
                buffer[rsp_offset..].as_mut_ptr().cast(),
                response_len,
            )
        };
        if status != 0 {
            bail!("QSEECom_send_cmd failed with status {status}");
        }

        Ok(buffer[rsp_offset..].to_vec())
    }
}

impl Drop for QseecomSession<'_> {
    fn drop(&mut self) {
        let mut handle = self.handle;
        unsafe {
            let _ = (self.api.shutdown_app)(&mut handle);
        }
    }
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let chunk = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| anyhow!("missing u32 at offset {offset}"))?;
    let array: [u8; 4] = chunk.try_into().map_err(|_| anyhow!("invalid u32 slice"))?;
    Ok(u32::from_ne_bytes(array))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
    let chunk = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| anyhow!("missing i32 at offset {offset}"))?;
    let array: [u8; 4] = chunk.try_into().map_err(|_| anyhow!("invalid i32 slice"))?;
    Ok(i32::from_ne_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::{DeviceIdsProfile, build_command, collect_ids, detect_defaults};

    #[test]
    fn collect_ids_requires_base_fields() {
        let error = collect_ids(&DeviceIdsProfile::default()).unwrap_err();
        assert!(error.to_string().contains("brand"));
    }

    #[test]
    fn build_command_contains_operation_id_prefix() {
        let profile = DeviceIdsProfile {
            brand: "google".into(),
            device: "husky".into(),
            product: "husky".into(),
            serial: "ABC123".into(),
            manufacturer: "Google".into(),
            model: "Pixel".into(),
            ..DeviceIdsProfile::default()
        };

        let ids = collect_ids(&profile).unwrap();
        let command = build_command(&ids).unwrap();

        assert_eq!(&command[..4], &0x220Au32.to_ne_bytes());
    }

    #[test]
    fn detect_defaults_keeps_ta_defaults() {
        let detected = detect_defaults();

        assert_eq!(detected.ta_name, "keymaster64");
        assert_eq!(detected.ta_path, "/vendor/firmware_mnt/image");
    }
}
