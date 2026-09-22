//! Merge preview use-case tests (docs/10 merge rules, ADR 0005).
//!
//! These run against the in-memory port doubles; the adapter-facing contract
//! runs against real SQLite in
//! `apps/desktop/src-tauri/tests/desktop_activity_duplicate_contracts.rs`.
//! Splitting them this way proves the merge rules at the application layer
//! without a Tauri command in the loop.

mod support;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::media_service::{CreateMedia, ExternalRefInput};
use assetmesh_core::application::merge_preview_service::{MergePreviewService, MergePreviewView};
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::service_service::CreateService;
use assetmesh_core::application::software_service::CreateSoftware;
use assetmesh_core::domain::asset::LifecycleState;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::{AppError, AppResult};
use support::TestEnv;

// ---------------------------------------------------------------------------
// Scaffolding
// ---------------------------------------------------------------------------

struct Fixture {
    env: TestEnv,
    winner: AssetId,
    loser: AssetId,
    /// An asset outside the merge, used as the far endpoint of a relation so a
    /// transfer can be told apart from something the winner already has.
    bystander: AssetId,
}

fn fixture() -> Fixture {
    let env = support::test_env();
    let winner = env
        .media_service()
        .create_media(anime("Winner", &["shared"]))
        .unwrap();
    let loser = env
        .media_service()
        .create_media(anime("Loser", &["shared", "only-loser"]))
        .unwrap();
    let bystander = env
        .media_service()
        .create_media(anime("Bystander", &[]))
        .unwrap();
    Fixture {
        env,
        winner: winner.entry.asset.id,
        loser: loser.entry.asset.id,
        bystander: bystander.entry.asset.id,
    }
}

fn anime(title: &str, tags: &[&str]) -> CreateMedia {
    CreateMedia {
        title: title.into(),
        media_type: MediaType::Anime,
        summary: None,
        status: None,
        rating: None,
        year: None,
        platform: None,
        progress: Default::default(),
        notes: None,
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

fn software(name: &str) -> CreateSoftware {
    CreateSoftware {
        name: name.into(),
        category: SoftwareCategory::Cli,
        summary: None,
        install_source: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
        installed_at: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
    }
}

fn preview(env: &TestEnv, winner: AssetId, loser: AssetId) -> AppResult<MergePreviewView> {
    MergePreviewService::new(env.factory.clone()).preview(winner, loser)
}

fn attach(env: &TestEnv, left: AssetId, kind: RelationType, right: AssetId) {
    RelationService::new(env.factory.clone(), env.clock.clone(), env.ids.clone())
        .attach(left, kind, right, None, RelationProvenance::Manual)
        .unwrap();
}

fn conflicting(view: &MergePreviewView, needle: &str) -> Option<String> {
    view.conflicts.iter().find(|c| c.contains(needle)).cloned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn preview_refuses_self_merge_as_invalid_input() {
    // `merge_assets` reports a self-merge as validation, and the preview must
    // agree: the same request cannot be a conflict in one call and invalid
    // input in the other, or the UI would reject it before it could explain why.
    let f = fixture();
    let err = preview(&f.env, f.winner, f.winner).unwrap_err();
    assert!(
        matches!(err, AppError::Validation { .. }),
        "self merge is a malformed request: {err}"
    );
}

#[test]
fn preview_reports_unknown_assets_as_not_found() {
    let f = fixture();
    let unknown = AssetId::generate();

    assert!(matches!(
        preview(&f.env, unknown, f.loser).unwrap_err(),
        AppError::NotFound { .. }
    ));
    assert!(matches!(
        preview(&f.env, f.winner, unknown).unwrap_err(),
        AppError::NotFound { .. }
    ));
}

#[test]
fn preview_blocks_merges_across_kinds() {
    let f = fixture();
    let tool = f
        .env
        .software_service()
        .create_software(software("ripgrep"))
        .unwrap();

    let cross_kind = preview(&f.env, f.winner, tool.entry.asset.id).unwrap();
    assert!(!cross_kind.can_merge);
    assert!(
        conflicting(&cross_kind, "Cannot merge assets of different kinds").is_some(),
        "the two kinds must be named: {:?}",
        cross_kind.conflicts
    );
}

#[test]
fn preview_requires_the_winner_to_be_active() {
    // An archived winner cannot receive a merge: the survivor must stay active,
    // or the library would silently archive a live record.
    let f = fixture();
    AssetService::new(
        f.env.factory.clone(),
        f.env.clock.clone(),
        f.env.ids.clone(),
    )
    .archive_asset(f.winner)
    .unwrap();

    let view = preview(&f.env, f.winner, f.loser).unwrap();
    assert!(!view.can_merge);
    assert!(
        conflicting(&view, "Winner must be active").is_some(),
        "the inactive winner must be reported: {:?}",
        view.conflicts
    );
}

#[test]
fn preview_transfers_only_tags_the_winner_lacks() {
    let f = fixture();
    let view = preview(&f.env, f.winner, f.loser).unwrap();

    assert!(view.can_merge, "no conflict expected: {:?}", view.conflicts);
    assert_eq!(view.transferred_tags, vec!["only-loser"]);
    // The two summaries are deliberately asymmetric: the winner column lists
    // what it has, the loser column what it contributes.
    assert_eq!(view.winner.tags, vec!["shared"]);
    assert_eq!(view.loser.tags, vec!["only-loser"]);
}

#[test]
fn preview_carries_external_references_over_to_the_winner() {
    let f = fixture();
    let mut cmd = anime("Loser", &[]);
    cmd.external_refs = vec![
        ExternalRefInput {
            namespace: "anilist".into(),
            external_id: "101".into(),
            source_url: None,
        },
        ExternalRefInput {
            namespace: "myanimelist".into(),
            external_id: "202".into(),
            source_url: None,
        },
    ];
    let loser = f.env.media_service().create_media(cmd).unwrap();

    let view = preview(&f.env, f.winner, loser.entry.asset.id).unwrap();

    let moved: Vec<(&str, &str)> = view
        .transferred_external_refs
        .iter()
        .map(|r| (r.namespace.as_str(), r.external_id.as_str()))
        .collect();
    assert_eq!(moved, vec![("anilist", "101"), ("myanimelist", "202")]);
    // Nothing is redundant: the winner holds no aliases, and a ref owned by the
    // loser resolves to the loser, never to the winner. The bucket exists to
    // match `merge_assets`, which deletes a loser's copy only in that case.
    assert!(view.redundant_external_refs.is_empty());
}

#[test]
fn preview_separates_relations_that_transfer_from_those_the_winner_has() {
    let f = fixture();

    // The winner already relates to the bystander with this relation type.
    attach(&f.env, f.winner, RelationType::Uses, f.bystander);
    // The loser relates to the bystander with the same type the winner has, a
    // different type the winner lacks, and an edge straight to the winner.
    attach(&f.env, f.loser, RelationType::Uses, f.bystander);
    attach(&f.env, f.loser, RelationType::DependsOn, f.bystander);
    attach(&f.env, f.loser, RelationType::Uses, f.winner);

    let view = preview(&f.env, f.winner, f.loser).unwrap();

    assert_eq!(
        view.transferred_relations_count, 1,
        "only the Requires edge is new"
    );
    assert_eq!(
        view.redundant_relations_count, 2,
        "the duplicate Uses edge collapses, and so does the edge to the winner"
    );
    assert!(view.can_merge, "duplicates are not conflicts");
}

#[test]
fn preview_names_disagreeing_service_fields_and_ignores_unset_ones() {
    let env = support::test_env();
    let mut winner_cmd = service("Copilot");
    winner_cmd.provider = Some("GitHub".into());
    winner_cmd.plan = Some("Free".into());
    let winner = env.service_service().create_service(winner_cmd).unwrap();

    let mut loser_cmd = service("Copilot Pro");
    loser_cmd.provider = Some("GitHub".into());
    loser_cmd.plan = Some("Pro".into());
    loser_cmd.notes = Some("billed via work card".into());
    let loser = env.service_service().create_service(loser_cmd).unwrap();

    let view = preview(&env, winner.entry.asset.id, loser.entry.asset.id).unwrap();

    assert!(!view.can_merge);
    let service_conflict = conflicting(&view, "Service details conflict")
        .unwrap_or_else(|| panic!("a service conflict must be reported: {:?}", view.conflicts));
    assert!(
        service_conflict.contains("plan: 'Free' vs 'Pro'"),
        "{service_conflict}"
    );
    assert!(
        !service_conflict.contains("provider"),
        "an equal field is not a conflict"
    );
    assert!(
        !service_conflict.contains("notes"),
        "an unset winner field is filled from the loser, not a conflict (docs/10 rule 5)"
    );
}

fn service(name: &str) -> CreateService {
    CreateService {
        name: name.into(),
        service_type: ServiceType::Saas,
        summary: None,
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost_minor: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
    }
}

#[test]
fn preview_of_an_already_merged_loser_flags_it_as_a_conflict() {
    // The write path refuses to merge an already-merged loser, so the preview
    // must say so instead of offering a merge that would fail on apply.
    let f = fixture();
    AssetService::new(
        f.env.factory.clone(),
        f.env.clock.clone(),
        f.env.ids.clone(),
    )
    .merge_assets(f.loser, f.winner)
    .unwrap();
    let mut lifecycle = None;
    f.env.factory.store_borrow_mut(|store| {
        lifecycle = store
            .assets
            .get(&f.loser.to_string())
            .map(|a| a.lifecycle_state)
    });
    assert_eq!(lifecycle, Some(LifecycleState::Merged));

    let stale = preview(&f.env, f.bystander, f.loser).unwrap();
    assert!(!stale.can_merge);
    assert!(
        conflicting(&stale, "already merged").is_some(),
        "an already-merged loser must be a conflict: {:?}",
        stale.conflicts
    );
}

#[test]
fn preview_reports_that_both_media_records_cannot_survive() {
    // Both sides hold media details: the winner keeps its record and the
    // loser's is preserved verbatim in the activity payload (docs/10 rules
    // 6-7). That is a fact the user must see, so it is a note, not a conflict.
    let f = fixture();
    let view = preview(&f.env, f.winner, f.loser).unwrap();

    assert!(view.can_merge);
    assert!(
        view.notes
            .iter()
            .any(|n| n.contains("Winner's media metadata is kept")),
        "the survivor-wins rule must be surfaced: {:?}",
        view.notes
    );
}
