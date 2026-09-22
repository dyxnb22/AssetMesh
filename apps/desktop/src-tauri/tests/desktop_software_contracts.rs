//! Contract tests for Desktop Software workflow commands & discovery (P5-06).

use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{
    library_get_impl, library_list_impl, software_command_impl, software_discover_impl,
};
use assetmesh_desktop_lib::dto::{CandidateRefDto, SoftwareCandidateDto, SoftwareCommandDto};
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-software-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

fn setup_test_state(name: &str) -> DesktopState {
    let db_path = temp_db_path(name);
    let state = DesktopState::with_clock_and_ids(Arc::new(SystemClock), Arc::new(UuidV7Generator));
    state.initialize(&db_path).expect("initialize");
    state
}

#[test]
fn software_workflow_create_and_read_back() {
    let state = setup_test_state("create");

    let create_cmd = SoftwareCommandDto::Create {
        name: "Visual Studio Code".into(),
        category: "application".into(),
        summary: Some("Code editor redefined".into()),
        install_source: Some("homebrew_cask".into()),
        version: Some("1.85.0".into()),
        install_location: Some("/Applications/Visual Studio Code.app".into()),
        executable_path: Some(
            "/Applications/Visual Studio Code.app/Contents/MacOS/Electron".into(),
        ),
        purpose: Some("Primary development editor".into()),
        notes: Some("Configured with Rust Analyzer".into()),
        architecture: Some("arm64".into()),
        tags: vec!["ide".into(), "development".into()],
    };

    let receipt = software_command_impl(create_cmd, &state).expect("create software");
    assert_eq!(receipt.operation, "software.create");
    assert!(receipt.changed);
    assert_eq!(receipt.revision, Some(1));
    assert_eq!(receipt.asset_ids.len(), 1);

    let asset_id = &receipt.asset_ids[0];

    // Read back canonical
    let detail = library_get_impl(asset_id, &state).expect("library get");
    assert_eq!(detail.name, "Visual Studio Code");
    assert_eq!(detail.kind, "software.app");
    assert_eq!(detail.revision, 1);
    assert_eq!(detail.details["module"], "software");
    assert_eq!(detail.details["version"], "1.85.0");
    assert_eq!(detail.details["install_source"], "homebrew_cask");
    assert_eq!(detail.details["purpose"], "Primary development editor");
    assert_eq!(detail.details["notes"], "Configured with Rust Analyzer");
    assert_eq!(detail.details["architecture"], "arm64");
}

#[test]
fn software_workflow_update_metadata_and_conflict() {
    let state = setup_test_state("update");

    let create_cmd = SoftwareCommandDto::Create {
        name: "Sublime Text".into(),
        category: "application".into(),
        summary: None,
        install_source: None,
        version: Some("4.0".into()),
        install_location: None,
        executable_path: None,
        purpose: Some("Fast text editor".into()),
        notes: None,
        architecture: None,
        tags: vec![],
    };
    let receipt = software_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // 1. Valid update
    let update_cmd = SoftwareCommandDto::UpdateMetadata {
        asset_id: asset_id.clone(),
        expected_revision: Some(1),
        name: Some("Sublime Text 4".into()),
        summary: Some("Sophisticated text editor".into()),
        version: Some("4.1.0".into()),
        install_location: None,
        executable_path: None,
        purpose: Some("Secondary text editor".into()),
        notes: Some("License active".into()),
        architecture: Some("universal".into()),
    };
    let update_receipt = software_command_impl(update_cmd, &state).expect("update");
    assert_eq!(update_receipt.operation, "software.update_metadata");
    assert!(update_receipt.changed);
    assert_eq!(update_receipt.revision, Some(2));

    // Read back
    let detail = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(detail.name, "Sublime Text 4");
    assert_eq!(detail.details["purpose"], "Secondary text editor");
    assert_eq!(detail.details["notes"], "License active");

    // 2. Stale revision conflict
    let stale_cmd = SoftwareCommandDto::UpdateMetadata {
        asset_id: asset_id.clone(),
        expected_revision: Some(1), // Actual is 2
        name: Some("Conflict Text".into()),
        summary: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
    };
    let err = software_command_impl(stale_cmd, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // 3. No-op update
    let noop_cmd = SoftwareCommandDto::UpdateMetadata {
        asset_id: asset_id.clone(),
        expected_revision: Some(2),
        name: None,
        summary: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
    };
    let noop_receipt = software_command_impl(noop_cmd, &state).expect("noop");
    assert!(!noop_receipt.changed);
}

#[test]
fn software_workflow_discover_readonly() {
    let state = setup_test_state("discover");

    // Pre-condition: library is empty
    let list_before = library_list_impl(None, &state).expect("list before");
    assert_eq!(list_before.items.len(), 0);

    // Run discovery preview
    let candidates = software_discover_impl(&state).expect("software discover");
    // On macOS, discovering apps/brew/cli might find candidates (or empty in CI)
    // The key invariant is that calling discover NEVER writes to the canonical library!
    let list_after = library_list_impl(None, &state).expect("list after");
    assert_eq!(
        list_after.items.len(),
        0,
        "discovery preview must NOT mutate canonical assets"
    );

    // Verify candidates have classification status
    for c in &candidates {
        assert!(!c.candidate.display_name.is_empty());
        assert!(!c.candidate.provider.is_empty());
    }
}

#[test]
fn software_workflow_adopt_candidate_preserves_user_purpose_and_notes() {
    let state = setup_test_state("adopt");

    let candidate_dto = SoftwareCandidateDto {
        provider: "brew".into(),
        display_name: "ripgrep".into(),
        category: "cli".into(),
        install_source: "homebrew_formula".into(),
        version: Some("14.1.0".into()),
        install_location: Some("/opt/homebrew/bin/rg".into()),
        executable_path: Some("/opt/homebrew/bin/rg".into()),
        external_refs: vec![CandidateRefDto {
            namespace: "homebrew_formula".into(),
            external_id: "ripgrep".into(),
        }],
        metadata: None,
    };

    // Adopt with explicit purpose and notes
    let adopt_cmd = SoftwareCommandDto::AdoptCandidate {
        candidate: candidate_dto,
        target: Some("create_new".into()),
        purpose: Some("Fast project code search".into()),
        notes: Some("Replaces ack and grep in daily workflow".into()),
        tags: vec!["cli".into(), "search".into()],
    };

    let receipt = software_command_impl(adopt_cmd, &state).expect("adopt candidate");
    assert_eq!(receipt.operation, "software.adopt");
    assert!(receipt.changed);
    assert_eq!(receipt.asset_ids.len(), 1);

    let asset_id = &receipt.asset_ids[0];
    let detail = library_get_impl(asset_id, &state).expect("get");
    assert_eq!(detail.name, "ripgrep");
    assert_eq!(detail.details["purpose"], "Fast project code search");
    assert_eq!(
        detail.details["notes"],
        "Replaces ack and grep in daily workflow"
    );
    assert_eq!(detail.details["version"], "14.1.0");
    assert_eq!(detail.details["install_source"], "homebrew_formula");
}

#[test]
fn software_workflow_archive() {
    let state = setup_test_state("archive");

    let create_cmd = SoftwareCommandDto::Create {
        name: "Old Editor".into(),
        category: "application".into(),
        summary: None,
        install_source: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
        tags: vec![],
    };
    let receipt = software_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Archive
    let archive_cmd = SoftwareCommandDto::Archive {
        asset_id: asset_id.clone(),
        expected_revision: receipt.revision,
    };
    let r_archive = software_command_impl(archive_cmd, &state).expect("archive");
    assert_eq!(r_archive.operation, "asset.archive");
    assert!(r_archive.revision.is_some());

    let detail = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(detail.lifecycle, "archived");
    assert!(detail.archived_at.is_some());
}

#[test]
fn software_stale_revision_is_rejected_with_stale_revision_category() {
    let state = setup_test_state("software_stale_rev");

    let create_cmd = SoftwareCommandDto::Create {
        name: "Stale Software".into(),
        category: "cli".into(),
        summary: None,
        install_source: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
        tags: vec![],
    };
    let receipt = software_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Archive with wrong expected revision
    let bad_archive = SoftwareCommandDto::Archive {
        asset_id: asset_id.clone(),
        expected_revision: Some(999),
    };
    let err = software_command_impl(bad_archive, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // Still active
    let d = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d.lifecycle, "active");
}
