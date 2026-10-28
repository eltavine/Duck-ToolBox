use crate::{
    inventory,
    kernel::Operations,
    model::{Config, Device, Stats, Status},
    mutation::{self, Operation, Request},
    store::Store,
};
use anyhow::Result;
fn config() -> Config {
    Config {
        size_bytes: 512 << 20,
        algorithm: "lz4".into(),
        priority: Some(100),
        memory_limit: 0,
    }
}
fn device() -> Device {
    Device {
        name: "zram0".into(),
        identity: "254:0".into(),
        path: Some("/dev/block/zram0".into()),
        size: 512 << 20,
        initialized: true,
        algorithms: vec!["lz4".into(), "zstd".into()],
        algorithm: Some("lz4".into()),
        swap: Some(crate::model::Swap {
            path: "/dev/block/zram0".into(),
            kind: "partition".into(),
            size: 512 << 20,
            used: 32 << 20,
            priority: 100,
            identity: Some("254:0".into()),
        }),
        stats: Stats {
            memory_limit: Some(0),
            ..Stats::default()
        },
        blocked: vec![],
        revision: "a".repeat(64),
    }
}
fn status() -> Status {
    Status {
        operations_supported: false,
        mmd_managed: Some(false),
        memory_total: 4 << 30,
        memory_available: Some(2 << 30),
        swap_in_pages: None,
        swap_out_pages: None,
        boot_id: "12345678-1234-1234-1234-123456789abc".into(),
        devices: vec![device()],
        swaps: vec![],
        warnings: vec![],
    }
}
#[test]
fn parses_swap_units_paths_and_priorities_without_guessing() {
    let swaps = inventory::parse_swaps("Filename Type Size Used Priority\n/dev/block/zram0 partition 1048572 2048 -2\n/data/a\\040b file 1024 0 32767\n").unwrap();
    assert_eq!(swaps[0].used, 2 << 20);
    assert_eq!(swaps[0].priority, -2);
    assert_eq!(swaps[1].path, "/data/a b");
    for text in [
        "",
        "Filename Type Size Used Priority\na b c d e",
        "Filename Type Size Used Priority\n/dev/zram0 partition 18446744073709551615 0 1",
        "Filename Type Size Used Priority\n/a\\000 file 1 0 1",
    ] {
        assert!(inventory::parse_swaps(text).is_err());
    }
}
#[test]
fn algorithm_selection_and_config_limits_follow_kernel_abi() {
    assert_eq!(inventory::algorithms("lzo [lz4] zstd\n").unwrap().1, "lz4");
    for s in ["", "lz4", "[lz4] [zstd]", "[lz4] ../zstd", "[lz4] lz4"] {
        assert!(inventory::algorithms(s).is_err());
    }
    assert!(config().validate(4 << 30).is_ok());
    let mut c = config();
    c.priority = Some(32768);
    assert!(c.validate(4 << 30).is_err());
    c = config();
    c.size_bytes = u64::MAX;
    assert!(c.validate(4 << 30).is_err());
    c.size_bytes = 1;
    assert!(c.validate(4 << 30).is_err());
    c.size_bytes = 0;
    assert!(c.validate(4 << 30).is_ok());
    for name in [
        "zram",
        "zram00",
        "zram-1",
        "zram0/../sda",
        "sda",
        "zram9999999",
    ] {
        assert!(!crate::model::valid_device(name));
    }
}
#[test]
fn rejects_stale_state_protected_devices_and_insufficient_headroom() {
    let mut s = status();
    assert!(mutation::validate(&s, "zram0", &"a".repeat(64), &config()).is_ok());
    assert!(mutation::validate(&s, "zram0", &"b".repeat(64), &config()).is_err());
    for reason in [
        "mmd",
        "writeback",
        "recompression",
        "mounted",
        "holders",
        "non_swap",
        "device_node",
        "manager_unknown",
        "stats_unknown",
        "swap_identity",
    ] {
        s.devices[0].blocked = vec![reason.into()];
        assert_eq!(
            crate::code_of(
                &mutation::validate(&s, "zram0", &"a".repeat(64), &config()).unwrap_err()
            ),
            "device_protected"
        );
    }
    s.devices[0].blocked.clear();
    s.devices[0].stats.memory_limit = Some(1024);
    assert_eq!(
        crate::code_of(&mutation::validate(&s, "zram0", &"a".repeat(64), &config()).unwrap_err()),
        "state_changed"
    );
    s.devices[0].stats.memory_limit = Some(0);
    s.memory_available = Some(300 << 20);
    assert_eq!(
        crate::code_of(&mutation::validate(&s, "zram0", &"a".repeat(64), &config()).unwrap_err()),
        "insufficient_memory"
    );
    s.memory_available = None;
    assert!(mutation::validate(&s, "zram0", &"a".repeat(64), &config()).is_err());
}
#[derive(Default)]
struct Fake {
    calls: Vec<&'static str>,
    fail: usize,
}
impl Fake {
    fn step(&mut self, name: &'static str) -> Result<()> {
        self.calls.push(name);
        anyhow::ensure!(self.fail != self.calls.len(), "injected failure");
        Ok(())
    }
}
impl Operations for Fake {
    fn swapoff(&mut self) -> Result<()> {
        self.step("swapoff")
    }
    fn reset(&mut self) -> Result<()> {
        self.step("reset")
    }
    fn algorithm(&mut self, _: &str) -> Result<()> {
        self.step("algorithm")
    }
    fn size(&mut self, _: u64) -> Result<()> {
        self.step("size")
    }
    fn limit(&mut self, _: u64) -> Result<()> {
        self.step("limit")
    }
    fn format(&mut self) -> Result<()> {
        self.step("format")
    }
    fn swapon(&mut self, _: Option<u16>) -> Result<()> {
        self.step("swapon")
    }
}
#[test]
fn every_failed_stage_stops_before_later_writes() {
    let mut c = config();
    c.memory_limit = 256 << 20;
    let steps = [
        "swapoff",
        "reset",
        "algorithm",
        "size",
        "limit",
        "format",
        "swapon",
    ];
    for stage in 1..=steps.len() {
        let mut f = Fake {
            fail: stage,
            ..Fake::default()
        };
        assert!(mutation::transition(&mut f, true, &c).is_err());
        assert_eq!(f.calls, steps[..stage]);
    }
    let mut f = Fake::default();
    mutation::transition(&mut f, true, &c).unwrap();
    assert_eq!(f.calls, steps);
    f.calls.clear();
    c.size_bytes = 0;
    mutation::transition(&mut f, true, &c).unwrap();
    assert_eq!(f.calls, ["swapoff", "reset"]);
    f.calls.clear();
    mutation::transition(&mut f, false, &c).unwrap();
    assert_eq!(f.calls, ["reset"]);
}
#[test]
fn request_round_trip_retains_confirmation_and_configuration() {
    let request = Request {
        device: "zram0".into(),
        revision: "a".repeat(64),
        confirmed: true,
        operation: Operation::Apply { config: config() },
    };
    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert!(matches!(parsed.operation, Operation::Apply { .. }));
    assert!(parsed.confirmed);
}
#[test]
fn private_store_is_atomic_locked_and_rejects_final_symlinks() {
    let t = tempfile::tempdir().unwrap();
    let ctx = duck_core::Context {
        paths: duck_core::AppPaths::for_root(t.path(), t.path()),
        sysroot: duck_core::Sysroot::new(t.path()),
    };
    let store = Store::open(&ctx).unwrap();
    assert_eq!(
        crate::code_of(&Store::open(&ctx).err().unwrap()),
        "operation_busy"
    );
    let state = store.load().unwrap();
    store.save(&state).unwrap();
    assert_eq!(store.load().unwrap().version, 1);
    drop(store);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = ctx.paths.var_dir.join("zram");
        std::fs::remove_file(dir.join("state.json")).unwrap();
        let target = t.path().join("target");
        std::fs::write(&target, "untouched").unwrap();
        symlink(&target, dir.join("state.json")).unwrap();
        assert!(Store::open(&ctx).unwrap().load().is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "untouched");
    }
}
