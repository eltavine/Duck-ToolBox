use crate::{fail, model::Identity, native};
use anyhow::{Result, ensure};
use std::{
    ffi::CString,
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    path::Path,
};

#[cfg(target_arch = "aarch64")]
const _: () = {
    assert!(libc::SYS_pidfd_open == 434);
    assert!(libc::SYS_pidfd_send_signal == 424);
    assert!(libc::SYS_unshare == 97);
    assert!(libc::SYS_setns == 268);
    assert!(libc::SYS_chroot == 51);
    assert!(libc::SYS_mount == 40);
    assert!(libc::SYS_prctl == 167);
};
pub fn pidfd(pid: u32) -> Result<File> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    ensure!(fd >= 0, io::Error::last_os_error());
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}
pub fn checked_pidfd(identity: &Identity) -> Result<File> {
    let file = pidfd(identity.pid)?;
    ensure!(
        native::alive(identity)?,
        fail("state_changed", "process identity changed")
    );
    Ok(file)
}
pub fn signal(file: &File, signal: i32) -> Result<()> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            file.as_raw_fd(),
            signal,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        ensure!(
            error.raw_os_error() == Some(libc::ESRCH) && exited(file),
            error
        );
    }
    Ok(())
}
pub fn exited(file: &File) -> bool {
    let mut event = libc::pollfd {
        fd: file.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe {
        libc::poll(&mut event, 1, 0) > 0 && event.revents & (libc::POLLIN | libc::POLLHUP) != 0
    }
}
pub fn isolate() -> Result<bool> {
    ensure!(
        unsafe { libc::unshare(libc::CLONE_NEWNS) } == 0,
        io::Error::last_os_error()
    );
    ensure!(
        unsafe {
            libc::mount(
                std::ptr::null(),
                c"/".as_ptr(),
                std::ptr::null(),
                (libc::MS_REC | libc::MS_PRIVATE) as _,
                std::ptr::null(),
            )
        } == 0,
        io::Error::last_os_error()
    );
    let pid_namespace = Path::new("/proc/self/ns/pid").try_exists()?;
    if pid_namespace {
        ensure!(
            unsafe { libc::unshare(libc::CLONE_NEWPID) } == 0,
            io::Error::last_os_error()
        );
    }
    Ok(pid_namespace)
}
pub fn enter(mount: &File, pid: Option<&File>) -> Result<()> {
    ensure!(
        unsafe { libc::setns(mount.as_raw_fd(), libc::CLONE_NEWNS) } == 0,
        io::Error::last_os_error()
    );
    if let Some(pid) = pid {
        ensure!(
            unsafe { libc::setns(pid.as_raw_fd(), libc::CLONE_NEWPID) } == 0,
            io::Error::last_os_error()
        );
    }
    Ok(())
}
pub struct ChildSetup {
    root: CString,
    proc: CString,
    parent: File,
    root_fd: Option<File>,
    scope: Option<File>,
}
impl ChildSetup {
    pub fn new(root: &Path) -> Result<Self> {
        use std::os::unix::ffi::OsStrExt;
        Ok(Self {
            root: CString::new(root.as_os_str().as_bytes())?,
            proc: CString::new(root.join("proc").as_os_str().as_bytes())?,
            parent: pidfd(std::process::id())?,
            root_fd: None,
            scope: None,
        })
    }
    pub fn from_root_fd(root: File) -> Result<Self> {
        let mut setup = Self::new(Path::new("/"))?;
        setup.root_fd = Some(root);
        Ok(setup)
    }
    pub fn in_scope(mut self, scope: Option<File>) -> Self {
        self.scope = scope;
        self
    }
    /// Called only from Command::pre_exec: no allocation, locks or Rust destructors.
    pub fn apply(&self, mount_proc: bool) -> io::Result<()> {
        let result = unsafe {
            if libc::prctl(
                libc::PR_SET_PDEATHSIG,
                libc::SIGKILL as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            ) != 0
            {
                return Err(io::Error::last_os_error());
            }
            let mut event = libc::pollfd {
                fd: self.parent.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            if libc::poll(&mut event, 1, 0) != 0 {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            if let Some(scope) = &self.scope
                && libc::write(scope.as_raw_fd(), c"0\n".as_ptr().cast(), 2) != 2
            {
                return Err(io::Error::last_os_error());
            }
            let mut signals = std::mem::zeroed::<libc::sigset_t>();
            libc::sigemptyset(&mut signals);
            if libc::sigprocmask(libc::SIG_SETMASK, &signals, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
            if mount_proc
                && libc::mount(
                    c"proc".as_ptr(),
                    self.proc.as_ptr(),
                    c"proc".as_ptr(),
                    (libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC) as _,
                    std::ptr::null(),
                ) != 0
            {
                return Err(io::Error::last_os_error());
            }
            let root = if let Some(file) = &self.root_fd {
                if libc::fchdir(file.as_raw_fd()) != 0 {
                    return Err(io::Error::last_os_error());
                }
                c".".as_ptr()
            } else {
                self.root.as_ptr()
            };
            if libc::chroot(root) != 0 {
                return Err(io::Error::last_os_error());
            }
            libc::chdir(c"/".as_ptr())
        };
        if result != 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
pub fn nonblocking(fd: i32) -> Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    ensure!(
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0,
        io::Error::last_os_error()
    );
    Ok(())
}
pub fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}
pub struct Signals(File);
impl Signals {
    pub fn new() -> Result<Self> {
        let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGTERM);
            libc::sigaddset(&mut set, libc::SIGINT);
        }
        let rc = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) };
        ensure!(rc == 0, io::Error::from_raw_os_error(rc));
        let fd = unsafe { libc::signalfd(-1, &set, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK) };
        ensure!(fd >= 0, io::Error::last_os_error());
        Ok(Self(unsafe { File::from_raw_fd(fd) }))
    }
    pub fn requested(&self) -> Result<bool> {
        let mut data = unsafe { std::mem::zeroed::<libc::signalfd_siginfo>() };
        let rc = unsafe {
            libc::read(
                self.0.as_raw_fd(),
                (&mut data as *mut libc::signalfd_siginfo).cast(),
                std::mem::size_of_val(&data),
            )
        };
        if rc < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock {
                return Ok(false);
            }
            return Err(e.into());
        }
        Ok(rc > 0)
    }
}
