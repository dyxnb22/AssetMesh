pub mod commands;
pub mod dto;
pub mod error;
pub mod state;

pub use state::DesktopState;

/// Single source of command names registered into the Tauri application runtime.
pub const REGISTERED_COMMAND_NAMES: &[&str] = &[
    "app_capabilities",
    "app_status",
    "app_init",
    "library_list",
    "library_get",
    "library_search",
    "software_command",
    "software_discover",
    "media_command",
    "service_command",
    "relation_list",
    "relation_neighbors",
    "relation_traverse",
    "relation_attach",
    "relation_remove",
    "activity_query",
    "duplicate_candidates",
    "merge_preview",
    "merge_apply",
    "pick_directory",
    "portable_export",
    "portable_import_preview",
    "portable_import_apply",
    "app_settings",
];

/// Configures a Tauri builder with the canonical invoke handlers.
///
/// This provides a single source of truth for command registration across
/// the production binary (`main.rs`), headless runner, and test harnesses.
pub fn configure_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::app_capabilities,
        commands::app_status,
        commands::app_init,
        commands::library_list,
        commands::library_get,
        commands::library_search,
        commands::software_command,
        commands::software_discover,
        commands::media_command,
        commands::service_command,
        commands::relation_list,
        commands::relation_neighbors,
        commands::relation_traverse,
        commands::relation_attach,
        commands::relation_remove,
        commands::activity_query,
        commands::duplicate_candidates,
        commands::merge_preview,
        commands::merge_apply,
        commands::pick_directory,
        commands::portable_export,
        commands::portable_import_preview,
        commands::portable_import_apply,
        commands::app_settings,
    ])
}

pub fn run() {
    let state = DesktopState::new();

    configure_builder(tauri::Builder::default())
        .manage(state)
        .setup(|app| {
            use tauri::Manager;
            let db_path = startup_database_path(app.handle())?;
            let _ = app.state::<DesktopState>().initialize(&db_path);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// The database file the launch should open: `ASSETMESH_DB` if set, otherwise a
/// file inside the application data directory.
///
/// `DesktopState::initialize` records the result, so `app_init` can retry it.
pub fn startup_database_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("cannot resolve the application data directory: {err}"))?;
    Ok(resolve_db_path(
        std::env::var("ASSETMESH_DB").ok().as_deref(),
        &data_dir,
    ))
}

/// Chooses the database file to open.
///
/// The fallback is always absolute: a bundled `.app` launched from a file manager
/// runs with the process working directory at `/`, which is read-only on macOS, so
/// a CWD-relative default can never be opened there.
pub fn resolve_db_path(
    env_override: Option<&str>,
    data_dir: &std::path::Path,
) -> std::path::PathBuf {
    match env_override.map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => std::path::PathBuf::from(path),
        None => data_dir.join("assetmesh.db"),
    }
}
