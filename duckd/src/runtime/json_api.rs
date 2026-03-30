use std::{
    fs::OpenOptions,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Error;
use serde::Serialize;
use serde_json::{Value, json};

use super::{errors::error_code, paths::AppPaths};

const MAX_LOG_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct JsonError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub fn success<T>(command: &str, data: T) -> Value
where
    T: Serialize,
{
    json!({
        "ok": true,
        "command": command,
        "data": data,
        "error": Value::Null,
        "ts": unix_now(),
    })
}

pub fn failure(command: &str, error: &Error, details: Option<Value>) -> Value {
    json!({
        "ok": false,
        "command": command,
        "data": Value::Null,
        "error": JsonError {
            code: error_code(error).to_owned(),
            message: error.to_string(),
            details,
        },
        "ts": unix_now(),
    })
}

pub fn emit(value: &Value) {
    println!(
        "{}",
        serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
    );
}

pub fn append_log(paths: &AppPaths, value: &Value) {
    if paths.ensure_runtime_dirs().is_err() {
        return;
    }

    let mut line = value.clone();
    if let Some(object) = line.as_object_mut()
        && !object.contains_key("ts")
    {
        object.insert("ts".into(), json!(unix_now()));
    }

    rotate_log_if_needed(paths);

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log_path)
    {
        let _ = writeln!(file, "{}", line);
    }
}

fn rotate_log_if_needed(paths: &AppPaths) {
    let Ok(metadata) = std::fs::metadata(&paths.log_path) else {
        return;
    };
    if metadata.len() < MAX_LOG_BYTES {
        return;
    }

    let rotated = paths.logs_dir.join("duckd.log.1");
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(&paths.log_path, rotated);
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
