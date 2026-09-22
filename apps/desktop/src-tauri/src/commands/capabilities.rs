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

#[tauri::command]
pub fn app_init(
    db_path: String,
    state: State<'_, DesktopState>,
) -> Result<AppStatus, DesktopError> {
    app_init_impl(&db_path, &state)
}

pub fn app_init_impl(db_path: &str, state: &DesktopState) -> Result<AppStatus, DesktopError> {
    state.initialize(std::path::Path::new(db_path))
}
