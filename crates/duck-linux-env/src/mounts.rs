use crate::{fail, model::Config};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
pub const TINI_SHA256: &str = "eae1d3aa50c48fb23b8cbdf4e369d0910dfc538566bfd09df89a774aa84a48b9";
pub fn helper(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && meta.len() <= 4 << 20,
        fail("runtime_unavailable", "invalid init helper")
    );
    let bytes = fs::read(path)?;
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == TINI_SHA256,
        fail("runtime_unavailable", "init helper integrity check failed")
    );
    Ok(bytes)
}
#[cfg(any(target_os = "android", target_os = "linux"))]
pub fn setup(root: &Path, config: &Config, init: &[u8], pid_namespace: bool) -> Result<()> {
    use crate::{guest_path, scripts};
    use std::{
        ffi::CString,
        os::unix::{
            ffi::OsStrExt,
            fs::{PermissionsExt, symlink},
        },
    };
    fn mount(
        source: Option<&Path>,
        target: &Path,
        kind: Option<&str>,
        flags: libc::c_ulong,
        data: Option<&str>,
    ) -> Result<()> {
        let source = source
            .map(|p| CString::new(p.as_os_str().as_bytes()))
            .transpose()?;
        let target = CString::new(target.as_os_str().as_bytes())?;
        let kind = kind.map(CString::new).transpose()?;
        let data = data.map(CString::new).transpose()?;
        let rc = unsafe {
            libc::mount(
                source.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                target.as_ptr(),
                kind.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                flags,
                data.as_ref()
                    .map_or(std::ptr::null(), |s| s.as_ptr().cast()),
            )
        };
        ensure!(rc == 0, std::io::Error::last_os_error());
        Ok(())
    }
    ensure!(
        crate::store::directory(root)?,
        fail("invalid_archive", "rootfs must be a real directory")
    );
    for name in ["dev", "proc", "sys", "run", "tmp"] {
        let path = root.join(name);
        if !path.try_exists()? {
            fs::create_dir(&path)?;
        }
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            fail(
                "invalid_archive",
                format!("{name} must be a real directory")
            )
        );
    }
    mount(Some(root), root, None, libc::MS_BIND, None)?;
    for (name, mode) in [
        ("dev", "mode=0755,size=16m"),
        ("run", "mode=0755,size=16m"),
        ("tmp", "mode=1777,size=128m"),
    ] {
        mount(
            None,
            &root.join(name),
            Some("tmpfs"),
            libc::MS_NOSUID | libc::MS_NODEV,
            Some(mode),
        )?;
    }
    // /dev contains only explicitly created character devices, never Android block devices.
    mount(
        None,
        &root.join("dev"),
        None,
        libc::MS_REMOUNT | libc::MS_NOSUID,
        Some("mode=0755,size=16m"),
    )?;
    for (name, major, minor) in [
        ("null", 1, 3),
        ("zero", 1, 5),
        ("random", 1, 8),
        ("urandom", 1, 9),
        ("tty", 5, 0),
    ] {
        let path = root.join("dev").join(name);
        let target = CString::new(path.as_os_str().as_bytes())?;
        ensure!(
            unsafe {
                libc::mknod(
                    target.as_ptr(),
                    libc::S_IFCHR | 0o666,
                    libc::makedev(major, minor),
                )
            } == 0,
            std::io::Error::last_os_error()
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o666))?;
    }
    fs::create_dir(root.join("dev/pts"))?;
    mount(
        None,
        &root.join("dev/pts"),
        Some("devpts"),
        libc::MS_NOSUID | libc::MS_NOEXEC,
        Some("newinstance,ptmxmode=0666,mode=0620"),
    )?;
    for (name, target) in [
        ("ptmx", "pts/ptmx"),
        ("fd", "/proc/self/fd"),
        ("stdin", "/proc/self/fd/0"),
        ("stdout", "/proc/self/fd/1"),
        ("stderr", "/proc/self/fd/2"),
    ] {
        symlink(target, root.join("dev").join(name))?;
    }
    mount(
        Some(Path::new("/sys")),
        &root.join("sys"),
        None,
        libc::MS_BIND,
        None,
    )?;
    mount(
        None,
        &root.join("sys"),
        None,
        libc::MS_BIND
            | libc::MS_REMOUNT
            | libc::MS_RDONLY
            | libc::MS_NOSUID
            | libc::MS_NODEV
            | libc::MS_NOEXEC,
        None,
    )?;
    if !pid_namespace {
        mount(
            Some(Path::new("/proc")),
            &root.join("proc"),
            None,
            libc::MS_BIND,
            None,
        )?;
        mount(
            None,
            &root.join("proc"),
            None,
            libc::MS_BIND
                | libc::MS_REMOUNT
                | libc::MS_RDONLY
                | libc::MS_NOSUID
                | libc::MS_NODEV
                | libc::MS_NOEXEC,
            None,
        )?;
    }
    scripts::write(root, config)?;
    fs::write(root.join("run/duck-toolbox/tini"), init)?;
    fs::set_permissions(
        root.join("run/duck-toolbox/tini"),
        fs::Permissions::from_mode(0o755),
    )?;
    if !config.dns.is_empty() {
        let target = guest_path::resolve(root, Path::new("/etc/resolv.conf"))?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if !target.try_exists()? {
            fs::write(&target, [])?;
        }
        ensure!(
            fs::symlink_metadata(&target)?.is_file(),
            fail("invalid_archive", "resolver target is not a regular file")
        );
        let source = root.join("run/duck-toolbox/resolv.conf");
        fs::write(
            &source,
            config
                .dns
                .iter()
                .map(|ip| format!("nameserver {ip}\n"))
                .collect::<String>(),
        )?;
        mount(Some(&source), &target, None, libc::MS_BIND, None)?;
        mount(
            None,
            &target,
            None,
            libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY,
            None,
        )?;
    }
    Ok(())
}
#[cfg(not(any(target_os = "android", target_os = "linux")))]
pub fn setup(root: &Path, config: &Config, init: &[u8], pid_namespace: bool) -> Result<()> {
    let _ = (root, config, init, pid_namespace);
    Err(fail("unsupported_environment", "Linux mounts required"))
}
