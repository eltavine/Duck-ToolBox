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
        // Bypass init property triggers; persistent storage remains untouched.
        skip_svc: true,
        persistent: false,
        persist_only: false,
        verbose: false,
        show_context: false,
        rebuild: false,
    })
}

pub(super) fn apply_resetprop(key: &str, value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        key.starts_with("ro."),
        "global mutable property writes are unsupported"
    );
    let rp = new_resetprop()?;

    // ksu_props ≥ ddb6ee7：`set()` 原地支持任意长度的新值（≥ PROP_VALUE_MAX 时内部
    // 重新分配 long buffer 并改写 prop_info 的 long offset），不再需要
    // delete+set 兜底——后者是全局可见的破坏性操作（delete 与 set 之间其他进程
    // 读到空值，set 再失败则属性永久丢失）。
    // Rebuilding would relocate prop_info entries cached by other processes.
    // Leave old long buffers allocated rather than compacting a live property area.
    rp.set(key, value)
        .map_err(|e| anyhow::anyhow!("resetprop set failed for {key}: {e}"))?;
    Ok(())
}

pub(super) fn resetprop_delete(key: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        key.starts_with("ro."),
        "global mutable property deletion is unsupported"
    );
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
    attempted: &mut HashSet<String>,
    generation: &str,
) -> anyhow::Result<()> {
    for (key, value) in props {
        if crate::gate::activation_at(std::path::Path::new("/")).as_deref() != Some(generation) {
            anyhow::bail!("global property activation cancelled");
        }
        attempted.insert(key.clone());
        apply_resetprop(key, value)?;
    }

    for key in delete_props {
        if crate::gate::activation_at(std::path::Path::new("/")).as_deref() != Some(generation) {
            anyhow::bail!("global property activation cancelled");
        }
        attempted.insert(key.clone());
        resetprop_delete(key)?;
    }

    Ok(())
}

pub(super) fn restore_owned_props(
    backups: &HashMap<String, String>,
    absent: &HashSet<String>,
    applied: &HashMap<String, String>,
    deleted: &[String],
    attempted: &HashSet<String>,
) -> anyhow::Result<()> {
    let mut failed = false;
    for (key, before) in backups {
        if !attempted.contains(key) {
            continue;
        }
        let exists = property_exists(key);
        if exists.is_none() {
            failed = true;
            continue;
        }
        let owned = if deleted.contains(key) {
            exists == Some(false)
        } else if let Some(after) = applied.get(key) {
            if exists == Some(true) {
                let Some(current) = property_value(key) else {
                    failed = true;
                    continue;
                };
                current == *after
            } else {
                false
            }
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

pub(super) fn set_density(user: u32, density: u32, generation: &str) -> anyhow::Result<()> {
    crate::density::apply(
        std::path::Path::new("/"),
        user,
        density,
        generation,
        &mut crate::density::AndroidBridge,
    )
}
pub(super) fn restore_density() -> anyhow::Result<()> {
    crate::density::recover(
        std::path::Path::new("/"),
        &mut crate::density::AndroidBridge,
    )
}

pub(super) fn claim_global(generation: &str, claimed: &mut bool) -> anyhow::Result<()> {
    use std::io::Write;
    let root = std::path::Path::new("/");
    let path = crate::global_marker::path(root);
    let parent = path.parent().unwrap();
    let text = crate::global_marker::encode(root, generation)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist_noclobber(&path)?;
    *claimed = true;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
