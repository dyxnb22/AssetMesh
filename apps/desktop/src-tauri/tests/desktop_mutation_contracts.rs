//! Desktop Mutation Pattern & Receipt Contract Tests (P5-05).
//!
//! Verifies the 4 primary mutation paths required by docs/12 P5-05:
//! 1. Success path: update metadata commits transaction, returns MutationReceiptDto, increments revision.
//! 2. Validation error path: invalid transport input rejects before transaction.
//! 3. Stale revision path: outdated revision returns conflict/stale_revision error.
//! 4. No-op path: unchanged/empty update returns no-change receipt (changed = false).
//! 5. Transaction failure / Not found path: non-existent asset returns not_found error.

use std::sync::Arc;

use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::library::library_get_impl;
use assetmesh_desktop_lib::commands::software::software_command_impl;
use assetmesh_desktop_lib::dto::SoftwareCommandDto;
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-mutation-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

#[test]
fn desktop_mutation_contracts_cover_success_validation_stale_and_noop() {
    let db_path = temp_db_path("mutation_contracts");
    let state = DesktopState::new();
    state.initialize(&db_path).expect("failed to init state");

    let clock = Arc::new(SystemClock);
    let ids = Arc::new(UuidV7Generator);
    let factory = state.factory.read().unwrap().clone().unwrap();

    // 1. Create a software asset
    let mut software_svc = SoftwareService::new(factory, clock, ids);
    let software = software_svc
        .create_software(CreateSoftware {
            name: "VS Code".into(),
            category: SoftwareCategory::Application,
            summary: Some("Code editor".into()),
            install_source: None,
            version: Some("1.85.0".into()),
            install_location: Some("/Applications/Visual Studio Code.app".into()),
            executable_path: Some("/usr/local/bin/code".into()),
            purpose: Some("General purpose code editing".into()),
            notes: Some("Original notes".into()),
            tags: vec!["editor".into(), "dev".into()],
            external_refs: Vec::new(),
            installed_at: None,
            architecture: Some("universal".into()),
        })
        .unwrap();

    let asset_id = software.entry.asset.id.to_string();
    let initial_rev = software.entry.asset.revision;

    // -------------------------------------------------------------------------
    // Path 1: Success Path
    // Edit purpose and notes with expected revision -> receipt + read-back
    // -------------------------------------------------------------------------
    let success_receipt = software_command_impl(
        SoftwareCommandDto::UpdateMetadata {
            asset_id: asset_id.clone(),
            expected_revision: Some(initial_rev),
            name: None,
            summary: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: Some("Rust and TypeScript development environment".into()),
            notes: Some("Updated configuration with extensions".into()),
            architecture: None,
        },
        &state,
    )
    .expect("mutation should succeed");

    assert_eq!(success_receipt.operation, "software.update_metadata");
    assert_eq!(success_receipt.asset_ids, vec![asset_id.clone()]);
    assert!(success_receipt.changed);
    assert_eq!(success_receipt.warnings.len(), 0);
    assert_eq!(
        success_receipt.revision,
        Some(initial_rev + 1),
        "Revision must be incremented upon mutation"
    );

    // Read-back verification via library_get_impl
    let detail = library_get_impl(&asset_id, &state).expect("read-back must succeed");
    assert_eq!(detail.revision, initial_rev + 1);
    let details_json = &detail.details;
    assert_eq!(
        details_json["purpose"].as_str(),
        Some("Rust and TypeScript development environment")
    );
    assert_eq!(
        details_json["notes"].as_str(),
        Some("Updated configuration with extensions")
    );

    let current_rev = detail.revision;

    // -------------------------------------------------------------------------
    // Path 2: Validation Failure Path
    // Empty name should fail validation with invalid_input category
    // -------------------------------------------------------------------------
    let validation_err = software_command_impl(
        SoftwareCommandDto::UpdateMetadata {
            asset_id: asset_id.clone(),
            expected_revision: Some(current_rev),
            name: Some("   ".into()), // Whitespace-only name rejected
            summary: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: None,
            notes: None,
            architecture: None,
        },
        &state,
    )
    .expect_err("empty name should fail validation");

    assert_eq!(
        validation_err.category, "invalid_input",
        "Validation failure must map to invalid_input category"
    );

    // Verify canonical state was untouched after validation error
    let detail_after_val_err = library_get_impl(&asset_id, &state).expect("read-back must succeed");
    assert_eq!(detail_after_val_err.revision, current_rev);
    assert_eq!(detail_after_val_err.name, "VS Code");

    // -------------------------------------------------------------------------
    // Path 3: Stale Revision Conflict Path
    // Supplying an outdated expected_revision must fail with stale_revision
    // -------------------------------------------------------------------------
    let stale_err = software_command_impl(
        SoftwareCommandDto::UpdateMetadata {
            asset_id: asset_id.clone(),
            expected_revision: Some(initial_rev), // Outdated revision!
            name: None,
            summary: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: Some("Conflicting edit".into()),
            notes: None,
            architecture: None,
        },
        &state,
    )
    .expect_err("stale revision must fail");

    assert_eq!(
        stale_err.category, "stale_revision",
        "Stale revision must map to stale_revision category"
    );

    // Verify canonical state was untouched after stale revision failure
    let detail_after_stale = library_get_impl(&asset_id, &state).expect("read-back must succeed");
    assert_eq!(detail_after_stale.revision, current_rev);
    assert_eq!(
        detail_after_stale.details["purpose"].as_str(),
        Some("Rust and TypeScript development environment")
    );

    // -------------------------------------------------------------------------
    // Path 4: No-op Path
    // Supplying no field updates returns changed = false without mutation
    // -------------------------------------------------------------------------
    let noop_receipt = software_command_impl(
        SoftwareCommandDto::UpdateMetadata {
            asset_id: asset_id.clone(),
            expected_revision: Some(current_rev),
            name: None,
            summary: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: None,
            notes: None,
            architecture: None,
        },
        &state,
    )
    .expect("no-op should return Ok receipt");

    assert!(!noop_receipt.changed);
    assert_eq!(noop_receipt.revision, Some(current_rev));
    assert!(
        !noop_receipt.warnings.is_empty(),
        "No-op receipt should explain that no fields were updated"
    );

    // -------------------------------------------------------------------------
    // Path 5: Not Found / Unknown Asset Path
    // Non-existent asset ID returns not_found category
    // -------------------------------------------------------------------------
    let non_existent_id = uuid::Uuid::now_v7().to_string();
    let not_found_err = software_command_impl(
        SoftwareCommandDto::UpdateMetadata {
            asset_id: non_existent_id,
            expected_revision: Some(1),
            name: None,
            summary: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: Some("Won't work".into()),
            notes: None,
            architecture: None,
        },
        &state,
    )
    .expect_err("unknown asset must fail");

    assert_eq!(not_found_err.category, "not_found");
}
