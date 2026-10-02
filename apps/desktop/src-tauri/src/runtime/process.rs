//! Owned-process lifecycle, signalling, reaping, and ended-run retention.
use super::logs::{pump_stream, LogBuffer, RuntimeLogLine};
use super::{RuntimeState, RuntimeStatus};
use assetmesh_core::domain::ids::AssetId;
use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a stop waits after SIGTERM before escalating to SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(8);
const ENDED_LOG_MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENDED_SERVICES: usize = 64;
/// How long a child must stay alive before `Starting` becomes `Running`.
const RUN_CONFIRM_DELAY: Duration = Duration::from_millis(400);

/// What the watcher observed when the child exited.
#[derive(Debug, Clone)]
struct ExitInfo {
    code: Option<i32>,
    signal: Option<i32>,
}

/// Keeps recent exit metadata and a bounded total of ended-process logs.
pub(super) fn prune_registry(registry: &mut HashMap<String, Arc<SharedProcess>>) {
    let mut ended: Vec<_> = registry
        .iter()
        .filter(|(_, process)| !process.state().is_live())
        .map(|(id, process)| (id.clone(), process.started_at.clone()))
        .collect();
    ended.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut bytes = 0usize;
    for (index, (id, _)) in ended.into_iter().enumerate() {
        if index >= MAX_ENDED_SERVICES {
            registry.remove(&id);
            continue;
        }
        if let Some(process) = registry.get(&id) {
            if let Ok(mut log) = process.logs.lock() {
                bytes += log.retain_within(ENDED_LOG_MAX_BYTES.saturating_sub(bytes));
            }
        }
    }
}

/// State shared between the registry, the output reader threads, and the
/// exit watcher for one spawned process.
pub(super) struct SharedProcess {
    asset_id: AssetId,
    /// PID of the direct child, which is also the process-group id because
    /// the child is spawned with `process_group(0)`.
    pid: u32,
    started_at: String,
    state: Mutex<RuntimeState>,
    configured_stop: AtomicBool,
    stop_requested_at: Mutex<Option<Instant>>,
    exit: Mutex<Option<ExitInfo>>,
    error: Mutex<Option<String>>,
    logs: Mutex<LogBuffer>,
}

impl SharedProcess {
    pub(super) fn new(asset_id: AssetId, pid: u32, error: Option<String>) -> Self {
        let state = if error.is_some() {
            RuntimeState::Failed
        } else {
            RuntimeState::Starting
        };
        Self {
            asset_id,
            pid,
            started_at: now_string(),
            state: Mutex::new(state),
            configured_stop: AtomicBool::new(false),
            stop_requested_at: Mutex::new(None),
            exit: Mutex::new(None),
            error: Mutex::new(error),
            logs: Mutex::new(LogBuffer::default()),
        }
    }

    pub(super) fn set_configured_stop(&self, value: bool) {
        self.configured_stop.store(value, Ordering::Release);
    }

    pub(super) fn logs(
        &self,
        since: u64,
        run_id: Option<&str>,
    ) -> (Vec<RuntimeLogLine>, bool, Option<String>) {
        let cursor = if run_id.is_some_and(|id| id != self.started_at) {
            0
        } else {
            since
        };
        let (lines, dropped) = self
            .logs
            .lock()
            .map(|buffer| buffer.snapshot(cursor))
            .unwrap_or_default();
        (lines, dropped, Some(self.started_at.clone()))
    }

    pub(super) fn state(&self) -> RuntimeState {
        self.state
            .lock()
            .map(|guard| *guard)
            .unwrap_or(RuntimeState::Failed)
    }

    pub(super) fn request_stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            if state.is_live() && *state != RuntimeState::Stopping {
                *state = RuntimeState::Stopping;
                *self.stop_requested_at.lock().unwrap() = Some(Instant::now());
                signal_group(self.pid, SIGTERM);
            }
        }
    }

    pub(super) fn signal(&self, signal: i32) {
        if let Ok(state) = self.state.lock() {
            if state.is_live() {
                signal_group(self.pid, signal);
            }
        }
    }

    fn promote(&self) {
        if let Ok(mut state) = self.state.lock() {
            if *state == RuntimeState::Starting {
                *state = RuntimeState::Running;
            }
        }
    }

    fn stop_expired(&self) -> bool {
        self.stop_requested_at
            .lock()
            .ok()
            .and_then(|at| *at)
            .is_some_and(|at| at.elapsed() >= STOP_GRACE)
    }

    pub(super) fn set_error(&self, message: String) {
        if let Ok(mut guard) = self.error.lock() {
            *guard = Some(message);
        }
    }
    pub(super) fn status(&self) -> RuntimeStatus {
        let state = self.state();
        let exit = self.exit.lock().ok().and_then(|guard| guard.clone());
        // A PID is only meaningful while the process is (or was just) alive; a
        // finished run reports none, so a stale PID can never be mistaken for a
        // stoppable process.
        let pid = match state {
            RuntimeState::Starting | RuntimeState::Running | RuntimeState::Stopping => {
                Some(self.pid)
            }
            RuntimeState::Stopped | RuntimeState::Failed | RuntimeState::External => None,
        };
        RuntimeStatus {
            asset_id: self.asset_id.to_string(),
            state,
            pid,
            started_at: Some(self.started_at.clone()),
            exit_code: exit.as_ref().and_then(|info| info.code),
            exit_signal: exit.as_ref().and_then(|info| info.signal),
            error: self.error.lock().ok().and_then(|guard| guard.clone()),
        }
    }
}

fn now_string() -> String {
    chrono::Local::now().to_rfc3339()
}

/// Signals a process group; `true` only when delivery succeeded. A stale
/// group id simply reports "already gone".
#[cfg(unix)]
fn signal_group(pgid: u32, sig: i32) -> bool {
    // SAFETY: sends a signal to an existing process group id; an invalid or
    // reaped id returns an error, which is the "already gone" case.
    unsafe { libc::kill(-(pgid as i32), sig) == 0 }
}

#[cfg(not(unix))]
fn signal_group(_pgid: u32, _sig: i32) -> bool {
    false
}

#[cfg(unix)]
const SIGTERM: i32 = libc::SIGTERM;
#[cfg(unix)]
pub(super) const SIGKILL: i32 = libc::SIGKILL;
#[cfg(not(unix))]
const SIGTERM: i32 = 15;
#[cfg(not(unix))]
pub(super) const SIGKILL: i32 = 9;

/// Spawns the output readers and the exit watcher for a freshly spawned
/// child. The watcher owns the `Child` and reaps it.
pub(super) fn spawn_watchers(
    shared: Arc<SharedProcess>,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    mut child: Child,
) {
    if let Some(out) = stdout {
        let shared = shared.clone();
        std::thread::spawn(move || pump_stream(out, "stdout", &shared.logs));
    }
    if let Some(err) = stderr {
        let shared = shared.clone();
        std::thread::spawn(move || pump_stream(err, "stderr", &shared.logs));
    }

    std::thread::spawn(move || {
        // Polling instead of `child.wait()`: the handle is owned here, but
        // polling keeps the watcher independent of the readers, so a
        // grandchild holding the pipe open cannot delay exit classification.
        // "Running" means the process is genuinely still alive after a short
        // confirmation window — never a health check of the service itself.
        let spawned_at = Instant::now();
        loop {
            match child_exited(shared.pid) {
                Ok(true) => break,
                Ok(false) => {
                    if spawned_at.elapsed() >= RUN_CONFIRM_DELAY {
                        shared.promote();
                    }
                    if shared.stop_expired() {
                        shared.signal(SIGKILL);
                    }
                    std::thread::sleep(if shared.state() == RuntimeState::Running {
                        Duration::from_millis(500)
                    } else {
                        Duration::from_millis(50)
                    });
                }
                Err(error) => {
                    shared.set_error(format!("could not observe the child: {error}"));
                    shared.signal(SIGKILL);
                    break;
                }
            }
        }

        let was_stopping = shared.state() == RuntimeState::Stopping
            || shared.configured_stop.load(Ordering::Acquire);
        // Keep the exited leader unreaped until all its group members are
        // gone. Its reserved PID prevents a recycled PGID from being signalled.
        shared.request_stop();
        loop {
            match group_has_descendants(shared.pid) {
                Ok(false) => break,
                Ok(true) if !shared.stop_expired() => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(true) => {
                    shared.signal(SIGKILL);
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    shared.signal(SIGKILL);
                    shared.set_error(format!(
                        "could not inspect descendants after terminating the group: {error}"
                    ));
                    std::thread::sleep(Duration::from_millis(50));
                    break;
                }
            }
        }
        // Serialize final reaping with signal delivery: no caller can signal
        // this PGID after the child has been reaped and made reusable.
        if let Ok(mut state) = shared.state.lock() {
            match child.wait() {
                Ok(status) => {
                    if let Ok(mut exit) = shared.exit.lock() {
                        *exit = Some(ExitInfo {
                            code: status.code(),
                            signal: unix_signal(&status),
                        });
                    }
                    if !was_stopping && !status.success() {
                        shared.set_error(describe_exit(&status));
                        *state = RuntimeState::Failed;
                    } else {
                        *state = RuntimeState::Stopped;
                    }
                }
                Err(error) => {
                    shared.set_error(format!("could not reap the child: {error}"));
                    *state = RuntimeState::Failed;
                }
            }
        }
    });
}

/// Observe exit without reaping: the group leader's PID remains reserved.
#[cfg(unix)]
fn child_exited(pid: u32) -> std::io::Result<bool> {
    // SAFETY: waitid writes a siginfo for our unreaped direct child only.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: waitid initialized the child-exit fields above.
        Ok(unsafe { info.si_pid() } != 0)
    }
}

#[cfg(not(unix))]
fn child_exited(_pid: u32) -> std::io::Result<bool> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "process groups unavailable",
    ))
}

/// Enumerate only to detect surviving members of our owned group. Signals
/// always target the registered, unreaped group leader, never a scanned PID.
fn group_has_descendants(leader: u32) -> std::io::Result<bool> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,pgid=,stat="])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            "could not inspect the owned process group",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        let mut fields = line.split_whitespace();
        let pid = fields.next().and_then(|v| v.parse::<u32>().ok());
        let pgid = fields.next().and_then(|v| v.parse::<u32>().ok());
        let state = fields.next().unwrap_or_default();
        pid != Some(leader) && pgid == Some(leader) && !state.starts_with('Z')
    }))
}

#[cfg(unix)]
fn unix_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn unix_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

#[cfg(unix)]
fn describe_exit(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    if let Some(signal) = status.signal() {
        format!("the process exited abnormally (terminated by signal {signal})")
    } else if let Some(code) = status.code() {
        match code {
            127 => "the process exited with code 127: the command was not found".to_string(),
            126 => "the process exited with code 126: the command is not executable".to_string(),
            other => format!("the process exited abnormally (exit code {other})"),
        }
    } else {
        "the process exited abnormally".to_string()
    }
}

#[cfg(not(unix))]
fn describe_exit(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("the process exited abnormally (exit code {code})"),
        None => "the process exited abnormally".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ended_processes_keep_recent_exit_metadata_with_a_bounded_total_log() {
        let mut registry = HashMap::new();
        for index in 0..80 {
            let mut logs = LogBuffer::default();
            for _ in 0..32 {
                logs.push("stdout", String::new(), "x".repeat(8192));
            }
            let process = Arc::new(SharedProcess {
                asset_id: AssetId::from_uuid(uuid::Uuid::now_v7()),
                pid: 0,
                started_at: format!("{index:08}"),
                state: Mutex::new(if index == 0 {
                    RuntimeState::Running
                } else {
                    RuntimeState::Stopped
                }),
                configured_stop: AtomicBool::new(false),
                stop_requested_at: Mutex::new(None),
                exit: Mutex::new(Some(ExitInfo {
                    code: Some(0),
                    signal: None,
                })),
                error: Mutex::new(None),
                logs: Mutex::new(logs),
            });
            registry.insert(index.to_string(), process);
        }
        let live_bytes = registry["0"].logs.lock().unwrap().bytes();
        prune_registry(&mut registry);
        assert_eq!(registry.len(), MAX_ENDED_SERVICES + 1);
        assert!(registry.contains_key("79"));
        assert!(!registry.contains_key("1"));
        assert_eq!(
            registry["0"].logs.lock().unwrap().bytes(),
            live_bytes,
            "live process logs are retained"
        );
        let ended: Vec<_> = registry
            .values()
            .filter(|process| !process.state().is_live())
            .collect();
        let bytes: usize = ended
            .iter()
            .map(|process| process.logs.lock().unwrap().bytes())
            .sum();
        assert!(bytes <= ENDED_LOG_MAX_BYTES);
        assert!(ended
            .iter()
            .any(|process| process.logs.lock().unwrap().snapshot(0).1));
        assert!(ended
            .iter()
            .all(|process| process.exit.lock().unwrap().as_ref().unwrap().code == Some(0)));
    }
}
