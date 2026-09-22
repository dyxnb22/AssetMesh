//! Desktop Tauri adapter integration contract tests for Import/Export and Settings (P5-09).
//!
//! Validates:
//! 1. Export creates valid bundle, manifest, and receipt.
//! 2. Import preview (dry-run) and apply are field-for-field symmetric.
//! 3. Import rejects malformed bundle and keeps destination pristine.
//! 4. Import rejects unsupported version.
//! 5. Import rejects cross-module collision/re-typing conflict without partial mutation.
//! 6. App settings reports real DB status, capabilities, and provider status.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{
    app_settings_impl, library_list_impl, media_command_impl, pick_directory_impl,
    portable_export_impl, portable_import_apply_impl, portable_import_preview_impl,
    service_command_impl, software_command_impl,
};
use assetmesh_desktop_lib::dto::{
    LibraryQueryDto, MediaCommandDto, ServiceCommandDto, SoftwareCommandDto,
};
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-portable-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

fn temp_dir_path(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "assetmesh-bundle-{}-{}",
        name,
        uuid::Uuid::now_v7()
    ));
    let _ = fs::create_dir_all(&p);
    p
}

fn setup_test_state(name: &str) -> (DesktopState, PathBuf) {
    let db_path = temp_db_path(name);
    let state = DesktopState::with_clock_and_ids(Arc::new(SystemClock), Arc::new(UuidV7Generator));
    state.initialize(&db_path).expect("initialize");
    (state, db_path)
}

fn create_test_media(title: &str, state: &DesktopState) -> String {
    let receipt = media_command_impl(
        MediaCommandDto::Create {
            title: title.into(),
            media_type: "movie".into(),
            summary: Some(format!("Summary for {title}")),
            status: Some("planned".into()),
            rating: Some(4.5),
            year: Some(2023),
            platform: None,
            progress_current: None,
            progress_total: None,
            progress_unit: None,
            notes: Some("Excellent read".into()),
            tags: vec!["reading".into()],
        },
        state,
    )
    .expect("create media");
    receipt.asset_ids[0].clone()
}

fn create_test_software(name: &str, state: &DesktopState) -> String {
    let receipt = software_command_impl(
        SoftwareCommandDto::Create {
            name: name.into(),
            category: "cli".into(),
            summary: Some(format!("Summary for {name}")),
            install_source: Some("homebrew_formula".into()),
            version: Some("1.85.0".into()),
            install_location: None,
            executable_path: None,
            purpose: Some(format!("Purpose for {name}")),
            notes: None,
            architecture: None,
            tags: vec!["dev".into()],
        },
        state,
    )
    .expect("create software");
    receipt.asset_ids[0].clone()
}

#[test]
fn test_export_creates_valid_bundle_and_returns_receipt() {
    let (state, _db_path) = setup_test_state("export_test");

    // Seed media, software, and service
    create_test_media("Sci-Fi Book", &state);
    create_test_software("Code Editor", &state);

    service_command_impl(
        ServiceCommandDto::Create {
            name: "Cloud Storage".into(),
            service_type: "saas".into(),
            summary: None,
            provider: Some("Backblaze".into()),
            account_label: None,
            endpoint_url: None,
            dashboard_url: None,
            domain_name: None,
            plan: Some("Standard 2TB".into()),
            cost: Some("6.00".into()),
            currency: Some("USD".into()),
            billing_cadence: Some("monthly".into()),
            renews_at: None,
            expires_at: None,
            auto_renew: Some(true),
            notes: None,
            tags: vec!["infra".into()],
        },
        &state,
    )
    .expect("seed service");

    let export_dir = temp_dir_path("bundle-export");
    let receipt = portable_export_impl(&export_dir.to_string_lossy(), &state).expect("export");

    assert_eq!(receipt.format, "assetmesh-portable-export");
    assert_eq!(receipt.version, 1);
    assert_eq!(receipt.record_counts.get("assets"), Some(&3));
    assert_eq!(receipt.record_counts.get("media"), Some(&1));
    assert_eq!(receipt.record_counts.get("software"), Some(&1));
    assert_eq!(receipt.record_counts.get("services"), Some(&1));

    // Files exist on disk
    assert!(export_dir.join("manifest.json").exists());
    assert!(export_dir.join("assets.jsonl").exists());
    assert!(export_dir.join("modules/media.jsonl").exists());
    assert!(export_dir.join("modules/software.jsonl").exists());
    assert!(export_dir.join("modules/services.jsonl").exists());
}

#[test]
fn test_import_preview_and_apply_are_symmetric() {
    let (source_state, _src_db) = setup_test_state("src_symm");

    // Seed 1 media, 1 software in source
    create_test_media("Dune", &source_state);
    create_test_software("Ripgrep", &source_state);

    let bundle_dir = temp_dir_path("export-symm");
    portable_export_impl(&bundle_dir.to_string_lossy(), &source_state).expect("export");

    // Create pristine destination DB
    let (dest_state, _dest_db) = setup_test_state("dest_symm");

    // Run preview (dry-run)
    let preview =
        portable_import_preview_impl(&bundle_dir.to_string_lossy(), &dest_state).expect("preview");
    assert!(preview.valid);
    assert!(preview.errors.is_empty());
    assert_eq!(preview.dispositions.assets_created, 2);
    assert_eq!(preview.dispositions.media_created, 1);
    assert_eq!(preview.dispositions.software_created, 1);
    assert_eq!(preview.dispositions.assets_updated, 0);

    // Destination library is still empty after preview!
    let list_before =
        library_list_impl(Some(LibraryQueryDto::default()), &dest_state).expect("list");
    assert_eq!(list_before.total, Some(0));

    // Run apply
    let receipt =
        portable_import_apply_impl(&bundle_dir.to_string_lossy(), &preview.fingerprint, &dest_state).expect("apply");
    assert!(receipt.success);

    // EXACT FIELD-FOR-FIELD SYMMETRY:
    assert_eq!(receipt.report, preview.dispositions);

    // Destination library now has the 2 assets!
    let list_after =
        library_list_impl(Some(LibraryQueryDto::default()), &dest_state).expect("list");
    assert_eq!(list_after.total, Some(2));
}

#[test]
fn test_import_rejects_malformed_bundle_and_preserves_destination() {
    let (state, _db) = setup_test_state("malformed_test");

    // Seed 1 asset in destination
    let initial_asset_id = create_test_media("Existing Film", &state);

    // Create a malformed bundle dir (corrupt assets.jsonl)
    let bad_dir = temp_dir_path("bad_bundle");

    let manifest_json = r#"{
        "format": "assetmesh-portable-export",
        "version": 1,
        "created_at": "2026-01-01T00:00:00Z",
        "app_version": "0.3.0",
        "modules": {},
        "record_counts": { "assets": 1 }
    }"#;
    fs::write(bad_dir.join("manifest.json"), manifest_json).expect("write manifest");
    // Invalid JSONL
    fs::write(bad_dir.join("assets.jsonl"), "{ invalid json \n").expect("write corrupt assets");

    // Preview reports invalid
    let preview =
        portable_import_preview_impl(&bad_dir.to_string_lossy(), &state).expect("preview");
    assert!(!preview.valid);
    assert!(!preview.errors.is_empty());

    // Apply fails loudly
    let apply_res = portable_import_apply_impl(&bad_dir.to_string_lossy(), &preview.fingerprint, &state);
    assert!(apply_res.is_err());

    // Destination is completely preserved!
    let list = library_list_impl(Some(LibraryQueryDto::default()), &state).expect("query");
    assert_eq!(list.total, Some(1));
    assert_eq!(list.items[0].id, initial_asset_id);
}

#[test]
fn test_import_rejects_unsupported_version() {
    let (state, _db) = setup_test_state("unsupported_ver");

    let bad_dir = temp_dir_path("bad_version_bundle");

    let manifest_json = r#"{
        "format": "assetmesh-portable-export",
        "version": 999,
        "created_at": "2026-01-01T00:00:00Z",
        "app_version": "99.0.0",
        "modules": {},
        "record_counts": {}
    }"#;
    fs::write(bad_dir.join("manifest.json"), manifest_json).expect("write manifest");

    let preview =
        portable_import_preview_impl(&bad_dir.to_string_lossy(), &state).expect("preview");
    assert!(!preview.valid);
    assert!(preview
        .errors
        .iter()
        .any(|e| e.contains("Unsupported export version 999")));

    let apply_res = portable_import_apply_impl(&bad_dir.to_string_lossy(), &preview.fingerprint, &state);
    assert!(apply_res.is_err());
}

#[test]
fn test_import_rejects_collision_and_preserves_state() {
    let (state, _db) = setup_test_state("collision_test");

    // Seed destination with 1 software asset
    let sw_id = create_test_software("cli-tool", &state);

    // Export a bundle containing this software asset
    let bundle_dir = temp_dir_path("col_bundle");
    portable_export_impl(&bundle_dir.to_string_lossy(), &state).expect("export bundle");

    // Mutate the exported bundle's assets.jsonl to claim the same ID as a "media.movie" asset!
    // And provide a media record in modules/media.jsonl while clearing modules/software.jsonl.
    // This creates an internally-valid bundle that collides with the destination by re-typing
    // an existing asset while the destination retains its software record!
    let assets_file = bundle_dir.join("assets.jsonl");
    let content = fs::read_to_string(&assets_file).expect("read assets");
    let mutated = content.replace("software.cli", "media.movie");
    fs::write(&assets_file, mutated).expect("rewrite assets");

    let media_record = format!(
        r#"{{"asset_id":"{sw_id}","media_type":"movie","status":"planned","rating":null,"year":2024,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}}"#
    );
    fs::write(
        bundle_dir.join("modules/media.jsonl"),
        format!("{media_record}\n"),
    )
    .expect("write media");
    fs::write(bundle_dir.join("modules/software.jsonl"), "").expect("clear software");

    let manifest_path = bundle_dir.join("manifest.json");
    let manifest_str = fs::read_to_string(&manifest_path).expect("read manifest");
    let mut manifest_val: serde_json::Value =
        serde_json::from_str(&manifest_str).expect("parse manifest");
    manifest_val["record_counts"]["media"] = serde_json::json!(1);
    manifest_val["record_counts"]["software"] = serde_json::json!(0);
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest_val).unwrap(),
    )
    .expect("rewrite manifest");

    // Preview catches collision / conflict
    let preview =
        portable_import_preview_impl(&bundle_dir.to_string_lossy(), &state).expect("preview");
    assert!(!preview.valid);
    assert!(preview
        .errors
        .iter()
        .any(|e| e.to_lowercase().contains("re-type")
            || e.to_lowercase().contains("strand")
            || e.to_lowercase().contains("conflict")));

    // Apply fails without mutating state
    let apply_res = portable_import_apply_impl(&bundle_dir.to_string_lossy(), &preview.fingerprint, &state);
    assert!(apply_res.is_err());

    // Destination still has only the original software asset
    let list = library_list_impl(Some(LibraryQueryDto::default()), &state).expect("query");
    assert_eq!(list.total, Some(1));
    assert_eq!(list.items[0].id, sw_id);
    assert_eq!(list.items[0].kind, "software.cli");
}

#[test]
fn test_import_rejects_fingerprint_mismatch_on_tampered_bundle() {
    let (source_state, _src_db) = setup_test_state("src_tamper");
    create_test_media("Original Title", &source_state);

    let bundle_dir = temp_dir_path("tamper_bundle");
    portable_export_impl(&bundle_dir.to_string_lossy(), &source_state).expect("export");

    let (dest_state, _dest_db) = setup_test_state("dest_tamper");

    // Preview computes initial fingerprint
    let preview =
        portable_import_preview_impl(&bundle_dir.to_string_lossy(), &dest_state).expect("preview");
    assert!(preview.valid);
    assert!(!preview.fingerprint.is_empty());
    let original_fingerprint = preview.fingerprint.clone();

    // Tamper with bundle after preview: modify manifest.json or files
    let assets_file = bundle_dir.join("assets.jsonl");
    let content = fs::read_to_string(&assets_file).expect("read assets");
    let tampered = content.replace("Original Title", "Tampered Title");
    fs::write(&assets_file, tampered).expect("write tampered assets");

    // Apply with previewed fingerprint must fail with conflict!
    let apply_err = portable_import_apply_impl(
        &bundle_dir.to_string_lossy(),
        &original_fingerprint,
        &dest_state,
    )
    .expect_err("apply must fail on tampered content");

    assert_eq!(apply_err.category, assetmesh_desktop_lib::error::DesktopErrorCategory::Conflict);
    assert!(apply_err.message.contains("Bundle content changed since preview"));

    // Destination library remains completely empty
    let list = library_list_impl(Some(LibraryQueryDto::default()), &dest_state).expect("list");
    assert_eq!(list.total, Some(0));
}

#[test]
fn test_settings_reports_real_db_and_provider_status() {
    let (state, db_path) = setup_test_state("settings_test");

    let settings = app_settings_impl(&state).expect("app_settings");

    assert_eq!(
        settings.db_path,
        Some(db_path.to_string_lossy().to_string())
    );
    assert_eq!(settings.db_status, "Ready");
    assert!(!settings.app_version.is_empty());
    assert_eq!(settings.providers.len(), 3);

    let names: Vec<_> = settings.providers.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"macos_applications"));
    assert!(names.contains(&"homebrew"));
    assert!(names.contains(&"cli_tools"));

    // Ensure capabilities are exposed
    assert!(settings.capabilities.modules.contains(&"media".into()));
    assert!(settings.capabilities.modules.contains(&"software".into()));
    assert!(settings.capabilities.modules.contains(&"services".into()));
}

/// Canned `osascript` replies, so the chooser contract is exercised without
/// raising a real macOS dialog that would block on a human.
#[derive(Debug)]
struct FakeChooserRunner {
    stdout: String,
    cancelled: bool,
    ran_with: std::sync::Mutex<Vec<String>>,
}

impl FakeChooserRunner {
    fn new(stdout: &str, cancelled: bool) -> Self {
        FakeChooserRunner {
            stdout: stdout.to_string(),
            cancelled,
            ran_with: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn ran_with(&self) -> Vec<String> {
        self.ran_with.lock().unwrap().clone()
    }
}

impl assetmesh_providers::CommandRunner for FakeChooserRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
    ) -> Result<std::process::Output, assetmesh_providers::CommandError> {
        assert_eq!(program, "osascript");
        *self.ran_with.lock().unwrap() = args.iter().map(|s| s.to_string()).collect();
        let status = if self.cancelled {
            std::process::Command::new("false").output().unwrap().status
        } else {
            std::process::Command::new("true").output().unwrap().status
        };
        Ok(std::process::Output {
            status,
            stdout: self.stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        })
    }
}

#[test]
fn test_pick_directory_cancellation_returns_none() {
    let runner = FakeChooserRunner::new("", true);
    let res = pick_directory_impl(Some("Select Folder".to_string()), &runner);
    assert!(res.unwrap().is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn test_pick_directory_returns_trimmed_selection_and_prompts() {
    let runner = FakeChooserRunner::new("/Users/dev/Downloads/\n", false);
    let res = pick_directory_impl(Some("Select Folder".to_string()), &runner);
    assert_eq!(res.unwrap().as_deref(), Some("/Users/dev/Downloads/"));

    let script = runner.ran_with().join(" ");
    assert!(script.contains("choose folder"), "got {script}");
    assert!(script.contains("Select Folder"), "got {script}");
}
