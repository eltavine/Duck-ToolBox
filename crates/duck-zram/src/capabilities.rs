use duck_core::Sysroot;
pub fn supported(sys: &Sysroot) -> bool {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        sys.root() == std::path::Path::new("/") && unsafe { libc::geteuid() } == 0
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = sys;
        false
    }
}
pub fn mmd_managed() -> Option<bool> {
    #[cfg(target_os = "android")]
    {
        // Bionic reads the property area directly; no child process can outlive the worker.
        property(c"ro.build.version.sdk").filter(|s| !s.is_empty())?;
        mmd_state(
            &property(c"init.svc.mmd")?,
            &property(c"mmd.enabled_aconfig")?,
            &property(c"mmd.zram.enabled")?,
        )
    }
    #[cfg(not(target_os = "android"))]
    {
        let raw = duck_platform::exec::stdout("getprop", &[]).ok()?;
        let props = duck_platform::props::parse_listing(&raw);
        if props.is_empty() {
            return None;
        }
        mmd_state(
            props.get("init.svc.mmd").map_or("", String::as_str),
            props.get("mmd.enabled_aconfig").map_or("", String::as_str),
            props.get("mmd.zram.enabled").map_or("", String::as_str),
        )
    }
}
#[cfg(target_os = "android")]
fn property(name: &std::ffi::CStr) -> Option<String> {
    let mut value = [0 as libc::c_char; libc::PROP_VALUE_MAX as usize];
    let length = unsafe { libc::__system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    if length < 0 || length as usize >= value.len() {
        return None;
    }
    let bytes: Vec<_> = value[..length as usize].iter().map(|&c| c as u8).collect();
    String::from_utf8(bytes).ok()
}
fn mmd_state(service: &str, enabled: &str, zram: &str) -> Option<bool> {
    if matches!(service, "running" | "restarting" | "stopping") {
        return Some(true);
    }
    if !matches!(service, "" | "stopped") {
        return None;
    }
    fn boolean(value: &str) -> Option<bool> {
        match value {
            "1" | "y" | "yes" | "on" | "true" => Some(true),
            "" | "0" | "n" | "no" | "off" | "false" => Some(false),
            _ => None,
        }
    }
    Some(boolean(enabled)? && boolean(zram)?)
}
#[cfg(test)]
mod tests {
    use super::mmd_state;
    #[test]
    fn android_boolean_spellings_cannot_bypass_mmd_protection() {
        for value in ["1", "y", "yes", "on", "true"] {
            assert_eq!(mmd_state("stopped", "true", value), Some(true));
        }
        for value in ["0", "n", "no", "off", "false", ""] {
            assert_eq!(mmd_state("stopped", "true", value), Some(false));
        }
        assert_eq!(mmd_state("running", "", ""), Some(true));
        assert_eq!(mmd_state("stopped", "true", "unexpected"), None);
        assert_eq!(mmd_state("unknown", "", ""), None);
    }
}
