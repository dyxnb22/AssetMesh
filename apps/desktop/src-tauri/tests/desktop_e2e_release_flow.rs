//! Real embedded Tauri E2E release workflow (P5-10, R5).
//!
//! Executes the full lifecycle against a real Tauri application runtime via
//! `configure_builder(tauri::test::mock_builder())` and `tauri::test::get_ipc_response`,
//! exercising real IPC serialization, deserialization, command routing, state management,
//! and SQLite storage operations end-to-end.

use assetmesh_desktop_lib::state::DesktopState;
use serde_json::Value;

struct E2eApp {
    _scratch: ScratchDir,
    db_path: String,
    export_path: String,
    webview: tauri::WebviewWindow<tauri::test::MockRuntime>,
}

struct ScratchDir {
    path: std::path::PathBuf,
}

impl ScratchDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("assetmesh-e2e-{label}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&path).expect("create scratch dir");
        Self { path }
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl E2eApp {
    fn init() -> Self {
        let scratch = ScratchDir::new("release-flow");
        let db_path = scratch.path.join("e2e.db").to_string_lossy().to_string();
        let export_path = scratch
            .path
            .join("export_bundle")
            .to_string_lossy()
            .to_string();

        let app = assetmesh_desktop_lib::configure_builder(tauri::test::mock_builder())
            .manage(DesktopState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build real tauri app");

        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("webview window");

        Self {
            _scratch: scratch,
            db_path,
            export_path,
            webview,
        }
    }

    fn invoke(&self, command: &str, payload: Value) -> Result<Value, String> {
        let request = tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(payload),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        };

        match tauri::test::get_ipc_response(&self.webview, request) {
            Ok(body) => Ok(body.deserialize::<Value>().unwrap_or(Value::Null)),
            Err(err_payload) => Err(err_payload.to_string()),
        }
    }
}

#[test]
fn real_tauri_runtime_full_lifecycle_e2e_flow() {
    let app = E2eApp::init();

    // 1. Check application capabilities
    let caps = app
        .invoke("app_capabilities", serde_json::json!({}))
        .expect("app_capabilities should succeed");
    let modules = caps["modules"].as_array().expect("modules array");
    assert!(
        modules.len() >= 3,
        "desktop runtime must report all shipped modules, got: {:?}",
        modules
    );

    // 2. Initialize database
    let status = app
        .invoke("app_init", serde_json::json!({ "dbPath": app.db_path }))
        .expect("app_init should succeed");
    assert_eq!(
        status["status"].as_str().unwrap_or(""),
        "ready",
        "app_init must report ready status, got: {:?}",
        status
    );

    // 3. Create Media asset
    let media_res = app
        .invoke(
            "media_command",
            serde_json::json!({
                "command": {
                    "action": "create",
                    "title": "Spirited Away",
                    "media_type": "anime",
                    "year": 2001,
                    "tags": ["ghibli", "classic"]
                }
            }),
        )
        .expect("media create should succeed");
    let media_id = media_res["asset_ids"][0]
        .as_str()
        .expect("must return asset_ids[0]")
        .to_string();

    // 4. Create Software asset
    let sw_res = app
        .invoke(
            "software_command",
            serde_json::json!({
                "command": {
                    "action": "create",
                    "name": "Ripgrep",
                    "category": "tool",
                    "install_source": "homebrew_formula",
                    "version": "14.1.0",
                    "tags": ["cli", "search"]
                }
            }),
        )
        .expect("software create should succeed");
    let sw_id = sw_res["asset_ids"][0]
        .as_str()
        .expect("must return asset_ids[0]")
        .to_string();

    // 5. Create Service asset
    let svc_res = app
        .invoke(
            "service_command",
            serde_json::json!({
                "command": {
                    "action": "create",
                    "name": "GitHub Copilot",
                    "service_type": "saas",
                    "plan": "Individual",
                    "cost": "10.00",
                    "currency": "USD",
                    "billing_cadence": "monthly",
                    "tags": ["ai", "dev"]
                }
            }),
        )
        .expect("service create should succeed");
    let svc_id = svc_res["asset_ids"][0]
        .as_str()
        .expect("must return asset_ids[0]")
        .to_string();

    // 6. Attach cross-module relation: Software uses Service
    let sw_before_relation = app
        .invoke("library_get", serde_json::json!({ "id": sw_id }))
        .expect("software detail should succeed");
    let svc_before_relation = app
        .invoke("library_get", serde_json::json!({ "id": svc_id }))
        .expect("service detail should succeed");
    let rel_res = app
        .invoke(
            "relation_attach",
            serde_json::json!({
                "payload": {
                    "source_asset_id": sw_id,
                    "target_asset_id": svc_id,
                    "relation_type": "uses",
                    "note": "AI code completion engine",
                    "expected_source_revision": sw_before_relation["revision"],
                    "expected_target_revision": svc_before_relation["revision"]
                }
            }),
        )
        .expect("relation_attach should succeed");
    assert_eq!(
        rel_res["changed"].as_bool(),
        Some(true),
        "relation attachment must report changed: true"
    );

    // 7. Query library list (assert SQL LIMIT/OFFSET pagination across all 3 modules)
    let list_res = app
        .invoke(
            "library_list",
            serde_json::json!({
                "query": {
                    "limit": 10,
                    "offset": 0
                }
            }),
        )
        .expect("library_list should succeed");
    assert_eq!(list_res["total"].as_u64(), Some(3));
    let items = list_res["items"].as_array().expect("items array");
    assert_eq!(items.len(), 3);

    // 8. Query typed detail
    let get_sw = app
        .invoke("library_get", serde_json::json!({ "id": sw_id }))
        .expect("library_get should succeed");
    assert_eq!(get_sw["name"].as_str(), Some("Ripgrep"));
    assert_eq!(get_sw["kind"].as_str(), Some("software.tool"));
    assert_eq!(get_sw["lifecycle"].as_str(), Some("active"));

    let get_media = app
        .invoke("library_get", serde_json::json!({ "id": media_id }))
        .expect("library_get media should succeed");
    assert_eq!(get_media["name"].as_str(), Some("Spirited Away"));
    assert_eq!(get_media["kind"].as_str(), Some("media.anime"));
    assert_eq!(get_media["lifecycle"].as_str(), Some("active"));

    // 9. Traverse relation graph
    let neighbors = app
        .invoke(
            "relation_neighbors",
            serde_json::json!({
                "query": {
                    "asset_id": sw_id
                }
            }),
        )
        .expect("relation_neighbors should succeed");
    let neighbors_arr = neighbors.as_array().expect("neighbors array");
    assert_eq!(neighbors_arr.len(), 1);
    assert_eq!(
        neighbors_arr[0]["asset"]["id"].as_str(),
        Some(svc_id.as_str())
    );
    assert_eq!(
        neighbors_arr[0]["edge"]["relation_type"].as_str(),
        Some("uses")
    );

    // 10. Query activity events
    let activity = app
        .invoke("activity_query", serde_json::json!({ "query": {} }))
        .expect("activity_query should succeed");
    let events = activity["items"].as_array().expect("activity items array");
    assert!(
        events.len() >= 4,
        "must record media, software, service creation and relation attachment"
    );

    // 11. Portable export
    let export_res = app
        .invoke(
            "portable_export",
            serde_json::json!({ "targetDir": app.export_path }),
        )
        .expect("portable_export should succeed");
    assert_eq!(
        export_res["target_dir"].as_str(),
        Some(app.export_path.as_str()),
        "export must return target_dir path"
    );
    let files = export_res["files"].as_array().expect("files array");
    assert!(!files.is_empty(), "must write bundle files");

    // 12. Portable import preview
    let preview_res = app
        .invoke(
            "portable_import_preview",
            serde_json::json!({ "sourceDir": app.export_path }),
        )
        .expect("portable_import_preview should succeed");
    assert_eq!(preview_res["valid"].as_bool(), Some(true));
    assert_eq!(
        preview_res["dispositions"]["assets_created"].as_u64(),
        Some(0),
        "existing assets match fingerprint so are not re-created"
    );
}

/// The setup card's Retry presses `app_init` with no argument, so the command
/// must accept that and re-open the location it was last asked for.
#[test]
fn app_init_retries_the_last_attempted_location_when_called_without_arguments() {
    let app = E2eApp::init();

    let blocked = app._scratch.path.join("blocker");
    std::fs::write(&blocked, b"not a directory").expect("blocker file");
    let unreachable = blocked.join("assetmesh.db").to_string_lossy().to_string();

    let failed = app
        .invoke(
            "app_init",
            serde_json::json!({ "dbPath": unreachable.as_str() }),
        )
        .expect("a failed open is reported as a status, not an invoke error");
    assert_eq!(
        failed["status"].as_str().unwrap_or(""),
        "setup_failure",
        "got: {failed:?}"
    );

    let recovered = app
        .invoke("app_init", serde_json::json!({ "dbPath": app.db_path }))
        .expect("app_init with a valid path should succeed");
    assert_eq!(
        recovered["status"].as_str().unwrap_or(""),
        "ready",
        "got: {recovered:?}"
    );

    let retried = app
        .invoke("app_init", serde_json::json!({}))
        .expect("app_init must accept a missing dbPath");
    assert_eq!(
        retried["status"].as_str().unwrap_or(""),
        "ready",
        "a retry with no argument must re-open the last location, got: {retried:?}"
    );

    let listed = app
        .invoke("library_list", serde_json::json!({}))
        .expect("the recovered state must serve reads");
    assert!(
        listed["items"].is_array(),
        "library_list should return a page after recovery"
    );
}
