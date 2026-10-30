//! UID observer notifications wake the session monitor; window focus authorizes mutation.
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use log::{debug, error, info};
use rsbinder::{ProcessState, Strong, hub};

include!(concat!(env!("OUT_DIR"), "/fg_binder.rs"));

use self::android::app::IActivityManager::IActivityManager;
use self::android::app::IUidObserver::{BnUidObserver, IUidObserver};

/// 前台源是否就绪（observer 注册成功，或轮询兜底已接管）。
pub static FG_READY: AtomicBool = AtomicBool::new(false);

/// AMS 进程状态：前台（top resumed activity 所在进程）。
pub const PROCESS_STATE_TOP: i32 = 2;
/// `ActivityManager.UID_OBSERVER_PROCSTATE = 1 << 0`（Android 定义）。
const UID_OBSERVER_PROCSTATE: i32 = 1 << 0;
/// `ActivityManager.UID_OBSERVER_GONE = 1 << 1`。
const UID_OBSERVER_GONE: i32 = 1 << 1;
/// Notifications carry no claimed foreground identity.
pub type FgSink = Arc<dyn Fn() + Send + Sync>;
pub struct UidObserverState {
    top_uids: Mutex<HashSet<i32>>,
    sink: FgSink,
}

impl rsbinder::Interface for UidObserverState {}

impl UidObserverState {
    fn on_top_change(&self, uid: i32, is_top: bool) {
        let changed = {
            let mut top = self.top_uids.lock().unwrap();
            if is_top {
                top.insert(uid)
            } else {
                top.remove(&uid)
            }
        };
        if changed {
            (self.sink)();
        }
    }
}

impl IUidObserver for UidObserverState {
    fn r#onUidStateChanged(
        &self,
        uid: i32,
        proc_state: i32,
        _seq: i64,
        _capability: i32,
    ) -> rsbinder::BinderResult<()> {
        debug!("fg observer: onUidStateChanged uid={uid} proc_state={proc_state}");
        self.on_top_change(uid, proc_state == PROCESS_STATE_TOP);
        Ok(())
    }

    fn r#onUidGone(&self, uid: i32, _disabled: bool) -> rsbinder::BinderResult<()> {
        debug!("fg observer: onUidGone uid={uid}");
        self.on_top_change(uid, false);
        Ok(())
    }

    fn r#onUidActive(&self, uid: i32) -> rsbinder::BinderResult<()> {
        debug!("fg observer: onUidActive uid={uid}");
        Ok(())
    }

    fn r#onUidIdle(&self, uid: i32, _disabled: bool) -> rsbinder::BinderResult<()> {
        debug!("fg observer: onUidIdle uid={uid}");
        Ok(())
    }

    fn r#onUidProcAdjChanged(&self, uid: i32, adj: i32) -> rsbinder::BinderResult<()> {
        debug!("fg observer: onUidProcAdjChanged uid={uid} adj={adj}");
        Ok(())
    }

    fn r#onUidCachedChanged(&self, uid: i32, _cached: bool) -> rsbinder::BinderResult<()> {
        debug!("fg observer: onUidCachedChanged uid={uid}");
        Ok(())
    }
}

/// 启动前台源：先试 UidObserver（事件驱动），失败退 0.5s 轮询。
/// 顶层调用一次；内部线程常驻。
pub fn spawn_fg_source(sink: FgSink) {
    std::thread::Builder::new()
        .name("fg-observer".into())
        .spawn(move || {
            if let Err(e) = register_observer(sink.clone()) {
                error!(
                    "UidObserver registration failed ({e}); periodic session monitor remains active"
                );
            }
        })
        .expect("failed to spawn fg-observer thread");
}

/// 注册 UidObserver 并启动 binder 线程池接收回调。返回 Ok 表示已接管前台源。
fn register_observer(sink: FgSink) -> anyhow::Result<()> {
    ProcessState::init_default()
        .map_err(|e| anyhow::anyhow!("binder ProcessState init failed: {e}"))?;

    // 阻塞等 ActivityManager 服务可用，返回强类型代理（BpActivityManager）。
    let bp: Strong<dyn IActivityManager> = hub::wait_for_interface("activity")?;

    let state = UidObserverState {
        top_uids: Mutex::new(HashSet::new()),
        sink,
    };
    let observer: Strong<dyn IUidObserver> = BnUidObserver::new_binder(state);

    bp.r#registerUidObserver(
        &observer,
        UID_OBSERVER_PROCSTATE | UID_OBSERVER_GONE,
        // 必须用 -1（全量回调，用户态过滤）：内核侧 cutpoint 过滤在部分 OEM ROM 上
        // 会丢掉非配置 app 首次进入 TOP 的事件，导致前台门控不恢复 density/props。
        // 正确性优先：全量回调 + on_top_change 的 proc_state==TOP 判断。
        -1,
        "device_faker",
    )?;

    // 启动 binder 线程池接收 onUidStateChanged/onUidGone 回调。
    ProcessState::start_thread_pool();

    FG_READY.store(true, Ordering::Release);
    info!("UidObserver active: event-driven foreground (PROCESS_STATE_TOP)");
    Ok(())
}

pub fn confirmed_fg() -> String {
    let focus = exec_dumpsys_focus();
    if focus != "-" {
        FG_READY.store(true, Ordering::Release);
    }
    focus
}
fn exec_dumpsys_focus() -> String {
    // Drain the dump through a bounded-output filter: a full window dump can
    // exceed the process adapter's output cap even when focus is available.
    let mut command = std::process::Command::new("/system/bin/sh");
    command.args([
        "-c",
        &format!(
            "/system/bin/dumpsys window | /system/bin/toybox grep -E '{}'",
            crate::focus::FILTER
        ),
    ]);
    match crate::bounded::output(command) {
        Ok(bytes) => crate::focus::parse_dump(&String::from_utf8_lossy(&bytes)),
        Err(_) => "-".into(),
    }
}
