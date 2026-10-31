use super::platform::*;
use super::registry::{Originals, Session, reconcile};
use log::{error, warn};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub(super) static SESSIONS: Mutex<Vec<Session>> = Mutex::new(Vec::new());

// ── 前台源（UidObserver 事件驱动，0.5s dumpsys 轮询兜底）───────────────────
//
// 首次 Apply 走 `spawn_fg_observer_once` 惰性启动，事件驱动
// （PROCESS_STATE_TOP）或轮询兜底喂 `handle_fg_event`。

static FG_OBSERVER_STARTED: OnceLock<()> = OnceLock::new();

/// 首次 Apply 时惰性启动前台源线程（UidObserver → 轮询兜底）。
pub(super) fn spawn_fg_observer_once() {
    FG_OBSERVER_STARTED.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let sink: crate::fg_observer::FgSink = std::sync::Arc::new(move || {
            // Binder callbacks only notify; they never run dumpsys or mutate global state.
            let _ = sender.try_send(());
        });
        crate::fg_observer::spawn_fg_source(sink);
        std::thread::spawn(move || {
            let mut last = Instant::now() - Duration::from_secs(1);
            loop {
                if matches!(
                    receiver.recv_timeout(Duration::from_millis(500)),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
                ) {
                    std::thread::sleep(Duration::from_millis(500));
                }
                while receiver.try_recv().is_ok() {}
                let wait = Duration::from_millis(200).saturating_sub(last.elapsed());
                std::thread::sleep(wait);
                last = Instant::now();
                let needs_focus = {
                    let guard = SESSIONS.lock().unwrap();
                    let generation = crate::gate::activation_at(std::path::Path::new("/"));
                    guard.iter().any(|session| {
                        generation.as_deref() == Some(session.generation.as_str())
                            && !session.closing
                            && crate::process_identity::start_at(
                                std::path::Path::new("/"),
                                session.app_pid,
                            ) == Some(session.start)
                    })
                };
                let observed = if needs_focus {
                    crate::fg_observer::confirmed_fg()
                } else {
                    "-".into()
                };
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

/// One monitor selects the latest live foreground target and restores the previous owner first.
pub(super) fn handle_fg_event(observed: &str) {
    let mut guard = SESSIONS.lock().unwrap();
    let generation = crate::gate::activation_at(std::path::Path::new("/"));
    reconcile(
        &mut guard,
        observed,
        |session| {
            generation.as_deref() == Some(session.generation.as_str())
                && crate::process_identity::start_at(std::path::Path::new("/"), session.app_pid)
                    == Some(session.start)
        },
        activate_session,
        |session| deactivate_session(session, observed),
    );
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
    sess.attempted.clear();
    sess.activated = true;
    if (!sess.spec.props.is_empty() || !sess.spec.delete_props.is_empty())
        && let Err(error) = claim_global(&sess.generation, &mut sess.claimed_global)
    {
        error!("global ownership could not be published: {error}");
        deactivate_session(sess, "ownership failed");
        sess.closing = true;
        return;
    }
    if let Err(error) = apply_props_batch(
        &sess.spec.props,
        &sess.spec.delete_props,
        &mut sess.attempted,
        &sess.generation,
    ) {
        error!("property activation failed: {error}");
        deactivate_session(sess, "activation failed");
        sess.closing = true;
        return;
    }
    if let Some(density) = sess.spec.density
        && let Err(error) = set_density(sess.user, density, &sess.generation)
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
        &sess.attempted,
    );
    let density = restore_density();
    let mut restored = props.is_ok() && density.is_ok();
    if restored && sess.claimed_global {
        restored =
            crate::global_marker::release(std::path::Path::new("/"), &sess.generation).is_ok();
        if restored {
            sess.claimed_global = false;
        }
    }
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
            if let Ok(boot) = std::fs::read_to_string("/proc/sys/kernel/random/boot_id") {
                let _ = write!(file, "{}:global_cleanup_pending", boot.trim());
            } else {
                let _ = file.write_all(b"global_cleanup_pending");
            }
        }
        error!("global cleanup remains pending");
    }
    restored
}
