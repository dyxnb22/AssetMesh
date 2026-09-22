//! Relation graph query use cases — Phase 4B (docs/11).
//!
//! Phase 2/3 built relation *persistence*: one canonical row per fact,
//! inverse/symmetric semantics resolved at view time, merge re-pointing.
//! This module adds the *query* layer on top — bounded, cycle-safe,
//! deterministic graph traversal over those same rows. It never writes, never
//! repairs a relation, and never reinterprets what a relation means.
//!
//! Rules this module upholds:
//!
//! - **Canonical storage stays canonical.** Callers pass and receive relation
//!   types from their own perspective; endpoint swapping and inverse
//!   derivation happen here, never in an adapter.
//! - **One traversal, one snapshot.** A whole BFS runs inside a single
//!   [`QueryUnitOfWork`], so a concurrent commit cannot make one traversal see
//!   half a graph.
//! - **Bounded work.** Depth is bounded, cycles are cut by a visited set, and
//!   each frontier is fetched with one batch query — no per-node round-trip.
//! - **Explainable, not predictive.** Impact reports the dependency paths that
//!   exist in canonical data. There is no risk score, confidence, or
//!   probability, and nothing here decides that an asset "will break".
//! - **Reused vocabulary.** Graph nodes are [`AssetSummary`], the same DTO the
//!   library list and search publish, so adapters do not learn a second asset
//!   shape.

use std::collections::{HashMap, HashSet};

use crate::application::library_service::{
    load_library_rows, AssetSummary, LibraryModule, LibraryRow,
};
use crate::domain::asset::{Asset, LifecycleState};
use crate::domain::ids::AssetId;
use crate::domain::relation::{Relation, RelationType};
use crate::ports::repos::{AssetFilter, LifecycleFilter};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde::Serialize;

/// Default traversal depth when a caller does not bound one.
pub const DEFAULT_MAX_DEPTH: usize = 8;

/// Hard ceiling on traversal depth. A deeper request is clamped rather than
/// rejected: adapters should not have to special-case an over-eager value, and
/// no real personal graph needs more than this.
pub const MAX_TRAVERSAL_DEPTH: usize = 32;

/// Relation types whose meaning supports "A needs B in order to work".
///
/// This is the single definition of dependency semantics (docs/11 4B).
/// `uses` and `points_to` are deliberately **not** dependencies: an asset can
/// use another without depending on its availability, and a name that points
/// at a target is not a requirement that the target exist. `related_to` is a
/// free-form association and is never reinterpreted as a dependency.
pub const DEPENDENCY_RELATION_TYPES: &[RelationType] = &[
    RelationType::DependsOn,
    RelationType::InstalledVia,
    RelationType::HostedOn,
];

/// Which way traversal follows edges, from the queried asset's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraversalDirection {
    /// Follow edges the asset is the source of ("what does it point at").
    #[default]
    Outgoing,
    /// Follow edges the asset is the target of ("what points at it").
    Incoming,
    /// Follow both.
    Both,
}

impl TraversalDirection {
    pub const fn as_str(&self) -> &'static str {
        match self {
            TraversalDirection::Outgoing => "outgoing",
            TraversalDirection::Incoming => "incoming",
            TraversalDirection::Both => "both",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "outgoing" | "out" => Some(TraversalDirection::Outgoing),
            "incoming" | "in" => Some(TraversalDirection::Incoming),
            "both" | "all" => Some(TraversalDirection::Both),
            _ => None,
        }
    }

    fn follows_outgoing(&self) -> bool {
        matches!(
            self,
            TraversalDirection::Outgoing | TraversalDirection::Both
        )
    }

    fn follows_incoming(&self) -> bool {
        matches!(
            self,
            TraversalDirection::Incoming | TraversalDirection::Both
        )
    }
}

/// Options shared by every graph query.
///
/// `relation_types` is matched against the relation type **as read from the
/// node being expanded**, so a caller asks for `depends_on` and never has to
/// know that the inverse `dependency_of` exists.
#[derive(Debug, Clone, PartialEq)]
pub struct TraversalOptions {
    pub direction: TraversalDirection,
    /// Restrict to these relation types, matched against the stored canonical
    /// type. Empty means every type. Inverse types are accepted and mapped
    /// onto the stored row they normalize to.
    pub relation_types: Vec<RelationType>,
    /// Hops to follow. `0` reaches nothing beyond the root; values above
    /// [`MAX_TRAVERSAL_DEPTH`] are clamped.
    pub max_depth: usize,
    /// Whether archived assets may appear as nodes and be traversed through.
    /// Merged tombstones are never nodes — they are redirects.
    pub include_archived: bool,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        TraversalOptions {
            direction: TraversalDirection::Outgoing,
            relation_types: Vec::new(),
            max_depth: DEFAULT_MAX_DEPTH,
            include_archived: false,
        }
    }
}

impl TraversalOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Depth actually used after the default and the hard ceiling.
    pub fn effective_max_depth(&self) -> usize {
        if self.max_depth == 0 {
            0
        } else {
            self.max_depth.min(MAX_TRAVERSAL_DEPTH)
        }
    }

    /// Whether a stored row is followed.
    ///
    /// The filter is matched against the row's **stored** type, not the type
    /// it reads as from the expanding node: a caller asking for `depends_on`
    /// means "the `depends_on` fact", whichever end of it they are standing
    /// on. Filtering on the effective type instead would make
    /// `direction = Incoming` + `[DependsOn]` return nothing, because from the
    /// target the same row reads as `dependency_of`. An inverse type in the
    /// filter is accepted too — `primary()` maps it onto the stored row it
    /// would be normalized into.
    fn allows(&self, stored: RelationType) -> bool {
        self.relation_types.is_empty()
            || self
                .relation_types
                .iter()
                .any(|wanted| wanted.primary() == stored.primary())
    }

    /// Options for `dependencies`: follow the dependency types outwards.
    pub fn dependencies() -> Self {
        TraversalOptions {
            direction: TraversalDirection::Outgoing,
            relation_types: DEPENDENCY_RELATION_TYPES.to_vec(),
            ..TraversalOptions::default()
        }
    }

    /// Options for `dependents` / `impact`: the same dependency types,
    /// followed inwards from the depended-on asset.
    pub fn dependents() -> Self {
        TraversalOptions {
            direction: TraversalDirection::Incoming,
            relation_types: DEPENDENCY_RELATION_TYPES.to_vec(),
            ..TraversalOptions::default()
        }
    }
}

/// One hop of a traversal path. `relation_type` reads from `from_asset_id`, so
/// a consumer never has to derive inverse semantics itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelationPathHop {
    pub from_asset_id: AssetId,
    pub to_asset_id: AssetId,
    pub relation_type: RelationType,
}

/// One node reached by a traversal, with the shortest path that reached it.
///
/// The same DTO serves `dependencies`, `dependents`, `traverse`, and `impact`:
/// one graph-node vocabulary instead of four shapes that drift.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraversalNode {
    pub asset: AssetSummary,
    /// Hops from the root; always >= 1 (the root itself is not a result).
    pub depth: usize,
    /// The shortest deterministic path from the root to this node. The first
    /// hop's `from_asset_id` is the root. Never empty.
    pub path: Vec<RelationPathHop>,
}

/// One neighbour of an asset: the other asset as a unified library summary,
/// plus the edge that connects it.
///
/// [`crate::application::relation_service::RelationView`] is reused verbatim
/// as the edge, so the graph and `relation list` cannot disagree about
/// inverse or symmetric semantics.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NeighborView {
    pub asset: AssetSummary,
    pub edge: crate::application::relation_service::RelationView,
}

/// The result of a bounded traversal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraversalView {
    pub root: AssetSummary,
    /// Reached nodes, ordered by depth then `AssetId`.
    pub nodes: Vec<TraversalNode>,
    /// True when the depth bound stopped the traversal, so deeper nodes may
    /// exist beyond what is reported.
    pub truncated: bool,
}

/// Read-only relation graph queries (docs/11 Phase 4B).
#[derive(Debug, Clone)]
pub struct RelationQueryService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> RelationQueryService<F> {
    pub fn new(factory: F) -> Self {
        RelationQueryService { factory }
    }

    /// Every asset directly connected to `asset_id`, one row per edge.
    ///
    /// Always both directions: a neighbour count that only follows outgoing
    /// edges is what [`RelationQueryService::outgoing`] is for. The
    /// `direction` field of `options` is ignored here on purpose.
    pub fn neighbors(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<Vec<NeighborView>> {
        self.with_direction(asset_id, options, TraversalDirection::Both)
    }

    /// Neighbours reached over edges the asset is the source of.
    pub fn outgoing(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<Vec<NeighborView>> {
        self.with_direction(asset_id, options, TraversalDirection::Outgoing)
    }

    /// Neighbours reached over edges the asset is the target of.
    pub fn incoming(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<Vec<NeighborView>> {
        self.with_direction(asset_id, options, TraversalDirection::Incoming)
    }

    /// What `asset_id` depends on, transitively, with the path that explains
    /// each dependency.
    pub fn dependencies(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<TraversalView> {
        self.traverse(
            asset_id,
            &with_dependency_types(options, TraversalDirection::Outgoing),
        )
    }

    /// What depends on `asset_id`, transitively.
    pub fn dependents(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<TraversalView> {
        self.traverse(
            asset_id,
            &with_dependency_types(options, TraversalDirection::Incoming),
        )
    }

    /// Bounded graph traversal from `asset_id` in the requested direction.
    pub fn traverse(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<TraversalView> {
        self.factory
            .read(&mut |q| run_traversal(q, asset_id, options))
    }

    /// What could be affected if `asset_id` stopped being available: every
    /// asset that transitively depends on it, with the dependency path.
    ///
    /// This is a structural statement about canonical relations, not a
    /// prediction — there is no risk score and no probability.
    pub fn impact(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
    ) -> AppResult<TraversalView> {
        self.dependents(asset_id, options)
    }

    /// Runs one query with a fixed direction, leaving the caller's options
    /// untouched.
    fn with_direction(
        &mut self,
        asset_id: AssetId,
        options: &TraversalOptions,
        direction: TraversalDirection,
    ) -> AppResult<Vec<NeighborView>> {
        let fixed = TraversalOptions {
            direction,
            ..options.clone()
        };
        self.factory
            .read(&mut |q| load_neighbors(q, asset_id, &fixed))
    }
}

/// Applies the dependency relation-type set for a direction, keeping the
/// caller's other options (depth, archived policy). One list serves both
/// directions because the filter matches the stored type.
fn with_dependency_types(
    options: &TraversalOptions,
    direction: TraversalDirection,
) -> TraversalOptions {
    TraversalOptions {
        direction,
        relation_types: DEPENDENCY_RELATION_TYPES.to_vec(),
        max_depth: options.max_depth,
        include_archived: options.include_archived,
    }
}

// ---------------------------------------------------------------------------
// Traversal engine
// ---------------------------------------------------------------------------

/// An edge oriented for traversal: `other` is the node reached from
/// `expanding`, and `effective` is how the edge reads from `expanding`.
struct OrientedEdge {
    other: AssetId,
    effective: RelationType,
}

/// Orients one stored row for expansion of `expanding`, or `None` when the
/// direction does not follow it.
///
/// Stored direction and effective direction are different things on purpose:
/// the row keeps its canonical primary type and endpoints, while the caller
/// sees the type as it reads from the node it asked about.
fn orient(
    relation: &Relation,
    expanding: AssetId,
    direction: TraversalDirection,
) -> Option<OrientedEdge> {
    if relation.source_asset_id == expanding {
        if !direction.follows_outgoing() {
            return None;
        }
        Some(OrientedEdge {
            other: relation.target_asset_id,
            effective: relation.relation_type,
        })
    } else if relation.target_asset_id == expanding {
        if !direction.follows_incoming() {
            return None;
        }
        Some(OrientedEdge {
            other: relation.source_asset_id,
            effective: relation
                .relation_type
                .effective_from(relation.source_asset_id, expanding),
        })
    } else {
        None
    }
}

/// Breadth-first traversal: one batch edge query per frontier, a visited set
/// for cycles, and a deterministic order inside every level.
///
/// The first time a node is reached it keeps that path, so a diamond
/// (`A→B→D`, `A→C→D`) reports `D` exactly once at the depth BFS found it,
/// reached by the smallest deterministic path — never twice.
fn run_traversal(
    q: &mut dyn QueryUnitOfWork,
    root: AssetId,
    options: &TraversalOptions,
) -> AppResult<TraversalView> {
    let hydration = GraphHydration::load(q)?;
    let root_asset = hydration
        .asset(root)?
        .ok_or_else(|| AppError::not_found("asset", root))?;
    if root_asset.lifecycle_state == LifecycleState::Merged {
        // A tombstone is a redirect, not a graph root: the caller is sent to
        // the survivor instead of being shown an empty graph.
        return Err(crate::application::library_service::merged_redirect_error(
            &root_asset,
        ));
    }
    let root_summary = hydration.summary(root)?;

    let max_depth = options.effective_max_depth();
    let mut nodes: Vec<TraversalNode> = Vec::new();
    let mut visited: HashSet<AssetId> = HashSet::from([root]);
    // AssetId → shortest path from the root, so a node discovered later can
    // still report how it was reached. The root's own path is empty.
    let mut paths: HashMap<AssetId, Vec<RelationPathHop>> = HashMap::from([(root, Vec::new())]);
    let mut frontier: Vec<AssetId> = vec![root];
    let mut stopped_at_bound = false;

    for depth in 1..=max_depth {
        if frontier.is_empty() {
            break;
        }
        let mut edges = q.relations().list_for_assets(&frontier)?;
        // Deterministic expansion order: without it, which of two equal-depth
        // parents claims a shared child would depend on row order.
        sort_edges(&mut edges);

        let mut next_frontier: Vec<AssetId> = Vec::new();
        for relation in &edges {
            // A row may touch two frontier nodes, and each of them is a valid
            // place to expand from — so try every frontier endpoint it has.
            for expanding in frontier_endpoints(relation, &frontier) {
                let Some(edge) = orient(relation, expanding, options.direction) else {
                    continue;
                };
                if !options.allows(relation.relation_type) {
                    continue;
                }
                // A relation to an already-visited node is either a cycle or a
                // shorter path already recorded; either way it is not a new
                // node, which is what keeps a diamond from reporting `D`
                // twice.
                if visited.contains(&edge.other) {
                    continue;
                }
                let Some(asset) = hydration.asset(edge.other)? else {
                    continue;
                };
                if !options.include_archived && asset.lifecycle_state == LifecycleState::Archived {
                    continue;
                }
                // Merged tombstones are redirects, never independent graph
                // nodes.
                if asset.lifecycle_state == LifecycleState::Merged {
                    continue;
                }

                visited.insert(edge.other);
                let mut path = paths.get(&expanding).cloned().unwrap_or_default();
                path.push(RelationPathHop {
                    from_asset_id: expanding,
                    to_asset_id: edge.other,
                    relation_type: edge.effective,
                });
                paths.insert(edge.other, path.clone());
                next_frontier.push(edge.other);
                nodes.push(TraversalNode {
                    asset: hydration.summary(edge.other)?,
                    depth,
                    path,
                });
            }
        }

        if depth == max_depth {
            // The bound stopped the walk. Whether that actually hid anything
            // is only known by looking at what the last level still points at.
            stopped_at_bound = true;
        }
        next_frontier.sort();
        frontier = next_frontier;
    }

    let truncated = stopped_at_bound
        && frontier_has_reachable_edges(q, &frontier, &visited, options, &hydration)?;

    nodes.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.asset.id.cmp(&b.asset.id))
    });
    Ok(TraversalView {
        root: root_summary,
        nodes,
        truncated,
    })
}

/// True when the last expanded level still has an edge the traversal itself
/// would have followed — the honest definition of "the depth bound hid
/// something".
///
/// Every rule the walk applies must be re-applied here, or `truncated` reports
/// a bound that hid nothing: an edge of a filtered-out relation type, an edge
/// in a direction the query does not follow, or an edge to an archived/merged
/// node the walk skipped. Back-edges to already-visited nodes do not count
/// either.
fn frontier_has_reachable_edges(
    q: &mut dyn QueryUnitOfWork,
    frontier: &[AssetId],
    visited: &HashSet<AssetId>,
    options: &TraversalOptions,
    hydration: &GraphHydration,
) -> AppResult<bool> {
    if frontier.is_empty() {
        return Ok(false);
    }
    let edges = q.relations().list_for_assets(frontier)?;
    for relation in &edges {
        for expanding in frontier_endpoints(relation, frontier) {
            let Some(edge) = orient(relation, expanding, options.direction) else {
                continue;
            };
            if !options.allows(relation.relation_type) || visited.contains(&edge.other) {
                continue;
            }
            let Some(asset) = hydration.asset(edge.other)? else {
                continue;
            };
            if !options.include_archived && asset.lifecycle_state == LifecycleState::Archived {
                continue;
            }
            if asset.lifecycle_state == LifecycleState::Merged {
                continue;
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// The frontier nodes a stored row can be expanded from. A row touching two
/// frontier nodes is expanded from both, so an intra-frontier edge is explored
/// in whichever direction the query follows.
fn frontier_endpoints(relation: &Relation, frontier: &[AssetId]) -> Vec<AssetId> {
    let mut endpoints: Vec<AssetId> = Vec::new();
    for endpoint in [relation.source_asset_id, relation.target_asset_id] {
        if frontier.contains(&endpoint) && !endpoints.contains(&endpoint) {
            endpoints.push(endpoint);
        }
    }
    endpoints
}

/// One stable order for relation rows, shared by the traversal and the
/// neighbour query so both report the same deterministic order.
fn sort_edges(edges: &mut [Relation]) {
    edges.sort_by(|a, b| {
        (
            a.source_asset_id,
            a.relation_type.as_str(),
            a.target_asset_id,
            a.id,
        )
            .cmp(&(
                b.source_asset_id,
                b.relation_type.as_str(),
                b.target_asset_id,
                b.id,
            ))
    });
}

/// Loads the one-hop neighbourhood of `asset_id` inside the caller's scope.
///
/// Only the neighbouring assets are hydrated: a one-hop question must not cost
/// a pass over the whole library, so this deliberately does not use the
/// traversal's full hydration index.
fn load_neighbors(
    q: &mut dyn QueryUnitOfWork,
    asset_id: AssetId,
    options: &TraversalOptions,
) -> AppResult<Vec<NeighborView>> {
    let mut edges = q.relations().list_for_asset(asset_id)?;
    sort_edges(&mut edges);

    // Only the queried asset and its neighbours are hydrated: a one-hop
    // question must not cost a pass over the whole library, so this
    // deliberately does not use the traversal's full hydration index.
    let mut wanted: Vec<AssetId> = vec![asset_id];
    for relation in &edges {
        for endpoint in [relation.source_asset_id, relation.target_asset_id] {
            if !wanted.contains(&endpoint) {
                wanted.push(endpoint);
            }
        }
    }
    let hydration = GraphHydration::for_ids(q, &wanted)?;
    let queried = hydration
        .asset(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    if queried.lifecycle_state == LifecycleState::Merged {
        return Err(crate::application::library_service::merged_redirect_error(
            &queried,
        ));
    }

    let mut views: Vec<NeighborView> = Vec::new();
    for relation in &edges {
        let Some(edge) = orient(relation, asset_id, options.direction) else {
            continue;
        };
        if !options.allows(relation.relation_type) {
            continue;
        }
        let Some(asset) = hydration.asset(edge.other)? else {
            continue;
        };
        if !options.include_archived && asset.lifecycle_state == LifecycleState::Archived {
            continue;
        }
        if asset.lifecycle_state == LifecycleState::Merged {
            continue;
        }
        views.push(NeighborView {
            asset: hydration.summary(edge.other)?,
            edge: crate::application::relation_service::RelationView {
                relation_id: relation.id,
                other_asset_id: edge.other,
                other_asset_name: asset.name.clone(),
                relation_type: edge.effective,
                outgoing: relation.source_asset_id == asset_id,
                note: relation.note.clone(),
                provenance: relation.provenance,
                created_at: relation.created_at,
            },
        });
    }
    views.sort_by(|a, b| {
        (a.edge.relation_type.as_str(), &a.asset.name, a.asset.id).cmp(&(
            b.edge.relation_type.as_str(),
            &b.asset.name,
            b.asset.id,
        ))
    });
    Ok(views)
}

// ---------------------------------------------------------------------------
// Node hydration
// ---------------------------------------------------------------------------

/// Everything a graph query needs to name a node, loaded with a fixed number
/// of queries and reused across the whole traversal.
///
/// Two sources, deliberately: the asset table (identity, kind, lifecycle) and
/// the library rows (typed details + tags). An asset without module details
/// still appears in graph results, because relations are shared infrastructure
/// and dropping such a node would silently lose a real dependency — unlike the
/// library list, which only contains assets a module owns.
struct GraphHydration {
    assets: HashMap<AssetId, Asset>,
    rows: HashMap<AssetId, LibraryRow>,
}

impl GraphHydration {
    /// Hydrates only the given assets, one point lookup each.
    ///
    /// Used by the one-hop queries, where the answer touches a handful of
    /// assets and reading the whole library would be disproportionate.
    fn for_ids(q: &mut dyn QueryUnitOfWork, ids: &[AssetId]) -> AppResult<Self> {
        let mut assets: HashMap<AssetId, Asset> = HashMap::new();
        let mut rows: HashMap<AssetId, LibraryRow> = HashMap::new();
        for id in ids {
            if assets.contains_key(id) {
                continue;
            }
            let Some(asset) = q.assets().get(*id)? else {
                continue;
            };
            let details = crate::application::library_service::load_details_for(q, &asset)?;
            let mut tags: Vec<String> = q
                .tags()
                .list_for_asset(*id)?
                .into_iter()
                .map(|tag| tag.name)
                .collect();
            tags.sort();
            rows.insert(
                *id,
                LibraryRow {
                    asset: asset.clone(),
                    details,
                    tags,
                },
            );
            assets.insert(*id, asset);
        }
        Ok(GraphHydration { assets, rows })
    }

    /// The full index: every asset plus every library row.
    ///
    /// Used by traversal, which can reach any node and therefore needs the
    /// whole graph's worth of identity in one pass.
    fn load(q: &mut dyn QueryUnitOfWork) -> AppResult<Self> {
        let assets = q
            .assets()
            .list(&AssetFilter {
                kind: None,
                lifecycle: Some(LifecycleFilter::All),
            })?
            .into_iter()
            .map(|asset| (asset.id, asset))
            .collect::<HashMap<_, _>>();
        let rows = load_library_rows(q, &LibraryModule::ALL)?
            .into_iter()
            .map(|row| (row.asset.id, row))
            .collect::<HashMap<_, _>>();
        Ok(GraphHydration { assets, rows })
    }

    fn asset(&self, id: AssetId) -> AppResult<Option<Asset>> {
        Ok(self.assets.get(&id).cloned())
    }

    /// A unified summary for any asset, whether or not a module owns its
    /// details. See the struct docs for why detail-less assets still appear.
    fn summary(&self, id: AssetId) -> AppResult<AssetSummary> {
        let Some(asset) = self.assets.get(&id) else {
            return Err(AppError::not_found("asset", id));
        };
        if let Some(row) = self.rows.get(&id) {
            return Ok(crate::application::library_service::summarize_row(row));
        }
        Ok(AssetSummary {
            id: asset.id,
            kind: asset.kind,
            name: asset.name.clone(),
            lifecycle: asset.lifecycle_state,
            subtitle: None,
            tags: Vec::new(),
            updated_at: asset.updated_at,
        })
    }
}
