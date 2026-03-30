use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};

pub fn write_string_atomic(path: &Path, contents: &str) -> Result<()> {
    write_bytes_atomic(path, contents.as_bytes())
}

pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;

    let temp_path = unique_temp_path(parent, path);
    let result = (|| -> Result<()> {
        let mut file =
            File::create(&temp_path).with_context(|| format!("create {}", temp_path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("sync {}", temp_path.display()))?;
        drop(file);

        replace_file(&temp_path, path)
    })();

    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

pub fn create_unique_dir(parent: &Path, prefix: &str) -> Result<PathBuf> {
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;

    for attempt in 0..32_u32 {
        let suffix = unique_suffix(attempt);
        let path = parent.join(format!("{prefix}-{suffix}"));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).with_context(|| format!("create {}", path.display())),
        }
    }

    bail!(
        "failed to create a unique directory under {}",
        parent.display()
    )
}

pub fn list_files_recursive(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .with_context(|| format!("read metadata for {}", path.display()))?;

            if metadata.is_dir() {
                stack.push(path);
                continue;
            }

            if metadata.is_file() {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn replace_file(temp_path: &Path, destination: &Path) -> Result<()> {
    match fs::rename(temp_path, destination) {
        Ok(()) => Ok(()),
        Err(_rename_error) if destination.exists() => {
            fs::remove_file(destination)
                .with_context(|| format!("remove {}", destination.display()))?;
            fs::rename(temp_path, destination).with_context(|| {
                format!(
                    "rename {} -> {}",
                    temp_path.display(),
                    destination.display()
                )
            })
        }
        Err(rename_error) => Err(rename_error).with_context(|| {
            format!(
                "rename {} -> {}",
                temp_path.display(),
                destination.display()
            )
        }),
    }
}

fn unique_temp_path(parent: &Path, destination: &Path) -> PathBuf {
    let stem = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("duck-toolbox");
    parent.join(format!(".{stem}.tmp-{}", unique_suffix(0)))
}

fn unique_suffix(attempt: u32) -> String {
    format!("{:x}-{}-{}", unix_now_nanos(), process::id(), attempt)
}

fn unix_now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::{create_unique_dir, list_files_recursive, write_bytes_atomic};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let root =
            env::temp_dir().join(format!("duck-toolbox-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn write_bytes_atomic_overwrites_existing_file() {
        let root = temp_root("atomic");
        let file = root.join("value.txt");

        write_bytes_atomic(&file, b"first").unwrap();
        write_bytes_atomic(&file, b"second").unwrap();

        assert_eq!(fs::read(&file).unwrap(), b"second");
    }

    #[test]
    fn create_unique_dir_creates_distinct_directories() {
        let root = temp_root("dirs");
        let first = create_unique_dir(&root, "run").unwrap();
        let second = create_unique_dir(&root, "run").unwrap();

        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
    }

    #[test]
    fn list_files_recursive_returns_nested_files_only() {
        let root = temp_root("files");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("a.txt"), b"a").unwrap();
        fs::write(nested.join("b.txt"), b"b").unwrap();

        let mut files = list_files_recursive(&root)
            .unwrap()
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        files.sort();

        assert_eq!(files, vec!["a.txt", "b.txt"]);
    }
}
