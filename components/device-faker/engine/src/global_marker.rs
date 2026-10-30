//! A durable ownership marker for volatile global-property backups.
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub fn path(root: &Path) -> PathBuf {
    root.join("data/adb/duck-device-faker/global-active")
}
#[derive(Debug)]
struct Owner {
    boot: String,
    pid: u32,
    start: u64,
    generation: String,
}
fn boot(root: &Path) -> io::Result<String> {
    let mut text = String::new();
    File::open(root.join("proc/sys/kernel/random/boot_id"))?
        .take(65)
        .read_to_string(&mut text)?;
    let value = text.trim();
    if value.is_empty() || value.len() > 64 || value.contains('\n') {
        return Err(invalid());
    }
    Ok(value.into())
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "global ownership marker is invalid",
    )
}
fn read(root: &Path) -> io::Result<Option<Owner>> {
    let file = match File::open(path(root)) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut text = String::new();
    file.take(1025).read_to_string(&mut text)?;
    if text.len() > 1024 {
        return Err(invalid());
    }
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 4 || lines[0].is_empty() || lines[3].is_empty() || lines[3].len() > 256 {
        return Err(invalid());
    }
    let pid = lines[1].parse::<u32>().map_err(|_| invalid())?;
    let start = lines[2].parse::<u64>().map_err(|_| invalid())?;
    if pid < 2 || start == 0 {
        return Err(invalid());
    }
    Ok(Some(Owner {
        boot: lines[0].into(),
        pid,
        start,
        generation: lines[3].into(),
    }))
}
pub fn encode(root: &Path, generation: &str) -> io::Result<String> {
    if generation.is_empty() || generation.len() > 256 || generation.contains(['\n', '\r']) {
        return Err(invalid());
    }
    let pid = std::process::id();
    let start = crate::process_identity::start_at(root, pid).ok_or_else(invalid)?;
    Ok(format!("{}\n{pid}\n{start}\n{generation}\n", boot(root)?))
}
pub fn permits(root: &Path, generation: &str) -> bool {
    match read(root) {
        Ok(None) => true,
        Ok(Some(owner)) => {
            boot(root).is_ok_and(|boot| boot == owner.boot)
                && owner.generation == generation
                && crate::process_identity::start_at(root, owner.pid) == Some(owner.start)
        }
        Err(_) => false,
    }
}
#[allow(dead_code)] // Called by the management executable, not the injected engine.
pub fn requires_reboot(root: &Path) -> bool {
    match read(root) {
        Ok(None) => false,
        Ok(Some(owner)) => boot(root).map_or(true, |boot| {
            boot == owner.boot
                && crate::process_identity::start_at(root, owner.pid) != Some(owner.start)
        }),
        Err(_) => true,
    }
}
#[allow(dead_code)] // Called by the management executable, not the injected engine.
pub fn prepare_boot(root: &Path) -> io::Result<()> {
    if let Some(owner) = read(root)? {
        if owner.boot == boot(root)? {
            return Err(io::Error::other(
                "unfinished global-property session requires reboot",
            ));
        }
        std::fs::remove_file(path(root))?;
    }
    Ok(())
}
pub fn release(root: &Path, generation: &str) -> io::Result<()> {
    let Some(owner) = read(root)? else {
        return Ok(());
    };
    if owner.pid != std::process::id()
        || owner.generation != generation
        || owner.boot != boot(root)?
        || crate::process_identity::start_at(root, owner.pid) != Some(owner.start)
    {
        return Err(invalid());
    }
    std::fs::remove_file(path(root))?;
    File::open(path(root).parent().unwrap())?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Fixture {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "global-owner-{}-{}",
            std::process::id(),
            ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("proc/sys/kernel/random")).unwrap();
        std::fs::create_dir_all(path(&root).parent().unwrap()).unwrap();
        std::fs::write(root.join("proc/sys/kernel/random/boot_id"), "boot-one").unwrap();
        process(&root, "S", "42");
        Fixture(root)
    }
    fn process(root: &Path, state: &str, start: &str) {
        let file = root.join(format!("proc/{}/stat", std::process::id()));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mut fields = vec!["0"; 20];
        fields[0] = state;
        fields[19] = start;
        std::fs::write(
            file,
            format!("{} (owner) {}", std::process::id(), fields.join(" ")),
        )
        .unwrap();
    }
    #[test]
    fn owner_start_time_generation_and_boot_must_all_match() {
        let fixture = fixture();
        let root = &fixture.0;
        std::fs::write(path(root), encode(root, "initial").unwrap()).unwrap();
        assert!(permits(root, "initial"));
        assert!(!permits(root, "another"));
        process(root, "S", "43");
        assert!(!permits(root, "initial"));
        assert!(requires_reboot(root));
        assert!(release(root, "initial").is_err());
        assert!(path(root).exists());
    }
    #[test]
    fn zombie_owner_is_not_a_live_owner_and_same_boot_cannot_clear_it() {
        let fixture = fixture();
        let root = &fixture.0;
        std::fs::write(path(root), encode(root, "initial").unwrap()).unwrap();
        process(root, "Z", "42");
        assert!(!permits(root, "initial"));
        assert!(requires_reboot(root));
        assert!(prepare_boot(root).is_err());
        assert!(path(root).exists());
    }
    #[test]
    fn only_a_new_boot_or_the_original_live_owner_can_clear_the_marker() {
        let fixture = fixture();
        let root = &fixture.0;
        std::fs::write(path(root), encode(root, "initial").unwrap()).unwrap();
        assert!(release(root, "wrong").is_err());
        release(root, "initial").unwrap();
        std::fs::write(path(root), encode(root, "initial").unwrap()).unwrap();
        std::fs::write(root.join("proc/sys/kernel/random/boot_id"), "boot-two").unwrap();
        assert!(!permits(root, "initial"));
        assert!(!requires_reboot(root));
        prepare_boot(root).unwrap();
        assert!(!path(root).exists());
    }
    #[test]
    fn corrupt_and_oversized_markers_fail_closed_without_deletion() {
        let fixture = fixture();
        let root = &fixture.0;
        for text in ["invalid".into(), "x".repeat(1025)] {
            std::fs::write(path(root), &text).unwrap();
            assert!(!permits(root, "initial"));
            assert!(requires_reboot(root));
            assert!(prepare_boot(root).is_err());
            assert_eq!(std::fs::read_to_string(path(root)).unwrap(), text);
        }
    }
}
