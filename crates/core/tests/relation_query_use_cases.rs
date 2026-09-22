//! Relation graph query use-case tests — Phase 4B (docs/11).
//!
//! These run against the in-memory port doubles; the same scenarios run
//! against real SQLite in `storage-sqlite/tests/relation_query_contracts.rs`.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::media_service::CreateMedia;
use assetmesh_core::application::relation_query_service::{
    RelationQueryService, TraversalDirection, TraversalOptions, DEFAULT_MAX_DEPTH,
    DEPENDENCY_RELATION_TYPES, MAX_TRAVERSAL_DEPTH,
};
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::service_service::CreateService;
use assetmesh_core::application::software_service::CreateSoftware;
use assetmesh_core::domain::asset::{Asset, AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, AppResult};
use support::test_env;

// ---------------------------------------------------------------------------
// Scaffolding
// ---------------------------------------------------------------------------

struct Fixture {
    env: support::TestEnv,
    media: AssetId,
    software: AssetId,
    cli: AssetId,
    service: AssetId,
    vps: AssetId,
}

/// Builds one asset per module, mirroring the docs/11 Phase 4D fixture:
/// a media asset, two software assets, a service, and a VPS.
fn fixture() -> Fixture {
    let env = test_env();
    let media = env
        .media_service()
        .create_media(media_cmd("Sousou no Frieren", MediaType::Anime))
        .unwrap();
    let software = env
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let cli = env
        .software_service()
        .create_software(software_cmd("fzf", SoftwareCategory::Cli))
        .unwrap();
    let service = env
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas))
        .unwrap();
    let vps = env
        .service_service()
        .create_service(service_cmd("Hetzner VPS", ServiceType::Vps))
        .unwrap();
    Fixture {
        env,
        media: media.entry.asset.id,
        software: software.entry.asset.id,
        cli: cli.entry.asset.id,
        service: service.entry.asset.id,
        vps: vps.entry.asset.id,
    }
}

fn media_cmd(title: &str, media_type: MediaType) -> CreateMedia {
    CreateMedia {
        title: title.into(),
        media_type,
        summary: None,
        status: None,
        rating: None,
        year: None,
        platform: None,
        progress: Default::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

fn software_cmd(name: &str, category: SoftwareCategory) -> CreateSoftware {
    CreateSoftware {
        name: name.into(),
        category,
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

fn service_cmd(name: &str, service_type: ServiceType) -> CreateService {
    CreateService {
        name: name.into(),
        service_type,
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

fn relations(fixture: &Fixture) -> RelationService<support::MemFactory> {
    RelationService::new(
        fixture.env.factory.clone(),
        fixture.env.clock.clone(),
        fixture.env.ids.clone(),
    )
}

fn graph(fixture: &Fixture) -> RelationQueryService<support::MemFactory> {
    RelationQueryService::new(fixture.env.factory.clone())
}

fn assets(fixture: &Fixture) -> AssetService<support::MemFactory> {
    AssetService::new(
        fixture.env.factory.clone(),
        fixture.env.clock.clone(),
        fixture.env.ids.clone(),
    )
}

fn relate(fixture: &Fixture, source: AssetId, relation_type: RelationType, target: AssetId) {
    relations(fixture)
        .attach(
            source,
            relation_type,
            target,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
}

/// Attaches a relation using the *inverse* type, the way a user states a fact
/// from the other end. Storage must still collapse it onto the primary row.
fn relate_inverse(
    fixture: &Fixture,
    source: AssetId,
    relation_type: RelationType,
    target: AssetId,
) {
    relations(fixture)
        .attach(
            source,
            relation_type,
            target,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
}

fn neighbor_names(
    views: &[assetmesh_core::application::relation_query_service::NeighborView],
) -> Vec<String> {
    views.iter().map(|view| view.asset.name.clone()).collect()
}

fn node_names(
    view: &assetmesh_core::application::relation_query_service::TraversalView,
) -> Vec<String> {
    view.nodes
        .iter()
        .map(|node| node.asset.name.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Neighbors
// ---------------------------------------------------------------------------

#[test]
fn neighbors_report_one_row_per_edge_with_effective_types() {
    let f = fixture();
    // fzf installed_via ripgrep; ripgrep uses fzf; media related_to service.
    relate(&f, f.cli, RelationType::InstalledVia, f.software);
    relate(&f, f.software, RelationType::Uses, f.cli);
    relate(&f, f.media, RelationType::RelatedTo, f.service);

    let mut g = graph(&f);
    let outgoing = g
        .outgoing(f.software, &TraversalOptions::default())
        .unwrap();
    assert_eq!(neighbor_names(&outgoing), vec!["fzf".to_string()]);
    assert_eq!(
        outgoing[0].edge.relation_type,
        RelationType::Uses,
        "the caller sees the effective type, never the stored inverse"
    );

    let incoming = g
        .incoming(f.software, &TraversalOptions::default())
        .unwrap();
    assert_eq!(neighbor_names(&incoming), vec!["fzf".to_string()]);
    assert_eq!(
        incoming[0].edge.relation_type,
        RelationType::Installs,
        "an incoming installed_via reads as `installs` from the target"
    );

    let both = g
        .neighbors(f.software, &TraversalOptions::default())
        .unwrap();
    assert_eq!(both.len(), 2, "one row per edge, not per neighbor");
}

#[test]
fn a_symmetric_relation_reads_the_same_from_both_endpoints() {
    let f = fixture();
    relate(&f, f.media, RelationType::RelatedTo, f.service);

    let mut g = graph(&f);
    let from_media = g.neighbors(f.media, &TraversalOptions::default()).unwrap();
    let from_service = g
        .neighbors(f.service, &TraversalOptions::default())
        .unwrap();
    assert_eq!(from_media.len(), 1);
    assert_eq!(from_service.len(), 1);
    // A symmetric type reads identically from either end, whichever endpoint
    // canonical storage happened to pick as the source.
    assert_eq!(
        from_media[0].edge.relation_type,
        from_service[0].edge.relation_type
    );
    assert_eq!(from_media[0].edge.relation_type, RelationType::RelatedTo);
    assert_eq!(from_media[0].asset.id, f.service);
    assert_eq!(from_service[0].asset.id, f.media);
    // Exactly one row exists, so `outgoing` is true from exactly one side.
    assert_ne!(from_media[0].edge.outgoing, from_service[0].edge.outgoing);
}

#[test]
fn a_fact_stated_via_an_inverse_type_is_queried_from_either_end() {
    let f = fixture();
    // Stated from the target's side: "fzf is installed_via ripgrep" is the
    // same canonical fact as "ripgrep installs fzf".
    relate_inverse(&f, f.cli, RelationType::InstalledVia, f.software);

    let mut g = graph(&f);
    let from_cli = g.neighbors(f.cli, &TraversalOptions::default()).unwrap();
    assert_eq!(neighbor_names(&from_cli), vec!["ripgrep".to_string()]);
    assert_eq!(from_cli[0].edge.relation_type, RelationType::InstalledVia);
    assert!(from_cli[0].edge.outgoing);

    let from_software = g
        .neighbors(f.software, &TraversalOptions::default())
        .unwrap();
    assert_eq!(neighbor_names(&from_software), vec!["fzf".to_string()]);
    assert_eq!(from_software[0].edge.relation_type, RelationType::Installs);
    assert!(!from_software[0].edge.outgoing);
}

#[test]
fn neighbor_relation_type_filter_matches_the_callers_perspective() {
    let f = fixture();
    relate(&f, f.cli, RelationType::InstalledVia, f.software);
    relate(&f, f.software, RelationType::Uses, f.cli);

    let mut g = graph(&f);
    let options = TraversalOptions {
        relation_types: vec![RelationType::Uses],
        ..TraversalOptions::default()
    };
    let filtered = g.neighbors(f.software, &options).unwrap();
    assert_eq!(neighbor_names(&filtered), vec!["fzf".to_string()]);
    assert_eq!(filtered[0].edge.relation_type, RelationType::Uses);

    let none = g
        .neighbors(
            f.software,
            &TraversalOptions {
                relation_types: vec![RelationType::DependsOn],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn neighbor_queries_reject_unknown_assets() {
    let f = fixture();
    let mut g = graph(&f);
    let error = g
        .neighbors(AssetId::generate(), &TraversalOptions::default())
        .unwrap_err();
    assert!(matches!(error, AppError::NotFound { .. }), "{error}");
}

// ---------------------------------------------------------------------------
// Traversal
// ---------------------------------------------------------------------------

#[test]
fn traversal_walks_multiple_hops_in_breadth_first_order() {
    let f = fixture();
    // media uses service; service hosted_on vps; vps points_to software.
    // `points_to` is a directed type, so the chain is unambiguous from either
    // end (a symmetric `related_to` would store in canonical endpoint order).
    relate(&f, f.media, RelationType::Uses, f.service);
    relate(&f, f.service, RelationType::HostedOn, f.vps);
    relate(&f, f.vps, RelationType::PointsTo, f.software);

    let mut g = graph(&f);
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(view.root.name, "Sousou no Frieren");
    let names = node_names(&view);
    assert_eq!(names, vec!["OpenAI", "Hetzner VPS", "ripgrep"]);
    assert_eq!(view.nodes[0].depth, 1);
    assert_eq!(view.nodes[1].depth, 2);
    assert_eq!(view.nodes[2].depth, 3);
    assert!(!view.truncated);

    // Every path starts at the root and grows by one hop per depth level.
    for node in &view.nodes {
        assert_eq!(node.path.len(), node.depth);
        assert_eq!(node.path.first().unwrap().from_asset_id, f.media);
        assert_eq!(node.path.last().unwrap().to_asset_id, node.asset.id);
    }
    assert_eq!(view.nodes[2].path.len(), 3);
    assert_eq!(view.nodes[2].path[1].relation_type, RelationType::HostedOn);
}

#[test]
fn a_diamond_reports_the_shared_child_once() {
    let f = fixture();
    // A → B, A → C, B → D, C → D
    relate(&f, f.media, RelationType::DependsOn, f.software);
    relate(&f, f.media, RelationType::DependsOn, f.cli);
    relate(&f, f.software, RelationType::DependsOn, f.service);
    relate(&f, f.cli, RelationType::DependsOn, f.service);

    let mut g = graph(&f);
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();

    let service_nodes: Vec<_> = view
        .nodes
        .iter()
        .filter(|node| node.asset.id == f.service)
        .collect();
    assert_eq!(
        service_nodes.len(),
        1,
        "the shared child must appear exactly once"
    );
    assert_eq!(service_nodes[0].depth, 2);
    // Deterministic path selection: the smaller parent wins.
    assert_eq!(service_nodes[0].path[1].from_asset_id, f.software);
    assert_eq!(service_nodes[0].path.len(), 2);

    // Repeated queries agree.
    let again = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&again), node_names(&view));
    assert_eq!(again.nodes[2].path, view.nodes[2].path);
}

#[test]
fn a_cycle_terminates_and_reports_each_node_once() {
    let f = fixture();
    // A depends_on B depends_on C depends_on A
    relate(&f, f.media, RelationType::DependsOn, f.software);
    relate(&f, f.software, RelationType::DependsOn, f.cli);
    relate(&f, f.cli, RelationType::DependsOn, f.media);

    let mut g = graph(&f);
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();

    let names = node_names(&view);
    assert_eq!(names.len(), 2, "only B and C are reachable; A is the root");
    assert!(!names.contains(&"Sousou no Frieren".to_string()));
    assert_eq!(names, vec!["ripgrep".to_string(), "fzf".to_string()]);
    assert!(!view.truncated, "a cycle must not look like a depth cutoff");

    // The same graph in the other direction terminates too.
    let reverse = g.traverse(
        f.cli,
        &TraversalOptions {
            direction: TraversalDirection::Incoming,
            ..TraversalOptions::default()
        },
    );
    assert!(reverse.is_ok());
    assert_eq!(node_names(&reverse.unwrap()).len(), 2);
}

#[test]
fn max_depth_bounds_the_walk_and_reports_truncation() {
    let f = fixture();
    relate(&f, f.media, RelationType::Uses, f.software);
    relate(&f, f.software, RelationType::Uses, f.cli);
    relate(&f, f.cli, RelationType::Uses, f.service);
    relate(&f, f.service, RelationType::Uses, f.vps);

    let mut g = graph(&f);
    let bounded = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 2,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&bounded), vec!["ripgrep", "fzf"]);
    assert!(bounded.truncated, "more edges existed past the bound");

    let exact = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 4,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&exact).len(), 4);
    assert!(!exact.truncated);

    // Depth 0 reaches nothing beyond the root.
    let none = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 0,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(none.nodes.is_empty());
    assert!(!none.truncated);

    // An over-eager depth is clamped, not rejected.
    let clamped = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: MAX_TRAVERSAL_DEPTH + 50,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&clamped).len(), 4);
}

#[test]
fn direction_and_type_filters_are_honored_during_traversal() {
    let f = fixture();
    relate(&f, f.media, RelationType::DependsOn, f.software);
    relate(&f, f.cli, RelationType::DependsOn, f.software);
    relate(&f, f.software, RelationType::RelatedTo, f.service);

    let mut g = graph(&f);

    let outgoing = g
        .traverse(
            f.software,
            &TraversalOptions {
                relation_types: vec![RelationType::RelatedTo],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&outgoing), vec!["OpenAI"]);

    let incoming = g
        .traverse(
            f.software,
            &TraversalOptions {
                direction: TraversalDirection::Incoming,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&incoming), vec!["Sousou no Frieren", "fzf"]);

    // The type filter matches the stored fact, so "incoming depends_on" finds
    // the rows whose stored type is `depends_on` — it must not return nothing
    // just because the same row reads as `dependency_of` from the target.
    let typed = g
        .traverse(
            f.software,
            &TraversalOptions {
                direction: TraversalDirection::Incoming,
                relation_types: vec![RelationType::DependsOn],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&typed), vec!["Sousou no Frieren", "fzf"]);
    // ...and they are still reported with the inverse resolved for the caller.
    assert_eq!(
        typed.nodes[0].path[0].relation_type,
        RelationType::DependencyOf
    );

    // An inverse type in the filter maps onto the same stored rows.
    let inverse_filter = g
        .traverse(
            f.software,
            &TraversalOptions {
                direction: TraversalDirection::Incoming,
                relation_types: vec![RelationType::DependencyOf],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        node_names(&inverse_filter),
        vec!["Sousou no Frieren", "fzf"]
    );

    // A type that does not appear matches nothing.
    let unrelated = g
        .traverse(
            f.software,
            &TraversalOptions {
                direction: TraversalDirection::Incoming,
                relation_types: vec![RelationType::PointsTo],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(unrelated.nodes.is_empty());

    let both = g
        .traverse(
            f.software,
            &TraversalOptions {
                direction: TraversalDirection::Both,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&both).len(), 3);
}

// ---------------------------------------------------------------------------
// Dependencies / dependents / impact
// ---------------------------------------------------------------------------

#[test]
fn dependencies_answer_what_this_asset_needs() {
    let f = fixture();
    // OpenAI is hosted on the VPS; the CLI is installed via ripgrep.
    relate(&f, f.service, RelationType::HostedOn, f.vps);
    relate(&f, f.cli, RelationType::InstalledVia, f.software);

    let mut g = graph(&f);
    let deps = g
        .dependencies(f.service, &TraversalOptions::default())
        .unwrap();
    assert_eq!(node_names(&deps), vec!["Hetzner VPS"]);
    assert_eq!(deps.nodes[0].depth, 1);
    assert_eq!(
        deps.nodes[0].path[0].relation_type,
        RelationType::HostedOn,
        "the dependency is stated from the dependent's perspective"
    );

    let cli_deps = g.dependencies(f.cli, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&cli_deps), vec!["ripgrep"]);
    assert_eq!(
        cli_deps.nodes[0].path[0].relation_type,
        RelationType::InstalledVia
    );
}

#[test]
fn dependents_answer_what_needs_this_asset() {
    let f = fixture();
    relate(&f, f.service, RelationType::HostedOn, f.vps);
    relate(&f, f.cli, RelationType::InstalledVia, f.software);
    // A second dependent of the VPS, two hops away.
    relate(&f, f.media, RelationType::Uses, f.service);

    let mut g = graph(&f);
    let dependents = g.dependents(f.vps, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&dependents), vec!["OpenAI"]);
    assert_eq!(
        dependents.nodes[0].path[0].relation_type,
        RelationType::Hosts,
        "the inverse is resolved for the caller"
    );

    // `uses` is deliberately not a dependency edge.
    let media_dependents = g
        .dependents(f.service, &TraversalOptions::default())
        .unwrap();
    assert!(media_dependents.nodes.is_empty());
}

#[test]
fn impact_reports_transitive_dependents_with_explainable_paths() {
    let f = fixture();
    // ripgrep ← fzf (installed_via) ← ... and media → service → vps
    relate(&f, f.cli, RelationType::InstalledVia, f.software);
    relate(&f, f.service, RelationType::HostedOn, f.vps);
    relate(&f, f.media, RelationType::Uses, f.service);
    // A chain that makes the VPS transitively depended upon by media:
    // media depends_on service, service hosted_on vps.
    relate(&f, f.media, RelationType::DependsOn, f.service);

    let mut g = graph(&f);
    let impact = g.impact(f.vps, &TraversalOptions::default()).unwrap();

    let names = node_names(&impact);
    assert!(names.contains(&"OpenAI".to_string()));
    assert!(names.contains(&"Sousou no Frieren".to_string()));

    let service = impact
        .nodes
        .iter()
        .find(|node| node.asset.id == f.service)
        .expect("the VPS hosts the service");
    assert_eq!(service.depth, 1);
    assert_eq!(service.path.len(), 1);
    assert_eq!(service.path[0].relation_type, RelationType::Hosts);

    let media = impact
        .nodes
        .iter()
        .find(|node| node.asset.id == f.media)
        .expect("media depends on the service that is hosted on the VPS");
    assert_eq!(media.depth, 2);
    // The path is ordered root → leaf, and every hop reads from its own
    // `from_asset_id`, so no consumer derives an inverse.
    assert_eq!(media.path.len(), 2);
    assert_eq!(media.path[0].from_asset_id, f.vps);
    assert_eq!(media.path[0].to_asset_id, f.service);
    assert_eq!(media.path[0].relation_type, RelationType::Hosts);
    assert_eq!(media.path[1].from_asset_id, f.service);
    assert_eq!(media.path[1].to_asset_id, f.media);
    assert_eq!(media.path[1].relation_type, RelationType::DependencyOf);
}

#[test]
fn impact_and_dependents_are_the_same_structural_query() {
    let f = fixture();
    relate(&f, f.cli, RelationType::DependsOn, f.software);

    let mut g = graph(&f);
    let impact = g.impact(f.software, &TraversalOptions::default()).unwrap();
    let dependents = g
        .dependents(f.software, &TraversalOptions::default())
        .unwrap();
    assert_eq!(impact, dependents);
    assert_eq!(node_names(&impact), vec!["fzf"]);
}

#[test]
fn dependency_relation_types_are_declared_in_one_place() {
    assert_eq!(
        DEPENDENCY_RELATION_TYPES,
        &[
            RelationType::DependsOn,
            RelationType::InstalledVia,
            RelationType::HostedOn,
        ]
    );
    // `uses`, `points_to`, and `related_to` are not dependency semantics.
    for relation_type in DEPENDENCY_RELATION_TYPES {
        assert!(!matches!(
            relation_type,
            RelationType::Uses | RelationType::PointsTo | RelationType::RelatedTo
        ));
    }
    // One list serves both directions: the filter matches the stored type, so
    // `dependencies` and `dependents` cannot drift apart.
    for relation_type in DEPENDENCY_RELATION_TYPES {
        assert!(
            relation_type.is_storable(),
            "{relation_type} is a stored type"
        );
        assert!(
            !DEPENDENCY_RELATION_TYPES.contains(&relation_type.inverse()),
            "the list holds primaries only, never an inverse alongside its pair"
        );
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn archived_nodes_are_opt_in_and_never_traversed_through() {
    let f = fixture();
    relate(&f, f.media, RelationType::Uses, f.software);
    relate(&f, f.software, RelationType::Uses, f.cli);

    let mut a = assets(&f);
    a.archive_asset(f.software).unwrap();

    let mut g = graph(&f);
    let hidden = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert!(
        node_names(&hidden).is_empty(),
        "an archived node is neither reported nor traversed through"
    );

    let shown = g
        .traverse(
            f.media,
            &TraversalOptions {
                include_archived: true,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&shown), vec!["ripgrep", "fzf"]);
    assert_eq!(shown.nodes[0].asset.lifecycle, LifecycleState::Archived);
}

#[test]
fn the_root_is_always_allowed_even_when_archived() {
    let f = fixture();
    relate(&f, f.media, RelationType::Uses, f.software);
    assets(&f).archive_asset(f.media).unwrap();

    let mut g = graph(&f);
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(view.root.lifecycle, LifecycleState::Archived);
    assert_eq!(node_names(&view), vec!["ripgrep"]);
}

#[test]
fn merged_tombstones_are_never_graph_nodes() {
    let f = fixture();
    let duplicate = f
        .env
        .software_service()
        .create_software(software_cmd("ripgrep copy", SoftwareCategory::Cli))
        .unwrap();
    relate(&f, f.media, RelationType::Uses, duplicate.entry.asset.id);
    relate(
        &f,
        duplicate.entry.asset.id,
        RelationType::DependsOn,
        f.service,
    );

    assets(&f)
        .merge_assets(duplicate.entry.asset.id, f.software)
        .unwrap();

    let mut g = graph(&f);
    for options in [
        TraversalOptions::default(),
        TraversalOptions {
            include_archived: true,
            ..TraversalOptions::default()
        },
        TraversalOptions {
            direction: TraversalDirection::Both,
            ..TraversalOptions::default()
        },
    ] {
        let view = g.traverse(f.media, &options).unwrap();
        assert!(
            !node_names(&view).contains(&"ripgrep copy".to_string()),
            "a tombstone is a redirect, not a graph node ({:?})",
            options.direction
        );
    }

    // The merge re-pointed both relations at the survivor, so the graph still
    // shows the real chain: media uses ripgrep, ripgrep depends_on OpenAI.
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&view), vec!["ripgrep", "OpenAI"]);
    // `uses` is not a dependency edge, so `dependencies` of the media asset is
    // empty even though the graph shows the relation.
    let deps = g
        .dependencies(f.media, &TraversalOptions::default())
        .unwrap();
    assert!(deps.nodes.is_empty());
    assert_eq!(
        node_names(
            &g.dependencies(f.software, &TraversalOptions::default())
                .unwrap()
        ),
        vec!["OpenAI"]
    );
}

// ---------------------------------------------------------------------------
// Consistency
// ---------------------------------------------------------------------------

/// Counts read scopes and repository-capability uses, exactly as the Phase 4A
/// library probe does.
#[derive(Clone, Default)]
struct ReadProbe {
    scopes: Rc<RefCell<usize>>,
    accesses: Rc<RefCell<Vec<&'static str>>>,
}

struct ProbeFactory {
    inner: support::MemFactory,
    probe: ReadProbe,
}

impl UnitOfWorkFactory for ProbeFactory {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn assetmesh_core::ports::uow::UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        self.inner.transact(work)
    }

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn assetmesh_core::ports::uow::QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        *self.probe.scopes.borrow_mut() += 1;
        let accesses = self.probe.accesses.clone();
        self.inner.read(&mut |inner| {
            let mut probe = ProbeQuery {
                inner,
                accesses: accesses.clone(),
            };
            work(&mut probe)
        })
    }
}

struct ProbeQuery<'a> {
    inner: &'a mut dyn assetmesh_core::ports::uow::QueryUnitOfWork,
    accesses: Rc<RefCell<Vec<&'static str>>>,
}

macro_rules! probe_capability {
    ($method:ident, $reader:path, $capability:literal) => {
        fn $method(&mut self) -> &mut dyn $reader {
            self.accesses.borrow_mut().push($capability);
            self.inner.$method()
        }
    };
}

impl assetmesh_core::ports::uow::QueryUnitOfWork for ProbeQuery<'_> {
    probe_capability!(assets, assetmesh_core::ports::repos::AssetReader, "assets");
    probe_capability!(media, assetmesh_core::ports::repos::MediaReader, "media");
    probe_capability!(
        software,
        assetmesh_core::ports::repos::SoftwareReader,
        "software"
    );
    probe_capability!(
        services,
        assetmesh_core::ports::repos::ServiceReader,
        "services"
    );
    probe_capability!(
        external_refs,
        assetmesh_core::ports::repos::ExternalRefReader,
        "external_refs"
    );
    probe_capability!(
        activity,
        assetmesh_core::ports::repos::ActivityReader,
        "activity"
    );
    probe_capability!(tags, assetmesh_core::ports::repos::TagReader, "tags");
    probe_capability!(
        relations,
        assetmesh_core::ports::repos::RelationReader,
        "relations"
    );
    probe_capability!(
        library,
        assetmesh_core::ports::repos::LibraryReadPort,
        "library"
    );
    probe_capability!(
        search_index,
        assetmesh_core::ports::search::SearchReader,
        "search_index"
    );
}

#[test]
fn a_traversal_runs_in_one_snapshot_with_one_query_per_frontier() {
    let f = fixture();
    // A fan-out: the root depends on every other asset. One batch query must
    // serve the whole frontier, where a per-node implementation would issue
    // one query per discovered node.
    for target in [f.software, f.cli, f.service, f.vps] {
        relate(&f, f.media, RelationType::DependsOn, target);
    }

    let probe = ReadProbe::default();
    let mut g = RelationQueryService::new(ProbeFactory {
        inner: f.env.factory.clone(),
        probe: probe.clone(),
    });

    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(view.nodes.len(), 4);

    assert_eq!(
        *probe.scopes.borrow(),
        1,
        "a whole traversal must see one consistent snapshot"
    );
    let accesses = probe.accesses.borrow();
    let relation_queries = accesses.iter().filter(|a| **a == "relations").count();
    // Level 1 (the fan-out) plus level 2 (which proves the leaves are dead
    // ends). A per-node implementation would need at least four.
    assert!(
        relation_queries <= 3,
        "expected one batch query per frontier, got {relation_queries}"
    );
    assert!(
        accesses.iter().filter(|a| **a == "assets").count() <= 1,
        "the asset index is loaded once, not per node"
    );
}

#[test]
fn traversal_never_mutates_canonical_state() {
    let f = fixture();
    relate(&f, f.media, RelationType::DependsOn, f.software);
    let before = f.env.factory.store().relations.len();
    let before_activity = f.env.factory.store().activity.len();

    let mut g = graph(&f);
    g.traverse(f.media, &TraversalOptions::default()).unwrap();
    g.dependencies(f.media, &TraversalOptions::default())
        .unwrap();
    g.impact(f.media, &TraversalOptions::default()).unwrap();
    g.neighbors(f.media, &TraversalOptions::default()).unwrap();

    // Compare against the snapshots taken BEFORE the queries, not against the
    // store itself — otherwise the second assertion is vacuous.
    let after = f.env.factory.store();
    assert_eq!(before, after.relations.len(), "queries must not write");
    assert_eq!(
        before_activity,
        after.activity.len(),
        "graph queries must not append activity"
    );
}

#[test]
fn truncated_reports_only_what_the_filters_would_have_hidden() {
    let f = fixture();
    relate(&f, f.media, RelationType::DependsOn, f.software);
    // Two edges the dependency traversal must NOT follow.
    relate(&f, f.software, RelationType::RelatedTo, f.cli);
    relate(&f, f.software, RelationType::Uses, f.service);

    let mut g = graph(&f);

    // At the bound with a dependency-type filter, nothing further is reachable,
    // so the depth bound hid nothing.
    let filtered = g
        .dependencies(
            f.media,
            &TraversalOptions {
                max_depth: 1,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&filtered), vec!["ripgrep"]);
    assert!(
        !filtered.truncated,
        "a filtered-out edge must not be reported as a hidden deeper node"
    );

    // The unfiltered walk from the same root does have more to reach.
    let unfiltered = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 1,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(
        unfiltered.truncated,
        "the unfiltered walk is genuinely cut short"
    );

    // An archived node the walk skipped is not a hidden node either.
    let chain = fixture();
    relate(&chain, chain.media, RelationType::DependsOn, chain.software);
    relate(&chain, chain.software, RelationType::DependsOn, chain.cli);
    assets(&chain).archive_asset(chain.cli).unwrap();
    let archived = graph(&chain)
        .dependencies(
            chain.media,
            &TraversalOptions {
                max_depth: 1,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&archived), vec!["ripgrep"]);
    assert!(
        !archived.truncated,
        "an archived node the walk skipped is not hidden"
    );
}

#[test]
fn dependencies_and_dependents_fix_their_own_relation_types() {
    let f = fixture();
    relate(&f, f.media, RelationType::DependsOn, f.software);
    relate(&f, f.software, RelationType::RelatedTo, f.cli);

    let mut g = graph(&f);
    // A caller-supplied type set and direction are deliberately overridden:
    // `dependencies` means "the dependency types, outwards". Pinned here so the
    // override cannot drift into a silent surprise.
    let view = g
        .dependencies(
            f.media,
            &TraversalOptions {
                direction: TraversalDirection::Incoming,
                relation_types: vec![RelationType::RelatedTo],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&view), vec!["ripgrep"]);
    assert_eq!(view.nodes[0].path[0].relation_type, RelationType::DependsOn);
}

#[test]
fn neighbors_hydrate_only_the_neighbouring_assets() {
    let f = fixture();
    relate(&f, f.media, RelationType::DependsOn, f.software);

    let probe = ReadProbe::default();
    let mut g = RelationQueryService::new(ProbeFactory {
        inner: f.env.factory.clone(),
        probe: probe.clone(),
    });
    g.neighbors(f.media, &TraversalOptions::default()).unwrap();

    assert_eq!(*probe.scopes.borrow(), 1);
    let accesses = probe.accesses.borrow();
    // Exactly the two hydrated assets' modules are read, once each, by
    // identity. Loading the library index instead would read all three module
    // readers regardless of which modules the two assets belong to — which is
    // exactly what this guards against.
    assert_eq!(
        accesses.iter().filter(|a| **a == "media").count(),
        1,
        "the queried media asset is one point lookup"
    );
    assert_eq!(
        accesses.iter().filter(|a| **a == "software").count(),
        1,
        "the software neighbour is one point lookup"
    );
    assert_eq!(
        accesses.iter().filter(|a| **a == "services").count(),
        0,
        "no service row is read for a media/software neighbourhood"
    );
    assert_eq!(
        accesses.iter().filter(|a| **a == "assets").count(),
        2,
        "the queried asset and its neighbour are fetched by identity"
    );
    assert_eq!(
        accesses.iter().filter(|a| **a == "tags").count(),
        2,
        "each hydrated asset's tags come from one lookup"
    );
}

#[test]
fn default_options_are_documented_and_bounded() {
    let options = TraversalOptions::default();
    assert_eq!(options.direction, TraversalDirection::Outgoing);
    assert!(options.relation_types.is_empty());
    assert_eq!(options.max_depth, DEFAULT_MAX_DEPTH);
    assert!(!options.include_archived);
    assert_eq!(options.effective_max_depth(), DEFAULT_MAX_DEPTH);

    assert_eq!(
        TraversalDirection::parse("IN").unwrap(),
        TraversalDirection::Incoming
    );
    assert_eq!(
        TraversalDirection::parse("both").unwrap(),
        TraversalDirection::Both
    );
    assert!(TraversalDirection::parse("sideways").is_none());
}

#[test]
fn a_detail_less_asset_still_appears_as_a_graph_node() {
    let f = fixture();
    // Relations are shared infrastructure: an asset with no module details
    // can still be a real graph node, and dropping it would lose a dependency.
    let bare = AssetId::generate();
    f.env.factory.store_borrow_mut(|store| {
        store.assets.insert(
            bare.to_string(),
            Asset::new(bare, AssetKind::SoftwareCli, "Bare Tool", None, store.now).unwrap(),
        );
    });
    relate(&f, f.media, RelationType::DependsOn, bare);

    let mut g = graph(&f);
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&view), vec!["Bare Tool"]);
    assert_eq!(view.nodes[0].asset.subtitle, None);
    assert!(view.nodes[0].asset.tags.is_empty());
    assert_eq!(view.nodes[0].asset.kind, AssetKind::SoftwareCli);
}

#[test]
fn a_traversal_over_an_empty_graph_returns_just_the_root() {
    let f = fixture();
    let mut g = graph(&f);
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert!(view.nodes.is_empty());
    assert!(!view.truncated);
    assert_eq!(view.root.name, "Sousou no Frieren");
}
