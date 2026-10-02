# Unified Library Core

The implemented application contract serves both CLI and Desktop. It provides
cross-module queries without exposing private repositories or moving business
rules into adapters. Optional directions live in the [Roadmap](07-roadmap.md).

## Purpose

Phase 4 turns three working module verticals into one coherent asset library.

The core question is no longer “can Media, Software, and Services coexist?” That has been proven. The Phase 4 question is:

> Can an interface adapter operate AssetMesh as one library — list assets, open a detail view, search, inspect relationships, answer dependency/impact questions, review duplicates, and query activity — without knowing module repository details or reimplementing business rules?

Desktop and CLI consume this application contract.

## Baseline already established

Phase 4 builds on contracts that already exist and must not be redesigned without a concrete defect:

- shared `Asset` identity and lifecycle state;
- module-owned `MediaRecord`, `SoftwareRecord`, and `ServiceRecord` details;
- namespaced external references;
- tags;
- explicit merge with merged tombstones/redirects;
- canonical one-row relation storage with inverse/symmetric semantics;
- cross-module activity events;
- rebuildable Search Projection / FTS index;
- portable export/import;
- `QueryUnitOfWork` read snapshots and transactional write scopes;
- CLI as a thin adapter over application services.

Phase 4 is primarily an **application/query-contract phase**, not a storage-model rewrite.

## Architectural rules

1. **Adapters depend on application contracts, not repositories.** Desktop, CLI, HTTP, or MCP adapters must not assemble a unified asset view by reaching into Media/Software/Services repositories themselves.
2. **Typed details remain typed.** The core application contract must not collapse module detail records into unstructured `serde_json::Value` merely to make a generic UI easier.
3. **One read request sees one snapshot.** A unified detail or traversal query that touches multiple repositories runs inside one `QueryUnitOfWork` read scope.
4. **No hidden canonical writes.** Search, graph traversal, duplicate detection, and impact analysis are read-only. Merge remains an explicit command.
5. **Deterministic ordering.** Every paginated or traversed result has a documented stable tie-breaker, normally canonical Asset ID after the requested sort key.
6. **Bounded graph work.** Relation traversal is cycle-safe and depth-bounded. Phase 4 does not introduce a graph database.
7. **Derived state stays rebuildable.** Search indexes and future query projections are not canonical user data.
8. **No presentation leakage.** React/Tauri navigation state, graph coordinates, table columns, or UI-only labels do not enter domain/application contracts.

## Phase 4A — Unified Library Query

### Goal

Expose a single application-facing read model over Media, Software, and Services so adapters do not branch into module services to build the global library.

**Status: complete.** Implemented in `crates/core/src/application/library_service.rs`, exercised by the CLI (`assetmesh library list|get|search`) as the first adapter.

### Application service

```text
LibraryService
  get_asset(id)                 -> AppResult<AssetDetailView>
  resolve_merge_redirect(id)    -> AppResult<AssetId>
  list_assets(query)            -> AppResult<Page<AssetSummary>>
  search_assets(query)          -> AppResult<Page<AssetSummary>>
```

`get_asset` returns one complete application view from one consistent read snapshot. `resolve_merge_redirect` exists so adapters never hand-roll `merged_into` chains (ADR 0005); it follows a redirect with a visited set, so a corrupt cycle fails loudly instead of looping.

### Core DTOs

The implemented contract:

```rust
pub struct AssetSummary {
    pub id: AssetId,
    pub kind: AssetKind,
    pub name: String,
    pub lifecycle: LifecycleState,
    pub subtitle: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: Timestamp,
    /// Typed module record when the adapter hydrates one (list rows can
    /// render status/rating/progress without a second per-asset fetch).
    pub details: Option<AssetDetails>,
}

pub enum AssetDetails {
    Media(MediaRecord),
    Software(SoftwareRecord),
    Service(ServiceRecord),
    Info(InfoRecord),
}

pub struct AssetDetailView {
    pub asset: Asset,
    pub details: AssetDetails,
    pub tags: Vec<String>,
    pub external_refs: Vec<AssetExternalRef>,
}
```

A future module extends `AssetDetails`; it must not require clients to read module-private tables.

Notes on the implemented shapes:

- `subtitle` is produced by the module's own summary helper in
  `application/projection.rs` (`media_subtitle` / `software_subtitle` /
  `service_subtitle`) — the same functions the search projection calls, so
  list, search, and FTS subtitles cannot drift.
- `AssetDetails` serializes as `{"module": "media", …record fields}` (the tag
  uses the same vocabulary as `AssetKind::module()`), so a JSON transport
  consumer gets a self-describing union rather than a nested wrapper object.
- `AssetDetailView` deliberately carries **no** relations. The relation query
  service (Phase 4B) composes its own bounded views over
  `RelationQueryService`; folding a relation list into the detail view would
  pre-empt that contract and mix an unbounded collection into a detail DTO.
- Tags are sorted by name and external refs by `(namespace, external_id)` in
  the library layer, so the DTO's ordering is a contract rather than a
  repository implementation detail.

### Unified query contract

```text
LibraryQuery            LibrarySearchQuery
  lifecycle               text
  modules                 lifecycle
  kinds                   modules
  tags                    kinds
  sort                    tags
  page                    page

PageRequest
  limit        (0 → DEFAULT_PAGE_LIMIT = 50, clamped to MAX_PAGE_LIMIT = 200)
  offset

Page<T>
  items
  offset
  limit
  total      Some(exact count) for the list; None for search (see below)
```

Rules, as implemented:

- filtering by lifecycle and asset kind/module; `modules` and `kinds` may be
  combined (both must match), and a contradictory pair resolves to an empty
  page without reading storage;
- tag filtering requires **every** requested tag, compared case-insensitively
  (the same tag semantics the module filters use);
- deterministic sorting: `updated_desc` (default), `updated_asc`, `name_asc`,
  `name_desc`, `kind_asc`, each with `AssetId` ascending as the secondary key;
- bounded page size; offset pagination, stable against a static library;
- default lifecycle is `LifecycleFilter::Active`, which excludes merged
  tombstones and archived assets; archived is opt-in via `ActiveOrArchived`
  or `All`;
- no N+1 repository pattern: one query uses one `QueryUnitOfWork`; hydration
  is limited to the returned page and tags are loaded in batches;
- search results and list rows map into the same `AssetSummary` — a test
  asserts the two are equal for the same asset.

#### Search semantics

`LibrarySearchQuery` reuses the existing Search Projection untouched: no FTS
change, no new ranking, no vector/embedding layer. The library asks the index
for exactly the window the page needs (`offset + limit`), hydrates the hits
through the same module readers the list uses, then applies the lifecycle /
module / kind / tag filters in relevance order.

Two consequences are part of the contract, not defects:

- relevance ordering belongs to the projection, so `Page.total` is `None` for
  search. Adapters detect more pages by comparing `items.len()` with `limit`.
- filters are applied after hydration, so a filtered search page can be
  shorter than `limit`. Widen the page or relax the filter when a full page is
  required.

The projection window one search page hydrates (`offset + limit`) is bounded,
so a pathologically deep offset yields an empty page rather than asking the
index for an unbounded number of rows.

Archived assets stay searchable in the projection (an existing invariant), so
they are opt-in here exactly as in the list. Merged tombstones are never
projected, so they can never be a search hit.

### Detail semantics

| Case | Behavior |
| --- | --- |
| unknown id | `AppError::not_found("asset", id)` |
| merged tombstone | `AppError::conflict` naming the surviving asset — a tombstone is a redirect, not a library entry |
| archived | readable; archiving only blocks mutation |
| asset with no module details | `AppError::not_found("module details", id)` — the library only contains assets a module owns |
| module record on another module's kind | not surfaced (defense in depth; the repository boundary already refuses to write it) |

### Cross-module lookup

Canonical Asset ID is the primary lookup key. Existing namespaced external-reference identity rules remain authoritative for exact external lookup. Phase 4 must not introduce a second identity system for the unified library.

### Composition and performance

Unified list filtering, ordering and pagination use the storage-backed
[`LibraryReadPort`](../crates/core/src/ports/repos.rs) inside `QueryUnitOfWork`.
SQLite performs `LIMIT/OFFSET` and two-stage counting; tag hydration and subtitles
load only for the returned page. Batch tag and relation queries chunk parameters
at 500 to respect SQLite limits. The optional scale gate exercises deep pagination.

Graph hydration and duplicate evidence still use module readers in the application
layer. Each module's list query loads its page's tags together rather than issuing
per-asset lookups. Adapters do not assemble either query themselves.

## Phase 4B — Relation Traversal and Impact

**Status: complete.** Implemented in `crates/core/src/application/relation_query_service.rs`.

### Goal

Promote the existing relation write/read primitives into useful cross-module graph queries.

The canonical relation model is retained unchanged: inverse-pair relations are stored in one primary direction, symmetric relations in canonical endpoint order, and inverse semantics are resolved for the caller at view time.

### Service and DTOs

```rust
RelationQueryService
  neighbors(asset_id, options)   -> Vec<NeighborView>
  outgoing(asset_id, options)    -> Vec<NeighborView>
  incoming(asset_id, options)    -> Vec<NeighborView>
  dependencies(asset_id, options) -> TraversalView
  dependents(asset_id, options)   -> TraversalView
  traverse(asset_id, options)     -> TraversalView
  impact(asset_id, options)       -> TraversalView
```

```rust
pub enum TraversalDirection { Outgoing, Incoming, Both }

pub struct TraversalOptions {
    pub direction: TraversalDirection,
    pub relation_types: Vec<RelationType>,
    pub max_depth: usize,
    pub include_archived: bool,
}

pub struct RelationPathHop {
    pub from_asset_id: AssetId,
    pub to_asset_id: AssetId,
    /// How the hop reads from `from_asset_id` — inverses are already resolved.
    pub relation_type: RelationType,
}

pub struct TraversalNode {
    pub asset: AssetSummary,
    pub depth: usize,
    pub path: Vec<RelationPathHop>,
}

pub struct TraversalView {
    pub root: AssetSummary,
    pub nodes: Vec<TraversalNode>,
    /// True when the depth bound stopped a walk that still had edges to follow.
    pub truncated: bool,
}

pub struct NeighborView {
    pub asset: AssetSummary,
    pub edge: RelationView,   // the Phase 2 edge DTO, reused verbatim
}
```

One graph-node vocabulary serves `dependencies`, `dependents`, `traverse`, and
`impact`: there is no `ImpactEntry` drifting beside a `DependencyNode`.

### Traversal semantics

- breadth-first, so `depth` is the shortest distance and each level is ordered
  deterministically;
- visited-set cycle protection: a cycle terminates and never re-reports a node,
  and a diamond (`A→B→D`, `A→C→D`) reports `D` exactly once at the depth BFS
  found it, reached by the smallest deterministic path;
- `max_depth = 0` reaches nothing beyond the root; values above
  `MAX_TRAVERSAL_DEPTH` (32) are clamped, not rejected;
- the relation-type filter matches the row's **stored** type, so
  `direction = Incoming` + `[DependsOn]` finds the rows whose stored type is
  `depends_on` instead of returning nothing. An inverse type in the filter is
  accepted and mapped onto the row it normalizes to;
- every relation type reported to the caller is the **effective** type from the
  expanding node — adapters never derive an inverse;
- `truncated` re-applies the traversal's own direction, type, and lifecycle
  filters, so it never claims the bound hid something the walk would have
  skipped anyway;
- one whole traversal runs inside a single `QueryUnitOfWork`, with one batch
  edge query per frontier (`RelationReader::list_for_assets`) — no per-node
  round-trip;
- read-only: traversal never repairs, creates, or deletes relations.

### Dependencies and dependents

`DEPENDENCY_RELATION_TYPES` is the single definition of dependency semantics:

```text
depends_on, installed_via, hosted_on
```

`uses`, `points_to`, and `related_to` are deliberately **not** dependencies: an
asset can use another without depending on its availability, a name that points
at a target does not require the target to exist, and `related_to` is a
free-form association. One list serves both directions because the filter
matches the stored type.

`dependencies`, `dependents`, and `impact` fix their own direction and relation
types; `max_depth` and `include_archived` from the caller's options are kept.
`impact` and `dependents` are the same structural query — impact is the name
that answers "what could be affected".

### Lifecycle in the graph

- archived nodes are opt-in (`include_archived`, default `false`) and are never
  traversed *through*;
- the root is always allowed, even archived — the caller asked for it
  explicitly;
- merged tombstones are never graph nodes, and a tombstone is refused as a root
  with the same redirect-naming conflict the library detail query uses;
- an asset with no module details **is** a graph node: relations are shared
  infrastructure, and dropping such a node would silently lose a real
  dependency. Its summary carries `subtitle: None` and no tags.

### Impact result

Impact is an explainable graph query, not a risk score. Every node carries the
shortest path that reached it, ordered root → leaf, with each hop reading from
its own `from_asset_id`. There is no confidence, probability, or risk score
anywhere in the contract.

## Phase 4C — Global Search, Activity, and Duplicate Review

**Status: complete.**

### Global search

The unified search query contract is delivered by
`LibraryService::search_assets` (Phase 4A) and documented there. It reuses the
existing Search Projection untouched: no FTS change, no new ranking, no
vector/embedding layer, CJK/substring fallback and rebuildability unchanged.
Phase 4 does **not** add semantic/vector search.

### Global activity query

**Implemented** in `crates/core/src/application/activity_service.rs`.

```rust
ActivityService
  query(&ActivityQuery)  -> AppResult<Page<ActivityView>>
  recent(&PageRequest)   -> AppResult<Page<ActivityView>>
  for_asset(id, &PageRequest) -> AppResult<Page<ActivityView>>

pub struct ActivityQuery {
    pub asset_id: Option<AssetId>,
    pub event_types: Vec<String>,
    pub modules: Vec<ActivityModule>,
    pub actors: Vec<String>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub page: PageRequest,
}

pub struct ActivityView {
    pub id: ActivityId,
    pub event_type: String,
    pub module: Option<ActivityModule>,
    pub occurred_at: Timestamp,
    pub actor: String,
    pub asset_id: Option<AssetId>,
    /// The asset's current name, when it still exists.
    pub asset_name: Option<String>,
    pub payload: serde_json::Value,
}
```

Rules, as implemented:

- every filter is optional and they compose with AND; an empty query is "most
  recent events", which is what a global feed wants;
- ordering is newest first with the **event id** as the deterministic
  tie-breaker, so two events sharing a timestamp still have one order and
  pagination never repeats or skips a row;
- a single `QueryUnitOfWork` contains the matching count and page query, so
  they cannot mix two moments of history. SQLite applies filters, ordering and
  LIMIT/OFFSET before decoding event payloads; asset names are joined only for
  the returned page. Existing time and per-asset indexes include the event-id
  tie-breaker, avoiding an extra sort for tied timestamps. Historical events
  are retained;
- history survives lifecycle changes: an archived or merged asset's events are
  still reported, and the asset is named as it stands today. An event about an
  asset that no longer exists is still reported, just without a name;
- the payload is the canonical event's own JSON, untouched — recorded
  provenance, not a substitute for typed module fields.

`ActivityModule` (in `domain/activity.rs`) is the **only** place an event type
is mapped to a subsystem, so no adapter string-matches prefixes. `media.imported`
is classified as `Import` before the prefix mapping, because it carries the
`import` actor rather than belonging to the Media module. An unrecognized event
type is reported as `module: None` rather than misclassified — a newer module's
events still appear.

### Duplicate review

**Implemented** in `crates/core/src/application/duplicate_review_service.rs`.

```rust
DuplicateReviewService
  candidates(&DuplicateQuery) -> AppResult<Page<DuplicateCandidate>>

pub struct DuplicateCandidate {
    pub left: AssetSummary,     // the smaller canonical Asset id
    pub right: AssetSummary,    // the larger canonical Asset id
    pub evidence: Vec<DuplicateEvidence>,
}

pub enum DuplicateEvidence {
    SameNormalizedName { normalized_name: String, kind: AssetKind },
    SameProvider { provider: String },
    SameDomain { domain: String },
    SameInstallLocation { location: String },
}
```

Rules, as implemented:

- **detection ≠ merge.** The service is read-only by construction. No evidence
  strength causes a canonical write; merging stays the explicit
  `AssetService::merge_assets` command, which is also where the real
  field-level conflict list comes from. There is no numeric confidence
  anywhere in the contract, and no `mergeable` / `needs_conflict_resolution`
  prediction, because predicting merge success would mean duplicating merge
  logic.
- **only reachable conditions.** Two live canonical assets can never share a
  `(namespace, external_id)` pair — storage enforces global uniqueness — so
  external-reference equality is deliberately *not* a candidate signal. It
  belongs to the import/discovery review path, which already reports it.
- **bounded identity matching.** Buckets use `(kind, normalized name)`, domain
  or install location. Provider equality is supplementary evidence, never a
  candidate by itself. A scan evaluates at most 10,000 distinct pairs; a larger
  result returns `total: None` and adapters identify it as partial. Buckets and
  pairs have stable ordering, and only the requested page gets full summaries.
- **deterministic pairs.** Every pair is ordered by canonical Asset id, so
  `A/B` and `B/A` are one candidate, and every evidence string is taken from
  the canonically smaller asset so the two storage adapters — which order rows
  differently — report the same candidate.
- **case rules follow the storage.** `provider` is free text and is compared
  case-insensitively; `domain_name` is normalized to lowercase by
  `ServiceRecord::validate`, so an exact comparison is already
  case-insensitive; `install_location` is a path stored verbatim and is
  compared exactly, because a path's case is not assumed insignificant.
- **lifecycle.** Merged tombstones are never candidates. Archived assets are
  reviewable by default, because merging an archived duplicate into a live
  survivor is a legitimate flow (ADR 0005); `include_archived: false` opts out.
- **no dismissal persistence.** Phase 4 adds no `duplicate_reviews` or
  `dismissed_duplicates` table. Candidates are ephemeral: a durable dismissal
  is a new canonical concept with its own identity and portability semantics,
  and no use case requires it yet.

## Adapter contract and verification

CLI and Desktop translate input into these application queries. Typed transport
DTOs map from application DTOs rather than SQLite rows; domain types do not depend
on Tauri annotations. See [Desktop Contract](12-desktop-contract.md) and
[CLI usage](../DEVELOPMENT.md#cli-usage).

Queries remain read-only and bounded, with deterministic ordering and one read
snapshot. Mutation, migration and portable-data requirements remain in the owning
module/storage contracts. Test ownership is maintained once in
[DEVELOPMENT.md](../DEVELOPMENT.md#test-ownership).
