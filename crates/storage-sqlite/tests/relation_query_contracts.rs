//! Relation graph contract tests over the real SQLite adapter (Phase 4B).
//!
//! These mirror `core/tests/relation_query_use_cases.rs` scenario for
//! scenario, so the in-memory test double and the production storage adapter
//! agree at the application boundary.

use std::sync::Arc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::relation_query_service::{
    RelationQueryService, TraversalDirection, TraversalOptions, DEPENDENCY_RELATION_TYPES,
    MAX_TRAVERSAL_DEPTH,
};
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::asset::{Asset, AssetKind};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::repos::LifecycleFilter;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use assetmesh_storage_sqlite::SharedSqlite;

fn env() -> TestSqlite {
    let factory = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    let clock: SharedClock = Arc::new(assetmesh_core::ports::clock::SystemClock);
    let ids: SharedIdGenerator = Arc::new(assetmesh_core::ports::ids::UuidV7Generator);
    TestSqlite {
        factory,
        clock,
        ids,
    }
}

struct TestSqlite {
    factory: SharedSqlite,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl TestSqlite {
    fn media_service(&self) -> MediaService<SharedSqlite> {
        MediaService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn software_service(&self) -> SoftwareService<SharedSqlite> {
        SoftwareService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn service_service(&self) -> ServiceService<SharedSqlite> {
        ServiceService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn asset_service(&self) -> AssetService<SharedSqlite> {
        AssetService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn relation_service(&self) -> RelationService<SharedSqlite> {
        RelationService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn graph(&self) -> RelationQueryService<SharedSqlite> {
        RelationQueryService::new(self.factory.clone())
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

struct Fixture {
    db: TestSqlite,
    media: AssetId,
    software: AssetId,
    cli: AssetId,
    service: AssetId,
    vps: AssetId,
}

fn fixture() -> Fixture {
    let db = env();
    let media = db
        .media_service()
        .create_media(media_cmd("Sousou no Frieren", MediaType::Anime))
        .unwrap();
    let software = db
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let cli = db
        .software_service()
        .create_software(software_cmd("fzf", SoftwareCategory::Cli))
        .unwrap();
    let service = db
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas))
        .unwrap();
    let vps = db
        .service_service()
        .create_service(service_cmd("Hetzner VPS", ServiceType::Vps))
        .unwrap();
    Fixture {
        db,
        media: media.entry.asset.id,
        software: software.entry.asset.id,
        cli: cli.entry.asset.id,
        service: service.entry.asset.id,
        vps: vps.entry.asset.id,
    }
}

fn relate(db: &TestSqlite, source: AssetId, relation_type: RelationType, target: AssetId) {
    db.relation_service()
        .attach(
            source,
            relation_type,
            target,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
}

fn node_names(
    view: &assetmesh_core::application::relation_query_service::TraversalView,
) -> Vec<String> {
    view.nodes
        .iter()
        .map(|node| node.asset.name.clone())
        .collect()
}

#[test]
fn sqlite_traversal_walks_a_cross_module_chain() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::Uses, f.service);
    relate(&f.db, f.service, RelationType::HostedOn, f.vps);
    relate(&f.db, f.vps, RelationType::PointsTo, f.software);

    let mut g = f.db.graph();
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&view), vec!["OpenAI", "Hetzner VPS", "ripgrep"]);
    assert_eq!(view.nodes[0].depth, 1);
    assert_eq!(view.nodes[2].depth, 3);
    assert_eq!(view.nodes[2].path.len(), 3);
    assert!(!view.truncated);
    // Deterministic across repeated queries over real storage.
    let again = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&again), node_names(&view));
}

#[test]
fn sqlite_diamond_reports_the_shared_child_once() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::DependsOn, f.software);
    relate(&f.db, f.media, RelationType::DependsOn, f.cli);
    relate(&f.db, f.software, RelationType::DependsOn, f.service);
    relate(&f.db, f.cli, RelationType::DependsOn, f.service);

    let mut g = f.db.graph();
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    let shared: Vec<_> = view
        .nodes
        .iter()
        .filter(|node| node.asset.id == f.service)
        .collect();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].depth, 2);
    assert_eq!(shared[0].path.len(), 2);
}

#[test]
fn sqlite_cycle_terminates() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::DependsOn, f.software);
    relate(&f.db, f.software, RelationType::DependsOn, f.cli);
    relate(&f.db, f.cli, RelationType::DependsOn, f.media);

    let mut g = f.db.graph();
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&view), vec!["ripgrep", "fzf"]);
    assert!(!view.truncated);
}

#[test]
fn sqlite_dependencies_and_impact_use_the_declared_dependency_set() {
    let f = fixture();
    relate(&f.db, f.service, RelationType::HostedOn, f.vps);
    relate(&f.db, f.cli, RelationType::InstalledVia, f.software);
    relate(&f.db, f.media, RelationType::Uses, f.service);
    relate(&f.db, f.media, RelationType::DependsOn, f.service);

    let mut g = f.db.graph();
    let deps = g
        .dependencies(f.service, &TraversalOptions::default())
        .unwrap();
    assert_eq!(node_names(&deps), vec!["Hetzner VPS"]);

    // `uses` is not a dependency edge.
    let uses_only = g
        .traverse(
            f.service,
            &TraversalOptions {
                relation_types: vec![RelationType::Uses],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(uses_only.nodes.is_empty());

    let impact = g.impact(f.vps, &TraversalOptions::default()).unwrap();
    let names = node_names(&impact);
    assert!(names.contains(&"OpenAI".to_string()), "{names:?}");
    assert!(
        names.contains(&"Sousou no Frieren".to_string()),
        "{names:?}"
    );
    let media = impact
        .nodes
        .iter()
        .find(|node| node.asset.id == f.media)
        .unwrap();
    assert_eq!(media.depth, 2);
    assert_eq!(media.path[0].relation_type, RelationType::Hosts);
    assert_eq!(media.path[1].relation_type, RelationType::DependencyOf);
    assert_eq!(DEPENDENCY_RELATION_TYPES.len(), 3);
}

#[test]
fn sqlite_type_filter_matches_the_stored_fact_from_either_end() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::DependsOn, f.software);
    relate(&f.db, f.cli, RelationType::DependsOn, f.software);

    let mut g = f.db.graph();
    let incoming = g
        .traverse(
            f.software,
            &TraversalOptions {
                direction: TraversalDirection::Incoming,
                relation_types: vec![RelationType::DependsOn],
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&incoming), vec!["Sousou no Frieren", "fzf"]);
    assert_eq!(
        incoming.nodes[0].path[0].relation_type,
        RelationType::DependencyOf
    );
}

#[test]
fn sqlite_depth_and_lifecycle_bounds_are_enforced() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::Uses, f.software);
    relate(&f.db, f.software, RelationType::Uses, f.cli);
    relate(&f.db, f.cli, RelationType::Uses, f.service);

    let mut g = f.db.graph();
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
    assert!(bounded.truncated);

    let full = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 3,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&full).len(), 3);
    assert!(!full.truncated, "the chain ends exactly at the bound");

    // Archived nodes are opt-in and never traversed through.
    f.db.asset_service().archive_asset(f.software).unwrap();
    let hidden = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert!(hidden.nodes.is_empty());
    let shown = g
        .traverse(
            f.media,
            &TraversalOptions {
                include_archived: true,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&shown).len(), 3);
}

#[test]
fn sqlite_merged_tombstones_are_not_graph_nodes() {
    let f = fixture();
    let duplicate =
        f.db.software_service()
            .create_software(software_cmd("ripgrep copy", SoftwareCategory::Cli))
            .unwrap();
    relate(&f.db, f.media, RelationType::Uses, duplicate.entry.asset.id);
    relate(
        &f.db,
        duplicate.entry.asset.id,
        RelationType::DependsOn,
        f.service,
    );
    f.db.asset_service()
        .merge_assets(duplicate.entry.asset.id, f.software)
        .unwrap();

    let mut g = f.db.graph();
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert!(!node_names(&view).contains(&"ripgrep copy".to_string()));
    assert_eq!(node_names(&view), vec!["ripgrep", "OpenAI"]);

    // A tombstone is refused as a graph root too, naming the survivor — the
    // same redirect semantics the library detail query uses.
    let error = g
        .traverse(duplicate.entry.asset.id, &TraversalOptions::default())
        .unwrap_err();
    assert!(matches!(error, AppError::Conflict { .. }), "{error}");
    assert!(
        error.to_string().contains(&f.software.to_string()),
        "{error}"
    );
    assert!(g
        .neighbors(duplicate.entry.asset.id, &TraversalOptions::default())
        .is_err());
}

#[test]
fn sqlite_neighbors_agree_with_the_relation_list_contract() {
    let f = fixture();
    relate(&f.db, f.cli, RelationType::InstalledVia, f.software);
    relate(&f.db, f.software, RelationType::Uses, f.cli);

    let mut g = f.db.graph();
    let both = g
        .neighbors(f.software, &TraversalOptions::default())
        .unwrap();
    assert_eq!(both.len(), 2);
    assert_eq!(both[0].edge.relation_type, RelationType::Installs);
    assert_eq!(both[1].edge.relation_type, RelationType::Uses);

    // The Phase 2 `relation list` contract still reports the same facts.
    let views = f.db.relation_service().list_for_asset(f.software).unwrap();
    assert_eq!(views.len(), 2);
    let stored: Vec<RelationType> = views.iter().map(|v| v.relation_type).collect();
    let graph_types: Vec<RelationType> = both.iter().map(|v| v.edge.relation_type).collect();
    assert_eq!(stored, graph_types);
}

#[test]
fn sqlite_depth_zero_and_the_depth_ceiling_are_bounded() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::Uses, f.software);
    relate(&f.db, f.software, RelationType::Uses, f.cli);

    let mut g = f.db.graph();
    let zero = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 0,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(zero.nodes.is_empty());
    assert!(!zero.truncated);

    // An over-eager depth is clamped rather than rejected, and the chain still
    // ends without a false truncation report.
    let clamped = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: MAX_TRAVERSAL_DEPTH + 50,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert_eq!(node_names(&clamped), vec!["ripgrep", "fzf"]);
    assert!(!clamped.truncated);
}

#[test]
fn sqlite_truncated_ignores_edges_the_filters_would_skip() {
    let f = fixture();
    relate(&f.db, f.media, RelationType::DependsOn, f.software);
    // Neither edge is a dependency, so the dependency walk stops here.
    relate(&f.db, f.software, RelationType::RelatedTo, f.cli);
    relate(&f.db, f.software, RelationType::Uses, f.service);

    let mut g = f.db.graph();
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

    let unfiltered = g
        .traverse(
            f.media,
            &TraversalOptions {
                max_depth: 1,
                ..TraversalOptions::default()
            },
        )
        .unwrap();
    assert!(unfiltered.truncated);
}

#[test]
fn sqlite_a_detail_less_asset_is_still_a_graph_node() {
    // Relations are shared infrastructure: the SQLite schema lets a relation
    // point at an asset with no module details, and dropping such a node would
    // silently lose a real dependency.
    let f = fixture();
    let bare = AssetId::generate();
    let mut factory = f.db.factory.clone();
    factory
        .transact(&mut |uow| {
            uow.assets().insert(&Asset::new(
                bare,
                AssetKind::SoftwareCli,
                "Bare Tool",
                None,
                chrono::Utc::now(),
            )?)
        })
        .unwrap();
    relate(&f.db, f.media, RelationType::DependsOn, bare);

    let mut g = f.db.graph();
    let view = g.traverse(f.media, &TraversalOptions::default()).unwrap();
    assert_eq!(node_names(&view), vec!["Bare Tool"]);
    assert_eq!(view.nodes[0].asset.subtitle, None);
    assert_eq!(view.nodes[0].asset.kind, AssetKind::SoftwareCli);
}

#[test]
fn sqlite_unknown_assets_are_not_found_for_graph_queries() {
    let f = fixture();
    let mut g = f.db.graph();
    let missing = AssetId::generate();
    for label in ["neighbors", "traverse"] {
        let error = if label == "neighbors" {
            g.neighbors(missing, &TraversalOptions::default())
                .unwrap_err()
        } else {
            g.traverse(missing, &TraversalOptions::default())
                .unwrap_err()
        };
        assert!(
            matches!(error, AppError::NotFound { .. }),
            "{label}: {error}"
        );
    }
}

#[test]
fn sqlite_graph_queries_never_mutate_state_and_keep_search_consistent() {
    let mut f = fixture();
    relate(&f.db, f.media, RelationType::DependsOn, f.service);

    let mut g = f.db.graph();
    let before =
        f.db.factory
            .read(&mut |q| Ok(q.relations().list_all()?.len()))
            .unwrap();
    g.traverse(f.media, &TraversalOptions::default()).unwrap();
    g.impact(f.service, &TraversalOptions::default()).unwrap();
    g.neighbors(f.media, &TraversalOptions::default()).unwrap();
    let after =
        f.db.factory
            .read(&mut |q| Ok(q.relations().list_all()?.len()))
            .unwrap();
    assert_eq!(before, after, "graph queries are read-only");

    // Rebuilding the derived search index does not change the graph either.
    SearchService::new(f.db.factory.clone(), f.db.clock.clone())
        .rebuild()
        .unwrap();
    let view = g
        .dependencies(f.media, &TraversalOptions::default())
        .unwrap();
    assert_eq!(node_names(&view), vec!["OpenAI"]);
    // Lifecycle filtering still behaves after a rebuild.
    assert_eq!(
        view.nodes[0].asset.lifecycle,
        assetmesh_core::domain::asset::LifecycleState::Active
    );
    let _ = LifecycleFilter::Active;
}
