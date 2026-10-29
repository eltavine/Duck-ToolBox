use crate::{archive, guest_path, model::*, native, store::*};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};

fn config() -> Config {
    Config {
        name: "Debian".into(),
        auto_start: false,
        dns: vec![],
        services: vec![],
    }
}
fn elf() -> Vec<u8> {
    let mut bytes = vec![0; 128];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&183u16.to_le_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
    bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
    bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
    bytes[64..68].copy_from_slice(&1u32.to_le_bytes());
    bytes[68..72].copy_from_slice(&5u32.to_le_bytes());
    bytes[96..104].copy_from_slice(&128u64.to_le_bytes());
    bytes[104..112].copy_from_slice(&128u64.to_le_bytes());
    bytes
}
fn entry(builder: &mut tar::Builder<Vec<u8>>, path: &str, bytes: &[u8], mode: u32) {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes).unwrap();
}
fn rootfs() -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for name in ["bin/sh", "bin/sleep", "bin/cat"] {
        entry(&mut builder, name, &elf(), 0o755);
    }
    entry(&mut builder, "etc/os-release", b"ID=debian\n", 0o644);
    builder.into_inner().unwrap()
}
#[test]
fn archive_identity_is_checked_before_publication() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("root.tar");
    fs::write(&source, rootfs()).unwrap();
    let inspected = archive::inspect(&source).unwrap();
    assert_eq!(inspected.sha256.len(), 64);
    let snapshot = t.path().join("snapshot");
    assert!(archive::snapshot(&source, &snapshot, &"0".repeat(64), |_| Ok(())).is_err());
    let mut file = archive::snapshot(&source, &snapshot, &inspected.sha256, |_| Ok(())).unwrap();
    let out = t.path().join("out");
    fs::create_dir(&out).unwrap();
    let (entries, _) = archive::validate_extract(&mut file, &out).unwrap();
    assert_eq!(entries, 4);
    assert_eq!(
        fs::read(out.join("etc/os-release")).unwrap(),
        b"ID=debian\n"
    );
}
#[test]
fn gzip_crc_truncation_and_cancellation_are_not_accepted() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("root.gz");
    let mut writer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    writer.write_all(&rootfs()).unwrap();
    let compressed = writer.finish().unwrap();
    fs::write(&path, &compressed).unwrap();
    let out = t.path().join("out");
    fs::create_dir(&out).unwrap();
    archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out).unwrap();
    fs::write(&path, &compressed[..compressed.len() - 6]).unwrap();
    assert!(archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out).is_err());
    assert!(
        archive::snapshot(
            &path,
            &t.path().join("snap"),
            &"a".repeat(64),
            |_| anyhow::bail!("cancelled")
        )
        .is_err()
    );
}
#[test]
fn paths_links_and_special_entries_cannot_escape_the_staging_directory() {
    let t = tempfile::tempdir().unwrap();
    let outside = t.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let mut builder = tar::Builder::new(Vec::new());
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_size(0);
    link.set_mode(0o777);
    link.set_cksum();
    builder.append_link(&mut link, "escape", &outside).unwrap();
    entry(&mut builder, "escape/changed", b"bad", 0o644);
    let path = t.path().join("bad.tar");
    fs::write(&path, builder.into_inner().unwrap()).unwrap();
    let out = t.path().join("out");
    fs::create_dir(&out).unwrap();
    assert!(archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out).is_err());
    assert!(!outside.join("changed").exists());
    let mut bytes = rootfs();
    bytes[..100].fill(0);
    bytes[..10].copy_from_slice(b"../outside");
    let checksum: u32 = bytes[..512]
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if (148..156).contains(&i) {
                32
            } else {
                u32::from(*b)
            }
        })
        .sum();
    bytes[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out).is_err());
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Char);
    header.set_size(0);
    header.set_mode(0o666);
    header.set_cksum();
    builder
        .append_data(&mut header, "dev/unsafe", Cursor::new([]))
        .unwrap();
    fs::write(&path, builder.into_inner().unwrap()).unwrap();
    assert!(archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out).is_err());
}
#[test]
fn symlink_resolution_uses_the_guest_root_and_rejects_upward_escape() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("usr/bin")).unwrap();
    fs::write(t.path().join("usr/bin/sh"), elf()).unwrap();
    symlink("/usr/bin", t.path().join("bin")).unwrap();
    assert_eq!(
        guest_path::resolve(t.path(), Path::new("/bin/sh")).unwrap(),
        t.path().join("usr/bin/sh")
    );
    symlink("../../outside", t.path().join("bad")).unwrap();
    assert!(guest_path::resolve(t.path(), Path::new("/bad/file")).is_err());
    symlink("loop", t.path().join("loop")).unwrap();
    assert!(guest_path::resolve(t.path(), Path::new("/loop")).is_err());
}
#[test]
fn configuration_and_revision_reject_stale_or_unconfirmed_requests() {
    let mut cfg = config();
    cfg.services.push(Service {
        id: "web".into(),
        command: "python3 -m http.server".into(),
        enabled: true,
    });
    assert!(cfg.validate().is_ok());
    cfg.services.push(cfg.services[0].clone());
    assert!(cfg.validate().is_err());
    cfg.services.clear();
    cfg.dns = vec!["1.1.1.1".into(), "::1".into()];
    assert!(cfg.validate().is_ok());
    cfg.dns.push("not-a-resolver".into());
    assert!(cfg.validate().is_err());
    let env = Environment {
        id: "debian".into(),
        instance: "a".repeat(64),
        archive_sha256: "b".repeat(64),
        expanded_bytes: 128,
        entries: 4,
        created_at: 1,
        config: config(),
    };
    let mut target = Target {
        id: env.id.clone(),
        revision: revision(&env).unwrap(),
        confirmed: true,
    };
    target.check(&env).unwrap();
    target.confirmed = false;
    assert!(target.check(&env).is_err());
    target.confirmed = true;
    target.revision = "c".repeat(64);
    assert!(target.check(&env).is_err());
    for value in ["../x", "A", "a/b", "a;sh", "", "-bad"] {
        assert!(!valid_id(value));
    }
}
#[test]
fn private_state_locks_symlinks_and_atomic_files_are_enforced() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    let ctx = duck_core::Context {
        paths: duck_core::AppPaths::for_root(t.path(), t.path()),
        sysroot: duck_core::Sysroot::default(),
    };
    let store = Store::lock(&ctx).unwrap();
    assert!(busy(&store.base).unwrap());
    assert!(Store::lock(&ctx).is_err());
    save(&store.base.join("state.json"), &config()).unwrap();
    let _: Config = load(&store.base.join("state.json")).unwrap();
    symlink(store.base.join("state.json"), store.base.join("link")).unwrap();
    assert!(open(&store.base.join("link"), false).is_err());
    drop(store);
    assert!(!busy(&base(&ctx)).unwrap());
}
#[test]
fn process_stat_names_and_runtime_dependency_pins_are_checked() {
    assert!(
        native::alive(&Identity {
            pid: 1,
            start: 0,
            boot: String::new()
        })
        .is_err()
    );
    let mut fields = vec!["S".to_string(); 20];
    fields[19] = "12345".into();
    assert_eq!(
        native::parse_start(&format!("42 (name with ) brackets) {}", fields.join(" "))).unwrap(),
        12345
    );
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../components/linux-env/tini.json")).unwrap();
    assert_eq!(manifest["sha256"], crate::mounts::TINI_SHA256);
}
#[test]
fn service_commands_remain_data_in_separate_guest_scripts() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join("run")).unwrap();
    let mut cfg = config();
    cfg.services = vec![Service {
        id: "web".into(),
        command: "printf '%s' \"$HOME\"; exit 7".into(),
        enabled: true,
    }];
    crate::scripts::write(t.path(), &cfg).unwrap();
    assert_eq!(
        fs::read_to_string(t.path().join("run/duck-toolbox/services/web.command")).unwrap(),
        cfg.services[0].command
    );
    let init = fs::read_to_string(t.path().join("run/duck-toolbox/init.sh")).unwrap();
    assert!(init.contains("services/web.sh"));
    assert!(!init.contains("$HOME"));
}

#[test]
fn archive_link_overwrites_and_pax_limits_preserve_host_files() {
    let temporary = tempfile::tempdir().unwrap();
    let sentinel = temporary.path().join("host-file");
    fs::write(&sentinel, b"keep").unwrap();
    let out = temporary.path().join("out");
    fs::create_dir(&out).unwrap();
    let path = temporary.path().join("archive.tar");
    for kind in [tar::EntryType::Symlink, tar::EntryType::Link] {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(kind);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();
        builder
            .append_link(&mut header, "target", &sentinel)
            .unwrap();
        entry(&mut builder, "target", b"overwrite", 0o644);
        fs::write(&path, builder.into_inner().unwrap()).unwrap();
        let _ = archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out);
        assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
    }
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::XHeader);
    header.set_mode(0o644);
    header.set_size(65537);
    header.set_cksum();
    builder
        .append_data(&mut header, "pax", Cursor::new(vec![0; 65537]))
        .unwrap();
    fs::write(&path, builder.into_inner().unwrap()).unwrap();
    assert!(archive::validate_extract(&mut fs::File::open(&path).unwrap(), &out).is_err());
}

#[test]
fn extraction_checks_interruption_before_reading_payload() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("archive.tar");
    fs::write(&path, rootfs()).unwrap();
    let out = temporary.path().join("out");
    fs::create_dir(&out).unwrap();
    let result = archive::extract(
        &mut fs::File::open(path).unwrap(),
        &out,
        &|| Err(crate::fail("cancelled", "import cancelled")),
        |_, _, _| Ok(()),
    );
    let error = result.unwrap_err();
    assert_eq!(crate::code_of(&error), "cancelled", "{error:#}");
    assert!(fs::read_dir(out).unwrap().next().is_none());
}

#[test]
fn private_state_can_round_trip_maximum_escaped_service_commands() {
    let temporary = tempfile::tempdir().unwrap();
    let mut cfg = config();
    for index in 0..8 {
        cfg.services.push(Service {
            id: format!("service-{index}"),
            command: format!("#{}", "\u{1}".repeat(8191)),
            enabled: true,
        });
    }
    cfg.validate().unwrap();
    let path = temporary.path().join("state.json");
    save(&path, &cfg).unwrap();
    assert!(fs::metadata(&path).unwrap().len() > 131072);
    let decoded: Config = load(&path).unwrap();
    assert_eq!(decoded.services[7].command, cfg.services[7].command);
}
