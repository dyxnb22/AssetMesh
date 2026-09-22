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
/// with a fake). Its `State` extraction is exercised by the code-reading guard
/// in [`every_command_declares_the_managed_state_type`] instead.
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

/// Invokes one command and reports what the command said, reaching only the
/// state extraction stage.
///
/// The payload sent is an empty JSON object. That is deliberately *not* valid
/// for the command's own parameters (a command taking `State<'_, DesktopState>`
/// and nothing else accepts no arguments at all), so the interesting
/// discrimination is:
///
/// - state resolution works → the command runs and returns its own typed
///   error (`setup_required`, `invalid_input`, ...), which is not an extraction
///   failure;
/// - state resolution fails → the IPC layer rejects with the wording this test
///   asserts against.
///
/// Commands that require *no* argument at all beyond state (`app_capabilities`,
/// `app_status`, `app_settings`, `pick_directory`, `software_discover`) may
/// succeed outright with an empty body; both outcomes are acceptable here.
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
/// Paths point into a fresh temp directory that [`Cleanup`] removes, so
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
        "app_capabilities"
        | "app_status"
        | "pick_directory"
        | "app_settings"
        | "software_discover"
        | "library_list"
        | "activity_query"
        | "duplicate_candidates" => map(&[]),
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

/// The commands excluded above cannot be reached through IPC from a test, so
/// their `State<'_>` type is checked here instead — by reading the source.
///
/// This is a tripwire, and deliberately a blunt one: it looks for the exact
/// wrapper signature the app uses. A command that takes its state any other way
/// (`State<'_, Arc<…>>`, `AppHandle`, or a `&DesktopState` parameter) fails,
/// which is the point — such a signature has to be consciously reviewed and
/// either fixed or added to the exception list with a reason.
#[test]
fn every_command_declares_the_managed_state_type() {
    let commands_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut checked = 0usize;
    let mut offenders = Vec::new();

    for entry in std::fs::read_dir(&commands_dir).expect("commands directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("command module");
        let module = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        // Walk the public command wrappers, not the `*_impl` helpers that take
        // `&DesktopState` directly. A signature spans several lines, so collect
        // from `pub` to the `{` that opens the body before judging it.
        let mut signature: Option<Vec<String>> = None;
        for line in source.lines() {
            let trimmed = line.trim();
            match &mut signature {
                None => {
                    if trimmed == "#[tauri::command]" {
                        signature = Some(Vec::new());
                    }
                }
                Some(parts) => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if !trimmed.starts_with("pub") && parts.is_empty() {
                        // A doc comment or another attribute; not a command.
                        if !trimmed.starts_with("#[") && !trimmed.starts_with("//!") {
                            signature = None;
                        }
                        continue;
                    }
                    parts.push(trimmed.to_string());
                    if trimmed.ends_with('{') {
                        let full = parts.join(" ");
                        let full = full.trim_end_matches('{').trim().to_string();
                        signature = None;
                        checked += 1;
                        if !declares_managed_state(&full) {
                            offenders.push(format!("{module}: {full}"));
                        }
                    }
                }
            }
        }
    }

    assert!(checked > 0, "no #[tauri::command] wrappers were found");
    assert!(
        offenders.is_empty(),
        "every command that touches storage must extract `State<'_, DesktopState>`\
         — the type lib.rs manages. Any other state type compiles and fails at\
         invoke time (see the fault-injection test above), so it is reported here\
         rather than shipped:\n{}",
        offenders.join("\n")
    );
}

/// Commands that legitimately take no state: they answer from static data or
/// shell out to the OS, and nothing they do can fail on an uninitialized
/// database.
const STATELESS_COMMANDS: &[&str] = &["app_capabilities", "pick_directory"];

/// True when a command's full signature is either stateless (and allowed to be)
/// or takes exactly the managed state type.
fn declares_managed_state(signature: &str) -> bool {
    let params = signature.split('(').nth(1).unwrap_or("");
    let name = signature
        .split_whitespace()
        .nth(2)
        .unwrap_or("")
        .split('(')
        .next()
        .unwrap_or("");

    if !params.contains("State<") {
        return STATELESS_COMMANDS.contains(&name);
    }

    // A state parameter of any other type is the bug this file exists for.
    params.contains("State<'_, DesktopState>")
        && !params.contains("State<'_, Arc<")
        && !params.contains("AppHandle")
        && !params.contains("Window<")
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
