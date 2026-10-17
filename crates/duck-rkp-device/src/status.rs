use duck_core::Context;
use duck_platform::{exec, props};
use serde::Serialize;

use crate::soc::{self, Detection, State};

pub(crate) const LIBRARY_PATH: &str = "/vendor/lib64/libkeymasterdeviceutils.so";
pub(crate) const DEVICE_PATH: &str = "/dev/smcinvoke";

/// The limitations every run reports. The CLI requires an explicit acknowledgement of
/// both, and the WebUI shows them in a dialog before a request is sent.
pub(crate) const LIMITATIONS: [&str; 2] = ["partial_device_support", "rkp_only"];

/// What the device offers, without sending anything to KeyMint or to a server.
///
/// Passing these checks only means the prerequisites are present: firmware support and
/// server acceptance stay unconfirmed until a run succeeds.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeviceStatus {
    pub prerequisites_met: bool,
    pub android_arm64: bool,
    pub root: bool,
    pub library_present: bool,
    pub device_present: bool,
    pub qualcomm_soc: bool,
    pub library_path: &'static str,
    pub device_path: &'static str,
    pub fingerprint: String,
    pub soc_manufacturer: String,
    pub soc_detection: Detection,
    pub limitations: [&'static str; 2],
    pub blockers: Vec<&'static str>,
}

pub(crate) fn detect(ctx: &Context) -> DeviceStatus {
    let properties = props::all();
    let fingerprint = properties
        .get("ro.build.fingerprint")
        .cloned()
        .unwrap_or_default();
    let soc_manufacturer = properties
        .get("ro.soc.manufacturer")
        .cloned()
        .unwrap_or_default();

    let android_arm64 = cfg!(all(target_os = "android", target_arch = "aarch64"));
    let root = exec::stdout_or_empty("id", &["-u"]).trim() == "0";
    let library_present = ctx.sysroot.path(LIBRARY_PATH).is_file();
    let device_present = ctx.sysroot.path(DEVICE_PATH).exists();
    let soc_detection = soc::detect(&properties, &ctx.sysroot);
    let qualcomm_soc = soc_detection.state == State::Qualcomm;

    let mut blockers = Vec::new();
    for (passed, name) in [
        (android_arm64, "android_arm64_required"),
        (root, "root_required"),
        (library_present, "keymint_library_missing"),
        (device_present, "smcinvoke_missing"),
        (!fingerprint.trim().is_empty(), "fingerprint_missing"),
    ] {
        if !passed {
            blockers.push(name);
        }
    }
    if let Some(blocker) = soc_detection.blocker() {
        blockers.push(blocker);
    }

    DeviceStatus {
        prerequisites_met: blockers.is_empty(),
        android_arm64,
        root,
        library_present,
        device_present,
        qualcomm_soc,
        library_path: LIBRARY_PATH,
        device_path: DEVICE_PATH,
        fingerprint,
        soc_manufacturer,
        soc_detection,
        limitations: LIMITATIONS,
        blockers,
    }
}

#[cfg(test)]
mod tests {
    use super::LIMITATIONS;

    #[test]
    fn every_limitation_has_a_stable_id() {
        assert_eq!(LIMITATIONS, ["partial_device_support", "rkp_only"]);
    }
}
