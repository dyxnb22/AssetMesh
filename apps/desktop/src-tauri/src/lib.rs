pub mod commands;
pub mod dto;
pub mod error;
pub mod state;

pub use state::DesktopState;

pub fn run() {
    let state = DesktopState::new();

    // Default db path from env or local file
    if let Ok(db_env) = std::env::var("ASSETMESH_DB") {
        let _ = state.initialize(std::path::Path::new(&db_env));
    } else {
        let _ = state.initialize(std::path::Path::new("assetmesh.db"));
    }

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
