// SPDX-License-Identifier: GPL-3.0-only
// Modified 2026-10-24: standalone engine operations; no toolbox runtime dependency.
use crate::{Engine, config, files, tasks, text};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{fs, process::Command};

pub const UPSTREAM: &str = "23ec56c2440c4fd1a5e893a23ef7861d6366a828";
pub fn options(engine: &Engine) -> Result<Value> {
    let mut result = json!({"enabled":false,"mode":"full","skipdelprop":false,"skippersistprop":false,"skipmatchdevice":false});
    let stored = files::json_file(&engine.data.join("options.json"))?;
    if !stored.is_object() {
        bail!("options.json must be an object");
    }
    for key in [
        "enabled",
        "mode",
        "skipdelprop",
        "skippersistprop",
        "skipmatchdevice",
    ] {
        if let Some(value) = stored.get(key) {
            result[key] = value.clone();
        }
    }
    for key in [
        "enabled",
        "skipdelprop",
        "skippersistprop",
        "skipmatchdevice",
    ] {
        if !result[key].is_boolean() {
            bail!("options.json: {key} must be a boolean");
        }
    }
    if result["mode"] != "full" && result["mode"] != "scripts-only" {
        bail!("options.json: invalid mode");
    }
    if crate::power::disabled(engine) {
        result["enabled"] = json!(false);
    }
    Ok(result)
}
pub fn conflicts(engine: &Engine) -> Vec<String> {
    ["playintegrityfix", "safetynet-fix"]
        .into_iter()
        .filter(|id| {
            let path = engine.device(&format!("/data/adb/modules/{id}"));
            path.join("module.prop").exists()
                && !path.join("disable").exists()
                && !path.join("remove").exists()
        })
        .map(str::to_owned)
        .collect()
}
pub fn run(engine: &Engine, operation: &str, input: Value) -> Result<Value> {
    match operation {
        "power" => crate::shutdown::status(engine),
        "disable" => crate::shutdown::stop(engine),
        "status" => status(engine),
        "config" => config::current(engine),
        "inspect" => crate::inspection::inspect(engine),
        "require-reboot" => {
            let _lock = files::Lock::acquire(&engine.data)?;
            let raw = files::read(&engine.data.join("custom.pif.json"))?;
            crate::activation::require(engine, &files::hash(raw.as_bytes()), true)?;
            crate::activation::pending(engine)
        }
        "profile-list" => crate::library::profiles(engine),
        "profile-save" => crate::library::save_profile(engine, &input),
        "profile-preview" => crate::library::preview_profile(engine, &input),
        "profile-delete" => crate::library::delete_profile(engine, &input),
        "record-list" => crate::library::records(engine),
        "record-save" => crate::library::save_record(engine, &input, || options(engine)),
        "record-import" => crate::library::import_record(engine, &input, || options(engine)),
        "record-clear" => crate::library::clear_records(engine),
        "preview" => config::preview(engine, &input),
        "import-preview" => {
            let source = text(&input, "source")?;
            if !source.starts_with('/') || source.contains('\0') {
                bail!("expected absolute device file path");
            }
            let content = files::read(&engine.device(source))?;
            if content.is_empty() {
                bail!("source file is empty or missing");
            }
            config::preview(
                engine,
                &json!({"content":content,"format":"auto","migrate":true,"advanced":true}),
            )
        }
        "prepare" => prepare(engine, &input),
        "apply" => apply(engine, &input),
        "restore" => restore(engine, &input),
        "options" => save_options(engine, &input),
        "task-start" => tasks::start(engine, &input),
        "task-status" => tasks::status(engine, text(&input, "id")?),
        "task-cancel" => tasks::cancel(engine, text(&input, "id")?),
        "restart" => restart(engine),
        "diagnose" => {
            let output = if engine.sysroot == std::path::Path::new("/") {
                Command::new("logcat")
                    .args([
                        "-d",
                        "-t",
                        "400",
                        "-s",
                        "PIF/Native",
                        "PIF/Java:DG",
                        "PIF/Java:PS",
                    ])
                    .output()
                    .ok()
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .chars()
                            .take(65536)
                            .collect::<String>()
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            Ok(json!({"status":status(engine)?,"log":output,"verdict_verified":false}))
        }
        _ => bail!("unknown operation: {operation}"),
    }
}
fn replacement_list(engine: &Engine) -> Result<String> {
    files::read(
        &if engine.data.join("custom.app_replace_list.txt").exists() {
            engine.data.join("custom.app_replace_list.txt")
        } else {
            engine.module.join("scripts/app_replace_list.txt")
        },
    )
}
fn compatibility(engine: &Engine) -> Result<Value> {
    Ok(
        json!({"replacement_hash":files::hash(replacement_list(engine)?.as_bytes()),"prop_handler_enabled":!engine.device("/data/adb/disable_prop_handler").exists()}),
    )
}
fn module_enabled(engine: &Engine) -> bool {
    let root = engine.module.parent().unwrap_or(&engine.module);
    !root.join("disable").exists() && !root.join("remove").exists()
}
pub fn active(engine: &Engine) -> Result<bool> {
    if !module_enabled(engine) || crate::power::disabled(engine) {
        return Ok(false);
    }
    let opts = options(engine)?;
    Ok(opts["enabled"] == true
        && conflicts(engine).is_empty()
        && (opts["mode"] == "scripts-only" || configured(engine)?))
}
fn configured(engine: &Engine) -> Result<bool> {
    let current = config::current(engine)?;
    Ok(current["entries"]
        .as_object()
        .is_some_and(|m| !m.is_empty()))
}
pub fn status(engine: &Engine) -> Result<Value> {
    let opts = options(engine)?;
    let conflict = conflicts(engine);
    let config = config::current(engine)?;
    let has_config = config["entries"].as_object().is_some_and(|m| !m.is_empty());
    let boot = files::read(&engine.device("/proc/sys/kernel/random/boot_id"))?
        .trim()
        .to_owned();
    let loaded = files::json_file(&engine.data.join("boot.json"))?;
    let evidence = files::json_file(&engine.data.join("attachment.json"))?;
    // Magisk's installation directory proves neither the setting nor a loaded
    // Zygisk runtime. Its boot-script environment is supplied by the daemon.
    let magisk_runtime = std::env::var("ZYGISK_ENABLED").as_deref() == Ok("1")
        || (!boot.is_empty() && loaded["boot_id"] == boot && loaded["zygisk_enabled"] == true);
    let zygisk = ["zygisksu", "rezygisk", "neozygisk"].iter().any(|id| {
        let path = engine.device(&format!("/data/adb/modules/{id}"));
        path.join("module.prop").exists()
            && !path.join("disable").exists()
            && !path.join("remove").exists()
    }) || magisk_runtime;
    let runtime_active = active(engine)?;
    let activation = crate::activation::pending(engine)?;
    let reboot = activation["reboot_required"] == true
        || loaded["active"] != runtime_active
        || loaded["boot_id"] != boot
        || loaded["options"] != opts
        || loaded["compatibility"] != compatibility(engine)?;
    Ok(
        json!({"api":1,"upstream_revision":UPSTREAM,"options":opts,"has_config":has_config,"lifecycle_eligible":runtime_active,"zygisk_detected":zygisk,
        "conflicts":conflict,"reboot_required":reboot,"process_restart_required":activation["process_restart_required"],"activation_reason":activation["reason"],"attachment":evidence,"boot":loaded,
        "state":if !module_enabled(engine) || !opts["enabled"].as_bool().unwrap_or(true){"disabled"}else if !conflict.is_empty(){"conflict"}else if opts["mode"]=="scripts-only"{"scripts-only"}else if !has_config{"configuration-required"}else if !zygisk{"zygisk-unconfirmed"}else{"configured"},
        "app_replace_list":if engine.data.join("custom.app_replace_list.txt").exists(){files::read(&engine.data.join("custom.app_replace_list.txt"))?}else{files::read(&engine.module.join("scripts/app_replace_list.txt"))?},"data_dir":engine.data,"source":"licenses/play-integrity/SOURCE.txt","license":"licenses/play-integrity/GPL-3.0.txt","verdict_verified":false}),
    )
}
fn prepare(engine: &Engine, input: &Value) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    let preview = config::preview(engine, input)?;
    if input["revision"] != preview["revision"] {
        bail!("configuration changed; preview again");
    }
    let old = files::read(&engine.data.join("custom.pif.json"))?;
    let new = text(&preview, "json")?;
    let token = files::hash(format!("{old}\0{new}").as_bytes());
    files::write_json(
        &engine.data.join("backups").join(format!("{token}.json")),
        &json!({"content":old,"applied_hash":files::hash(new.as_bytes())}),
    )?;
    Ok(json!({"backup":token}))
}
fn apply(engine: &Engine, input: &Value) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    let preview = config::preview(engine, input)?;
    if input["revision"] != preview["revision"] {
        bail!("configuration changed; preview again");
    }
    let path = engine.data.join("custom.pif.json");
    let old = files::read(&path)?;
    let previous_activation = files::read(&engine.data.join("activation.json"))?;
    let new = text(&preview, "json")?;
    let token = files::hash(format!("{old}\0{new}").as_bytes());
    if input.get("backup").is_some_and(|backup| backup != &token) {
        bail!("prepared configuration changed; preview again");
    }
    let backup = engine.data.join("backups").join(format!("{token}.json"));
    files::write_json(
        &backup,
        &json!({"content":old,"applied_hash":files::hash(new.as_bytes())}),
    )?;
    let publication = (|| -> Result<Value> {
        if old != new {
            crate::activation::require(engine, &files::hash(new.as_bytes()), false)?;
        }
        files::atomic(&path, new.as_bytes())?;
        let saved = config::current(engine)?;
        if saved["revision"] != files::hash(new.as_bytes()) {
            bail!("configuration readback failed");
        }
        Ok(saved)
    })();
    match publication {
        Ok(saved) => Ok(json!({"config":saved,"backup":token,"restart_required":true})),
        Err(error) => {
            if let Err(recovery) = files::atomic(&path, old.as_bytes()).and_then(|()| {
                let activation = engine.data.join("activation.json");
                if previous_activation.is_empty() {
                    if activation.exists() {
                        fs::remove_file(activation)?;
                        fs::File::open(&engine.data)?.sync_all()?;
                    }
                    Ok(())
                } else {
                    files::atomic(&activation, previous_activation.as_bytes())
                }
            }) {
                return Err(files::RecoveryError(json!({"path":path,"backup_path":backup,"apply_error":format!("{error:#}"),"restore_error":format!("{recovery:#}")})).into());
            }
            Err(error.context("publication failed; original configuration restored"))
        }
    }
}
fn restore(engine: &Engine, input: &Value) -> Result<Value> {
    let _lock = files::Lock::acquire(&engine.data)?;
    let token = text(input, "backup")?;
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid backup token");
    }
    let backup = engine.data.join("backups").join(format!("{token}.json"));
    let saved = files::json_file(&backup)?;
    let path = engine.data.join("custom.pif.json");
    let current = files::read(&path)?;
    if current != text(&saved, "content")?
        && saved["applied_hash"] != files::hash(current.as_bytes())
    {
        bail!("configuration changed after apply; restore refused");
    }
    crate::activation::require(
        engine,
        &files::hash(text(&saved, "content")?.as_bytes()),
        false,
    )?;
    files::atomic(&path, text(&saved, "content")?.as_bytes()).map_err(|e| {
        files::RecoveryError(json!({"path":path,"backup_path":backup,"reason":e.to_string()}))
    })?;
    Ok(json!({"config":config::current(engine)?,"restart_required":true}))
}
fn save_options(engine: &Engine, input: &Value) -> Result<Value> {
    if input["enabled"] == false {
        let result = crate::shutdown::stop(engine)?;
        let mut opts = options(engine).unwrap_or_else(|_| json!({"enabled":false,"mode":"full","skipdelprop":false,"skippersistprop":false,"skipmatchdevice":false}));
        opts["enabled"] = json!(false);
        return Ok(json!({"options":opts,"reboot_required":true,"shutdown":result}));
    }
    let stop_revision = files::read(&engine.data.join("disabled"))?;
    let _lock = files::Lock::acquire(&engine.data)?;
    if input["enabled"] == true
        && (files::read(&engine.data.join("disabled"))? != stop_revision
            || input
                .get("expected_stop_revision")
                .is_some_and(|r| r != &files::hash(stop_revision.as_bytes())))
    {
        bail!("shutdown changed; refresh power state before enabling");
    }
    let mut opts = options(engine)?;
    for key in [
        "enabled",
        "skipdelprop",
        "skippersistprop",
        "skipmatchdevice",
    ] {
        if let Some(value) = input.get(key) {
            if !value.is_boolean() {
                bail!("{key}: expected boolean");
            }
            opts[key] = value.clone();
        }
    }
    if let Some(mode) = input.get("mode") {
        if mode != "full" && mode != "scripts-only" {
            bail!("unsupported mode");
        }
        opts["mode"] = mode.clone();
    }
    if let Some(list) = input.get("app_replace_list") {
        let list = list
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("app_replace_list must be text"))?;
        crate::paths::validate_list(list)?;
        files::atomic(
            &engine.data.join("custom.app_replace_list.txt"),
            list.as_bytes(),
        )?;
    }
    if input["enabled"] == true {
        opts["enabled"] = json!(true);
    }
    files::write_json(&engine.data.join("options.json"), &opts)?;
    if input["enabled"] == true {
        files::atomic(&engine.data.join("enabled-token"), stop_revision.as_bytes())?;
    }
    opts["enabled"] = json!(!crate::power::disabled(engine));
    Ok(json!({"options":opts,"reboot_required":true}))
}
fn restart(engine: &Engine) -> Result<Value> {
    if engine.sysroot != std::path::Path::new("/") {
        bail!("process restart requires a live device");
    }
    let _lock = files::Lock::acquire(&engine.data)?;
    let revision = config::current(engine)?["revision"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let mut results = Vec::new();
    for process in ["com.google.android.gms.unstable", "com.android.vending"] {
        let output = crate::command::run(
            Command::new("killall").arg(process),
            std::time::Duration::from_secs(2),
        )?;
        results.push(json!({"process":process,"exit_code":output.status.code(),"output":String::from_utf8_lossy(&output.stderr)}));
    }
    if results.iter().all(|r| r["exit_code"] == 0) {
        crate::activation::restarted(engine, &revision)?;
    }
    Ok(json!({"processes":results,"verdict_verified":false}))
}
pub fn stage(engine: &Engine, phase: &str) -> Result<Value> {
    if !["early", "late", "setup", "uninstall"].contains(&phase) {
        bail!("invalid lifecycle phase");
    }
    if phase == "uninstall" {
        let stopped = crate::shutdown::stop(engine)?;
        if stopped["cleanup_pending"] == true {
            bail!(
                "cleanup incomplete; engine data retained: {}",
                engine.data.display()
            );
        }
        let _lock = files::Lock::acquire(&engine.data)?;
        return Ok(json!({"stage":phase,"output":crate::stages::run(engine,"uninstall","full")?}));
    }
    if crate::power::disabled(engine) {
        if phase != "early" {
            return Ok(json!({"skipped":true}));
        }
        let result = crate::shutdown::stop(engine)?;
        files::write_json(
            &engine.data.join("boot.json"),
            &json!({"boot_id":files::read(&engine.device("/proc/sys/kernel/random/boot_id"))?.trim(),"options":options(engine).unwrap_or_else(|_|json!({"enabled":false})),"active":false,"compatibility":compatibility(engine).unwrap_or(Value::Null),"zygisk_enabled":false}),
        )?;
        return Ok(json!({"stage":phase,"shutdown":result}));
    }
    let _lock = files::Lock::acquire(&engine.data)?;
    let opts = options(engine)?;
    let active = active(engine)?;
    let script_phase = if phase == "early" && !active {
        "cleanup"
    } else {
        phase
    };
    if phase != "early" && phase != "uninstall" && !active {
        return Ok(json!({"skipped":true}));
    }
    if active && opts["mode"] == "full" && ["setup", "early"].contains(&phase) {
        crate::paths::validate_list(&replacement_list(engine)?)?;
    }
    files::private_dir(&engine.data)?;
    for (key, file) in [
        ("skipdelprop", "skipdelprop"),
        ("skippersistprop", "skippersistprop"),
        ("skipmatchdevice", "skipmatchdevice"),
    ] {
        let path = engine.data.join(file);
        if opts[key] == true {
            files::atomic(&path, b"")?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let mode = opts["mode"].as_str().unwrap_or("full");
    if phase == "early" && active {
        crate::denylist::manage(engine, mode)?;
    }
    if script_phase == "cleanup" || phase == "uninstall" {
        crate::denylist::restore(engine)?;
        crate::selinux::restore(engine)?;
    }
    let output = crate::stages::run(engine, script_phase, mode)?;
    if phase == "late" && active && !engine.device("/data/adb/disable_prop_handler").exists() {
        crate::selinux::apply(engine)?;
    }
    if phase == "early" {
        files::write_json(
            &engine.data.join("boot.json"),
            &json!({"boot_id":files::read(&engine.device("/proc/sys/kernel/random/boot_id"))?.trim(),"options":opts,"active":active,"compatibility":compatibility(engine)?,"zygisk_enabled":std::env::var("ZYGISK_ENABLED").as_deref()==Ok("1")}),
        )?;
    }
    Ok(json!({"stage":phase,"output":output}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_manager_flags_release_scripts_without_changing_saved_enable_intent() {
        let temporary = tempfile::tempdir().unwrap();
        let engine = Engine {
            data: temporary.path().join("data"),
            module: temporary.path().join("module/play-integrity"),
            sysroot: temporary.path().join("sys"),
        };
        files::atomic(
            &engine.module.join("scripts/stage.sh"),
            b"printf '%s' \"$1\"\n",
        )
        .unwrap();
        files::atomic(
            &engine.data.join("custom.pif.json"),
            br#"{"MODEL":"Fixture"}"#,
        )
        .unwrap();
        save_options(&engine, &json!({"enabled":true})).unwrap();
        assert!(active(&engine).unwrap());
        for flag in ["disable", "remove"] {
            let path = engine.module.parent().unwrap().join(flag);
            files::atomic(&path, b"").unwrap();
            assert!(!active(&engine).unwrap());
            assert_eq!(stage(&engine, "early").unwrap()["output"], "cleanup");
            assert_eq!(stage(&engine, "late").unwrap()["skipped"], true);
            assert_eq!(options(&engine).unwrap()["enabled"], true);
            assert!(!engine.data.join("disabled").exists());
            fs::remove_file(path).unwrap();
            assert!(active(&engine).unwrap());
        }
    }
    #[test]
    fn lifecycle_requires_config_but_preserves_explicit_scripts_mode() {
        let root = tempfile::tempdir().unwrap();
        let e = Engine {
            data: root.path().join("data"),
            module: root.path().join("module"),
            sysroot: root.path().join("sys"),
        };
        fs::create_dir_all(e.module.join("scripts")).unwrap();
        fs::write(e.module.join("scripts/stage.sh"), "printf '%s' \"$1\"\n").unwrap();
        save_options(&e, &json!({"enabled":true})).unwrap();
        assert_eq!(stage(&e, "early").unwrap()["output"], "cleanup");
        assert!(!active(&e).unwrap());
        assert_eq!(stage(&e, "setup").unwrap()["skipped"], true);
        assert_eq!(stage(&e, "late").unwrap()["skipped"], true);
        let mut input = json!({"content":"MODEL=A","format":"prop"});
        input["revision"] = config::preview(&e, &input).unwrap()["revision"].clone();
        apply(&e, &input).unwrap();
        assert!(active(&e).unwrap());
        fs::remove_file(e.data.join("custom.pif.json")).unwrap();
        save_options(&e, &json!({"mode":"scripts-only"})).unwrap();
        assert!(active(&e).unwrap());
        save_options(&e, &json!({"enabled":false})).unwrap();
        assert!(!active(&e).unwrap());
        assert!(save_options(&e, &json!({"app_replace_list":"/system/"})).is_err());
    }
    #[test]
    fn prepared_backup_is_guarded_and_recovery_is_repeatable() {
        let root = tempfile::tempdir().unwrap();
        let e = Engine {
            data: root.path().join("data"),
            module: root.path().join("module"),
            sysroot: root.path().join("sys"),
        };
        let mut input = json!({"content":"MODEL=A","format":"prop"});
        input["revision"] = config::preview(&e, &input).unwrap()["revision"].clone();
        let saved = prepare(&e, &input).unwrap();
        input["backup"] = json!("wrong");
        assert!(apply(&e, &input).is_err());
        assert_eq!(config::current(&e).unwrap()["entries"], json!({}));
        input["backup"] = saved["backup"].clone();
        apply(&e, &input).unwrap();
        restore(&e, &saved).unwrap();
        restore(&e, &saved).unwrap();
    }
    #[test]
    fn preview_apply_stale_revision_and_restore() {
        let root = tempfile::tempdir().unwrap();
        let e = Engine {
            data: root.path().join("data"),
            module: root.path().join("module"),
            sysroot: root.path().join("sys"),
        };
        assert_eq!(options(&e).unwrap()["enabled"], false);
        let mut input = json!({"content":"MODEL=A\nCUSTOM=x","format":"prop"});
        let p = config::preview(&e, &input).unwrap();
        input["revision"] = p["revision"].clone();
        let applied = apply(&e, &input).unwrap();
        assert_eq!(config::current(&e).unwrap()["entries"]["MODEL"], "A");
        assert!(!active(&e).unwrap());
        assert!(apply(&e, &input).is_err());
        restore(&e, &json!({"backup":applied["backup"]})).unwrap();
        assert_eq!(config::current(&e).unwrap()["entries"], json!({}));
        assert!(restore(&e, &json!({"backup":"../escape"})).is_err());
    }
}
