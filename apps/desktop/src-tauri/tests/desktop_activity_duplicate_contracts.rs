//! Activity and Duplicate Review Contract Tests — Phase 5 (P5-08).
//!
//! Verifies activity query with filters/pagination, candidate review with evidence,
//! merge preview conflict detection and relation impact, and atomic merge execution.
//! All through desktop command adapters over real temporary SQLite database.

use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{
    activity_query_impl, duplicate_candidates_impl, library_get_impl, media_command_impl,
    merge_apply_impl, merge_preview_impl, relation_attach_impl, service_command_impl,
    software_command_impl,
};
use assetmesh_desktop_lib::dto::{
    ActivityQueryDto, DuplicateQueryDto, MediaCommandDto, MergeApplyDto, MergePreviewQueryDto,
    RelationAttachDto, ServiceCommandDto, SoftwareCommandDto,
};
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-act-dup-{}-{}.db",
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

fn create_software(name: &str, state: &DesktopState) -> String {
    let receipt = software_command_impl(
        SoftwareCommandDto::Create {
            name: name.into(),
            category: "cli".into(),
            summary: Some(format!("Summary for {name}")),
            install_source: Some("homebrew_formula".into()),
            version: Some("1.0.0".into()),
            install_location: None,
            executable_path: None,
            purpose: Some(format!("Purpose for {name}")),
            notes: None,
            architecture: None,
            tags: vec!["cli".into()],
        },
        state,
    )
    .expect("create software");
    receipt.asset_ids[0].clone()
}

fn create_media(title: &str, state: &DesktopState) -> String {
    let receipt = media_command_impl(
        MediaCommandDto::Create {
            title: title.into(),
            media_type: "anime".into(),
            status: Some("in_progress".into()),
            summary: Some(format!("Summary for {title}")),
            year: Some(2023),
            rating: Some(9.0),
            progress_unit: Some("episodes".into()),
            progress_current: Some(1.0),
            progress_total: Some(12.0),
            platform: Some("Crunchyroll".into()),
            notes: None,
            tags: vec!["anime".into()],
        },
        state,
    )
    .expect("create media");
    receipt.asset_ids[0].clone()
}

fn create_service(name: &str, plan: Option<&str>, state: &DesktopState) -> String {
    let receipt = service_command_impl(
        ServiceCommandDto::Create {
            name: name.into(),
            service_type: "saas".into(),
            summary: None,
            provider: Some("OpenAI".into()),
            account_label: Some("Work".into()),
            endpoint_url: None,
            dashboard_url: Some("https://platform.openai.com".into()),
            domain_name: None,
            plan: plan.map(|s| s.to_string()),
            cost: Some("20.00".into()),
            currency: Some("USD".into()),
            billing_cadence: Some("monthly".into()),
            renews_at: None,
            expires_at: None,
            auto_renew: Some(true),
            notes: None,
            tags: vec!["ai".into()],
        },
        state,
    )
    .expect("create service");
    receipt.asset_ids[0].clone()
}

#[test]
fn activity_workflow_query_and_filters() {
    let state = setup_test_state("activity-query");

    let media_id = create_media("Steins;Gate", &state);
    let software_id = create_software("RipGrep", &state);
    let service_id = create_service("OpenAI API", Some("Tier 1"), &state);

    // Attach a relation between software and service
    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: software_id.clone(),
            relation_type: "depends_on".into(),
            target_asset_id: service_id.clone(),
            note: Some("CLI depends on API".into()),
        },
        &state,
    )
    .expect("attach relation");

    // 1. Query global activity: returns all events in newest-first order
    let all_page = activity_query_impl(ActivityQueryDto::default(), &state).expect("query all");
    assert!(
        all_page.items.len() >= 4,
        "expected at least 4 events, got {}",
        all_page.items.len()
    );
    assert_eq!(all_page.total, Some(all_page.items.len()));

    // Verify ordering: newest first
    for window in all_page.items.windows(2) {
        assert!(
            window[0].occurred_at >= window[1].occurred_at,
            "events must be in descending order: {} < {}",
            window[0].occurred_at,
            window[1].occurred_at
        );
    }

    // 2. Filter by asset_id (software only)
    let asset_page = activity_query_impl(
        ActivityQueryDto {
            asset_id: Some(software_id.clone()),
            ..Default::default()
        },
        &state,
    )
    .expect("query for software asset");
    assert!(!asset_page.items.is_empty());
    for item in &asset_page.items {
        assert_eq!(item.asset_id.as_deref(), Some(software_id.as_str()));
    }

    // 3. Filter by module (media only)
    let media_page = activity_query_impl(
        ActivityQueryDto {
            modules: Some(vec!["media".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("query media module");
    assert!(!media_page.items.is_empty());
    for item in &media_page.items {
        assert_eq!(item.module.as_deref(), Some("media"));
        assert_eq!(item.asset_id.as_deref(), Some(media_id.as_str()));
    }

    // 4. Pagination (limit 2, offset 1)
    let paged = activity_query_impl(
        ActivityQueryDto {
            limit: Some(2),
            offset: Some(1),
            ..Default::default()
        },
        &state,
    )
    .expect("query paged");
    assert_eq!(paged.items.len(), 2);
    assert_eq!(paged.offset, 1);
    assert_eq!(paged.limit, 2);
    assert_eq!(paged.items[0].id, all_page.items[1].id);
    assert_eq!(paged.items[1].id, all_page.items[2].id);
}

#[test]
fn activity_workflow_history_survives_archive_and_merge() {
    let state = setup_test_state("activity-survives");

    let keep_id = create_software("RipGrep Keeper", &state);
    let dup_id = create_software("RipGrep Duplicate", &state);

    // Merge duplicate into keeper
    merge_apply_impl(
        MergeApplyDto {
            winner_id: keep_id.clone(),
            loser_id: dup_id.clone(),
        },
        &state,
    )
    .expect("merge apply");

    // Query events for the merged duplicate: history survives!
    let dup_events = activity_query_impl(
        ActivityQueryDto {
            asset_id: Some(dup_id.clone()),
            ..Default::default()
        },
        &state,
    )
    .expect("query merged asset history");

    assert!(
        !dup_events.items.is_empty(),
        "history for merged asset must not be erased"
    );
    assert_eq!(
        dup_events.items[0].asset_name.as_deref(),
        Some("RipGrep Duplicate")
    );

    // Winner should have an asset.merged event
    let winner_events = activity_query_impl(
        ActivityQueryDto {
            asset_id: Some(keep_id.clone()),
            event_types: Some(vec!["asset.merged".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("query winner merge event");

    assert_eq!(winner_events.items.len(), 1);
    assert_eq!(winner_events.items[0].event_type, "asset.merged");
    assert_eq!(winner_events.items[0].payload["loser_id"], dup_id.as_str());
}

#[test]
fn duplicate_workflow_candidate_detection_and_evidence() {
    let state = setup_test_state("dup-detection");

    // Two tools with same normalized name and same kind
    let id_a = create_software("ripgrep", &state);
    let id_b = create_software("RipGrep", &state);

    let cand_page = duplicate_candidates_impl(
        DuplicateQueryDto {
            kinds: Some(vec!["software.tool".into(), "software.cli".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("query duplicate candidates");

    assert_eq!(
        cand_page.items.len(),
        1,
        "expected exactly 1 candidate pair for ripgrep and RipGrep"
    );

    let cand = &cand_page.items[0];
    // Ordered pair: left < right
    assert!(cand.left.id < cand.right.id);
    let ids = [cand.left.id.clone(), cand.right.id.clone()];
    assert!(ids.contains(&id_a));
    assert!(ids.contains(&id_b));

    // Evidence must contain same_normalized_name without confidence percentage
    assert!(!cand.evidence.is_empty());
    assert!(cand
        .evidence_labels
        .iter()
        .any(|l| l.contains("same normalized name (ripgrep)")));

    // Filter by kind (media.anime) returns 0 candidates
    let empty_page = duplicate_candidates_impl(
        DuplicateQueryDto {
            kinds: Some(vec!["media.anime".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("query media candidates");
    assert_eq!(empty_page.items.len(), 0);
}

#[test]
fn merge_workflow_preview_and_conflict_detection() {
    let state = setup_test_state("merge-conflict");

    // Two services with differing plan fields -> conflict
    let serv_a = create_service("OpenAI Account A", Some("Plus"), &state);
    let serv_b = create_service("OpenAI Account B", Some("Pro"), &state);

    let preview = merge_preview_impl(
        MergePreviewQueryDto {
            winner_id: serv_a.clone(),
            loser_id: serv_b.clone(),
        },
        &state,
    )
    .expect("preview");

    assert!(!preview.can_merge);
    assert!(!preview.conflicts.is_empty());
    assert!(preview
        .conflicts
        .iter()
        .any(|c| c.contains("plan: 'Plus' vs 'Pro'")));

    // Attempting to apply merge must fail cleanly and preserve canonical states
    let err = merge_apply_impl(
        MergeApplyDto {
            winner_id: serv_a.clone(),
            loser_id: serv_b.clone(),
        },
        &state,
    )
    .expect_err("merge apply must fail on conflict");

    assert_eq!(err.category, "conflict");

    // Both assets are still active!
    let a_detail = library_get_impl(&serv_a, &state).expect("get serv_a");
    let b_detail = library_get_impl(&serv_b, &state).expect("get serv_b");
    assert_eq!(a_detail.lifecycle, "active");
    assert_eq!(b_detail.lifecycle, "active");
}

#[test]
fn merge_workflow_preview_and_successful_apply() {
    let state = setup_test_state("merge-success");

    let winner_id = create_software("NeoVim Main", &state);
    let loser_id = create_software("NeoVim Secondary", &state);
    let other_service_id = create_service("GitHub Copilot", Some("Personal"), &state);

    // Attach relation on loser: loser depends_on other_service
    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: loser_id.clone(),
            relation_type: "depends_on".into(),
            target_asset_id: other_service_id.clone(),
            note: Some("loser dependency".into()),
        },
        &state,
    )
    .expect("attach relation");

    // Run preview
    let preview = merge_preview_impl(
        MergePreviewQueryDto {
            winner_id: winner_id.clone(),
            loser_id: loser_id.clone(),
        },
        &state,
    )
    .expect("preview");

    assert!(preview.can_merge, "expected merge to be permissible");
    assert!(preview.conflicts.is_empty());
    assert_eq!(preview.transferred_relations_count, 1);
    assert_eq!(preview.redundant_relations_count, 0);

    // Apply merge
    let receipt = merge_apply_impl(
        MergeApplyDto {
            winner_id: winner_id.clone(),
            loser_id: loser_id.clone(),
        },
        &state,
    )
    .expect("merge apply");

    assert_eq!(receipt.operation, "asset.merge");
    assert!(receipt.changed);

    // Winner is active
    let winner = library_get_impl(&winner_id, &state).expect("get winner");
    assert_eq!(winner.lifecycle, "active");

    // Loser is now merged tombstone redirecting to winner
    let loser = library_get_impl(&loser_id, &state).expect("get loser");
    assert_eq!(loser.lifecycle, "merged");
    assert_eq!(loser.merged_into.as_deref(), Some(winner_id.as_str()));
    assert_eq!(loser.details["module"], "merged_redirect");
    assert_eq!(loser.details["surviving_asset_id"], winner_id.as_str());
}

#[test]
fn merge_workflow_rejects_different_kinds_and_self_merge() {
    let state = setup_test_state("merge-invalid");

    let software_id = create_software("Tool X", &state);
    let media_id = create_media("Anime X", &state);

    // 1. Self merge returns conflict
    let self_err = merge_preview_impl(
        MergePreviewQueryDto {
            winner_id: software_id.clone(),
            loser_id: software_id.clone(),
        },
        &state,
    )
    .expect_err("self merge preview");
    assert_eq!(self_err.category, "conflict");

    // 2. Different kinds preview returns can_merge: false
    let diff_preview = merge_preview_impl(
        MergePreviewQueryDto {
            winner_id: software_id.clone(),
            loser_id: media_id.clone(),
        },
        &state,
    )
    .expect("different kinds preview");
    assert!(!diff_preview.can_merge);
    assert!(diff_preview
        .conflicts
        .iter()
        .any(|c| c.contains("Cannot merge assets of different kinds")));

    // 3. Different kinds apply returns conflict error
    let apply_err = merge_apply_impl(
        MergeApplyDto {
            winner_id: software_id,
            loser_id: media_id,
        },
        &state,
    )
    .expect_err("different kinds apply");
    assert_eq!(apply_err.category, "conflict");
}
