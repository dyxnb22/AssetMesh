//! Thin backup adapter. Scheduling, retention and recovery are core use cases.
use crate::{error::DesktopError, state::DesktopState};
use assetmesh_core::{
    application::backup_service::BackupStatus,
    ports::backup::{BackupEntry, BackupPreferences, RestoreReceipt},
};
use tauri::State;

#[tauri::command]
pub fn backup_status(state: State<'_, DesktopState>) -> Result<BackupStatus, DesktopError> {
    Ok(state.backup()?.status()?)
}
#[tauri::command]
pub fn backup_create(state: State<'_, DesktopState>) -> Result<BackupEntry, DesktopError> {
    Ok(state.backup()?.create("manual")?)
}
#[tauri::command]
pub fn backup_preview(
    source_dir: String,
    state: State<'_, DesktopState>,
) -> Result<BackupEntry, DesktopError> {
    Ok(state.backup()?.preview(&source_dir)?)
}
#[tauri::command]
pub fn backup_restore(
    source_dir: String,
    expected_fingerprint: String,
    state: State<'_, DesktopState>,
) -> Result<RestoreReceipt, DesktopError> {
    Ok(state
        .backup()?
        .restore(&source_dir, &expected_fingerprint)?)
}
#[tauri::command]
pub fn backup_preferences(
    state: State<'_, DesktopState>,
) -> Result<BackupPreferences, DesktopError> {
    Ok(state.backup()?.preferences()?)
}
#[tauri::command]
pub fn backup_save_preferences(
    preferences: BackupPreferences,
    state: State<'_, DesktopState>,
) -> Result<(), DesktopError> {
    Ok(state.backup()?.save_preferences(preferences)?)
}
#[tauri::command]
pub fn backup_tick(state: State<'_, DesktopState>) -> Result<(), DesktopError> {
    Ok(state.backup()?.tick(env!("CARGO_PKG_VERSION"), false)?)
}

#[tauri::command]
pub fn backup_export_copy(
    target_dir: String,
    state: State<'_, DesktopState>,
) -> Result<String, DesktopError> {
    Ok(state
        .backup()?
        .export_copy(&target_dir, env!("CARGO_PKG_VERSION"))?)
}
