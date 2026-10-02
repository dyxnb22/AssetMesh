//! Local-service runtime commands: start, stop, status, and logs.
//!
//! The frontend only sends operations and displays what the backend reports;
//! every lifecycle decision (whether a process lives, failed, or was stopped)
//! is made here against the real child process. Read paths additionally
//! report one non-managed observation: a local service with no held process
//! whose access address answers on loopback shows as `external`.

use std::collections::HashSet;

use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::LifecycleState;
use assetmesh_core::ports::repos::{ServiceFilter, ServiceSort};
use tauri::{Manager, State};

use crate::error::DesktopError;
use crate::runtime::{RuntimeLogLine, RuntimeStatus};
use crate::state::DesktopState;

#[tauri::command]
pub fn service_runtime_start(
    asset_id: String,
    state: State<'_, DesktopState>,
) -> Result<RuntimeStatus, DesktopError> {
    service_runtime_start_impl(&asset_id, &state)
}

pub fn service_runtime_start_impl(
    asset_id: &str,
    state: &DesktopState,
) -> Result<RuntimeStatus, DesktopError> {
    state.with_service_operation(|| {
        let (project_dir, start_command, endpoint_target) = load_launch_config(asset_id, state)?;
        state
            .service_runtime()
            .start(asset_id, &project_dir, &start_command, endpoint_target)
    })
}

#[tauri::command]
pub async fn service_runtime_stop<R: tauri::Runtime>(
    asset_id: String,
    app: tauri::AppHandle<R>,
) -> Result<Option<RuntimeStatus>, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        service_runtime_stop_impl(&asset_id, &app.state::<DesktopState>())
    })
    .await
    .map_err(|e| DesktopError::internal(e.to_string()))?
}

pub fn service_runtime_stop_impl(
    asset_id: &str,
    state: &DesktopState,
) -> Result<Option<RuntimeStatus>, DesktopError> {
    state.with_service_operation(|| stop_configured(asset_id, state))
}

#[tauri::command]
pub async fn service_runtime_restart<R: tauri::Runtime>(
    asset_id: String,
    app: tauri::AppHandle<R>,
) -> Result<RuntimeStatus, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        service_runtime_restart_impl(&asset_id, &app.state::<DesktopState>())
    })
    .await
    .map_err(|e| DesktopError::internal(e.to_string()))?
}

pub fn service_runtime_restart_impl(
    asset_id: &str,
    state: &DesktopState,
) -> Result<RuntimeStatus, DesktopError> {
    state.with_service_operation(|| {
        // Validate the replacement before stopping the existing service.
        let (dir, command, target) = load_launch_config(asset_id, state)?;
        if !std::path::Path::new(&dir).is_dir() {
            return Err(DesktopError::invalid_input(format!(
                "project directory does not exist: {dir}"
            )));
        }
        stop_configured(asset_id, state)?;
        state
            .service_runtime()
            .stop_blocking(asset_id, std::time::Duration::from_secs(8))?;
        wait_for_address_release(target.as_ref())?;
        state
            .service_runtime()
            .start(asset_id, &dir, &command, target)
    })
}

fn stop_configured(
    asset_id: &str,
    state: &DesktopState,
) -> Result<Option<RuntimeStatus>, DesktopError> {
    let id = uuid::Uuid::parse_str(asset_id)
        .map(assetmesh_core::domain::ids::AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;
    let view = state.with_modules(|modules| Ok(modules.service().get_service(id)?))?;
    if view.entry.record.service_type != ServiceType::Local {
        return Err(DesktopError::invalid_input(
            "only a local service can be started or stopped here",
        ));
    }
    // A menu or window may still display the previous active snapshot after
    // archival. Archive cleans up owned processes itself; an old Stop action
    // must never execute an archived record's configured external command.
    if view.entry.asset.lifecycle_state != LifecycleState::Active {
        return Err(DesktopError::invalid_input(
            "archived or merged services cannot be stopped here",
        ));
    }
    let runtime = state.service_runtime();
    let managed = runtime.is_active(asset_id);
    let target = view
        .entry
        .record
        .endpoint_url
        .as_deref()
        .and_then(crate::runtime::parse_loopback_target);
    let answering = match target.as_ref() {
        Some(target) => crate::runtime::loopback_target_listening(target, None)?,
        None => false,
    };
    if !managed && !answering {
        return runtime.stop(asset_id);
    }
    if let Some(command) = view.entry.record.stop_command.as_deref() {
        let dir = view.entry.record.project_dir.as_deref().ok_or_else(|| {
            DesktopError::invalid_input(
                "Configure a project directory before stopping this service",
            )
        })?;
        runtime.run_stop_command(asset_id, dir, command)?;
    } else if !managed {
        return Err(DesktopError::invalid_input(
            "Configure a stop command in Edit Configuration to control this service",
        ));
    }
    let result = runtime.stop(asset_id)?;
    if !managed {
        wait_for_address_release(target.as_ref())?;
    }
    Ok(result)
}

fn wait_for_address_release(
    target: Option<&crate::runtime::LoopbackTarget>,
) -> Result<(), DesktopError> {
    let Some(target) = target else {
        return Ok(());
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while crate::runtime::loopback_target_listening(target, Some(deadline))? {
        if std::time::Instant::now() >= deadline {
            return Err(DesktopError::conflict(
                "The service address is still in use; check the stop command before restarting",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

#[tauri::command]
pub fn service_runtime_status(
    asset_id: String,
    state: State<'_, DesktopState>,
) -> Result<RuntimeStatus, DesktopError> {
    service_runtime_status_impl(&asset_id, &state)
}

pub fn service_runtime_status_impl(
    asset_id: &str,
    state: &DesktopState,
) -> Result<RuntimeStatus, DesktopError> {
    if state.service_runtime().holds(asset_id) {
        return Ok(state.service_runtime().status(asset_id));
    }
    let id = uuid::Uuid::parse_str(asset_id)
        .map(assetmesh_core::domain::ids::AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;
    state.with_modules(|modules| {
        let view = modules
            .service()
            .get_service(id)
            .map_err(DesktopError::from)?;
        let asset = &view.entry.asset;
        let record = &view.entry.record;
        unmanaged_read(
            asset_id,
            asset.lifecycle_state,
            record.service_type,
            record.endpoint_url.as_deref(),
            None,
        )
    })
}

#[tauri::command]
pub fn service_runtime_statuses(
    force_refresh: Option<bool>,
    state: State<'_, DesktopState>,
) -> Result<Vec<RuntimeStatus>, DesktopError> {
    if force_refresh.unwrap_or(false) {
        state.invalidate_service_statuses();
    }
    service_runtime_statuses_impl(&state)
}

pub fn service_runtime_statuses_impl(
    state: &DesktopState,
) -> Result<Vec<RuntimeStatus>, DesktopError> {
    state.cached_service_statuses(|| service_runtime_statuses_fresh_impl(state))
}

fn service_runtime_statuses_fresh_impl(
    state: &DesktopState,
) -> Result<Vec<RuntimeStatus>, DesktopError> {
    let mut statuses = state.service_runtime().statuses();
    let held: HashSet<String> = statuses.iter().map(|s| s.asset_id.clone()).collect();
    let rows = state.with_modules(|modules| {
        let filter = ServiceFilter {
            service_type: Some(ServiceType::Local),
            provider: None,
            tag: None,
            sort: ServiceSort::TitleAsc,
        };
        Ok(modules
            .service()
            .list_services(&filter)?
            .into_iter()
            .filter(|row| {
                row.entry.asset.lifecycle_state == LifecycleState::Active
                    && !held.contains(&row.entry.asset.id.to_string())
            })
            .collect::<Vec<_>>())
    })?;
    // Start the probe budget after the database read. Divide the remaining
    // time among remaining records, so earlier timeouts cannot starve a later
    // healthy address. A round that cannot attempt an address fails; the
    // frontend preserves its last snapshot instead of inventing stopped facts.
    let deadline = std::time::Instant::now() + STATUS_PROBE_ROUND_BUDGET;
    for (index, row) in rows.iter().enumerate() {
        let now = std::time::Instant::now();
        let remaining_records = u32::try_from(rows.len() - index).unwrap_or(u32::MAX);
        let record_deadline = now + deadline.saturating_duration_since(now) / remaining_records;
        let asset = &row.entry.asset;
        let record = &row.entry.record;
        statuses.push(unmanaged_read(
            &asset.id.to_string(),
            asset.lifecycle_state,
            record.service_type,
            record.endpoint_url.as_deref(),
            Some(record_deadline),
        )?);
    }
    Ok(statuses)
}

/// How long the whole `statuses` read may spend probing unmanaged access
/// addresses. Per-attempt connect timeouts bound a single probe; this bounds
/// the round.
const STATUS_PROBE_ROUND_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

/// The unmanaged read shared by the single-status and list paths: an active
/// `local` service whose loopback access address answers within the budget
/// shows an apparent external instance. Ineligible records and attempted
/// addresses that did not answer report stopped. Budget exhaustion before an
/// attempt is a retryable error; it is not a negative observation.
fn unmanaged_read(
    asset_id: &str,
    lifecycle: LifecycleState,
    service_type: ServiceType,
    endpoint_url: Option<&str>,
    deadline: Option<std::time::Instant>,
) -> Result<RuntimeStatus, DesktopError> {
    let target = endpoint_url
        .filter(|_| lifecycle == LifecycleState::Active && service_type == ServiceType::Local)
        .and_then(crate::runtime::parse_loopback_target);
    let listening = match target {
        Some(target) => crate::runtime::loopback_target_listening(&target, deadline)?,
        None => false,
    };
    Ok(RuntimeStatus::unmanaged(asset_id, listening))
}

#[tauri::command]
pub fn service_runtime_logs(
    asset_id: String,
    since: Option<u64>,
    run_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<ServiceRuntimeLogs, DesktopError> {
    service_runtime_logs_impl(&asset_id, since, run_id.as_deref(), &state)
}

pub fn service_runtime_logs_impl(
    asset_id: &str,
    since: Option<u64>,
    run_id: Option<&str>,
    state: &DesktopState,
) -> Result<ServiceRuntimeLogs, DesktopError> {
    let (lines, dropped, run_id) =
        state
            .service_runtime()
            .logs(asset_id, since.unwrap_or(0), run_id);
    Ok(ServiceRuntimeLogs {
        lines,
        dropped,
        run_id,
    })
}

/// The bounded log window of one service, polled incrementally by `seq`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceRuntimeLogs {
    pub run_id: Option<String>,
    pub lines: Vec<RuntimeLogLine>,
    /// True once older lines were dropped to keep the buffer bounded.
    pub dropped: bool,
}

/// Loads and validates what `start` needs from the canonical record: only an
/// active local service with a configured command and existing directory can
/// start, and its access address supplies the duplicate-instance port check.
fn load_launch_config(
    asset_id: &str,
    state: &DesktopState,
) -> Result<(String, String, Option<crate::runtime::LoopbackTarget>), DesktopError> {
    let id = uuid::Uuid::parse_str(asset_id)
        .map(assetmesh_core::domain::ids::AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

    let view = state.with_modules(|modules| {
        let mut svc = modules.service();
        svc.get_service(id).map_err(DesktopError::from)
    })?;

    let asset = &view.entry.asset;
    if asset.lifecycle_state != assetmesh_core::domain::LifecycleState::Active {
        return Err(DesktopError::invalid_input(
            "an archived or merged service cannot be started",
        ));
    }

    let record = &view.entry.record;
    if record.service_type != assetmesh_core::domain::service::ServiceType::Local {
        return Err(DesktopError::invalid_input(
            "only a local service can be started or stopped here",
        ));
    }
    let start_command = record
        .start_command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            DesktopError::invalid_input(
                "the service has no start command configured; edit the service to add one",
            )
        })?
        .to_string();
    let project_dir = record
        .project_dir
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .unwrap_or_default()
        .to_string();
    if project_dir.is_empty() {
        return Err(DesktopError::invalid_input(
            "the service has no project directory configured; edit the service to add one",
        ));
    }

    let endpoint_target = record
        .endpoint_url
        .as_deref()
        .and_then(crate::runtime::parse_loopback_target);

    Ok((project_dir, start_command, endpoint_target))
}

/// Resolve the canonical address instead of accepting an arbitrary frontend URL.
pub fn service_page_url_impl(asset_id: &str, state: &DesktopState) -> Result<String, DesktopError> {
    let id = uuid::Uuid::parse_str(asset_id)
        .map(assetmesh_core::domain::ids::AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;
    state.with_modules(|modules| {
        let view = modules.service().get_service(id)?;
        let address = view
            .entry
            .record
            .endpoint_url
            .as_deref()
            .ok_or_else(|| DesktopError::invalid_input("the service has no access address"))?;
        let url = tauri::Url::parse(address)
            .map_err(|_| DesktopError::invalid_input("invalid service access address"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(DesktopError::invalid_input(
                "the access address must be an HTTP or HTTPS URL",
            ));
        }
        Ok(url.to_string())
    })
}

#[tauri::command]
pub fn service_open_page(
    asset_id: String,
    state: State<'_, DesktopState>,
) -> Result<(), DesktopError> {
    service_open_page_impl(&asset_id, &state)
}

pub fn service_open_page_impl(asset_id: &str, state: &DesktopState) -> Result<(), DesktopError> {
    let url = service_page_url_impl(asset_id, state)?;
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .status();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&url).status();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        return Err(DesktopError::unsupported(
            "opening service pages is unavailable on this platform",
        ));
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match result {
        Ok(status) if status.success() => Ok(()),
        _ => Err(DesktopError::unavailable(
            "could not open the service page in your browser",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unattempted_endpoint_does_not_become_a_stopped_observation() {
        let deadline = std::time::Instant::now();
        let error = unmanaged_read(
            "unmanaged",
            LifecycleState::Active,
            ServiceType::Local,
            Some("http://127.0.0.1:8080"),
            Some(deadline),
        )
        .unwrap_err();
        assert_eq!(error.category, "unavailable");

        // A record without a probe target needs no budget in the first place.
        let status = unmanaged_read(
            "unconfigured",
            LifecycleState::Active,
            ServiceType::Local,
            None,
            Some(deadline),
        )
        .unwrap();
        assert_eq!(status.state, crate::runtime::RuntimeState::Stopped);
    }
}
