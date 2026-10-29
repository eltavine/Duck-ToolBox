//! Resolve guest symlinks relative to the guest root, never the Android root.
use crate::fail;
use anyhow::{Result, ensure};
use std::{
    collections::VecDeque,
    fs,
    path::{Component, Path, PathBuf},
};

pub fn resolve(root: &Path, path: &Path) -> Result<PathBuf> {
    let mut pending: VecDeque<_> = path
        .components()
        .filter(|c| !matches!(c, Component::RootDir))
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let mut relative = PathBuf::new();
    let mut links = 0;
    while let Some(component) = pending.pop_front() {
        if component == "." {
            continue;
        }
        if component == ".." {
            ensure!(
                relative.pop(),
                fail("invalid_archive", "symlink escapes guest root")
            );
            continue;
        }
        relative.push(component);
        match fs::symlink_metadata(root.join(&relative)) {
            Ok(meta) if meta.file_type().is_symlink() => {
                links += 1;
                ensure!(
                    links <= 40,
                    fail("invalid_archive", "too many guest symlinks")
                );
                let target = fs::read_link(root.join(&relative))?;
                relative.pop();
                if target.is_absolute() {
                    relative.clear();
                }
                for c in target
                    .components()
                    .filter(|c| !matches!(c, Component::RootDir))
                    .rev()
                {
                    pending.push_front(c.as_os_str().to_owned());
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(root.join(relative))
}
pub fn arm64_executable(path: &Path) -> Result<()> {
    use object::{Object, ObjectSegment};
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 64 << 20,
        fail("invalid_archive", "invalid guest executable")
    );
    let bytes = fs::read(path)?;
    let object = object::File::parse(bytes.as_slice())?;
    ensure!(
        object.architecture() == object::Architecture::Aarch64
            && object.is_64()
            && object.is_little_endian()
            && object.segments().next().is_some_and(|s| s.size() > 0),
        fail("invalid_archive", "rootfs requires ARM64 Linux executables")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o111 != 0,
            fail(
                "invalid_archive",
                "guest executable has no execute permission"
            )
        );
    }
    Ok(())
}
