//! Android ABI, sysfs writes and the platform swap formatter stay at this boundary.
use crate::{
    fail, inventory,
    model::{Config, Device},
};
use anyhow::{Context as _, Result, ensure};
use duck_core::Sysroot;
use std::{fs, io::Write, path::Path};

#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const _: () = {
    assert!(libc::SYS_swapon == 224);
    assert!(libc::SYS_swapoff == 225);
    assert!(std::mem::size_of::<libc::c_long>() == 8);
};
pub fn supported(sys: &Sysroot) -> bool {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        sys.root() == Path::new("/") && unsafe { libc::geteuid() } == 0
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = sys;
        false
    }
}
pub fn mmd_managed() -> Option<bool> {
    let tool = if cfg!(target_os = "android") {
        "/system/bin/getprop"
    } else {
        "getprop"
    };
    let props = duck_platform::exec::stdout(tool, &[]).ok()?;
    let props = duck_platform::props::parse_listing(&props);
    Some(
        props
            .get("init.svc.mmd")
            .is_some_and(|v| matches!(v.as_str(), "running" | "restarting"))
            || (props
                .get("mmd.enabled_aconfig")
                .is_some_and(|v| v == "true")
                && props.get("mmd.zram.enabled").is_some_and(|v| v == "true")),
    )
}
pub trait Operations {
    fn swapoff(&mut self) -> Result<()>;
    fn reset(&mut self) -> Result<()>;
    fn algorithm(&mut self, value: &str) -> Result<()>;
    fn size(&mut self, bytes: u64) -> Result<()>;
    fn limit(&mut self, bytes: u64) -> Result<()>;
    fn format(&mut self) -> Result<()>;
    fn swapon(&mut self, priority: Option<u16>) -> Result<()>;
}
pub struct Android<'a> {
    pub device: &'a Device,
}
impl Android<'_> {
    fn path(&self) -> Result<&str> {
        let path = self
            .device
            .path
            .as_deref()
            .ok_or_else(|| fail("state_unavailable", "device node unavailable"))?;
        ensure!(
            inventory::identity(Path::new(path)).as_deref() == Some(&self.device.identity),
            fail("state_changed", "block device identity changed")
        );
        Ok(path)
    }
    fn write(&self, attribute: &str, value: &str) -> Result<()> {
        self.path()?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(format!("/sys/block/{}/{attribute}", self.device.name))
            .with_context(|| format!("open {}/{attribute}", self.device.name))?;
        file.write_all(value.as_bytes())
            .with_context(|| format!("write {}/{attribute}", self.device.name))?;
        Ok(())
    }
}
impl Operations for Android<'_> {
    fn swapoff(&mut self) -> Result<()> {
        swap(self.path()?, None)
    }
    fn reset(&mut self) -> Result<()> {
        self.write("reset", "1")
    }
    fn algorithm(&mut self, value: &str) -> Result<()> {
        self.write("comp_algorithm", value)
    }
    fn size(&mut self, bytes: u64) -> Result<()> {
        self.write("disksize", &bytes.to_string())
    }
    fn limit(&mut self, bytes: u64) -> Result<()> {
        self.write("mem_limit", &bytes.to_string())
    }
    fn format(&mut self) -> Result<()> {
        let current = inventory::status(&Sysroot::default())?;
        let device = current
            .devices
            .iter()
            .find(|d| d.name == self.device.name)
            .ok_or_else(|| fail("state_changed", "device disappeared"))?;
        ensure!(
            device.swap.is_none() && device.blocked.iter().all(|b| b == "non_swap"),
            fail("state_changed", "device acquired another user")
        );
        let status = std::process::Command::new("/system/bin/mkswap")
            .arg(self.path()?)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| {
                fail(
                    "format_failed",
                    format!("platform mkswap: {e}; device may remain inactive"),
                )
            })?;
        ensure!(
            status.success(),
            fail(
                "format_failed",
                format!("platform mkswap exited with {status}; device may remain inactive")
            )
        );
        Ok(())
    }
    fn swapon(&mut self, priority: Option<u16>) -> Result<()> {
        swap(self.path()?, Some(priority))
    }
}
fn swap(path: &str, activate: Option<Option<u16>>) -> Result<()> {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        let name = std::ffi::CString::new(path)?;
        // libc supplies the target ABI. Only SWAP_FLAG_PREFER and its priority bits are set.
        let rc = unsafe {
            match activate {
                None => libc::swapoff(name.as_ptr()),
                Some(priority) => {
                    libc::swapon(name.as_ptr(), priority.map_or(0, |p| 0x8000 | i32::from(p)))
                }
            }
        };
        ensure!(
            rc == 0,
            fail(
                "kernel_rejected",
                format!(
                    "{}: {}; refresh device state",
                    if activate.is_some() {
                        "swapon"
                    } else {
                        "swapoff"
                    },
                    std::io::Error::last_os_error()
                )
            )
        );
        Ok(())
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = (path, activate);
        Err(fail(
            "unsupported_environment",
            "mutations require Android ARM64 root",
        ))
    }
}
pub fn preflight(device: &Device, config: &Config) -> Result<()> {
    ensure!(
        formatter_available() || config.size_bytes == 0,
        fail("formatter_unavailable", "platform mkswap unavailable")
    );
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    ensure!(
        page > 0 && config.size_bytes.is_multiple_of(page as u64),
        fail(
            "invalid_input",
            "size must be aligned to the running kernel page size"
        )
    );
    ensure!(device.path.as_ref().is_some_and(|p| inventory::identity(Path::new(p)).as_deref() == Some(&device.identity)), fail("state_changed", "device node identity changed"));
    Ok(())
}
pub fn terminate_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

fn formatter_available() -> bool {
    let Ok(metadata) = fs::metadata("/system/bin/mkswap") else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}
