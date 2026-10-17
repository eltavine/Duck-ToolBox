use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

use anyhow::Result;
use base64ct::{Base64, Encoding};
use duck_core::AppPaths;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::error::MaterialError;

pub(crate) const MAX_INPUT: usize = 256 * 1024;
// Two base64 uploads plus JSON must fit in one shell argument even with 4 KiB pages
// (Linux MAX_ARG_STRLEN = PAGE_SIZE * 32). Larger captures use device file paths.
pub(crate) const MAX_INLINE: usize = 32 * 1024;
const MAX_JSON: usize = MAX_INLINE * 3;

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum InputFile {
    Path { path: String },
    Content { base64: String },
}

impl Drop for InputFile {
    fn drop(&mut self) {
        if let Self::Content { base64 } = self {
            base64.zeroize();
        }
    }
}

impl InputFile {
    pub fn read(&self, paths: &AppPaths) -> Result<Zeroizing<Vec<u8>>> {
        match self {
            Self::Path { path } => read(&paths.resolve_in_root(path)),
            Self::Content { base64 } => {
                if base64.len() > MAX_INLINE.div_ceil(3) * 4 {
                    return Err(MaterialError::Input(
                        "inline input exceeds 32 KiB; use a device file path for larger captures",
                    )
                    .into());
                }
                let bytes = Zeroizing::new(
                    Base64::decode_vec(base64)
                        .map_err(|_| MaterialError::Input("input is not valid base64"))?,
                );
                if bytes.len() > MAX_INLINE {
                    return Err(MaterialError::Input(
                        "inline input exceeds 32 KiB; use a device file path for larger captures",
                    )
                    .into());
                }
                check_size(&bytes)?;
                Ok(bytes)
            }
        }
    }
}

pub(crate) fn read_json<T: DeserializeOwned>() -> Result<T> {
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(MAX_JSON as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| MaterialError::Input("cannot read request"))?;
    if bytes.len() > MAX_JSON {
        return Err(MaterialError::Input("request exceeds its size limit").into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| MaterialError::Input("request fields or values are invalid").into())
}

pub(crate) fn read(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| MaterialError::Input("cannot open input as a regular file"))?;
    let metadata = file
        .metadata()
        .map_err(|_| MaterialError::Input("cannot inspect input file"))?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT as u64 {
        return Err(MaterialError::Input("input must be a regular file of at most 256 KiB").into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| MaterialError::Input("cannot read input file"))?;
    check_size(&bytes)?;
    Ok(bytes)
}

fn check_size(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT {
        return Err(MaterialError::Input("input must contain 1..262144 bytes").into());
    }
    Ok(())
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Serialize)]
pub(crate) struct Artifacts {
    pub material_path: String,
    pub report_path: String,
}

#[derive(Serialize)]
pub(crate) struct Outcome<T> {
    pub report: T,
    pub artifacts: Option<Artifacts>,
}

pub(crate) fn export(
    paths: &AppPaths,
    name: &str,
    material: &[u8],
    report: &impl Serialize,
) -> Result<Artifacts> {
    let save = || -> Result<Artifacts> {
        fs::create_dir_all(&paths.outputs_dir)?;
        let mut builder = tempfile::Builder::new();
        builder.prefix("tee-attestation-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir_in(&paths.outputs_dir)?;
        let material_path = directory.path().join(name);
        let report_path = directory.path().join("report.json");
        write_private(&material_path, material)?;
        write_private(&report_path, &serde_json::to_vec_pretty(report)?)?;
        let artifacts = Artifacts {
            material_path: material_path.display().to_string(),
            report_path: report_path.display().to_string(),
        };
        let _ = directory.keep();
        Ok(artifacts)
    };
    save()
        .map_err(|_| MaterialError::Output("cannot save complete private output directory").into())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
