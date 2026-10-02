//! Local-service process runtime.
//!
//! Owns the processes AssetMesh started for local services, in memory only:
//! running state, PID and captured logs are temporary runtime facts, never
//! recoverable business state (docs/10). The durable launch configuration
//! lives in the Services module's canonical record; this module only knows
//! how to run and stop what the user saved.
//!
//! Guarantees:
//! - one process per service: `start` holds the registry lock across the
//!   spawn and registers the entry before releasing it, so concurrent
//!   invocations cannot create a second instance;
//! - a child is spawned into its own process group (`process_group(0)`), so
//!   stopping signals the whole group — the command's own subprocess tree is
//!   cleaned up, not just the direct child;
//! - only processes this runtime spawned are ever signalled: the registry is
//!   the only source of PIDs, and there is deliberately no port/name/PID
//!   scanning that could kill an externally started process;
//! - captured output is bounded (line count and bytes), so a chatty process
//!   cannot grow memory without limit;
//! - on shutdown every managed process group is stopped with a bounded
//!   SIGTERM → SIGKILL escalation.

use crate::error::DesktopError;
use assetmesh_core::domain::ids::AssetId;
use serde::Serialize;
use std::collections::HashMap;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod logs;
mod probe;
mod process;

pub use logs::RuntimeLogLine;
pub(crate) use probe::loopback_target_listening;
pub use probe::{parse_loopback_target, LoopbackTarget};
#[cfg(unix)]
use process::spawn_watchers;
use process::{prune_registry, SharedProcess, SIGKILL};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const MAX_LIVE_SERVICES: usize = 32;

/// Lifecycle of one managed process, driven by what actually happens to the
/// child — never inferred from a port or a name. The one exception is
/// [`RuntimeState::External`], which is not a managed lifecycle state at all:
/// it is a read-time observation about a service AssetMesh holds no process
/// for (see `unmanaged`), reported so the UI can say "an instance appears to
/// be running outside AssetMesh" instead of the false "stopped".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    /// Observed, never managed: nothing is held in the registry, but the
    /// service's access address answers on its loopback port. It has no pid,
    /// accepts no stop, and is never stored — every read recomputes it.
    External,
}

impl RuntimeState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeState::Starting => "starting",
            RuntimeState::Running => "running",
            RuntimeState::Stopping => "stopping",
            RuntimeState::Stopped => "stopped",
            RuntimeState::Failed => "failed",
            RuntimeState::External => "external",
        }
    }

    fn is_live(&self) -> bool {
        matches!(
            self,
            RuntimeState::Starting | RuntimeState::Running | RuntimeState::Stopping
        )
    }
}

/// Snapshot of one managed process, crossing the transport.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub asset_id: String,
    pub state: RuntimeState,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    pub error: Option<String>,
}

impl RuntimeStatus {
    fn stopped(asset_id: &str) -> Self {
        RuntimeStatus {
            asset_id: asset_id.to_string(),
            state: RuntimeState::Stopped,
            pid: None,
            started_at: None,
            exit_code: None,
            exit_signal: None,
            error: None,
        }
    }

    /// The status shown for a service AssetMesh holds no process for. When
    /// its access address answers on loopback (`listening`), the record shows
    /// an apparent external instance instead of the false "stopped". This is
    /// a display-only observation: nothing is stored, and no process is ever
    /// signalled from this path.
    pub fn unmanaged(asset_id: &str, listening: bool) -> Self {
        if listening {
            RuntimeStatus {
                asset_id: asset_id.to_string(),
                state: RuntimeState::External,
                pid: None,
                started_at: None,
                exit_code: None,
                exit_signal: None,
                error: None,
            }
        } else {
            RuntimeStatus::stopped(asset_id)
        }
    }
}

/// The runtime refuses to manage processes where the process-group machinery
/// does not exist, so no platform can present a control that does nothing.
#[cfg(not(unix))]
fn unsupported() -> DesktopError {
    DesktopError::unsupported("local service start/stop is not available on this platform")
}

/// Registry of the processes AssetMesh started. Keyed by asset id; every
/// entry stays until the next start replaces it, so a finished or failed run
/// keeps its outcome visible (with its logs) until the user acts again or
/// the application closes.
pub struct LocalServiceRuntime {
    processes: Mutex<HashMap<String, Arc<SharedProcess>>>,
}

/// The Unix implementation of [`LocalServiceRuntime::start`].
#[cfg(unix)]
fn start_managed(
    runtime: &LocalServiceRuntime,
    asset_id: &str,
    project_dir: &str,
    start_command: &str,
    local_target: Option<LoopbackTarget>,
) -> Result<RuntimeStatus, DesktopError> {
    let parsed_id = parse_asset_id(asset_id)?;
    let mut registry = runtime
        .processes
        .lock()
        .map_err(|_| DesktopError::internal("Runtime lock failed"))?;

    prune_registry(&mut registry);
    if registry
        .values()
        .filter(|process| process.state().is_live())
        .count()
        >= MAX_LIVE_SERVICES
    {
        return Err(DesktopError::conflict(
            "Too many services are running; stop one before starting another",
        ));
    }
    if let Some(existing) = registry.get(asset_id) {
        let state = existing.state();
        if state.is_live() {
            return Err(DesktopError::conflict(format!(
                "the service is already {} and managed by AssetMesh; \
                 stop it before starting again",
                state.as_str()
            )));
        }
    }

    if let Some(target) = local_target {
        if loopback_target_listening(&target, None)? {
            return Err(DesktopError::conflict(format!(
                "local address {target} is already in use — the service appears to be running \
                 outside AssetMesh. AssetMesh does not manage processes it did not start, \
                 so it cannot stop that instance; stop it yourself before starting here."
            )));
        }
    }

    let dir = std::path::Path::new(project_dir);
    if !dir.is_dir() {
        return Err(DesktopError::invalid_input(format!(
            "project directory does not exist: {project_dir}"
        )));
    }
    if start_command.trim().is_empty() {
        return Err(DesktopError::invalid_input(
            "the service has no start command configured",
        ));
    }

    let mut command = Command::new("/bin/bash");
    command
        .arg("-c")
        .arg(start_command)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    {
        use std::os::unix::process::CommandExt;
        // The child leads its own process group: stopping later reaches
        // every descendant that did not escape the group.
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            // Spawn failures leave no process behind: register a failed
            // entry so the reason stays visible until the next attempt.
            let reason = format!("failed to start the command: {error}");
            let shared = Arc::new(SharedProcess::new(parsed_id, 0, Some(reason.clone())));
            registry.insert(asset_id.to_string(), shared);
            return Err(DesktopError::invalid_input(reason));
        }
    };
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let shared = Arc::new(SharedProcess::new(parsed_id, pid, None));
    registry.insert(asset_id.to_string(), shared.clone());
    drop(registry);

    spawn_watchers(shared.clone(), stdout, stderr, child);

    Ok(shared.status())
}

/// The Unix implementation of [`LocalServiceRuntime::stop`].
#[cfg(unix)]
fn stop_managed(
    runtime: &LocalServiceRuntime,
    asset_id: &str,
) -> Result<Option<RuntimeStatus>, DesktopError> {
    let shared = {
        let registry = runtime
            .processes
            .lock()
            .map_err(|_| DesktopError::internal("Runtime lock failed"))?;
        match registry.get(asset_id) {
            Some(shared) if shared.state().is_live() => shared.clone(),
            Some(shared) => return Ok(Some(shared.status())),
            None => return Ok(None),
        }
    };

    shared.request_stop();

    Ok(Some(shared.status()))
}

impl Drop for LocalServiceRuntime {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

impl Default for LocalServiceRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalServiceRuntime {
    /// Execute a service-specific stop command in isolation. Only this command's
    /// own process group is managed; it cannot replace the service's registry entry.
    /// A hung command is stopped after a bounded wait, and failure output is returned.
    pub fn run_stop_command(
        &self,
        asset_id: &str,
        project_dir: &str,
        command: &str,
    ) -> Result<(), DesktopError> {
        let managed = self
            .processes
            .lock()
            .map_err(|_| DesktopError::internal("Runtime lock failed"))?
            .get(asset_id)
            .filter(|shared| shared.state().is_live())
            .cloned();
        if let Some(shared) = &managed {
            shared.set_configured_stop(true);
        }
        let result = self.execute_stop_command(asset_id, project_dir, command);
        if result.is_err() {
            if let Some(shared) = &managed {
                shared.set_configured_stop(false);
            }
        }
        result
    }

    fn execute_stop_command(
        &self,
        asset_id: &str,
        project_dir: &str,
        command: &str,
    ) -> Result<(), DesktopError> {
        let control = LocalServiceRuntime::new();
        control.start(asset_id, project_dir, command, None)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while control.is_active(asset_id) {
            if Instant::now() >= deadline {
                control.stop_blocking(asset_id, Duration::from_millis(200))?;
                return Err(DesktopError::conflict(
                    "Stop command timed out; check its configuration",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let status = control.status(asset_id);
        if status.state == RuntimeState::Failed || status.exit_code != Some(0) {
            let (logs, _, _) = control.logs(asset_id, 0, None);
            let tail = logs
                .iter()
                .rev()
                .take(5)
                .rev()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Err(DesktopError::conflict(format!(
                "Stop command failed (exit code {:?}): {} {tail}",
                status.exit_code,
                status.error.unwrap_or_default()
            )));
        }
        Ok(())
    }

    pub fn new() -> Self {
        LocalServiceRuntime {
            processes: Mutex::new(HashMap::new()),
        }
    }

    /// Spawns the saved command in the project directory.
    ///
    /// `local_target` is the loopback probe target parsed from the service's
    /// access address, when it has one. When AssetMesh holds no process but
    /// that target already answers, the start is refused: something is almost
    /// certainly already serving this project from outside AssetMesh, and
    /// starting a second instance would only fail. That external process is
    /// NOT touched — it was not started here, so it cannot be stopped here.
    pub fn start(
        &self,
        asset_id: &str,
        project_dir: &str,
        start_command: &str,
        local_target: Option<LoopbackTarget>,
    ) -> Result<RuntimeStatus, DesktopError> {
        #[cfg(not(unix))]
        {
            let _ = (asset_id, project_dir, start_command, local_target);
            return Err(unsupported());
        }
        #[cfg(unix)]
        {
            start_managed(self, asset_id, project_dir, start_command, local_target)
        }
    }

    /// Stops the process AssetMesh started for `asset_id`, escalating from
    /// SIGTERM to SIGKILL after the grace period. Returns `Ok(None)` when
    /// AssetMesh holds no process for the service — an externally started
    /// process is never touched.
    pub fn stop(&self, asset_id: &str) -> Result<Option<RuntimeStatus>, DesktopError> {
        #[cfg(not(unix))]
        {
            let _ = asset_id;
            return Err(unsupported());
        }
        #[cfg(unix)]
        {
            stop_managed(self, asset_id)
        }
    }

    /// Stop + bounded wait, for paths that must not leave a live process
    /// behind them (archival, application exit).
    pub fn stop_blocking(&self, asset_id: &str, grace: Duration) -> Result<(), DesktopError> {
        self.stop(asset_id)?;
        let deadline = Instant::now() + grace;
        while self.is_active(asset_id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Some(shared) = self
            .processes
            .lock()
            .ok()
            .and_then(|r| r.get(asset_id).cloned())
        {
            shared.signal(SIGKILL);
        }
        // The watcher reaps the leader only after cleaning its descendants.
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.is_active(asset_id) {
            if Instant::now() >= deadline {
                return Err(DesktopError::conflict(
                    "the service is still stopping; try again once it has stopped",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    /// Whether the registry holds an entry for the service at all — managed
    /// or finished. A held entry always wins over the read-time external
    /// probe: the outcome of the last managed run stays visible until the
    /// user acts again or the application closes.
    pub fn holds(&self, asset_id: &str) -> bool {
        self.processes
            .lock()
            .map(|registry| registry.contains_key(asset_id))
            .unwrap_or(false)
    }

    /// Whether AssetMesh currently holds a live (starting/running/stopping) process
    /// for the service. Guards editing launch configuration and archival.
    pub fn is_active(&self, asset_id: &str) -> bool {
        self.processes
            .lock()
            .map(|registry| {
                registry
                    .get(asset_id)
                    .map(|shared| shared.state().is_live())
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Includes stopping/failed/stopped entries: the user-facing status of a
    /// service is "what did the last attempt do", which survives until the
    /// next start or the application closes.
    pub fn statuses(&self) -> Vec<RuntimeStatus> {
        if let Ok(mut registry) = self.processes.lock() {
            prune_registry(&mut registry);
        }
        self.processes
            .lock()
            .map(|registry| registry.values().map(|shared| shared.status()).collect())
            .unwrap_or_default()
    }

    pub fn status(&self, asset_id: &str) -> RuntimeStatus {
        self.processes
            .lock()
            .ok()
            .and_then(|registry| registry.get(asset_id).cloned())
            .map(|shared| shared.status())
            .unwrap_or_else(|| RuntimeStatus::stopped(asset_id))
    }

    /// Output lines captured after `since` (incremental polling), plus
    /// whether any lines were dropped to keep the buffer bounded.
    pub fn logs(
        &self,
        asset_id: &str,
        since: u64,
        run_id: Option<&str>,
    ) -> (Vec<RuntimeLogLine>, bool, Option<String>) {
        self.processes
            .lock()
            .ok()
            .and_then(|registry| registry.get(asset_id).cloned())
            .map(|shared| shared.logs(since, run_id))
            .unwrap_or((Vec::new(), false, None))
    }

    /// Shuts down every managed process group: SIGTERM to all, bounded wait,
    /// SIGKILL to whatever remains. Called on application exit so a closed
    /// AssetMesh leaves no services running in the background.
    pub fn shutdown_all(&self) {
        let targets: Vec<(String, Arc<SharedProcess>)> = self
            .processes
            .lock()
            .map(|registry| {
                registry
                    .iter()
                    .filter(|(_, shared)| shared.state().is_live())
                    .map(|(id, shared)| (id.clone(), shared.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if targets.is_empty() {
            return;
        }

        for (_, shared) in &targets {
            shared.request_stop();
        }
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            if targets.iter().all(|(_, shared)| !shared.state().is_live()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        for (_, shared) in &targets {
            shared.signal(SIGKILL);
        }
        // Keep the application alive while watchers finish reaping children.
        let deadline = Instant::now() + Duration::from_secs(2);
        while targets.iter().any(|(_, shared)| shared.state().is_live())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn parse_asset_id(asset_id: &str) -> Result<AssetId, DesktopError> {
    uuid::Uuid::parse_str(asset_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))
}
