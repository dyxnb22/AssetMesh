pub mod commands;
pub mod dto;
pub mod error;
pub mod menu_bar;
pub mod runtime;
pub mod startup;
pub mod state;

pub use state::DesktopState;

/// Single source of command names registered into the Tauri application runtime.
pub const REGISTERED_COMMAND_NAMES: &[&str] = &[
    "app_capabilities",
    "app_status",
    "app_startup_timing",
    "app_init",
    "library_list",
    "library_get",
    "library_search",
    "library_media_status_counts",
    "software_command",
    "software_discover",
    "media_command",
    "service_command",
    "service_open_page",
    "service_runtime_start",
    "service_runtime_stop",
    "service_runtime_restart",
    "service_runtime_status",
    "service_runtime_statuses",
    "service_runtime_logs",
    "info_command",
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
    "backup_tick",
    "backup_export_copy",
    "backup_save_preferences",
    "backup_preferences",
    "backup_restore",
    "backup_preview",
    "backup_create",
    "backup_status",
];

/// Configures a Tauri builder with the canonical invoke handlers.
///
/// This provides a single source of truth for command registration across
/// the production binary (`main.rs`), headless runner, and test harnesses.
pub fn configure_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        commands::app_capabilities,
        commands::app_status,
        commands::app_startup_timing,
        commands::app_init,
        commands::library_list,
        commands::library_get,
        commands::library_search,
        commands::library_media_status_counts,
        commands::software_command,
        commands::software_discover,
        commands::media_command,
        commands::service_command,
        commands::service_open_page,
        commands::service_runtime_start,
        commands::service_runtime_stop,
        commands::service_runtime_restart,
        commands::service_runtime_status,
        commands::service_runtime_statuses,
        commands::service_runtime_logs,
        commands::info_command,
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
        commands::backup_tick,
        commands::backup_export_copy,
        commands::backup_save_preferences,
        commands::backup_preferences,
        commands::backup_restore,
        commands::backup_preview,
        commands::backup_create,
        commands::backup_status,
    ])
}

pub fn run() {
    let state = DesktopState::new();

    configure_builder(tauri::Builder::default())
        .manage(state)
        .setup(|app| {
            use tauri::Manager;
            let state = app.state::<DesktopState>();
            let timing_dir = std::env::var_os("ASSETMESH_STARTUP_PROFILE_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            state.startup_timings.set_directory(&timing_dir);
            state.startup_timings.native("setup_started");
            let db_path = startup_database_path(app.handle())?;
            let result = state.initialize(&db_path);
            state
                .startup_timings
                .native(if matches!(result, Ok(state::AppStatus::Ready { .. })) {
                    "database_ready"
                } else {
                    "database_failed"
                });
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                handle.state::<DesktopState>().backup_tick(false);
                std::thread::sleep(std::time::Duration::from_secs(60));
            });
            #[cfg(target_os = "macos")]
            if let Err(error) = menu_bar::install(app) {
                // Keep normal window closing behavior if the native entry
                // point could not be created, so the app remains reachable.
                eprintln!("[assetmesh] menu bar is unavailable: {error}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(target_os = "macos")]
            menu_bar::window_event(window, event);
            #[cfg(not(target_os = "macos"))]
            let _ = (window, event);
        })
        .on_page_load(|webview, payload| {
            use tauri::Manager;
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                webview
                    .state::<DesktopState>()
                    .startup_timings
                    .native("webview_loaded");
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| handle_run_event(app, &event));
}

/// Shared by the real event loop and runtime contracts, including native macOS
/// termination paths that do not issue an ExitRequested event.
pub fn handle_run_event<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: &tauri::RunEvent) {
    #[cfg(target_os = "macos")]
    if matches!(event, tauri::RunEvent::Reopen { .. }) {
        menu_bar::reopen(app);
    }
    if matches!(
        event,
        tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
    ) {
        use tauri::Manager;
        #[cfg(target_os = "macos")]
        menu_bar::stop_refreshing(app);
        let state = app.state::<DesktopState>();
        if state.shutdown_service_runtime() {
            state.backup_tick(true);
        }
    }
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
    if std::env::var("ASSETMESH_DB")
        .ok()
        .is_none_or(|value| value.trim().is_empty())
    {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&data_dir)
            .map_err(|error| format!("cannot create private application folder: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("cannot make application folder private: {error}"))?;
        }
    }
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
