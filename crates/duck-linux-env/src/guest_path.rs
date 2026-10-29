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
pub fn arm64_executable(path: &Path) -> Result<Vec<u8>> {
    use object::{Object, ObjectSegment};
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 64 << 20,
        fail("invalid_archive", "invalid guest executable")
    );
    let bytes = fs::read(path)?;
    let object = object::File::parse(bytes.as_slice())?;
    ensure!(
        object.format() == object::BinaryFormat::Elf
            && matches!(object.kind(), object::ObjectKind::Executable | object::ObjectKind::Dynamic)
            && object.architecture() == object::Architecture::Aarch64
            && object.is_64()
            && object.is_little_endian()
            && object.segments().any(|s| s.size() > 0 && matches!(s.flags(), object::SegmentFlags::Elf { p_flags, .. } if p_flags.contains(object::elf::PF_X))),
        fail("invalid_archive", "rootfs requires ARM64 Linux executables")
    );
    for segment in object.segments() {
        let (offset, size) = segment.file_range();
        ensure!(
            size <= segment.size()
                && (size == 0
                    || offset
                        .checked_add(size)
                        .is_some_and(|end| end <= bytes.len() as u64)),
            fail("invalid_archive", "invalid ELF load segment range")
        );
    }
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
    Ok(bytes)
}

pub fn validate_program(root: &Path, program: &Path) -> Result<()> {
    use object::read::elf::ProgramHeader;
    let bytes = arm64_executable(&resolve(root, program)?)?;
    let object::File::Elf64(image) = object::File::parse(bytes.as_slice())? else {
        return Err(fail("invalid_archive", "rootfs program must be ELF64"));
    };
    let mut interpreters = image
        .elf_program_headers()
        .iter()
        .filter(|header| header.p_type(image.endian()) == object::elf::PT_INTERP);
    if let Some(header) = interpreters.next() {
        ensure!(
            interpreters.next().is_none(),
            fail("invalid_archive", "duplicate ELF interpreters")
        );
        let path = header
            .data(image.endian(), bytes.as_slice())
            .map_err(|_| fail("invalid_archive", "ELF interpreter extends beyond its file"))?;
        ensure!(
            path.len() >= 2
                && path.len() <= 4096
                && path.last() == Some(&0)
                && !path[..path.len() - 1].contains(&0),
            fail("invalid_archive", "invalid ELF interpreter path")
        );
        #[cfg(unix)]
        let interpreter = {
            use std::os::unix::ffi::OsStrExt;
            Path::new(std::ffi::OsStr::from_bytes(&path[..path.len() - 1]))
        };
        #[cfg(not(unix))]
        let interpreter = Path::new(std::str::from_utf8(&path[..path.len() - 1])?);
        arm64_executable(&resolve(root, interpreter)?)?;
    }
    Ok(())
}
