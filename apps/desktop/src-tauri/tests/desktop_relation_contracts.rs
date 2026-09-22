//! Relation Contract Tests — Phase 5 (P5-07).
//!
//! Verifies inverse semantics, symmetric canonicalization, cycle termination,
//! depth bounding/truncation, archived filtering, and removal.
//! All through desktop command adapters over real temporary SQLite database.

use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{
    media_command_impl, relation_attach_impl, relation_list_impl, relation_neighbors_impl,
    relation_remove_impl, relation_traverse_impl, software_command_impl,
};
use assetmesh_desktop_lib::dto::{
    MediaCommandDto, RelationAttachDto, RelationNeighborsQueryDto, RelationRemoveDto,
    RelationTraverseQueryDto, SoftwareCommandDto,
};
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-relation-{}-{}.db",
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

fn create_software_asset(name: &str, state: &DesktopState) -> String {
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
            tags: vec!["test".into()],
        },
        state,
    )
    .expect("create software");
    receipt.asset_ids[0].clone()
}

fn create_media_asset(title: &str, state: &DesktopState) -> String {
    let receipt = media_command_impl(
        MediaCommandDto::Create {
            title: title.into(),
            media_type: "anime".into(),
            summary: Some(format!("Summary for {title}")),
            status: Some("in_progress".into()),
            rating: None,
            year: Some(2024),
            platform: Some("Streaming".into()),
            progress_unit: None,
            progress_current: None,
            progress_total: None,
            notes: None,
            tags: vec!["anime".into()],
        },
        state,
    )
    .expect("create media");
    receipt.asset_ids[0].clone()
}

#[test]
fn relation_workflow_inverse_and_duplicate_prevention() {
    let state = setup_test_state("inverse");

    let a = create_software_asset("ripgrep", &state);
    let b = create_software_asset("rust", &state);

    // 1. Attach A depends_on B (primary type)
    let receipt = relation_attach_impl(
        RelationAttachDto {
            source_asset_id: a.clone(),
            relation_type: "depends_on".into(),
            target_asset_id: b.clone(),
            note: Some("ripgrep is written in Rust".into()),
        },
        &state,
    )
    .expect("attach relation");
    assert_eq!(receipt.operation, "relation.attach");
    assert!(receipt.changed);

    // 2. Query relation_list for A: reads "depends_on", outgoing == true
    let list_a = relation_list_impl(a.clone(), &state).expect("list a");
    assert_eq!(list_a.len(), 1);
    assert_eq!(list_a[0].relation_type, "depends_on");
    assert!(list_a[0].outgoing);
    assert_eq!(list_a[0].other_asset_id, b);
    assert_eq!(list_a[0].other_asset_name, "rust");
    assert_eq!(
        list_a[0].note.as_deref(),
        Some("ripgrep is written in Rust")
    );

    // 3. Query relation_list for B: reads "dependency_of", outgoing == false
    let list_b = relation_list_impl(b.clone(), &state).expect("list b");
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].relation_type, "dependency_of");
    assert!(!list_b[0].outgoing);
    assert_eq!(list_b[0].other_asset_id, a);
    assert_eq!(list_b[0].other_asset_name, "ripgrep");

    // 4. Directional neighbors query
    let out_a = relation_neighbors_impl(
        RelationNeighborsQueryDto {
            asset_id: a.clone(),
            direction: Some("outgoing".into()),
            relation_types: None,
            include_archived: None,
        },
        &state,
    )
    .expect("neighbors outgoing a");
    assert_eq!(out_a.len(), 1);
    assert_eq!(out_a[0].asset.id, b);

    let in_a = relation_neighbors_impl(
        RelationNeighborsQueryDto {
            asset_id: a.clone(),
            direction: Some("incoming".into()),
            relation_types: None,
            include_archived: None,
        },
        &state,
    )
    .expect("neighbors incoming a");
    assert_eq!(in_a.len(), 0);

    let in_b = relation_neighbors_impl(
        RelationNeighborsQueryDto {
            asset_id: b.clone(),
            direction: Some("incoming".into()),
            relation_types: None,
            include_archived: None,
        },
        &state,
    )
    .expect("neighbors incoming b");
    assert_eq!(in_b.len(), 1);
    assert_eq!(in_b[0].asset.id, a);

    // 5. Attempting to state the same fact via inverse type "B dependency_of A"
    // MUST reject as duplicate conflict (same canonical row)
    let dup_err = relation_attach_impl(
        RelationAttachDto {
            source_asset_id: b.clone(),
            relation_type: "dependency_of".into(),
            target_asset_id: a.clone(),
            note: None,
        },
        &state,
    )
    .expect_err("should reject duplicate");
    assert_eq!(dup_err.category, "conflict");
}

#[test]
fn relation_workflow_symmetric_canonicalization() {
    let state = setup_test_state("symmetric");

    let a = create_software_asset("neovim", &state);
    let b = create_software_asset("helix", &state);

    // Attach A related_to B
    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: a.clone(),
            relation_type: "related_to".into(),
            target_asset_id: b.clone(),
            note: Some("both modal editors".into()),
        },
        &state,
    )
    .expect("attach symmetric");

    // Both view as related_to
    let list_a = relation_list_impl(a.clone(), &state).expect("list a");
    assert_eq!(list_a.len(), 1);
    assert_eq!(list_a[0].relation_type, "related_to");

    let list_b = relation_list_impl(b.clone(), &state).expect("list b");
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].relation_type, "related_to");

    // Stating B related_to A fails with conflict
    let err = relation_attach_impl(
        RelationAttachDto {
            source_asset_id: b,
            relation_type: "related_to".into(),
            target_asset_id: a,
            note: None,
        },
        &state,
    )
    .expect_err("conflict duplicate symmetric");
    assert_eq!(err.category, "conflict");
}

#[test]
fn relation_workflow_cycle_safe_traversal() {
    let state = setup_test_state("cycle");

    let a = create_software_asset("Asset A", &state);
    let b = create_software_asset("Asset B", &state);
    let c = create_software_asset("Asset C", &state);

    // Create a cycle: A -> B -> C -> A
    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: a.clone(),
            relation_type: "depends_on".into(),
            target_asset_id: b.clone(),
            note: None,
        },
        &state,
    )
    .expect("A -> B");

    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: b.clone(),
            relation_type: "depends_on".into(),
            target_asset_id: c.clone(),
            note: None,
        },
        &state,
    )
    .expect("B -> C");

    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: c.clone(),
            relation_type: "depends_on".into(),
            target_asset_id: a.clone(),
            note: None,
        },
        &state,
    )
    .expect("C -> A");

    // Traverse dependencies of A: should safely visit B (depth 1) and C (depth 2)
    // and terminate cleanly without infinite recursion
    let trav = relation_traverse_impl(
        RelationTraverseQueryDto {
            asset_id: a.clone(),
            mode: Some("dependencies".into()),
            direction: None,
            relation_types: None,
            max_depth: Some(10),
            include_archived: None,
        },
        &state,
    )
    .expect("traverse cycle");

    assert_eq!(trav.nodes.len(), 2);
    assert_eq!(trav.nodes[0].asset.id, b);
    assert_eq!(trav.nodes[0].depth, 1);
    assert_eq!(trav.nodes[1].asset.id, c);
    assert_eq!(trav.nodes[1].depth, 2);
    assert!(!trav.truncated);
}

#[test]
fn relation_workflow_depth_cap_and_truncation() {
    let state = setup_test_state("depth");

    let n1 = create_software_asset("Node 1", &state);
    let n2 = create_software_asset("Node 2", &state);
    let n3 = create_software_asset("Node 3", &state);
    let n4 = create_software_asset("Node 4", &state);
    let n5 = create_software_asset("Node 5", &state);

    // Chain: N1 -> N2 -> N3 -> N4 -> N5
    for (src, dst) in [(&n1, &n2), (&n2, &n3), (&n3, &n4), (&n4, &n5)] {
        relation_attach_impl(
            RelationAttachDto {
                source_asset_id: src.clone(),
                relation_type: "depends_on".into(),
                target_asset_id: dst.clone(),
                note: None,
            },
            &state,
        )
        .expect("attach chain hop");
    }

    // Traverse with max_depth = 2: reaches N2 (depth 1) and N3 (depth 2)
    // Since N3 has an outgoing edge to N4, truncated MUST be true!
    let trav_bounded = relation_traverse_impl(
        RelationTraverseQueryDto {
            asset_id: n1.clone(),
            mode: Some("dependencies".into()),
            direction: None,
            relation_types: None,
            max_depth: Some(2),
            include_archived: None,
        },
        &state,
    )
    .expect("traverse bounded");

    assert_eq!(trav_bounded.nodes.len(), 2);
    assert_eq!(trav_bounded.nodes[0].depth, 1);
    assert_eq!(trav_bounded.nodes[1].depth, 2);
    assert!(
        trav_bounded.truncated,
        "truncated must be true when deeper nodes exist"
    );

    // Traverse with max_depth = 100: clamped to MAX_TRAVERSAL_DEPTH (32) and succeeds
    let trav_clamped = relation_traverse_impl(
        RelationTraverseQueryDto {
            asset_id: n1,
            mode: Some("dependencies".into()),
            direction: None,
            relation_types: None,
            max_depth: Some(100),
            include_archived: None,
        },
        &state,
    )
    .expect("traverse clamped");

    assert_eq!(trav_clamped.nodes.len(), 4);
    assert!(!trav_clamped.truncated);
}

#[test]
fn relation_workflow_archived_filtering() {
    let state = setup_test_state("archived");

    let root = create_software_asset("Root App", &state);
    let active_dep = create_software_asset("Active Library", &state);
    let to_archive = create_media_asset("Old Soundtrack", &state);

    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: root.clone(),
            relation_type: "uses".into(),
            target_asset_id: active_dep.clone(),
            note: None,
        },
        &state,
    )
    .expect("attach active");

    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: root.clone(),
            relation_type: "uses".into(),
            target_asset_id: to_archive.clone(),
            note: None,
        },
        &state,
    )
    .expect("attach to_archive");

    // Archive the soundtrack asset
    media_command_impl(
        MediaCommandDto::Archive {
            asset_id: to_archive.clone(),
        },
        &state,
    )
    .expect("archive media");

    // Query neighbors without include_archived (default): only active_dep is returned
    let neighbors_active_only = relation_neighbors_impl(
        RelationNeighborsQueryDto {
            asset_id: root.clone(),
            direction: Some("both".into()),
            relation_types: None,
            include_archived: Some(false),
        },
        &state,
    )
    .expect("neighbors active only");

    assert_eq!(neighbors_active_only.len(), 1);
    assert_eq!(neighbors_active_only[0].asset.id, active_dep);

    // Query neighbors with include_archived = true: both are returned
    let neighbors_all = relation_neighbors_impl(
        RelationNeighborsQueryDto {
            asset_id: root,
            direction: Some("both".into()),
            relation_types: None,
            include_archived: Some(true),
        },
        &state,
    )
    .expect("neighbors all");

    assert_eq!(neighbors_all.len(), 2);
}

#[test]
fn relation_workflow_remove_and_activity() {
    let state = setup_test_state("remove");

    let a = create_software_asset("App X", &state);
    let b = create_software_asset("App Y", &state);

    relation_attach_impl(
        RelationAttachDto {
            source_asset_id: a.clone(),
            relation_type: "uses".into(),
            target_asset_id: b.clone(),
            note: Some("Temporary relation".into()),
        },
        &state,
    )
    .expect("attach");

    let list_before = relation_list_impl(a.clone(), &state).expect("list before");
    assert_eq!(list_before.len(), 1);
    let rel_id = list_before[0].relation_id.clone();

    // Remove the relation
    let receipt = relation_remove_impl(
        RelationRemoveDto {
            relation_id: rel_id,
        },
        &state,
    )
    .expect("remove");
    assert_eq!(receipt.operation, "relation.remove");
    assert!(receipt.changed);

    // Verify list is now empty for both endpoints
    let list_a_after = relation_list_impl(a, &state).expect("list a after");
    assert_eq!(list_a_after.len(), 0);

    let list_b_after = relation_list_impl(b, &state).expect("list b after");
    assert_eq!(list_b_after.len(), 0);
}

#[test]
fn relation_workflow_merged_tombstone_redirect_error() {
    let state = setup_test_state("merged");

    let survivor = create_software_asset("Survivor App", &state);
    let duplicate = create_software_asset("Duplicate App", &state);

    // Merge duplicate into survivor using AssetService
    let dup_id = uuid::Uuid::parse_str(&duplicate).unwrap();
    let surv_id = uuid::Uuid::parse_str(&survivor).unwrap();
    state
        .with_factory(|factory| {
            let mut svc = assetmesh_core::application::asset_service::AssetService::new(
                factory.clone(),
                state.clock.clone(),
                state.ids.clone(),
            );
            svc.merge_assets(
                assetmesh_core::domain::ids::AssetId::from_uuid(dup_id),
                assetmesh_core::domain::ids::AssetId::from_uuid(surv_id),
            )?;
            Ok(())
        })
        .expect("merge duplicate into survivor");

    // Querying traverse of merged tombstone MUST return redirect error
    let err_traverse = relation_traverse_impl(
        RelationTraverseQueryDto {
            asset_id: duplicate.clone(),
            mode: None,
            direction: None,
            relation_types: None,
            max_depth: None,
            include_archived: None,
        },
        &state,
    )
    .expect_err("traverse should reject merged tombstone");

    assert_eq!(err_traverse.category, "conflict");
    assert!(err_traverse.message.contains("merged into"));
    assert!(err_traverse.message.contains(&survivor));
}
