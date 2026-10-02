//! Command-registration contracts for the Tauri layer.
//!
//! `generate_handler!` registers a command's argument extraction without
//! eagerly type-checking it: a command whose `State<'_>` type disagrees with
//! the type passed to `.manage()` still compiles, and fails at invoke time
//! instead. That is the exact shape of the relation commands' original bug
//! (`State<'_, Arc<DesktopState>>` against a plain `DesktopState`), which no
//! amount of `cargo build`/`cargo test` on the lib would have caught.
//!
//! So this file drives the real handler list through a Tauri mock runtime with
//! a managed `DesktopState`, and asserts every command gets past state
//! extraction.

use assetmesh_desktop_lib::error::DesktopError;
use assetmesh_desktop_lib::state::DesktopState;
use tauri::{Manager, State};

/// Builds a mock app wired exactly like the real one: the same generated
/// handler list over the same managed state via single-source configure_builder.
fn build_app() -> tauri::App<tauri::test::MockRuntime> {
    assetmesh_desktop_lib::configure_builder(tauri::test::mock_builder())
        .manage(DesktopState::new())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app with the real handler list")
}

const COMMANDS: &[&str] = assetmesh_desktop_lib::REGISTERED_COMMAND_NAMES;

#[test]
fn frontend_and_backend_command_registrations_are_in_sync() {
    let transport_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/features/library/transport.ts");
    let content = std::fs::read_to_string(&transport_path)
        .expect("apps/desktop/src/features/library/transport.ts must exist");

    let mut frontend_commands = std::collections::BTreeSet::new();
    for line in content.lines() {
        if line.trim_start().starts_with("import") {
            continue;
        }
        if let Some(pos) = line.find("invoke") {
            let rest = &line[pos..];
            if let Some(quote_start) = rest.find('\'') {
                if let Some(quote_end) = rest[quote_start + 1..].find('\'') {
                    let cmd = &rest[quote_start + 1..quote_start + 1 + quote_end];
                    if !cmd.is_empty() && !cmd.contains(' ') {
                        frontend_commands.insert(cmd.to_string());
                    }
                }
            }
        }
    }

    let backend_commands: std::collections::BTreeSet<String> =
        assetmesh_desktop_lib::REGISTERED_COMMAND_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect();

    let missing_in_backend: Vec<_> = frontend_commands
        .difference(&backend_commands)
        .cloned()
        .collect();
    assert!(
        missing_in_backend.is_empty(),
        "Frontend invokes commands not registered in backend: {:?}",
        missing_in_backend
    );

    let missing_in_frontend: Vec<_> = backend_commands
        .difference(&frontend_commands)
        .cloned()
        .collect();
    assert!(
        missing_in_frontend.is_empty(),
        "Backend registers commands not invoked by frontend: {:?}",
        missing_in_frontend
    );
}

/// Commands that must be *registered* but must not be *invoked* from a test.
///
/// `pick_directory` opens the OS directory chooser through the real
/// `SystemCommandRunner`; reaching it from a test pops a modal dialog and blocks
/// until a human dismisses it (see the `CommandRunner` seam in
/// `commands/portable.rs`, which exists so the `_impl` variant can be tested
/// with a fake). This command is stateless; the remaining commands exercise
/// managed-state extraction through the real handler list below.
const NOT_INVOKABLE_HERE: &[&str] = &["pick_directory"];

/// True when a response means the command's managed state could not be
/// resolved — the failure a `State<'_>` type mismatch produces.
///
/// Note what is *not* here: "invalid args" / "missing required key". Those are
/// the command's own argument extraction, which Tauri runs first, and they are
/// what a healthy command reports for a payload it does not accept. Only the
/// state-resolution wording below counts as the bug this file guards.
fn is_state_resolution_failure(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("state not managed")
        || m.contains("managed state")
        || m.contains("failed to extract state")
        || m.contains("state not found")
}

/// Invokes a real registered command with deserializable arguments. A typed
/// application error is acceptable; a managed-state extraction failure is not.
fn invoke_isolating_state(
    webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    command: &str,
    payload: serde_json::Value,
) -> Result<String, String> {
    let request = tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: tauri::ipc::InvokeBody::Json(payload),
        headers: Default::default(),
        invoke_key: tauri::test::INVOKE_KEY.to_string(),
    };

    match tauri::test::get_ipc_response(webview, request) {
        Ok(body) => Ok(body
            .deserialize::<serde_json::Value>()
            .map(|v| v.to_string())
            .unwrap_or_default()),
        Err(payload) => Err(payload.to_string()),
    }
}

/// A per-command payload whose own arguments deserialize, so the only failure
/// left to surface is `State<'_>` extraction.
///
/// Argument extraction runs before state extraction: an invocation with a
/// missing argument is rejected with "missing required key", which looks the
/// same whether or not the state resolves. So each command needs a payload
/// that clears its own checks. Values are real-but-nonexistent: `not_found`,
/// `setup_required` and `invalid_input` responses are all fine, because they
/// prove the handler body ran.
///
/// Paths point into a fresh temp directory that [`ScratchDir`] removes, so
/// `app_init` and the portable commands can do real work without leaving
/// artifacts behind or colliding with a parallel test.
fn payload_for(command: &str, scratch: &std::path::Path) -> serde_json::Value {
    let id = uuid::Uuid::now_v7().to_string();
    let scratch = scratch.to_string_lossy().to_string();
    let map = |pairs: &[(&str, serde_json::Value)]| {
        serde_json::Value::Object(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    };
    match command {
        "app_startup_timing" => map(&[
            ("stage", serde_json::json!("frontend_loaded")),
            ("webMs", serde_json::json!(100.0)),
        ]),
        "app_capabilities"
        | "app_status"
        | "pick_directory"
        | "software_discover"
        | "library_list"
        | "library_media_status_counts"
        | "activity_query"
        | "backup_status"
        | "backup_create"
        | "backup_preferences"
        | "backup_tick"
        | "duplicate_candidates" => map(&[]),
        "backup_preview" | "backup_restore" => map(&[
            ("sourceDir", serde_json::json!(format!("{scratch}/missing"))),
            ("expectedFingerprint", serde_json::json!("test")),
        ]),
        "backup_save_preferences" => map(&[("preferences", serde_json::json!({}))]),
        "backup_export_copy" => map(&[("targetDir", serde_json::json!(scratch))]),
        "app_init" => map(&[("dbPath", serde_json::json!(format!("{scratch}/probe.db")))]),
        "library_get" => map(&[("id", serde_json::json!(id))]),
        "library_search" => map(&[("query", serde_json::json!({"text": "probe"}))]),
        "portable_export" => map(&[("targetDir", serde_json::json!(format!("{scratch}/bundle")))]),
        "portable_import_preview" | "portable_import_apply" => {
            map(&[("sourceDir", serde_json::json!(format!("{scratch}/source")))])
        }
        "software_command" | "media_command" => map(&[(
            "command",
            serde_json::json!({"action": "archive", "asset_id": id}),
        )]),
        "service_command" => map(&[(
            "command",
            serde_json::json!({"action": "archive", "asset_id": id}),
        )]),
        "service_runtime_start"
        | "service_runtime_stop"
        | "service_runtime_restart"
        | "service_runtime_status"
        | "service_open_page" => map(&[("assetId", serde_json::json!(id))]),
        "service_runtime_statuses" => map(&[]),
        "service_runtime_logs" => map(&[("assetId", serde_json::json!(id))]),
        "info_command" => map(&[(
            "command",
            serde_json::json!({"action": "archive", "asset_id": id, "expected_revision": 1}),
        )]),
        "relation_list" => map(&[("assetId", serde_json::json!(id))]),
        "relation_neighbors" | "relation_traverse" => {
            map(&[("query", serde_json::json!({"asset_id": id}))])
        }
        "relation_attach" => map(&[(
            "payload",
            serde_json::json!({
                "source_asset_id": id,
                "relation_type": "uses",
                "target_asset_id": id,
            }),
        )]),
        "relation_remove" => map(&[("payload", serde_json::json!({"relation_id": id}))]),
        "merge_preview" => map(&[(
            "query",
            serde_json::json!({"winner_id": id, "loser_id": id}),
        )]),
        "merge_apply" => map(&[(
            "input",
            serde_json::json!({"winner_id": id, "loser_id": id}),
        )]),
        other => panic!("no payload defined for command `{other}` — add one"),
    }
}

#[test]
fn every_registered_command_resolves_the_managed_state() {
    let app = build_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview window");

    assert!(
        app.try_state::<DesktopState>().is_some(),
        "the commands extract `State<'_, DesktopState>`, so it must be managed"
    );

    let scratch = ScratchDir::new("desktop-command-registration");
    let mut failures = Vec::new();
    for command in COMMANDS {
        if NOT_INVOKABLE_HERE.contains(command) {
            continue;
        }
        let payload = payload_for(command, scratch.path());
        match invoke_isolating_state(&webview, command, payload) {
            Ok(_) => {}
            Err(payload) => {
                if is_state_resolution_failure(&payload) {
                    failures.push(format!("{command}: {payload}"));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "these commands could not resolve their managed state — a `State<'_>` \
         type mismatch against `manage(DesktopState::new())` in lib.rs:\n{}",
        failures.join("\n")
    );
}

/// A unique temp directory, removed on drop.
///
/// Some commands (`app_init`, the portable ones) genuinely touch the
/// filesystem, so they need somewhere to do it that no other test shares and
/// that a failed run does not leave behind.
struct ScratchDir {
    path: std::path::PathBuf,
}

impl ScratchDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("assetmesh-{label}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&path).expect("scratch directory");
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A command intentionally written the way the relation commands' original bug
/// was written: `State<'_, Arc<DesktopState>>` while `lib.rs` manages a plain
/// `DesktopState`.
///
/// This compiles, because `generate_handler!` does not eagerly type-check the
/// state extraction — which is precisely why the bug shipped. It exists so the
/// detection below can be proven against the real failure rather than assumed.
#[tauri::command]
async fn deliberate_state_mismatch(
    asset_id: String,
    state: State<'_, std::sync::Arc<DesktopState>>,
) -> Result<Vec<String>, DesktopError> {
    let _ = &state;
    Ok(vec![asset_id])
}

#[test]
fn the_state_mismatch_this_guards_against_is_actually_detectable() {
    // Fault injection, not a placeholder. The test above asserts none of the
    // real commands report a state-resolution failure; without this one it
    // would pass just as well if the predicate never matched anything. Here the
    // same predicate is pointed at a command that is *known* to be broken, so
    // the assertion is that it fires.
    let app = tauri::test::mock_builder()
        .manage(DesktopState::new())
        .invoke_handler(tauri::generate_handler![deliberate_state_mismatch])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app with the deliberately broken command");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview window");

    let response = invoke_isolating_state(
        &webview,
        "deliberate_state_mismatch",
        serde_json::json!({"assetId": uuid::Uuid::now_v7().to_string()}),
    )
    .expect_err("the mismatched state must be rejected");

    assert!(
        is_state_resolution_failure(&response),
        "the predicate the real test relies on must recognize this failure: {response}"
    );
}
