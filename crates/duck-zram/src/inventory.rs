use crate::{
    capabilities, fail,
    model::{
        Device, MAX_DEVICES, Stats, Status, Swap, valid_algorithm, valid_boot_id, valid_device,
    },
};
use anyhow::{Context as _, Result, ensure};
use duck_core::Sysroot;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

pub fn read(path: &Path) -> Result<String> {
    let mut text = String::new();
    fs::File::open(path)?
        .take(262_145)
        .read_to_string(&mut text)?;
    ensure!(
        text.len() <= 262_144,
        fail("state_unavailable", "kernel file exceeds read limit")
    );
    Ok(text)
}
fn number(root: &Path, file: &str) -> Result<u64> {
    read(&root.join(file))?
        .trim()
        .parse()
        .map_err(|_| fail("state_unavailable", format!("invalid {file}")))
}
pub fn counters(text: &str, unit: u64) -> std::collections::BTreeMap<String, u64> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let key = fields.next()?.trim_end_matches(':');
            let value = fields.next()?.parse::<u64>().ok()?.checked_mul(unit)?;
            Some((key.to_owned(), value))
        })
        .collect()
}

pub fn parse_swaps(text: &str) -> Result<Vec<Swap>> {
    let mut lines = text.lines();
    ensure!(
        lines.next().is_some_and(|h| h
            .split_whitespace()
            .eq(["Filename", "Type", "Size", "Used", "Priority"])),
        fail("state_unavailable", "invalid /proc/swaps header")
    );
    let mut swaps = Vec::new();
    for line in lines {
        let f: Vec<_> = line.split_whitespace().collect();
        ensure!(
            f.len() == 5 && f[0].starts_with('/'),
            fail("state_unavailable", "invalid /proc/swaps entry")
        );
        let bytes = |v: &str| -> Result<u64> {
            v.parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(1024))
                .ok_or_else(|| fail("state_unavailable", "invalid swap size"))
        };
        ensure!(
            matches!(f[1], "partition" | "file") && bytes(f[3])? <= bytes(f[2])?,
            fail("state_unavailable", "invalid swap type or usage")
        );
        let path = decode_path(f[0])?;
        swaps.push(Swap {
            path,
            kind: f[1].into(),
            size: bytes(f[2])?,
            used: bytes(f[3])?,
            priority: f[4].parse().context("invalid swap priority")?,
            identity: None,
        });
        ensure!(
            swaps.len() <= 256,
            fail("state_unavailable", "too many swap entries")
        );
    }
    Ok(swaps)
}
fn decode_path(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            ensure!(
                i + 3 < bytes.len()
                    && bytes[i + 1..i + 4]
                        .iter()
                        .all(|n| (b'0'..=b'7').contains(n)),
                fail("state_unavailable", "invalid escaped swap path")
            );
            let value = (bytes[i + 1] - b'0') as u16 * 64
                + (bytes[i + 2] - b'0') as u16 * 8
                + (bytes[i + 3] - b'0') as u16;
            ensure!(
                value > 0 && value <= 255,
                fail("state_unavailable", "invalid escaped swap byte")
            );
            output.push(value as u8);
            i += 4;
        } else {
            output.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(output).map_err(|_| fail("state_unavailable", "swap path is not UTF-8"))
}
pub fn algorithms(text: &str) -> Result<(Vec<String>, String)> {
    let mut names = Vec::new();
    let mut selected = None;
    for word in text.split_whitespace() {
        let active = word.starts_with('[') && word.ends_with(']');
        let name = if active {
            &word[1..word.len() - 1]
        } else {
            word
        };
        ensure!(
            valid_algorithm(name) && !names.iter().any(|v| v == name),
            fail("state_unavailable", "invalid algorithm list")
        );
        if active {
            ensure!(
                selected.is_none(),
                fail("state_unavailable", "multiple active algorithms")
            );
            selected = Some(name.to_owned());
        }
        names.push(name.to_owned());
    }
    Ok((
        names,
        selected.ok_or_else(|| fail("state_unavailable", "active algorithm unavailable"))?,
    ))
}
#[cfg(unix)]
pub fn identity(path: &Path) -> Option<String> {
    block_identity(&fs::metadata(path).ok()?)
}
#[cfg(unix)]
pub fn block_identity(meta: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if !meta.file_type().is_block_device() {
        return None;
    }
    Some(format!(
        "{}:{}",
        libc::major(meta.rdev() as libc::dev_t),
        libc::minor(meta.rdev() as libc::dev_t)
    ))
}
#[cfg(not(unix))]
pub fn identity(_: &Path) -> Option<String> {
    None
}
#[cfg(not(unix))]
pub fn block_identity(_: &fs::Metadata) -> Option<String> {
    None
}

pub fn status(sys: &Sysroot) -> Result<Status> {
    let mut warnings = Vec::new();
    let mem = counters(&read(&sys.path("/proc/meminfo"))?, 1024);
    let total = *mem
        .get("MemTotal")
        .filter(|&&v| v > 0)
        .ok_or_else(|| fail("state_unavailable", "MemTotal unavailable"))?;
    let vm = read(&sys.path("/proc/vmstat"))
        .ok()
        .map(|t| counters(&t, 1))
        .unwrap_or_default();
    let boot_id = read(&sys.path("/proc/sys/kernel/random/boot_id"))?
        .trim()
        .to_owned();
    ensure!(
        valid_boot_id(&boot_id),
        fail("state_unavailable", "boot ID unavailable")
    );
    let mut swaps = parse_swaps(&read(&sys.path("/proc/swaps"))?)?;
    let mut swap_identity_known = true;
    for swap in &mut swaps {
        if swap.kind == "partition" {
            swap.identity = identity(&sys.path(&swap.path));
            if swap.identity.is_none() {
                swap_identity_known = false;
            }
        }
    }
    let mmd = capabilities::mmd_managed();
    let mut devices = Vec::new();
    let mut scanned = 0;
    for entry in fs::read_dir(sys.path("/sys/block"))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_device(&name) {
            continue;
        }
        scanned += 1;
        ensure!(
            scanned <= MAX_DEVICES,
            fail("state_unavailable", "device count exceeds limit")
        );
        match device(sys, &name, &boot_id, &swaps, mmd, swap_identity_known) {
            Ok(item) => devices.push(item),
            Err(e) => warnings.push(format!("{name}: {e}")),
        }
    }
    devices.sort_by_key(|v| v.name[4..].parse::<u32>().unwrap_or(0));
    Ok(Status {
        operations_supported: capabilities::supported(sys),
        mmd_managed: mmd,
        memory_total: total,
        memory_available: mem.get("MemAvailable").copied(),
        swap_in_pages: vm.get("pswpin").copied(),
        swap_out_pages: vm.get("pswpout").copied(),
        boot_id,
        devices,
        swaps,
        warnings,
    })
}
fn optional(root: &Path, file: &str) -> Result<Option<String>> {
    match read(&root.join(file)) {
        Ok(v) => Ok(Some(v)),
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
fn device(
    sys: &Sysroot,
    name: &str,
    boot: &str,
    swaps: &[Swap],
    mmd: Option<bool>,
    identities_known: bool,
) -> Result<Device> {
    let root = sys.path(format!("/sys/block/{name}"));
    let id = read(&root.join("dev"))?.trim().to_owned();
    let (major, minor) = id
        .split_once(':')
        .ok_or_else(|| fail("state_unavailable", "invalid device ID"))?;
    ensure!(
        major.parse::<u32>().is_ok() && minor.parse::<u32>().is_ok(),
        fail("state_unavailable", "invalid device ID")
    );
    let generation = generation(&root, boot, &id)?;
    let read_only = match number(&root, "ro")? {
        0 => false,
        1 => true,
        _ => return Err(fail("state_unavailable", "invalid block read-only state")),
    };
    let size = number(&root, "disksize")?;
    let initialized = match number(&root, "initstate")? {
        0 => false,
        1 => true,
        _ => return Err(fail("state_unavailable", "invalid initstate")),
    };
    ensure!(
        initialized == (size > 0),
        fail(
            "state_changed",
            "ZRAM initialization changed during inventory"
        )
    );
    let (algorithms, algorithm) = algorithms(&read(&root.join("comp_algorithm"))?)?;
    let mut blocked = Vec::new();
    if read_only {
        blocked.push("read_only".into());
    }
    let path = [format!("/dev/block/{name}"), format!("/dev/{name}")]
        .into_iter()
        .find(|p| identity(&sys.path(p)).as_deref() == Some(&id));
    if path.is_none() {
        blocked.push("device_node".into());
    }
    if !identities_known {
        blocked.push("swap_identity".into());
    }
    match mmd {
        Some(true) => blocked.push("mmd".into()),
        None => blocked.push("manager_unknown".into()),
        _ => {}
    }
    let swap = swaps
        .iter()
        .find(|s| s.identity.as_deref() == Some(&id))
        .cloned();
    ensure!(
        swap.is_none() || initialized,
        fail("state_changed", "active swap is not initialized")
    );
    if initialized && swap.is_none() {
        blocked.push("non_swap".into());
    }
    let backing = optional(&root, "backing_dev")?;
    if backing
        .as_deref()
        .is_some_and(|s| s.trim() != "none" && !s.trim().is_empty())
    {
        blocked.push("writeback".into());
    }
    let recomp = optional(&root, "recomp_algorithm")?;
    if recomp.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        blocked.push("recompression".into());
    }
    if fs::read_dir(root.join("holders"))?.next().is_some() {
        blocked.push("holders".into());
    }
    let mounts = read(&sys.path("/proc/self/mountinfo"))?;
    if mounts
        .lines()
        .any(|l| l.split_whitespace().nth(2) == Some(&id))
    {
        blocked.push("mounted".into());
    }
    let values = optional(&root, "mm_stat")?
        .map(|s| {
            s.split_whitespace()
                .map(str::parse::<u64>)
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .transpose()?;
    let stats = values
        .as_ref()
        .filter(|v| v.len() >= 4)
        .map(|v| Stats {
            original: Some(v[0]),
            compressed: Some(v[1]),
            memory_used: Some(v[2]),
            memory_limit: Some(v[3]),
        })
        .unwrap_or_default();
    if stats.memory_limit.is_none() {
        blocked.push("stats_unknown".into());
    }
    ensure!(
        generation == self::generation(&root, boot, &id)?
            && read(&root.join("dev"))?.trim() == id
            && size == number(&root, "disksize")?
            && u64::from(initialized) == number(&root, "initstate")?,
        fail("state_changed", "ZRAM device changed during inventory")
    );
    let revision = hex::encode(Sha256::digest(serde_json::to_vec(&(
        boot,
        name,
        &id,
        &generation,
        read_only,
        size,
        initialized,
        &algorithm,
        swap.as_ref().map(|s| s.priority),
        &backing,
        &recomp,
        stats.memory_limit,
    ))?));
    Ok(Device {
        name: name.into(),
        identity: id,
        generation,
        path,
        size,
        initialized,
        algorithms,
        algorithm: Some(algorithm),
        swap,
        stats,
        blocked,
        revision,
    })
}

pub(crate) fn generation(root: &Path, boot: &str, identity: &str) -> Result<String> {
    let sequence = optional(root, "diskseq")?
        .map(|s| s.trim().parse::<u64>())
        .transpose()?;
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(root)?;
        (metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let inode = (0u64, 0u64);
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        boot, identity, sequence, inode,
    ))?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_distinguishes_recreated_block_devices() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("diskseq"), "1\n").unwrap();
        let first = generation(t.path(), "boot", "254:0").unwrap();
        fs::write(t.path().join("diskseq"), "2\n").unwrap();
        assert_ne!(first, generation(t.path(), "boot", "254:0").unwrap());
        fs::write(t.path().join("diskseq"), "invalid").unwrap();
        assert!(generation(t.path(), "boot", "254:0").is_err());
    }
}
