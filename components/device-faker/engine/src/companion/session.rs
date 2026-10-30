use super::platform::*;
use log::{error, warn};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
pub(super) static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// 单个前台门控会话：Apply 登记，FG 事件驱动激活/恢复。
pub(super) struct Session {
    pub(super) package: String,
    pub(super) app_pid: u32,
    pub(super) start: u64,
    pub(super) unknown_since: Option<Instant>,
    pub(super) closing: bool,
    /// 本次伪装规格（来自 Apply 请求）
    pub(super) spec: SessionSpec,
    /// Apply 时采样的原始值备份（此刻全局处于干净状态）
    pub(super) originals: Originals,
    /// 是否已随 FG 事件中央激活
    pub(super) activated: bool,
}

pub(super) struct SessionSpec {
    pub(super) props: HashMap<String, String>,
    pub(super) delete_props: Vec<String>,
    pub(super) density: Option<u32>,
}

pub(super) struct Originals {
    pub(super) prop_backups: HashMap<String, String>,
    /// 备份时**真机上不存在**的 key（`prop_backups` 里只是空串占位）。restore 时
    /// 必须 `delete` 而不是 `set ""`——否则会把"本来没有的属性"写成**空值属性**：
    /// key 存在但值为空，`getprop` 读出来同样是空串，与"不存在"难以区分，在
    /// `__system_property_foreach` / 全量 dump 里就是真机没有的可枚举幽灵属性。
    pub(super) absent_props: HashSet<String>,
}

// ── 前台源（UidObserver 事件驱动，0.5s dumpsys 轮询兜底）───────────────────
//
// 首次 Apply 走 `spawn_fg_observer_once` 惰性启动，事件驱动
// （PROCESS_STATE_TOP）或轮询兜底喂 `handle_fg_event`。

static FG_OBSERVER_STARTED: OnceLock<()> = OnceLock::new();

/// 首次 Apply 时惰性启动前台源线程（UidObserver → 轮询兜底）。
pub(super) fn spawn_fg_observer_once() {
    FG_OBSERVER_STARTED.get_or_init(|| {
        let sink: crate::fg_observer::FgSink = std::sync::Arc::new(|_pkg: &str| {
            if SESSION.lock().unwrap().is_some() {
                handle_fg_event(&crate::fg_observer::confirmed_fg());
            }
        });
        crate::fg_observer::spawn_fg_source(sink);
        // A stop or disabled parent must restore a global session even without a focus event.
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if SESSION.lock().unwrap().is_none() {
                    continue;
                }
                let observed = crate::fg_observer::confirmed_fg();
                handle_fg_event(&observed);
            }
        });
    });
}

/// 前台源（observer 或轮询）是否已就绪。
pub(super) fn fg_ready() -> bool {
    crate::fg_observer::FG_READY.load(std::sync::atomic::Ordering::Acquire)
}

/// D2 限频告警间隔：同一分钟内最多一条 WARN。
const FG_WARN_INTERVAL: Duration = Duration::from_secs(60);

/// D2 限频 WARN：前台源未就绪时属性/密度一律不应用。
pub(super) fn warn_fg_unavailable() {
    static LAST: OnceLock<Mutex<Instant>> = OnceLock::new();
    let due = LAST
        .get_or_init(|| Mutex::new(Instant::now() - FG_WARN_INTERVAL))
        .lock()
        .map(|mut last| {
            let notify = last.elapsed() >= FG_WARN_INTERVAL;
            if notify {
                *last = Instant::now();
            }
            notify
        })
        .unwrap_or(false);
    if due {
        warn!(
            "foreground source not ready; session registration pending activation (will apply once FG events arrive)"
        );
    }
}

// ── 中央状态机：FG 事件驱动激活/恢复 ────────────────────────────────────────
//
// | 条件                                    | 动作                             |
// |-----------------------------------------|----------------------------------|
// | 死会话（kill 0 失败）且未激活             | 清空（搭车式惰性回收，D5）        |
// | FG == 包名 && !activated                 | 应用 props 批量 + density，激活   |
// | FG == 包名 && activated                  | 幂等跳过                          |
// | FG != 包名 && activated                  | 恢复 props + density，失活        |
// | FG == "-"（过渡态）                      | 保持现状（宁可晚不错）            |
// | 其它                                     | 无操作                            |

pub(super) fn handle_fg_event(observed: &str) {
    let mut guard = SESSION.lock().unwrap();
    let Some(sess) = guard.as_mut() else {
        return;
    };
    let live = crate::process_identity::start_at(std::path::Path::new("/"), sess.app_pid)
        == Some(sess.start);
    if !crate::gate::enabled() || !live || sess.closing {
        let mut taken = guard.take().unwrap();
        if taken.activated && !deactivate_session(&mut taken, "closed") {
            *guard = Some(taken);
        }
        return;
    }
    let focus = observed.to_string();
    if focus == "-" {
        let since = sess.unknown_since.get_or_insert_with(Instant::now);
        if !sess.activated || since.elapsed() < Duration::from_secs(2) {
            return;
        }
    } else {
        sess.unknown_since = None;
    }
    if focus == sess.package {
        if !sess.activated {
            activate_session(sess);
        }
    } else if sess.activated {
        deactivate_session(sess, &focus);
    }
}

fn activate_session(sess: &mut Session) {
    if crate::density::recover(
        std::path::Path::new("/"),
        &mut crate::density::AndroidBridge,
    )
    .is_err()
    {
        error!("previous density recovery remains pending");
        return;
    }
    let snapshot = (|| -> anyhow::Result<Originals> {
        let mut backups = HashMap::new();
        let mut absent = HashSet::new();
        for key in sess.spec.props.keys().chain(sess.spec.delete_props.iter()) {
            match property_exists(key) {
                Some(false) => {
                    absent.insert(key.clone());
                    backups.insert(key.clone(), String::new());
                }
                Some(true) => {
                    backups.insert(key.clone(), backup_property(key)?);
                }
                None => anyhow::bail!("property presence is unavailable"),
            }
        }
        Ok(Originals {
            prop_backups: backups,
            absent_props: absent,
        })
    })();
    let Ok(originals) = snapshot else {
        error!("property activation backup failed");
        return;
    };
    sess.originals = originals;
    // A partial apply must still enter the cleanup path with its original baseline.
    sess.activated = true;
    if let Err(error) = apply_props_batch(&sess.spec.props, &sess.spec.delete_props) {
        error!("property activation failed: {error}");
        deactivate_session(sess, "activation failed");
        sess.closing = true;
        return;
    }
    if let Some(density) = sess.spec.density
        && let Err(error) = set_density(Some(density))
    {
        error!("density activation failed: {error}");
        deactivate_session(sess, "activation failed");
        sess.closing = true;
    }
}

pub(super) fn deactivate_session(sess: &mut Session, _focused: &str) -> bool {
    let props = restore_owned_props(
        &sess.originals.prop_backups,
        &sess.originals.absent_props,
        &sess.spec.props,
        &sess.spec.delete_props,
    );
    let density = restore_density();
    let restored = props.is_ok() && density.is_ok();
    sess.closing = !restored;
    if restored {
        sess.activated = false;
    }
    let marker = std::path::Path::new("/data/adb/duck-device-faker/cleanup-failed");
    if restored {
        let _ = std::fs::remove_file(marker);
    } else {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(marker)
        {
            let _ = file.write_all(b"global_cleanup_pending");
        }
        error!("global cleanup remains pending");
    }
    restored
}
