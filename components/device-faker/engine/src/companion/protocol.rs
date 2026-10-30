use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::net::UnixStream,
};
use zygisk_api::api::{V4, ZygiskApi};

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteLogRequest {
    pub lines: Vec<String>,
}

pub fn send_companion_command(
    api: &mut ZygiskApi<V4>,
    request: &CompanionRequest,
) -> anyhow::Result<CompanionResponse> {
    let payload = serde_json::to_vec(request)?;
    if payload.len() > MAX_FRAME {
        anyhow::bail!("companion request exceeds limit");
    }
    api.with_companion(|stream| -> anyhow::Result<CompanionResponse> {
        stream.set_nonblocking(true)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut length = (payload.len() as u32).to_le_bytes();
        transfer(stream, &mut length, true, deadline)?;
        let mut payload = payload;
        transfer(stream, &mut payload, true, deadline)?;
        let bytes = frame(stream, deadline)?;
        Ok(serde_json::from_slice(&bytes)?)
    })
    .map_err(|error| anyhow::anyhow!("companion connection failed: {error}"))?
}

const MAX_FRAME: usize = 256 * 1024;
fn transfer(
    mut stream: &UnixStream,
    bytes: &mut [u8],
    writing: bool,
    deadline: std::time::Instant,
) -> anyhow::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("companion exchange timed out");
        }
        let result = if writing {
            stream.write(&bytes[offset..])
        } else {
            stream.read(&mut bytes[offset..])
        };
        match result {
            Ok(0) => anyhow::bail!("companion connection closed"),
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn frame(stream: &UnixStream, deadline: std::time::Instant) -> anyhow::Result<Vec<u8>> {
    let mut length = [0; 4];
    transfer(stream, &mut length, false, deadline)?;
    let size = u32::from_le_bytes(length) as usize;
    if size == 0 || size > MAX_FRAME {
        anyhow::bail!("invalid companion frame length");
    }
    let mut bytes = vec![0; size];
    transfer(stream, &mut bytes, false, deadline)?;
    Ok(bytes)
}
pub(super) fn read_companion_request(stream: &mut UnixStream) -> anyhow::Result<CompanionRequest> {
    stream.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    Ok(serde_json::from_slice(&frame(stream, deadline)?)?)
}
pub(crate) fn write_companion_response(
    stream: &mut UnixStream,
    response: &CompanionResponse,
) -> anyhow::Result<()> {
    stream.set_nonblocking(true)?;
    let mut bytes = serde_json::to_vec(response)?;
    if bytes.len() > MAX_FRAME {
        anyhow::bail!("companion response exceeds limit");
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut length = (bytes.len() as u32).to_le_bytes();
    transfer(stream, &mut length, true, deadline)?;
    transfer(stream, &mut bytes, true, deadline)
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ResetpropSessionRequest {
    pub(super) pid: u32,
    pub(super) start: u64,
    pub(super) generation: String,
    pub(super) props: HashMap<String, String>,
    /// `custom_props` 里用户显式写出的 key：不受「本机不存在就不写」门控。
    #[serde(default)]
    pub(super) explicit_props: Vec<String>,
    pub(super) delete_props: Vec<String>,
    pub(super) package_name: String,
    #[serde(default)]
    pub(super) density: Option<u32>,
    /// 全局 debug 开关：debug=false 时 companion 完全不打印日志。
    #[serde(default)]
    pub(super) debug: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd", content = "payload")]
pub enum CompanionRequest {
    Apply(ResetpropSessionRequest),
    WriteLog(WriteLogRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CompanionResponse {
    pub status: i32,
    pub message: Option<String>,
    pub backups: Option<HashMap<String, String>>,
}

impl CompanionResponse {
    pub fn ok() -> Self {
        Self {
            status: 0,
            message: None,
            backups: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            status: -1,
            message: Some(msg.into()),
            backups: None,
        }
    }

    pub fn ok_with_backups(backups: HashMap<String, String>) -> Self {
        Self {
            status: 0,
            message: None,
            backups: Some(backups),
        }
    }
}
