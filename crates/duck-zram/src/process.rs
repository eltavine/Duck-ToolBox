//! Observe worker exit without reaping its PID until its process group is terminated.
use std::io;

#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const _: () = assert!(libc::SYS_waitid == 95);

pub fn exited(pid: u32) -> io::Result<bool> {
    #[cfg(unix)]
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        if libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        ) != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info.si_pid() != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
pub fn kill_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}
