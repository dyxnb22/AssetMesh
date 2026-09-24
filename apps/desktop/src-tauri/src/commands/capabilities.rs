//! Capabilities and status commands.

use assetmesh_core::application::library_service::AppCapabilities;
use tauri::State;

use crate::error::DesktopError;
use crate::state::{AppStatus, DesktopState};

#[tauri::command]
pub fn app_capabilities() -> Result<AppCapabilities, DesktopError> {
    Ok(AppCapabilities::current())
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
