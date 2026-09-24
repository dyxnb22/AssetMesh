//! Contract tests for Desktop Media workflow commands (P5-06).

use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{library_get_impl, media_command_impl};
use assetmesh_desktop_lib::dto::MediaCommandDto;
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-media-{}-{}.db",
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
fn empty_metadata_update_still_checks_existence_and_revision() {
    let state = setup_test_state("empty-update");
    let empty = |asset_id: String, expected_revision| MediaCommandDto::UpdateMetadata {
        asset_id,
        expected_revision,
        title: None,
        summary: None,
        year: None,
        platform: None,
        notes: None,
    };
    let missing =
        media_command_impl(empty(uuid::Uuid::now_v7().to_string(), Some(1)), &state).unwrap_err();
    assert_eq!(missing.category, "not_found");

    let created = media_command_impl(
        MediaCommandDto::Create {
            title: "No-op target".into(),
            media_type: "movie".into(),
            summary: None,
            status: None,
            rating: None,
            year: None,
            platform: None,
            progress_unit: None,
            progress_current: None,
            progress_total: None,
            notes: None,
            tags: Vec::new(),
        },
        &state,
    )
    .unwrap();
    let id = created.asset_ids[0].clone();
    let missing_revision = media_command_impl(empty(id.clone(), None), &state).unwrap_err();
    assert_eq!(missing_revision.category, "invalid_input");
    let stale = media_command_impl(empty(id.clone(), Some(2)), &state).unwrap_err();
    assert_eq!(stale.category, "stale_revision");
    let receipt = media_command_impl(empty(id, Some(1)), &state).unwrap();
    assert!(!receipt.changed);
    assert_eq!(receipt.revision, Some(1));
}

#[test]
fn media_workflow_create_and_read_back() {
    let state = setup_test_state("create");

    let create_cmd = MediaCommandDto::Create {
        title: "Frieren: Beyond Journey's End".into(),
        media_type: "anime".into(),
        summary: Some("Elven mage post-adventure tale".into()),
        status: Some("planned".into()),
        rating: Some(9.8),
        year: Some(2023),
        platform: Some("Crunchyroll".into()),
        progress_unit: Some("episodes".into()),
        progress_current: Some(0.0),
        progress_total: Some(28.0),
        notes: Some("Masterpiece fantasy".into()),
        tags: vec!["anime".into(), "fantasy".into()],
    };

    let receipt = media_command_impl(create_cmd, &state).expect("create media");
    assert_eq!(receipt.operation, "media.create");
    assert!(receipt.changed);
    assert_eq!(receipt.revision, Some(1));
    assert_eq!(receipt.asset_ids.len(), 1);

    let asset_id = &receipt.asset_ids[0];

    // Read back through canonical get
    let detail = library_get_impl(asset_id, &state).expect("library get");
    assert_eq!(detail.name, "Frieren: Beyond Journey's End");
    assert_eq!(detail.kind, "media.anime");
    assert_eq!(detail.revision, 1);
    assert_eq!(detail.details["module"], "media");
    assert_eq!(detail.details["status"], "planned");
    assert_eq!(detail.details["year"], 2023);
    assert_eq!(detail.details["progress"]["unit"], "episodes");
    assert_eq!(detail.details["progress"]["total"], 28.0);
}

#[test]
fn media_workflow_update_metadata_and_conflict() {
    let state = setup_test_state("update");

    let create_cmd = MediaCommandDto::Create {
        title: "Initial Title".into(),
        media_type: "movie".into(),
        summary: None,
        status: Some("planned".into()),
        rating: None,
        year: Some(2020),
        platform: None,
        progress_unit: None,
        progress_current: None,
        progress_total: None,
        notes: None,
        tags: vec![],
    };
    let receipt = media_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // 1. Valid update
    let update_cmd = MediaCommandDto::UpdateMetadata {
        asset_id: asset_id.clone(),
        expected_revision: Some(1),
        title: Some("Updated Movie Title".into()),
        summary: Some("Updated summary".into()),
        year: Some(2021),
        platform: Some("Cinema".into()),
        notes: Some("Watched in IMAX".into()),
    };
    let update_receipt = media_command_impl(update_cmd, &state).expect("update");
    assert_eq!(update_receipt.operation, "media.update_metadata");
    assert!(update_receipt.changed);
    assert_eq!(update_receipt.revision, Some(2));

    // 2. Stale revision conflict
    let stale_cmd = MediaCommandDto::UpdateMetadata {
        asset_id: asset_id.clone(),
        expected_revision: Some(1), // Expected 1, but actual is 2!
        title: Some("Conflicting Title".into()),
        summary: None,
        year: None,
        platform: None,
        notes: None,
    };
    let err = media_command_impl(stale_cmd, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // 3. No-op update
    let noop_cmd = MediaCommandDto::UpdateMetadata {
        asset_id: asset_id.clone(),
        expected_revision: Some(2),
        title: None,
        summary: None,
        year: None,
        platform: None,
        notes: None,
    };
    let noop_receipt = media_command_impl(noop_cmd, &state).expect("noop");
    assert!(!noop_receipt.changed);
}

#[test]
fn media_workflow_status_transitions() {
    let state = setup_test_state("transitions");

    let create_cmd = MediaCommandDto::Create {
        title: "Game Title".into(),
        media_type: "game".into(),
        summary: None,
        status: Some("planned".into()),
        rating: None,
        year: Some(2022),
        platform: Some("Steam".into()),
        progress_unit: Some("hours".into()),
        progress_current: Some(0.0),
        progress_total: None,
        notes: None,
        tags: vec![],
    };
    let receipt = media_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Transition: planned -> in_progress
    let start_cmd = MediaCommandDto::TransitionStatus {
        asset_id: asset_id.clone(),
        status: "in_progress".into(),
        expected_revision: receipt.revision,
    };
    let r1 = media_command_impl(start_cmd, &state).expect("start");
    assert_eq!(r1.operation, "media.transition.in_progress");
    assert!(r1.revision.is_some());

    // Check read-back
    let d1 = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d1.details["status"], "in_progress");

    // Transition: in_progress -> paused
    let pause_cmd = MediaCommandDto::TransitionStatus {
        asset_id: asset_id.clone(),
        status: "paused".into(),
        expected_revision: r1.revision,
    };
    let r2 = media_command_impl(pause_cmd, &state).expect("pause");
    assert_eq!(r2.operation, "media.transition.paused");

    // Transition: paused -> completed
    let complete_cmd = MediaCommandDto::TransitionStatus {
        asset_id: asset_id.clone(),
        status: "completed".into(),
        expected_revision: r2.revision,
    };
    let r3 = media_command_impl(complete_cmd, &state).expect("complete");
    assert_eq!(r3.operation, "media.transition.completed");

    let d3 = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d3.details["status"], "completed");
}

#[test]
fn media_workflow_progress_and_rating() {
    let state = setup_test_state("progress_rating");

    let create_cmd = MediaCommandDto::Create {
        title: "Book Title".into(),
        media_type: "tv".into(),
        summary: None,
        status: Some("in_progress".into()),
        rating: None,
        year: Some(2023),
        platform: None,
        progress_unit: Some("chapters".into()),
        progress_current: Some(1.0),
        progress_total: Some(10.0),
        notes: None,
        tags: vec![],
    };
    let receipt = media_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Update progress
    let prog_cmd = MediaCommandDto::UpdateProgress {
        asset_id: asset_id.clone(),
        unit: Some("chapters".into()),
        current: Some(5.0),
        total: Some(10.0),
        expected_revision: receipt.revision,
    };
    let r_prog = media_command_impl(prog_cmd, &state).expect("update progress");
    assert_eq!(r_prog.operation, "media.update_progress");

    let d_prog = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d_prog.details["progress"]["current"], 5.0);

    // Negative progress validation failure
    let invalid_prog_cmd = MediaCommandDto::UpdateProgress {
        asset_id: asset_id.clone(),
        unit: Some("chapters".into()),
        current: Some(-1.0),
        total: Some(10.0),
        expected_revision: r_prog.revision,
    };
    let err_prog = media_command_impl(invalid_prog_cmd, &state).unwrap_err();
    assert_eq!(err_prog.category, "invalid_input");

    // Rate media
    let rate_cmd = MediaCommandDto::Rate {
        asset_id: asset_id.clone(),
        rating: 8.5,
        expected_revision: r_prog.revision,
    };
    let r_rate = media_command_impl(rate_cmd, &state).expect("rate");
    assert_eq!(r_rate.operation, "media.rate");

    let d_rate = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d_rate.details["rating"], 8.5);

    // Out-of-bounds rating (> 10.0) validation failure
    let invalid_rate_cmd = MediaCommandDto::Rate {
        asset_id: asset_id.clone(),
        rating: 12.0,
        expected_revision: r_rate.revision,
    };
    let err_rate = media_command_impl(invalid_rate_cmd, &state).unwrap_err();
    assert_eq!(err_rate.category, "invalid_input");
}

#[test]
fn media_workflow_archive() {
    let state = setup_test_state("archive");

    let create_cmd = MediaCommandDto::Create {
        title: "To Archive".into(),
        media_type: "anime".into(),
        summary: None,
        status: Some("completed".into()),
        rating: Some(9.0),
        year: Some(2019),
        platform: None,
        progress_unit: None,
        progress_current: None,
        progress_total: None,
        notes: None,
        tags: vec![],
    };
    let receipt = media_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Archive
    let archive_cmd = MediaCommandDto::Archive {
        asset_id: asset_id.clone(),
        expected_revision: receipt.revision,
    };
    let r_archive = media_command_impl(archive_cmd, &state).expect("archive");
    assert_eq!(r_archive.operation, "asset.archive");
    assert!(r_archive.revision.is_some());

    // Read back and verify lifecycle is archived
    let detail = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(detail.lifecycle, "archived");
    assert!(detail.archived_at.is_some());
}

#[test]
fn media_stale_revision_is_rejected_with_stale_revision_category() {
    let state = setup_test_state("media_stale_rev");

    let create_cmd = MediaCommandDto::Create {
        title: "Stale Test".into(),
        media_type: "anime".into(),
        summary: None,
        status: Some("planned".into()),
        rating: None,
        year: None,
        platform: None,
        progress_unit: None,
        progress_current: None,
        progress_total: None,
        notes: None,
        tags: vec![],
    };
    let receipt = media_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Transition with stale expected revision 999
    let bad_transition = MediaCommandDto::TransitionStatus {
        asset_id: asset_id.clone(),
        status: "in_progress".into(),
        expected_revision: Some(999),
    };
    let err = media_command_impl(bad_transition, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // Verify state was not modified (rolled back)
    let d = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d.details["status"], "planned");

    // Update progress with stale expected revision 999
    let bad_progress = MediaCommandDto::UpdateProgress {
        asset_id: asset_id.clone(),
        unit: Some("episodes".into()),
        current: Some(3.0),
        total: Some(12.0),
        expected_revision: Some(999),
    };
    let err = media_command_impl(bad_progress, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // Rate with stale expected revision 999
    let bad_rate = MediaCommandDto::Rate {
        asset_id: asset_id.clone(),
        rating: 9.0,
        expected_revision: Some(999),
    };
    let err = media_command_impl(bad_rate, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // Archive with stale expected revision 999
    let bad_archive = MediaCommandDto::Archive {
        asset_id: asset_id.clone(),
        expected_revision: Some(999),
    };
    let err = media_command_impl(bad_archive, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // State still planned and active
    let d_final = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d_final.lifecycle, "active");
    assert_eq!(d_final.details["status"], "planned");
}
