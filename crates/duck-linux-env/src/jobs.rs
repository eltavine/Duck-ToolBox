use crate::{
    archive, code_of, fail,
    model::{Config, Environment, MAX_ENVIRONMENTS, valid_digest, valid_id},
    native,
    store::{self, Store},
};
use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub id: String,
    pub config: Config,
    pub path: String,
    pub sha256: String,
    pub confirmed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub token: String,
    pub request: ImportRequest,
    pub phase: String,
    pub entries: u64,
    pub bytes: u64,
    pub code: String,
    pub message: String,
}
pub fn start(ctx: &Context, request: ImportRequest) -> Result<Job> {
    native::require(ctx)?;
    ensure!(
        request.confirmed,
        fail(
            "confirmation_required",
            "confirm import of a trusted rootfs"
        )
    );
    ensure!(
        valid_id(&request.id)
            && valid_digest(&request.sha256)
            && Path::new(&request.path).is_absolute(),
        fail("invalid_input", "invalid import request")
    );
    request.config.validate()?;
    let store = Store::lock(ctx)?;
    ensure!(
        store::list(&store.base)?.len() < MAX_ENVIRONMENTS,
        fail("invalid_input", "environment limit reached")
    );
    ensure!(
        !store.env_dir(&request.id)?.try_exists()?,
        fail("invalid_input", "environment ID already exists")
    );
    let token = crate::model::revision(&(duck_core::unix_now(), std::process::id(), &request))?;
    let job = Job {
        token: token.clone(),
        request,
        phase: "queued".into(),
        entries: 0,
        bytes: 0,
        code: String::new(),
        message: String::new(),
    };
    store::save(&store.base.join("import.json"), &job)?;
    let mut command = Command::new(std::env::current_exe()?);
    native::detach(&mut command);
    let result = command
        .args(["linux-env", "import-worker", "--token", &token])
        .env("DUCK_TOOLBOX_ROOT", &ctx.paths.root)
        .env("DUCK_TOOLBOX_DATA_ROOT", &ctx.paths.data_root)
        .stdin(store.lock.try_clone()?)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(e) = result {
        let mut failed = job.clone();
        failed.phase = "failed".into();
        failed.code = "import_failed".into();
        failed.message = e.to_string();
        store::save(&store.base.join("import.json"), &failed)?;
        return Err(e.into());
    }
    Ok(job)
}
pub fn snapshot(base: &Path) -> Result<Option<Job>> {
    let path = base.join("import.json");
    if !path.try_exists()? {
        return Ok(None);
    }
    let mut job: Job = store::load(&path)?;
    if !matches!(job.phase.as_str(), "done" | "failed") && !store::busy(base)? {
        job.phase = "failed".into();
        job.code = "interrupted".into();
        job.message = "Import worker stopped before recording completion".into();
    }
    Ok(Some(job))
}
pub fn cancel(ctx: &Context, token: &str) -> Result<bool> {
    native::require(ctx)?;
    ensure!(
        valid_digest(token),
        fail("invalid_input", "invalid job token")
    );
    let base = store::base(ctx);
    let job: Job = store::load(&base.join("import.json"))?;
    ensure!(
        job.token == token,
        fail("state_changed", "import job changed")
    );
    ensure!(
        !matches!(job.phase.as_str(), "done" | "failed"),
        fail("state_changed", "import is already complete")
    );
    store::save(&base.join("cancel.json"), &token)?;
    Ok(true)
}
pub fn worker(ctx: &Context, token: &str) -> Result<()> {
    native::require(ctx)?;
    let base = store::base(ctx);
    native::check_lease(&base.join("operation.lock"))?;
    let path = base.join("import.json");
    let mut job: Job = store::load(&path)?;
    ensure!(
        job.token == token && valid_digest(token) && job.phase == "queued" && job.request.confirmed,
        fail("state_changed", "import authorization changed")
    );
    let deadline = Instant::now() + Duration::from_secs(1800);
    let check = || -> Result<()> {
        ensure!(
            Instant::now() < deadline,
            fail("operation_timeout", "rootfs import exceeded 30 minutes")
        );
        let cancel_path = base.join("cancel.json");
        let cancel: Option<String> = if cancel_path.try_exists()? {
            Some(store::load(&cancel_path)?)
        } else {
            None
        };
        ensure!(
            cancel.as_deref() != Some(token),
            fail("cancelled", "import cancelled")
        );
        Ok(())
    };
    let result = (|| -> Result<()> {
        cleanup_staging(&base)?;
        let staging = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(&base)?;
        store::private_dir(staging.path())?;
        let rootfs = staging.path().join("rootfs");
        fs::create_dir(&rootfs)?;
        let mut last = Instant::now() - Duration::from_secs(1);
        job.phase = "copying".into();
        store::save(&path, &job)?;
        let source_path = job.request.path.clone();
        let expected = job.request.sha256.clone();
        let mut archive = archive::snapshot(
            Path::new(&source_path),
            &staging.path().join("archive"),
            &expected,
            |bytes| {
                check()?;
                if last.elapsed() > Duration::from_millis(300) {
                    job.bytes = bytes;
                    store::save(&path, &job)?;
                    last = Instant::now();
                }
                Ok(())
            },
        )?;
        job.phase = "validating".into();
        job.bytes = 0;
        job.entries = 0;
        store::save(&path, &job)?;
        let (entries, expanded) =
            archive::extract(&mut archive, &rootfs, &check, |phase, entries, bytes| {
                check()?;
                if job.phase != phase || last.elapsed() > Duration::from_millis(300) {
                    job.phase = phase.into();
                    job.entries = entries;
                    job.bytes = bytes;
                    store::save(&path, &job)?;
                    last = Instant::now();
                }
                Ok(())
            })?;
        check()?;
        let env = Environment {
            id: job.request.id.clone(),
            instance: token.into(),
            archive_sha256: job.request.sha256.clone(),
            expanded_bytes: expanded,
            entries,
            created_at: duck_core::unix_now(),
            config: job.request.config.clone(),
        };
        env.validate()?;
        store::save(&staging.path().join("environment.json"), &env)?;
        drop(archive);
        fs::remove_file(staging.path().join("archive"))?;
        let destination = store::env_dir(&base, &env.id)?;
        ensure!(
            !destination.try_exists()?,
            fail("state_changed", "environment already exists")
        );
        fs::rename(staging.path(), &destination)?;
        job.entries = entries;
        job.bytes = expanded;
        Ok(())
    })();
    match result {
        Ok(()) => job.phase = "done".into(),
        Err(e) => {
            job.phase = "failed".into();
            job.code = code_of(&e).into();
            job.message = format!("{e:#}");
        }
    }
    store::save(&path, &job)?;
    Ok(())
}

// Only called by the import worker while it owns the global operation lease.
fn cleanup_staging(base: &Path) -> Result<()> {
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(".import-") {
            ensure!(
                entry.file_type()?.is_dir(),
                fail("state_unavailable", "invalid import staging directory")
            );
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
