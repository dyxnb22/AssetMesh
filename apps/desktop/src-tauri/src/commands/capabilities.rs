//! Capabilities and status commands.

use assetmesh_core::application::library_service::AppCapabilities;
use tauri::State;

use crate::error::DesktopError;
use crate::state::{AppStatus, DesktopState};

#[tauri::command]
pub fn app_startup_timing(
    stage: crate::startup::FrontendStage,
    web_ms: f64,
    state: State<'_, DesktopState>,
) -> Result<(), DesktopError> {
    state.startup_timings.frontend(stage, web_ms)
}

#[tauri::command]
pub fn app_capabilities() -> Result<AppCapabilities, DesktopError> {
    let mut capabilities = AppCapabilities::current();
    // The local-service runtime owns its processes through Unix process
    // groups; where that does not exist the UI must not offer the controls.
    capabilities.features.local_service_runtime = cfg!(unix);
    Ok(capabilities)
}

#[tauri::command]
pub fn app_status(state: State<'_, DesktopState>) -> Result<AppStatus, DesktopError> {
    app_status_impl(&state)
}

pub fn app_status_impl(state: &DesktopState) -> Result<AppStatus, DesktopError> {
    Ok(state.get_status())
}

/// Re-opens the database, which is how the setup card recovers.
///
/// With no argument it retries the location the launch attempted; with one it
/// switches to that file.
#[tauri::command]
pub fn app_init(
    db_path: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<AppStatus, DesktopError> {
    let path = match db_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(explicit) => std::path::PathBuf::from(explicit),
        None => state.default_db_path().ok_or_else(|| {
            DesktopError::setup_required("No database location has been chosen yet.")
        })?,
    };
    state.initialize(&path)
}
