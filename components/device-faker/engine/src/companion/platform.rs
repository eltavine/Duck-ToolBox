use super::rebuild_all_contexts;
use log::info;
use prop_rs_android::{resetprop::ResetProp, sys_prop};
use std::collections::{HashMap, HashSet};
pub(super) fn property_exists(key: &str) -> Option<bool> {
    sys_prop::init().ok()?;
    Some(sys_prop::find(key).is_some())
}

/// 读属性当前值（`None` = 本进程读不到：不存在，或该 area 未映射/无可读权限）。
pub(super) fn property_value(key: &str) -> Option<String> {
    sys_prop::init().ok()?;
    sys_prop::get(key)
}

/// 备份单个属性的当前全局值。
///
/// 快路径：`sys_prop::get` 在本进程直接读属性区（已存在的属性**零 fork**）。
/// 回落：读不到时用 `getprop` 子进程取真值（root 视角，能覆盖"属性存在但本进程
/// 读不到该 area"的情形），保持与旧实现完全一致的备份语义（不存在 → 空串）。
/// 因此每次 Apply 的 `getprop` 子进程数量从「属性数」降到「不存在的属性数」。
pub(super) fn backup_property(key: &str) -> anyhow::Result<String> {
    if let Some(value) = property_value(key) {
        return Ok(value);
    }

    let mut command = std::process::Command::new("/system/bin/getprop");
    command.arg(key);
    let output = crate::bounded::output(command)?;

    let value = String::from_utf8_lossy(&output)
        .trim_end_matches(['\n', '\r'])
        .to_string();
    Ok(value)
}

pub(super) fn new_resetprop() -> anyhow::Result<ResetProp> {
    sys_prop::init()
        .map_err(|e| anyhow::anyhow!("failed to initialize system property API: {e}"))?;

    Ok(ResetProp {
        // `-n`: bypass property_service, direct mmap write.
        // All properties we set (ro.*, persist.*, etc.) benefit from direct
        // mmap — no SELinux policy denials, no init service restarts, no
        // PROP_VALUE_MAX limit.  ro.* is forced to mmap regardless, but
        // skip_svc=true also covers non-ro keys in custom_props.
        skip_svc: true,
        persistent: false,
        persist_only: false,
        verbose: false,
        show_context: false,
        rebuild: false,
    })
}

pub(super) fn apply_resetprop(key: &str, value: &str) -> anyhow::Result<()> {
    let rp = new_resetprop()?;

    // ksu_props ≥ ddb6ee7：`set()` 原地支持任意长度的新值（≥ PROP_VALUE_MAX 时内部
    // 重新分配 long buffer 并改写 prop_info 的 long offset），不再需要
    // delete+set 兜底——后者是全局可见的破坏性操作（delete 与 set 之间其他进程
    // 读到空值，set 再失败则属性永久丢失）。
    // 返回的 bool 表示旧 long buffer 成为孤儿、需要 rebuild 回收；调用方
    // （apply_props_batch / restore）结束时会统一 rebuild_all_contexts。
    rp.set(key, value)
        .map_err(|e| anyhow::anyhow!("resetprop set failed for {key}: {e}"))?;
    Ok(())
}

pub(super) fn resetprop_delete(key: &str) -> anyhow::Result<()> {
    let rp = new_resetprop()?;

    match rp.delete(key) {
        Ok(true) => Ok(()),
        // 属性族展开后，删除列表包含设备上可能不存在的分区副本
        // （如旧设备没有 system_dlkm/vendor_dlkm 构建 props）；
        // 本来就不存在的属性视为已删除，避免整个 Apply 会话失败回滚。
        Ok(false) => {
            info!("resetprop delete: '{key}' not present, treating as deleted");
            Ok(())
        }
        Err(_) => anyhow::bail!("resetprop delete failed for {key}"),
    }
}

pub(super) fn apply_props_batch(
    props: &HashMap<String, String>,
    delete_props: &[String],
) -> anyhow::Result<()> {
    for (key, value) in props {
        if !crate::gate::enabled() {
            anyhow::bail!("global property activation cancelled");
        }
        apply_resetprop(key, value)?;
    }

    for key in delete_props {
        if !crate::gate::enabled() {
            anyhow::bail!("global property activation cancelled");
        }
        resetprop_delete(key)?;
    }

    rebuild_all_contexts(props.keys().chain(delete_props.iter()));

    Ok(())
}

pub(super) fn restore_owned_props(
    backups: &HashMap<String, String>,
    absent: &HashSet<String>,
    applied: &HashMap<String, String>,
    deleted: &[String],
) -> anyhow::Result<()> {
    let mut failed = false;
    for (key, before) in backups {
        let exists = property_exists(key);
        if exists.is_none() {
            failed = true;
            continue;
        }
        let owned = if let Some(after) = applied.get(key) {
            if exists == Some(true) {
                let Some(current) = property_value(key) else {
                    failed = true;
                    continue;
                };
                current == *after
            } else {
                false
            }
        } else if deleted.contains(key) {
            exists == Some(false)
        } else {
            false
        };
        if !owned {
            continue;
        }
        let result = if absent.contains(key) {
            resetprop_delete(key)
        } else {
            apply_resetprop(key, before)
        };
        if result.is_err() {
            failed = true;
        }
    }
    if failed {
        anyhow::bail!("conditional property restoration failed");
    }
    Ok(())
}

pub(super) fn set_density(density: Option<u32>) -> anyhow::Result<()> {
    let root = std::path::Path::new("/");
    if let Some(density) = density {
        crate::density::apply(root, density, &mut crate::density::AndroidBridge)
    } else {
        crate::density::recover(root, &mut crate::density::AndroidBridge)
    }
}
pub(super) fn restore_density() -> anyhow::Result<()> {
    crate::density::recover(
        std::path::Path::new("/"),
        &mut crate::density::AndroidBridge,
    )
}
