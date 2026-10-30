//! Platform adapter for the toolbox's ordered global-property boot pass.
use anyhow::Result;
use std::{path::Path, process::Command};
fn legacy(module: &Path, fallback: bool) -> Result<()> {
    let mut command = Command::new(if cfg!(target_os = "android") {
        "/system/bin/sh"
    } else {
        "/bin/sh"
    });
    command.arg(module.join("prop.sh"));
    if fallback {
        command.arg("--fallback");
    }
    crate::bounded::output(command).map(|_| ())
}
pub fn stages(module: &Path) -> Result<()> {
    let properties = legacy(module, false);
    let engine = module.join("play-integrity/pif-engine");
    let integrity = if engine.is_file() {
        let mut command = Command::new(&engine);
        command
            .args(["stage", "late"])
            .env("PIF_ENGINE_MODULE", module.join("play-integrity"));
        crate::bounded::output(command)
            .map(|_| ())
            .or_else(|_| legacy(module, true))
    } else {
        Ok(())
    };
    properties.and(integrity)
}
