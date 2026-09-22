# Unified Library Core — Phase 4

Status: **Phase 4 — Unified Library Core: complete.** Phase 1 Media Records, Phase 2 Software Inventory, Phase 3 Services and Subscriptions, and Phase 4 are complete as headless vertical slices. Phase 4 did not add another asset domain; it stabilizes the cross-module application contract that Phase 5 Desktop will consume.

```text
Phase 4A — Unified Library Query .......................... complete
Phase 4B — Relation Traversal & Impact .................... complete
Phase 4C — Global Search / Activity / Duplicate Review .... complete
Phase 4D — Contract Hardening ............................. complete

Phase 5 — Application Shell / Desktop UI ................. complete (P5-01 through P5-10, R1-R6)
Phase 6 — Runtime Enrichment ............................. next
```

## Purpose

Phase 4 turns three working module verticals into one coherent asset library.

The core question is no longer “can Media, Software, and Services coexist?” That has been proven. The Phase 4 question is:

> Can an interface adapter operate AssetMesh as one library — list assets, open a detail view, search, inspect relationships, answer dependency/impact questions, review duplicates, and query activity — without knowing module repository details or reimplementing business rules?

The answer must be yes before the desktop shell is allowed to define presentation patterns.

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

### Required application service

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
}

pub enum AssetDetails {
    Media(MediaRecord),
    Software(SoftwareRecord),
    Service(ServiceRecord),
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
- no N+1 repository pattern: one unified read opens exactly one
  `QueryUnitOfWork` and calls each module reader at most once (module list
  rows already carry their tags);
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

The unified list is composed in the application layer from the existing module
readers — no new port method, no new SQL, no migration. For each selected
module the library calls the module reader once; module list rows already
carry their tags, because the repository resolves a whole page's tags in one
query rather than one per row, so tags never become a per-asset query. A
`--module` filter also narrows which module tables are read at all.

The unified list originally loaded rows in memory during Phase 4A.
In Phase 5 remediation (P5-04 / R4), this was pushed down to the storage seam
via [`LibraryReadPort`](crate::ports::repos::LibraryReadPort) on `QueryUnitOfWork`:
- Storage implementations (such as `SqliteLibraryRepo`) perform SQL-level `LIMIT :limit OFFSET :offset`
  and two-stage counting directly in the database engine;
- Tag hydration and module subtitles are loaded only for the returned page slice;
- Batch tag and relation queries enforce safe parameter chunking (`<= 500`);
- A 50k-scale performance gate (`scale_benchmark.rs`) proves constant memory footprint and bounded sub-second query latency under deep pagination.

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
- one query reads the events and the asset-name index in a single
  `QueryUnitOfWork`, so a page cannot mix two moments of history;
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
- **bucketed, not pairwise.** Candidates come from buckets keyed by
  `(kind, normalized name)` and by module-specific deterministic keys, so the
  cost is bounded by the largest bucket rather than by the square of the
  library size.
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

## Phase 4D — Contract Hardening

**Status: complete.**

### Goal

Treat the Phase 4 application surface as the contract Phase 5 Desktop can rely on without bypassing it.

The intended dependency shape is:

```text
CLI ---------\
Desktop ------> Application / Unified Library Core -> Domain + Ports
HTTP --------/
MCP --------/
```

HTTP and MCP implementations are **not** Phase 4 deliverables. They are used as architecture tests: the application contract should be transport-neutral enough that adding those adapters later does not require moving business logic out of core.

### CLI verification

The CLI exercises the same Phase 4 application services the desktop will call.
The implemented surface:

```text
assetmesh library list|get|search            # Phase 4A
assetmesh relation neighbors <id>            # Phase 4B
assetmesh relation dependencies <id> [--depth N]
assetmesh relation dependents <id>   [--depth N]
assetmesh relation impact <id>       [--depth N]
assetmesh relation traverse <id> --direction outgoing|incoming|both \
                                  --type <relation-type> --depth N [--include-archived]
assetmesh activity list [--asset ID] [--type T] [--module M] \
                        [--actor A] [--since D] [--until D] [--limit N] [--offset N]
assetmesh duplicates list [--kind K] [--active-only] [--limit N] [--offset N]
```

Every one of them only parses arguments, builds an application query, calls the
service, and formats the result — no CLI-side traversal, inverse derivation,
module dispatch, duplicate matching, or lifecycle rule. `relation add`,
`relation list`, and `relation remove` keep their Phase 2/3 behaviour.

Merging is still `assetmesh asset merge <loser> <winner>`; there is
deliberately no `duplicates fix-all`.

### Transport/view boundary

Application DTOs may be mapped into adapter-specific transport DTOs, but domain types must not gain Tauri/HTTP/MCP annotations just to avoid mapping code.

If generated Rust ↔ TypeScript contracts are adopted in Phase 5, they should be generated from or mapped from this stable application boundary rather than from SQLite rows.

## Testing requirements

Phase 4 is complete with cross-module coverage in three layers:

- **core use cases** (in-memory port doubles):
  - `tests/library_use_cases.rs` — unified list/detail/search, ordering,
    pagination, lifecycle, merged tombstones, tags, empty library, kind/module
    filters, a filtered-search pagination regression, and a probe asserting one
    unified read opens exactly one `QueryUnitOfWork` and uses no repository
    capability more than once (the N+1 guard).
  - `tests/relation_query_use_cases.rs` — single/inverse/symmetric edges,
    multi-hop, diamond, cycle in both directions, depth 0 / bound / clamp,
    type filters from both ends, archived opt-in and non-traversal, merged
    tombstones as node and as root, a detail-less node, one-snapshot and
    one-query-per-frontier probes, and read-only assertions.
  - `tests/activity_duplicate_use_cases.rs` — module classification of every
    known event type, cross-module ordering with an event-id tie-break, all
    five filters, archived/merged history, pagination; and duplicate
    no-candidates, single candidate, pair dedup with merged evidence,
    same-name-different-kind, case/whitespace normalization, all three
    module-specific evidence kinds, archived policy both ways, merged-tombstone
    exclusion, row-order independence, deterministic re-scan, the 5→10-pair
    bucket walk, and read-only assertions.
- **SQLite contract parity** (the real adapter, mirroring the core scenarios):
  `storage-sqlite/tests/library_query_contracts.rs`,
  `relation_query_contracts.rs`, `activity_duplicate_contracts.rs`.
- **CLI end-to-end** (the compiled binary): `cli/tests/cli_e2e.rs` — the
  Phase 4A library commands plus
  `unified_core_works_end_to_end_across_modules`, one fixture of Media +
  2 Software + 2 Services with cross-module relations, a renewal, an archive
  and a merge, driving library/search/traversal/impact/activity/duplicates and
  then export → import → rebuild to prove the derived views survive.

Normal quality gates remain:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Explicit non-goals

Phase 4 does not include:

- Tauri or React UI;
- desktop navigation/layout;
- graph visualization;
- React list/detail components;
- Docker/OrbStack/process/port monitoring;
- health checks or logs;
- start/stop/restart/uninstall actions;
- HTTP server;
- MCP server;
- semantic/vector search;
- AI-generated canonical data;
- plugin SDK/ABI;
- background-job framework without a concrete long-running requirement;
- multi-device sync/CRDTs;
- cloud account/login system.

These remain in later phases or explicitly deferred.

## Exit criteria

Phase 4 is complete when an interface adapter can use only stable application contracts to:

1. list and page across Media, Software, and Services as one library;
2. open a complete typed asset detail view;
3. search globally with the same summary/view vocabulary;
4. inspect incoming/outgoing relations;
5. answer bounded dependency/dependent/impact queries with explainable paths;
6. query cross-module activity;
7. review duplicate evidence and invoke the existing explicit merge workflow;
8. perform all of the above without direct repository/SQLite access and without reimplementing module business rules.

**All eight criteria are met.** An adapter can operate the whole library through
`LibraryService`, `RelationQueryService`, `ActivityService`, and
`DuplicateReviewService` — with `AssetService::merge_assets` as the only
canonical write in the review flow — without direct repository or SQLite
access and without reimplementing module business rules.

At that point Phase 5 may build the desktop application as an adapter over the proven contract rather than discovering backend semantics inside React/Tauri code.
