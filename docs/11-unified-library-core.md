# Unified Library Core — Phase 4

Status: **current implementation target**. Phase 1 Media Records, Phase 2 Software Inventory, and Phase 3 Services and Subscriptions are complete as headless vertical slices. Phase 4 does not add another asset domain; it stabilizes the cross-module application contract that Phase 5 Desktop will consume.

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

### Required application service

Introduce a unified library query boundary (name may evolve, concept may not), for example:

```text
LibraryService
  get_asset(id)
  list_assets(query)
  search_assets(query)
```

`get_asset` returns one complete application view from one consistent read snapshot.

### Core DTOs

The exact Rust names may evolve, but the contract should preserve these meanings:

```rust
pub struct AssetSummary {
    pub id: AssetId,
    pub kind: AssetKind,
    pub name: String,
    pub lifecycle: LifecycleState,
    pub tags: Vec<String>,
    pub subtitle: Option<String>,
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
    pub relations: Vec<RelationView>,
}
```

A future module extends `AssetDetails`; it must not require clients to read module-private tables.

### Unified query contract

Define shared query primitives rather than adding presentation-specific query parameters later:

```text
LibraryQuery
  kinds/modules
  lifecycle
  tags
  text (optional)
  sort
  page

PageRequest
  limit
  cursor or offset

Page<T>
  items
  next cursor / page metadata
```

Requirements:

- filtering by lifecycle and asset kind/module;
- tag filtering;
- deterministic sorting;
- bounded page size;
- stable pagination semantics;
- merged tombstones excluded from the normal library by default;
- archived assets included only when requested (or by an explicitly documented library mode);
- no N+1 repository pattern for ordinary list pages;
- search results and normal list rows map into the same `AssetSummary` vocabulary rather than creating competing UI models.

### Cross-module lookup

Canonical Asset ID is the primary lookup key. Existing namespaced external-reference identity rules remain authoritative for exact external lookup. Phase 4 must not introduce a second identity system for the unified library.

## Phase 4B — Relation Traversal and Impact

### Goal

Promote the existing relation write/read primitives into useful cross-module graph queries.

The current canonical relation model is retained: inverse-pair relations are stored in one primary direction, symmetric relations in canonical endpoint order, and inverse semantics are resolved for the caller at view time.

### Required queries

The application layer should provide equivalents of:

```text
neighbors(asset_id, filter)
incoming(asset_id, filter)
outgoing(asset_id, filter)
dependencies(asset_id, options)
dependents(asset_id, options)
traverse(asset_id, options)
impact(asset_id, options)
```

`dependencies` and `dependents` are semantic convenience queries over relation types whose meaning supports dependency traversal. They do not reinterpret arbitrary `related_to` edges as dependencies.

### Traversal options

At minimum:

```text
TraversalOptions
  direction
  relation_types
  max_depth
  include_archived
```

Rules:

- breadth-first traversal by default;
- visited-set cycle protection;
- root asset never reappears as its own descendant;
- deterministic ordering within each depth;
- bounded `max_depth` with a conservative application maximum;
- merged tombstones resolve or are excluded according to the existing redirect semantics, never treated as independent live graph nodes;
- read-only: traversal never repairs, creates, or deletes relations.

### Impact result

Impact is an explainable graph query, not a risk score. It should return enough evidence to answer “what depends on this?” and how each result was reached.

A result should preserve at least:

```text
asset
minimum depth
path / predecessor evidence
relation types used
```

Do not add an opaque AI/confidence/risk score in Phase 4.

## Phase 4C — Global Search, Activity, and Duplicate Review

### Global search

The existing Search Projection remains the search engine. Phase 4 adds the unified application contract around it.

Requirements:

- optional kind/module/lifecycle/tag filters;
- bounded pagination/limit;
- results map to `AssetSummary`;
- merged tombstones are not normal search hits;
- archived behavior stays explicit and consistent with the library query;
- CJK/substring fallback behavior remains an infrastructure/search contract, not UI code;
- search remains derived state and rebuildable from canonical data.

Phase 4 does **not** add semantic/vector search.

### Global activity query

Promote the existing activity repository capabilities into a stable application query service.

Support:

```text
recent activity
activity for one asset
filter by event type
filter by module/asset kind where derivable
filter by time range
bounded pagination
```

Activity is historical provenance. Query services must not synthesize events that were never recorded.

### Duplicate review

Add a read/review workflow around existing deterministic identity and explicit merge rules.

A duplicate candidate should carry **evidence**, for example:

- same exact namespaced external reference;
- same normalized name and compatible kind;
- same package/provider/domain identity according to a documented module rule;
- discovery/import evidence already classified as potential duplicate/conflict.

The application contract should support review dispositions such as:

```text
candidate -> merge explicitly
candidate -> dismiss / keep separate
```

Rules:

- duplicate detection never auto-merges;
- merge continues to use the existing explicit `merge_assets` command and module-specific conflict semantics;
- an opaque numeric “confidence” must not become the canonical decision rule;
- if a dismissal is persisted, its identity and portability semantics must be designed explicitly before adding storage. Phase 4 may initially keep duplicate candidates ephemeral if no durable dismissal use case is required.

## Phase 4D — Contract Hardening

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

The CLI should exercise the same Phase 4 application services that the desktop will call. Exact command spelling may evolve, but coverage should exist for the equivalent of:

```text
library list
library show <id>
library search <query>
relation list <id>
dependencies <id>
dependents <id>
impact <id>
activity list
duplicates list/review
```

Do not create CLI-only business logic to satisfy this requirement.

### Transport/view boundary

Application DTOs may be mapped into adapter-specific transport DTOs, but domain types must not gain Tauri/HTTP/MCP annotations just to avoid mapping code.

If generated Rust ↔ TypeScript contracts are adopted in Phase 5, they should be generated from or mapped from this stable application boundary rather than from SQLite rows.

## Testing requirements

Phase 4 is incomplete without cross-module tests. Add coverage for at least:

- one unified library page containing Media, Software, and Services;
- deterministic filtering/sorting/pagination across module boundaries;
- complete unified detail views without adapter-side repository joins;
- search returning the unified summary contract;
- incoming/outgoing relation views with inverse/symmetric semantics preserved;
- a cross-module dependency chain and transitive impact traversal;
- cycle handling and traversal depth limits;
- archived and merged lifecycle behavior in library/search/traversal queries;
- cross-module activity filtering/pagination;
- duplicate evidence that never mutates canonical data until an explicit merge command;
- merge conflicts leaving all canonical state unchanged;
- SQLite and in-memory/test-double behavior agreeing on the application contract;
- CLI end-to-end coverage over the same services.

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

At that point Phase 5 may build the desktop application as an adapter over the proven contract rather than discovering backend semantics inside React/Tauri code.
