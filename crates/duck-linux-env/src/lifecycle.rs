use crate::{
    fail,
    model::{Config, Environment, Runtime, Target, revision},
    mounts, native, scope,
    store::{self, Store},
};
use anyhow::{Result, ensure};
use duck_core::Context;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Serialize)]
pub struct View {
    pub environment: Environment,
    pub revision: String,
    pub phase: String,
    pub message: String,
    pub runtime: Option<Runtime>,
}
pub fn view(base: &Path, environment: Environment) -> Result<View> {
    let dir = store::env_dir(base, &environment.id)?;
    let path = dir.join("runtime.json");
    let runtime: Option<Runtime> = if path.try_exists()? {
        Some(store::load(&path)?)
    } else {
        None
    };
    let busy = store::locked(&dir.join("environment.lock"))?;
    let populated = scope::populated(&environment.instance)?;
    let (phase, message) = match &runtime {
        Some(r) if r.instance == environment.instance => {
            r.config.validate()?;
            let init_alive = r
                .init
                .as_ref()
                .map(native::alive)
                .transpose()?
                .unwrap_or(false);
            if busy && r.phase == "running" && !init_alive {
                (
                    "unknown".into(),
                    "Guest init ended while the supervisor lease is still held".into(),
                )
            } else if !busy && (populated || init_alive) {
                (
                    "unknown".into(),
                    "Guest processes remain after their supervisor ended".into(),
                )
            } else if busy || matches!(r.phase.as_str(), "stopped" | "failed") {
                (r.phase.clone(), r.message.clone())
            } else {
                (
                    "failed".into(),
                    "Runtime ended before recording completion".into(),
                )
            }
        }
        _ if busy || populated => (
            "unknown".into(),
            "Environment lease or populated cgroup lacks matching runtime state".into(),
        ),
        _ => ("stopped".into(), String::new()),
    };
    Ok(View {
        revision: revision(&environment)?,
        environment,
        phase,
        message,
        runtime,
    })
}
pub fn start(ctx: &Context, target: Target) -> Result<bool> {
    native::require(ctx)?;
    let store = Store::lock(ctx)?;
    let env = store.environment(&target.id)?;
    target.check(&env)?;
    start_locked(ctx, &store, &env)
}
fn start_locked(ctx: &Context, store: &Store, env: &Environment) -> Result<bool> {
    mounts::helper(&ctx.paths.root.join("bin/tini-arm64"))?;
    let dir = store.env_dir(&env.id)?;
    ensure!(
        !scope::populated(&env.instance)?,
        fail("operation_busy", "environment cgroup is populated")
    );
    let lease = store::open(&dir.join("environment.lock"), true)?;
    lease.try_lock().map_err(|_| {
        fail(
            "operation_busy",
            "environment is already starting or running",
        )
    })?;
    if dir.join("runtime.json").try_exists()? {
        let runtime: Runtime = store::load(&dir.join("runtime.json"))?;
        ensure!(
            !runtime
                .init
                .as_ref()
                .map(native::alive)
                .transpose()?
                .unwrap_or(false),
            fail("operation_busy", "guest init is still alive")
        );
    }
    let runtime = Runtime {
        instance: env.instance.clone(),
        supervisor: None,
        init: None,
        phase: "starting".into(),
        message: String::new(),
        pid_namespace: false,
        config: env.config.clone(),
    };
    store::save(&dir.join("runtime.json"), &runtime)?;
    let mut command = Command::new(std::env::current_exe()?);
    native::detach(&mut command);
    let result = command
        .args([
            "linux-env",
            "supervise",
            "--id",
            &env.id,
            "--instance",
            &env.instance,
        ])
        .env("DUCK_TOOLBOX_ROOT", &ctx.paths.root)
        .env("DUCK_TOOLBOX_DATA_ROOT", &ctx.paths.data_root)
        .stdin(lease)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(e) = result {
        let mut runtime = runtime;
        runtime.phase = "failed".into();
        runtime.message = e.to_string();
        store::save(&dir.join("runtime.json"), &runtime)?;
        return Err(e.into());
    }
    Ok(true)
}
pub fn stop(ctx: &Context, target: Target) -> Result<bool> {
    native::require(ctx)?;
    let store = Store::lock(ctx)?;
    let env = store.environment(&target.id)?;
    target.check(&env)?;
    let path = store.env_dir(&env.id)?.join("runtime.json");
    let mut runtime: Runtime = if path.try_exists()? {
        store::load(&path)?
    } else {
        Runtime {
            instance: env.instance.clone(),
            supervisor: None,
            init: None,
            phase: "unknown".into(),
            message: String::new(),
            pid_namespace: false,
            config: env.config.clone(),
        }
    };
    ensure!(
        runtime.instance == env.instance,
        fail("state_changed", "runtime instance differs")
    );
    let mut handles = Vec::new();
    track_processes(&runtime, &mut handles)?;
    ensure!(
        !handles.is_empty() || scope::populated(&env.instance)?,
        fail("state_changed", "environment has already stopped; refresh")
    );
    if let Some(first) = handles.first() {
        native::signal(first, libc::SIGTERM)?;
    } else {
        scope::kill(&env.instance)?;
    }
    wait_processes(&handles, Duration::from_secs(6));
    kill_processes(&handles)?;
    wait_processes(&handles, Duration::from_secs(2));
    ensure!(
        handles.iter().all(native::exited),
        fail(
            "operation_unknown",
            "process termination could not be confirmed"
        )
    );
    // A queued supervisor may publish init after this stop request captured its state.
    // Its lease must end before reading that final process identity and recording success.
    ensure!(
        !store::locked(&store.env_dir(&env.id)?.join("environment.lock"))?,
        fail(
            "operation_unknown",
            "environment supervisor lease is still held"
        )
    );
    if path.try_exists()? {
        runtime = store::load(&path)?;
        ensure!(
            runtime.instance == env.instance,
            fail("state_changed", "runtime instance changed during stop")
        );
        track_processes(&runtime, &mut handles)?;
    }
    kill_processes(&handles)?;
    scope::finish(&env.instance)?;
    wait_processes(&handles, Duration::from_secs(2));
    ensure!(
        handles.iter().all(native::exited),
        fail(
            "operation_unknown",
            "guest termination could not be confirmed"
        )
    );
    let mut completed = runtime;
    completed.phase = "stopped".into();
    completed.message.clear();
    store::save(&store.env_dir(&env.id)?.join("runtime.json"), &completed)?;
    Ok(true)
}
fn track_processes(runtime: &Runtime, handles: &mut Vec<fs::File>) -> Result<()> {
    for identity in [runtime.supervisor.as_ref(), runtime.init.as_ref()]
        .into_iter()
        .flatten()
    {
        if native::alive(identity)? {
            match native::checked_pidfd(identity) {
                Ok(handle) => handles.push(handle),
                Err(_) if !native::alive(identity)? => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}
fn wait_processes(handles: &[fs::File], duration: Duration) {
    let deadline = Instant::now() + duration;
    while handles.iter().any(|fd| !native::exited(fd)) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(40));
    }
}
fn kill_processes(handles: &[fs::File]) -> Result<()> {
    for handle in handles {
        if !native::exited(handle) {
            native::signal(handle, libc::SIGKILL)?;
        }
    }
    Ok(())
}
pub fn start_boot(ctx: &Context, target: Target) -> Result<bool> {
    native::require(ctx)?;
    let store = Store::lock(ctx)?;
    let env = store.environment(&target.id)?;
    target.check(&env)?;
    ensure!(
        env.config.auto_start,
        fail("state_changed", "boot start was disabled")
    );
    let path = store.env_dir(&env.id)?.join("boot.json");
    let boot = native::boot()?;
    let attempted: Option<(String, String)> = if path.try_exists()? {
        Some(store::load(&path)?)
    } else {
        None
    };
    ensure!(
        attempted.as_ref() != Some(&(boot.clone(), env.instance.clone())),
        fail(
            "already_attempted",
            "environment already attempted in this boot"
        )
    );
    store::save(&path, &(boot, env.instance.clone()))?;
    start_locked(ctx, &store, &env)
}
pub fn stop_all(ctx: &Context) -> Result<bool> {
    native::require(ctx)?;
    if let Some(job) = crate::jobs::snapshot(&store::base(ctx))?
        && !matches!(job.phase.as_str(), "done" | "failed")
    {
        crate::jobs::cancel(ctx, &job.token)?;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while store::busy(&store::base(ctx))? && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    ensure!(
        !store::busy(&store::base(ctx))?,
        fail("operation_busy", "import worker is still active")
    );
    for env in store::list(&store::base(ctx))? {
        let view = view(&store::base(ctx), env.clone())?;
        if matches!(view.phase.as_str(), "stopped" | "failed") {
            continue;
        }
        stop(
            ctx,
            Target {
                id: env.id.clone(),
                revision: revision(&env)?,
                confirmed: true,
            },
        )?;
    }
    Ok(true)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub target: Target,
    pub config: Config,
}
pub fn save(ctx: &Context, request: SaveRequest) -> Result<Environment> {
    native::require(ctx)?;
    request.config.validate()?;
    let store = Store::lock(ctx)?;
    let mut env = store.environment(&request.target.id)?;
    request.target.check(&env)?;
    env.config = request.config;
    store.save_environment(&env)?;
    Ok(env)
}
pub fn delete(ctx: &Context, target: Target) -> Result<bool> {
    native::require(ctx)?;
    let store = Store::lock(ctx)?;
    let env = store.environment(&target.id)?;
    target.check(&env)?;
    let dir = store.env_dir(&env.id)?;
    ensure!(
        !scope::populated(&env.instance)?,
        fail("operation_busy", "environment cgroup is populated")
    );
    let lease = store::open(&dir.join("environment.lock"), true)?;
    lease
        .try_lock()
        .map_err(|_| fail("operation_busy", "stop the environment before removing it"))?;
    if dir.join("runtime.json").try_exists()? {
        let runtime: Runtime = store::load(&dir.join("runtime.json"))?;
        ensure!(
            !runtime
                .init
                .as_ref()
                .map(native::alive)
                .transpose()?
                .unwrap_or(false),
            fail("operation_busy", "guest init is still alive")
        );
    }
    let canonical = fs::canonicalize(&dir)?;
    ensure!(
        canonical.parent() == Some(fs::canonicalize(&store.base)?.as_path()),
        fail("state_unavailable", "environment directory escaped storage")
    );
    let mountinfo = fs::read_to_string("/proc/self/mountinfo")?;
    ensure!(
        !mountinfo
            .lines()
            .filter_map(|line| line.split_whitespace().nth(4))
            .any(|mount| Path::new(
                &mount
                    .replace("\\040", " ")
                    .replace("\\011", "\t")
                    .replace("\\012", "\n")
                    .replace("\\134", "\\")
            )
            .starts_with(&canonical)),
        fail("operation_busy", "environment contains an external mount")
    );
    scope::remove(&env.instance)?;
    fs::remove_dir_all(&dir)?;
    Ok(true)
}
pub fn shell(ctx: &Context, id: &str) -> Result<i32> {
    native::require(ctx)?;
    let base = store::base(ctx);
    let env = store::environment(&base, id)?;
    let runtime: Runtime = store::load(&store::env_dir(&base, id)?.join("runtime.json"))?;
    ensure!(
        runtime.instance == env.instance && runtime.phase == "running",
        fail("state_changed", "start the environment before entering it")
    );
    let init = runtime
        .init
        .ok_or_else(|| fail("state_changed", "guest init unavailable"))?;
    let handle = native::checked_pidfd(&init)?;
    let mount = fs::File::open(format!("/proc/{}/ns/mnt", init.pid))?;
    let pid = if runtime.pid_namespace {
        Some(fs::File::open(format!("/proc/{}/ns/pid", init.pid))?)
    } else {
        None
    };
    let root = fs::File::open(format!("/proc/{}/root", init.pid))?;
    ensure!(
        native::alive(&init)? && !native::exited(&handle),
        fail("state_changed", "guest init ended")
    );
    let scope = if runtime.pid_namespace {
        None
    } else {
        Some(scope::attach(&env.instance)?)
    };
    native::enter(&mount, pid.as_ref())?;
    let setup = native::ChildSetup::from_root_fd(root)?.in_scope(scope);
    let mut command = Command::new("/bin/sh");
    command
        .arg("-l")
        .env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/root")
        .env("LANG", "C.UTF-8")
        .env(
            "TERM",
            std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into()),
        );
    crate::supervisor::prepare(&mut command, setup, false);
    Ok(command.status()?.code().unwrap_or(1))
}
pub fn logs(ctx: &Context, id: &str) -> Result<String> {
    let env = store::environment(&store::base(ctx), id)?;
    let dir = store::env_dir(&store::base(ctx), &env.id)?;
    if !store::directory(&dir.join("logs"))? {
        return Ok(String::new());
    }
    let mut output = String::new();
    for name in ["stdout.log", "stderr.log"] {
        let path = dir.join("logs").join(name);
        if !path.try_exists()? {
            continue;
        }
        let mut bytes = Vec::new();
        store::open(&path, false)?
            .take(crate::model::MAX_LOG)
            .read_to_end(&mut bytes)?;
        output.push_str(&format!("{name}\n{}\n", String::from_utf8_lossy(&bytes)));
    }
    Ok(output)
}
