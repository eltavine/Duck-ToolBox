//! The only kernel syscall boundary. Flags never bypass version/signature checks or force removal.

use anyhow::Result;
use duck_core::Sysroot;

use crate::ModuleError;

// ACK MODULE_NAME_LEN = 64 - sizeof(unsigned long), including the terminating NUL.
// Mutations are supported only on the 64-bit ABI; browsing can expose longer sysfs names.
pub const MAX_NAME_BYTES: usize = 55;

// AArch64 Linux UAPI and Bionic syscall.S: syscall number in x8, arguments in x0-x5.
// Constants still come from libc, so another target can never reuse these numbers.
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const _: () = {
    assert!(libc::SYS_init_module == 105);
    assert!(libc::SYS_delete_module == 106);
    assert!(libc::SYS_finit_module == 273);
    assert!(std::mem::size_of::<libc::c_long>() == 8);
};

pub fn supported(sysroot: &Sysroot) -> bool {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        sysroot.root() == std::path::Path::new("/") && unsafe { libc::geteuid() } == 0
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = sysroot;
        false
    }
}

pub fn load(bytes: &[u8], parameters: &str) -> Result<()> {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        let parameters = std::ffi::CString::new(parameters)?;
        // bytes and parameters remain live for the entire syscall; no force flags exist here.
        let result = unsafe {
            libc::syscall(
                libc::SYS_init_module,
                bytes.as_ptr(),
                bytes.len(),
                parameters.as_ptr(),
            )
        };
        if result != 0 {
            return Err(
                ModuleError::KernelRejected(std::io::Error::last_os_error().to_string()).into(),
            );
        }
        Ok(())
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = (bytes, parameters);
        Err(ModuleError::Unsupported.into())
    }
}

pub fn unload(name: &str) -> Result<()> {
    #[cfg(all(target_os = "android", target_arch = "aarch64"))]
    {
        let name = std::ffi::CString::new(name)?;
        // O_NONBLOCK refuses busy modules; O_TRUNC (forced removal) is never set.
        let result = unsafe {
            libc::syscall(
                libc::SYS_delete_module,
                name.as_ptr(),
                libc::c_long::from(libc::O_NONBLOCK),
            )
        };
        if result != 0 {
            return Err(
                ModuleError::KernelRejected(std::io::Error::last_os_error().to_string()).into(),
            );
        }
        Ok(())
    }
    #[cfg(not(all(target_os = "android", target_arch = "aarch64")))]
    {
        let _ = name;
        Err(ModuleError::Unsupported.into())
    }
}
