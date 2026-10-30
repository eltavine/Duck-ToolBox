mod platform;
mod protocol;
mod session;
use platform::*;
pub use protocol::*;
use session::*;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::net::UnixStream,
};

use log::{LevelFilter, error, info, warn};
use zygisk_api::api::{V4, ZygiskApi};

use crate::config::{DPI_MAX, DPI_MIN};

// ── 前台门控会话（D1/D4）────────────────────────────────────────────────────
//
// 前后台真值由 companion 内嵌的 UidObserver 提供（src/fg_observer.rs，
// ActivityManager PROCESS_STATE_TOP 事件，0.5s dumpsys 轮询兜底）。Apply 只
// 采样备份并登记未激活会话，**不急切应用任何全局态**；`FG == 包名` 才中央
// 激活属性批量与密度，`FG != 包名` 恢复。前台源未就绪时一律不做全局变更
// （D2 宁缺毋滥）。

pub(crate) struct SessionTarget<'a> {
    pub(crate) package: &'a str,
    pub(crate) generation: &'a str,
}

pub fn spoof_system_props_via_companion(
    api: &mut ZygiskApi<V4>,
    prop_map: &HashMap<String, String>,
    explicit_props: &HashSet<String>,
    delete_props: &[String],
    target: SessionTarget<'_>,
    density: Option<u32>,
    debug: bool,
) -> anyhow::Result<()> {
    if prop_map.is_empty() && delete_props.is_empty() && density.is_none() {
        return Ok(());
    }

    let request = CompanionRequest::Apply(ResetpropSessionRequest {
        pid: std::process::id(),
        generation: target.generation.to_string(),
        start: crate::process_identity::start_at(std::path::Path::new("/"), std::process::id())
            .ok_or_else(|| anyhow::anyhow!("process identity unavailable"))?,
        props: prop_map.clone(),
        explicit_props: explicit_props.iter().cloned().collect(),
        delete_props: delete_props.to_vec(),
        package_name: target.package.to_string(),
        density,
        debug,
    });

    let response = send_companion_command(api, &request)?;
    if response.status != 0 {
        anyhow::bail!(
            response
                .message
                .unwrap_or_else(|| "companion resetprop failed".to_string())
        );
    }

    // companion 侧现在自己管理会话状态与前台门控逻辑；
    // Zygisk 模块侧不再需要 ACTIVE_RESET_SESSION。
    Ok(())
}

pub fn handle_companion_request(stream: &mut UnixStream) {
    // companion 进程不会调用 ZygiskModule::on_load，因此需要自行初始化日志。
    #[cfg(target_os = "android")]
    crate::file_logger::init();

    let request = match read_companion_request(stream) {
        Ok(request) => request,
        Err(err) => {
            error!("Companion failed to parse request: {err}");
            let response = CompanionResponse::err("invalid request");
            if let Err(e) = write_companion_response(stream, &response) {
                warn!("Failed to write companion response: {e}");
            }
            return;
        }
    };

    match request {
        CompanionRequest::Apply(request) => {
            if crate::gate::activation_at(std::path::Path::new("/")).as_deref()
                != Some(request.generation.as_str())
            {
                let _ =
                    write_companion_response(stream, &CompanionResponse::err("engine disabled"));
                return;
            }
            let response = match apply_resetprop_session(request) {
                Ok(backups) => CompanionResponse::ok_with_backups(backups),
                Err(err) => {
                    error!("Companion failed to apply resetprop session: {err}");
                    CompanionResponse::err(err.to_string())
                }
            };
            if let Err(e) = write_companion_response(stream, &response) {
                warn!("Failed to write companion response: {e}");
            }
        }
        CompanionRequest::WriteLog(request) => {
            let response = match write_log_lines(request) {
                Ok(_) => CompanionResponse::ok(),
                Err(err) => {
                    error!("Companion failed to write log: {err}");
                    CompanionResponse::err(err.to_string())
                }
            };
            if let Err(e) = write_companion_response(stream, &response) {
                warn!("Failed to write companion response: {e}");
            }
        }
    }
}

/// 同步 companion 日志级别：debug=true 保持 Debug（保留 fg_observer/会话等观察性日志），
/// debug=false → Off（完全不写日志）。fork 出来的 CPU spoof 子进程会继承该级别，
/// 因此调用点必须在 fork 之前。
pub(crate) fn sync_log_level(debug: bool) {
    crate::file_logger::set_level(if debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    });
}

/// Apply 请求处理器：只采样备份 + 登记未激活会话（D1），
/// 不急切应用任何全局态；激活由 FG 事件驱动。
fn apply_resetprop_session(
    mut request: ResetpropSessionRequest,
) -> anyhow::Result<HashMap<String, String>> {
    sync_log_level(request.debug);
    if crate::process_identity::start_at(std::path::Path::new("/"), request.pid)
        != Some(request.start)
    {
        anyhow::bail!("target process identity changed");
    }

    if request.props.is_empty() && request.delete_props.is_empty() && request.density.is_none() {
        return Ok(HashMap::new());
    }

    if let Some(density) = request.density
        && !(DPI_MIN..=DPI_MAX).contains(&density)
    {
        anyhow::bail!(
            "DPI {} is outside the supported range {}..={}",
            density,
            DPI_MIN,
            DPI_MAX
        );
    }

    // 「本机可能不存在」的 key（ro.product.{dlkm 分区}.* / ro.{dlkm 分区}.build.*、
    // OEM 营销名族 `ro.product[.<分区>].marketname` 与别名 `ro.vendor.oplus.market.name`）
    // 只在部分 ROM 上存在，且都是模块**自动展开**出来的副本。resetprop `set()` 会在设备上
    // 不存在的 key 上创建**全局**幽灵属性（shell `getprop` 全量 dump 可见，比进程内副本
    // 显眼得多），且 backup/restore 还会把它留成空值 ⇒ 登记前先剔除真机不存在的 key
    // （连带不进 originals 备份，restore 也就不会重建它）。
    //
    // 例外：`custom_props` 里用户显式写出的 key（`explicit_props`）是明确指令，照写不误。
    //
    // 存在性用 `sys_prop::find`（bionic `__system_property_find`，本进程直接查属性区，
    // 首次查询触发该 context area 的惰性映射）——**不 fork/exec `getprop`**。判定不了
    // （bionic 符号加载失败）时保守放行，保持旧行为。
    if request
        .props
        .keys()
        .any(|key| crate::config::is_patch_only_key(key))
    {
        let explicit: HashSet<&str> = request.explicit_props.iter().map(String::as_str).collect();
        let mut dropped_keys: Vec<String> = Vec::new();
        request.props.retain(|key, _| {
            let keep = !crate::config::is_patch_only_key(key)
                || explicit.contains(key.as_str())
                || property_exists(key) != Some(false);
            if !keep {
                dropped_keys.push(key.clone());
            }
            keep
        });
        if !dropped_keys.is_empty() {
            info!(
                "Apply for '{}': dropped {} keys absent on this ROM: {}",
                request.package_name,
                dropped_keys.len(),
                dropped_keys.join(", ")
            );
        }
    }

    // 首次 Apply 时惰性启动前台源（UidObserver 事件驱动；失败退 0.5s 轮询）。
    spawn_fg_observer_once();

    // D2 语义：前台源未就绪时不急切应用任何全局态；但**照常采样登记 session**。
    // 之前这里直接 return（连 session 都不登记），observer 注册完成后到达的首次
    // TOP 事件会因 `SESSION == None` 无法激活 → 本次启动属性永不应用（首启竞态，
    // 冷启动后首个 companion app 必命中，极易误报"某版本属性失效"）。
    // 修复：不 return 不等待——session 登记后，registerUidObserver 完成时 AMS 会
    // 回放当前 uid 状态（或 0.5s dumpsys 轮询兜底）产生 FG 事件驱动激活。
    if !fg_ready() {
        warn_fg_unavailable();
        info!(
            "Apply for '{}' session will be registered; awaiting fg source ready to activate",
            request.package_name
        );
    }

    let mut guard = SESSION.lock().unwrap();

    // 同包去重：同包 + 旧进程存活 + dpi 未变 → 幂等返回现有备份。
    if let Some(sess) = guard.as_ref()
        && sess.package == request.package_name
        && sess.app_pid == request.pid
        && sess.start == request.start
        && sess.generation == request.generation
        && sess.spec.density == request.density
    {
        info!(
            "Skipping duplicate Apply for '{}' (pid {}), session already registered",
            request.package_name, request.pid
        );
        return Ok(sess.originals.prop_backups.clone());
    }

    if let Some(old) = guard.as_mut()
        && old.activated
        && !deactivate_session(old, &request.package_name)
    {
        anyhow::bail!("previous global cleanup is still pending");
    }
    let backups_for_response = HashMap::new();
    *guard = Some(Session {
        package: request.package_name.clone(),
        generation: request.generation,
        attempted: HashSet::new(),
        claimed_global: false,
        app_pid: request.pid,
        start: request.start,
        unknown_since: None,
        closing: false,
        spec: SessionSpec {
            props: request.props,
            delete_props: request.delete_props,
            density: request.density,
        },
        originals: Originals {
            prop_backups: HashMap::new(),
            absent_props: HashSet::new(),
        },
        activated: false,
    });
    drop(guard);

    Ok(backups_for_response)
}

const LOG_PATH: &str = "/data/adb/duck-device-faker/logs/device_faker.log";

fn write_log_lines(request: WriteLogRequest) -> anyhow::Result<()> {
    if request.lines.is_empty() {
        return Ok(());
    }

    write_log_lines_to_path(LOG_PATH, &request.lines)
}

fn write_log_lines_to_path(path: &str, lines: &[String]) -> anyhow::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    for line in lines {
        writeln!(file, "{line}")?;
    }

    file.flush()?;
    Ok(())
}
