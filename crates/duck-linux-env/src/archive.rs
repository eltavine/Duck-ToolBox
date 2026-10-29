use crate::{
    fail, guest_path,
    model::{MAX_ARCHIVE, MAX_ENTRIES, MAX_EXPANDED},
    store,
};
use anyhow::{Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path},
    time::{Duration, Instant},
};

#[derive(Serialize)]
pub struct Inspection {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
pub fn inspect(path: &Path) -> Result<Inspection> {
    let mut source = source(path)?;
    let bytes = source.metadata()?.len();
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 131072];
    let mut read = 0;
    loop {
        let size = source.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        read += size as u64;
        ensure!(
            read <= MAX_ARCHIVE,
            fail("invalid_archive", "archive exceeds 4 GiB")
        );
        hash.update(&buffer[..size]);
    }
    ensure!(
        read == bytes,
        fail("state_changed", "archive changed during inspection")
    );
    Ok(Inspection {
        path: path.to_string_lossy().into_owned(),
        bytes,
        sha256: hex::encode(hash.finalize()),
    })
}
fn source(path: &Path) -> Result<File> {
    ensure!(
        path.is_absolute(),
        fail("invalid_input", "archive path must be absolute")
    );
    let file = store::open(path, false)?;
    let size = file.metadata()?.len();
    ensure!(
        size > 0 && size <= MAX_ARCHIVE,
        fail(
            "invalid_archive",
            "archive must be between 1 byte and 4 GiB"
        )
    );
    Ok(file)
}
pub fn snapshot(
    path: &Path,
    destination: &Path,
    expected: &str,
    mut check: impl FnMut(u64) -> Result<()>,
) -> Result<File> {
    let mut input = source(path)?;
    let mut output = File::create(destination)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 131072];
    let mut bytes = 0;
    loop {
        check(bytes)?;
        let size = input.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        bytes += size as u64;
        ensure!(
            bytes <= MAX_ARCHIVE,
            fail("invalid_archive", "archive grew beyond size limit")
        );
        hash.update(&buffer[..size]);
        output.write_all(&buffer[..size])?;
    }
    ensure!(
        hex::encode(hash.finalize()).eq_ignore_ascii_case(expected),
        fail("state_changed", "archive hash differs from inspection")
    );
    output.sync_all()?;
    drop(output);
    Ok(File::open(destination)?)
}
fn reader(file: File, interrupt: &dyn Fn() -> Result<()>) -> Result<Box<dyn Read + '_>> {
    let mut file = file;
    file.rewind()?;
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic)?;
    file.rewind()?;
    let input: Box<dyn Read> = if magic == [0x1f, 0x8b] {
        Box::new(flate2::read::MultiGzDecoder::new(file))
    } else {
        Box::new(file)
    };
    Ok(Box::new(CheckedReader {
        input,
        bytes: 0,
        interrupt,
        last: Instant::now() - Duration::from_secs(1),
    }))
}
struct CheckedReader<'a> {
    input: Box<dyn Read>,
    bytes: u64,
    interrupt: &'a dyn Fn() -> Result<()>,
    last: Instant,
}
impl Read for CheckedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.last.elapsed() >= Duration::from_millis(100) {
            (self.interrupt)().map_err(|error| {
                std::io::Error::other(crate::Error {
                    code: crate::code_of(&error),
                    message: format!("{error:#}"),
                })
            })?;
            self.last = Instant::now();
        }
        let limit = buffer.len().min(131072);
        let count = self.input.read(&mut buffer[..limit])?;
        self.bytes += count as u64;
        if self.bytes > MAX_EXPANDED + MAX_ENTRIES * 1024 {
            return Err(std::io::Error::other(
                "decompressed archive stream exceeds limit",
            ));
        }
        Ok(count)
    }
}
fn relative(path: &Path) -> bool {
    !path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
}
pub fn extract(
    file: &mut File,
    destination: &Path,
    interrupt: &dyn Fn() -> Result<()>,
    mut check: impl FnMut(&str, u64, u64) -> Result<()>,
) -> Result<(u64, u64)> {
    // A raw pass bounds extension headers before tar allocates long-path/PAX data.
    let mut raw = tar::Archive::new(reader(file.try_clone()?, interrupt)?);
    let mut count = 0u64;
    let mut bytes = 0u64;
    for entry in raw.entries()?.raw(true) {
        let entry = entry?;
        count += 1;
        ensure!(
            count <= MAX_ENTRIES,
            fail("invalid_archive", "too many archive entries")
        );
        let kind = entry.header().entry_type();
        let size = entry.size();
        let extension = kind.is_gnu_longname()
            || kind.is_gnu_longlink()
            || kind.is_pax_local_extensions()
            || kind.is_pax_global_extensions();
        ensure!(
            (extension && size <= 65536) || (!extension && size <= MAX_ARCHIVE),
            fail("invalid_archive", "oversized archive entry or extension")
        );
        ensure!(
            extension
                || kind.is_file()
                || kind.is_dir()
                || kind.is_symlink()
                || kind.is_hard_link(),
            fail(
                "invalid_archive",
                "device nodes, sparse files and special entries are unsupported"
            )
        );
        bytes = bytes
            .checked_add(size)
            .ok_or_else(|| fail("invalid_archive", "archive size overflow"))?;
        ensure!(
            bytes <= MAX_EXPANDED,
            fail("invalid_archive", "expanded archive exceeds 16 GiB")
        );
        check("validating", count, bytes)?;
    }
    drain_padding(raw.into_inner())?;
    file.seek(SeekFrom::Start(0))?;
    let mut archive = tar::Archive::new(reader(file.try_clone()?, interrupt)?);
    archive.set_preserve_permissions(true);
    archive.set_preserve_ownerships(cfg!(target_os = "android"));
    let mut entries = 0;
    let mut expanded = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        entries += 1;
        expanded = expanded
            .checked_add(entry.size())
            .ok_or_else(|| fail("invalid_archive", "archive size overflow"))?;
        check("extracting", entries, expanded)?;
        ensure!(
            entries <= MAX_ENTRIES
                && entry.size() <= MAX_ARCHIVE
                && expanded <= MAX_EXPANDED
                && relative(&entry.path()?),
            fail(
                "invalid_archive",
                "archive path or expanded size is invalid"
            )
        );
        if entry.header().entry_type().is_hard_link() {
            ensure!(
                entry.link_name()?.is_some_and(|p| relative(&p)),
                fail("invalid_archive", "invalid hard link target")
            );
        }
        ensure!(
            entry.unpack_in(destination)?,
            fail(
                "invalid_archive",
                "entry could not be confined to the rootfs"
            )
        );
    }
    drain_padding(archive.into_inner())?;
    for executable in ["/bin/sh", "/bin/sleep", "/bin/cat"] {
        guest_path::arm64_executable(&guest_path::resolve(destination, Path::new(executable))?)?;
    }
    Ok((entries, expanded))
}
fn drain_padding(mut reader: impl Read) -> Result<()> {
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            buffer[..count].iter().all(|b| *b == 0),
            fail("invalid_archive", "unexpected data after tar archive")
        );
    }
    Ok(())
}

#[cfg(test)]
pub fn validate_extract(file: &mut File, destination: &Path) -> Result<(u64, u64)> {
    extract(file, destination, &|| Ok(()), |_, _, _| Ok(()))
}
