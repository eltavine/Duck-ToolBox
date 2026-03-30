use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

const DATA_ROOT_ENV: &str = "DUCK_TOOLBOX_DATA_ROOT";
const ANDROID_DATA_ROOT: &str = "/data/adb/duck-toolbox";

#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub root: PathBuf,
    pub data_root: PathBuf,
    pub var_dir: PathBuf,
    pub profile_path: PathBuf,
    pub profile_secrets_path: PathBuf,
    pub outputs_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub log_path: PathBuf,
    pub wrapper_path: PathBuf,
    pub binary_path: PathBuf,
}

impl AppPaths {
    pub fn resolve(path: &str) -> Result<PathBuf> {
        Ok(Self::discover()?.resolve_in_root(path))
    }

    pub fn discover() -> Result<Self> {
        let root = if let Ok(explicit) = env::var("DUCK_TOOLBOX_ROOT") {
            resolve_root(PathBuf::from(explicit))?
        } else {
            find_module_root()?
        };

        let data_root = resolve_data_root(
            &root,
            env::var_os(DATA_ROOT_ENV).map(PathBuf::from),
            cfg!(target_os = "android"),
        );
        let var_dir = data_root.join("var");
        let logs_dir = var_dir.join("logs");
        let outputs_dir = var_dir.join("outputs");
        let tmp_dir = var_dir.join("tmp");

        Ok(Self {
            root: root.clone(),
            data_root,
            var_dir: var_dir.clone(),
            profile_path: var_dir.join("profile.toml"),
            profile_secrets_path: var_dir.join("profile.secrets.toml"),
            outputs_dir,
            tmp_dir,
            logs_dir: logs_dir.clone(),
            log_path: logs_dir.join("duckd.log"),
            wrapper_path: root.join("bin").join("duckctl.sh"),
            binary_path: root.join("bin").join("duckd"),
        })
    }

    pub fn ensure_runtime_dirs(&self) -> Result<()> {
        for dir in [
            &self.var_dir,
            &self.outputs_dir,
            &self.tmp_dir,
            &self.logs_dir,
        ] {
            fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        Ok(())
    }

    pub fn resolve_in_root(&self, path: &str) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            resolve_relative_path(&self.root, &self.data_root, candidate)
        }
    }
}

fn resolve_data_root(
    module_root: &Path,
    explicit_data_root: Option<PathBuf>,
    target_is_android: bool,
) -> PathBuf {
    if let Some(explicit) = explicit_data_root.filter(|path| !path.as_os_str().is_empty()) {
        if explicit.is_absolute() {
            return explicit;
        }
        return module_root.join(explicit);
    }

    if target_is_android {
        return PathBuf::from(ANDROID_DATA_ROOT);
    }

    module_root.to_path_buf()
}

fn resolve_relative_path(module_root: &Path, data_root: &Path, candidate: &Path) -> PathBuf {
    match candidate.components().next() {
        Some(std::path::Component::Normal(component)) if component == "var" => {
            data_root.join(candidate)
        }
        _ => module_root.join(candidate),
    }
}

fn find_module_root() -> Result<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(env::current_dir().context("read current directory")?);

    if let Ok(exe) = env::current_exe() {
        candidates.push(exe);
    }

    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    for candidate in candidates {
        for ancestor in candidate.ancestors() {
            if looks_like_module_root(ancestor) {
                return resolve_root(ancestor.to_path_buf());
            }
        }
    }

    bail!("failed to locate Duck ToolBox module root")
}

fn resolve_root(root: PathBuf) -> Result<PathBuf> {
    let root = if root.is_absolute() {
        root
    } else {
        env::current_dir()
            .context("read current directory")?
            .join(root)
    };

    if !looks_like_module_root(&root) {
        bail!("{} is not a valid Duck ToolBox module root", root.display());
    }

    Ok(root)
}

fn looks_like_module_root(path: &Path) -> bool {
    path.join("module.prop").is_file()
        && (path.join("ui").is_dir() || path.join("webroot").is_dir() || path.join("bin").is_dir())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{ANDROID_DATA_ROOT, resolve_data_root, resolve_relative_path};

    #[test]
    fn non_android_defaults_data_root_to_module_root() {
        let module_root = Path::new("/workspace/duck-toolbox");

        let data_root = resolve_data_root(module_root, None, false);

        assert_eq!(data_root, module_root);
    }

    #[test]
    fn android_defaults_data_root_to_shared_adb_directory() {
        let data_root = resolve_data_root(Path::new("/data/adb/modules/duck-toolbox"), None, true);

        assert_eq!(data_root, PathBuf::from(ANDROID_DATA_ROOT));
    }

    #[test]
    fn relative_var_paths_resolve_under_data_root() {
        let resolved = resolve_relative_path(
            Path::new("/data/adb/modules/duck-toolbox"),
            Path::new("/data/adb/duck-toolbox"),
            Path::new("var/outputs/keybox.xml"),
        );

        assert_eq!(
            resolved,
            PathBuf::from("/data/adb/duck-toolbox/var/outputs/keybox.xml")
        );
    }

    #[test]
    fn non_var_relative_paths_stay_under_module_root() {
        let resolved = resolve_relative_path(
            Path::new("/data/adb/modules/duck-toolbox"),
            Path::new("/data/adb/duck-toolbox"),
            Path::new("bin/duckctl.sh"),
        );

        assert_eq!(
            resolved,
            PathBuf::from("/data/adb/modules/duck-toolbox/bin/duckctl.sh")
        );
    }
}
