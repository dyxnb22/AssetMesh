# Roadmap

## Phase 0 — Architecture baseline

Deliverables:

- product definition;
- domain model;
- storage and portability contract;
- module boundaries;
- architecture decision records;
- repository skeleton.

Exit criteria: Media Records can be implemented without inventing new cross-cutting architecture mid-feature.

## Phase 0.5 — Foundation hardening

Purpose: define the few long-lived contracts that are difficult to retrofit after multiple modules exist, without implementing speculative systems.

Deliverables:

- namespaced external references / aliases;
- explicit asset merge semantics and redirect/tombstone behavior;
- relation registry with inverse/symmetric semantics;
- shared rebuildable Search Projection contract;
- SQLite WAL / busy-timeout / short-transaction concurrency policy;
- module-owned schema versioning distinct from DB/export versions;
- canonical attachment/blob boundary vs provider cache;
- lightweight revision field reserved for stale-write/future evolution where useful.

Explicitly deferred:

- durable background job queue;
- multi-device sync;
- CRDTs;
- third-party plugin ABI;
- semantic/vector search;
- local daemon as mandatory architecture.

Exit criteria: Media, Software, and Services can reasonably grow on the same identity/search/storage contracts without changing the meaning of existing canonical data.

Relevant ADRs: 0005–0009.

## Phase 1 — Media Records vertical slice

Status: **complete as a headless vertical slice**. Desktop presentation is intentionally deferred to Phase 5.

Deliverables:

- Asset + MediaRecord schema;
- AssetExternalRef schema and matching path used by import where identifiers exist;
- SQLite migrations and shared connection initialization;
- CRUD application services;
- list/search/filter;
- Media -> SearchDocument projection;
- tags;
- progress/status/rating;
- activity events;
- JSON/CSV legacy import with preview;
- portable export with module schema version;
- CLI/test harness exercising the same application layer future adapters will use.

Not required unless real migration data needs them:

- binary attachment implementation;
- durable job queue;
- external metadata providers;
- merge UI beyond import conflict review.

Exit criteria: historical media data can be imported, edited, searched, exported, and round-tripped safely, and the search index can be rebuilt from canonical data without requiring a desktop client.

## Phase 2 — Software Inventory vertical slice

Purpose: validate that the shared AssetMesh kernel can support a second substantially different asset domain without introducing UI-specific assumptions or weakening the contracts established by Media Records.

Deliverables:

- Software asset kind and module-owned schema;
- Software CRUD application services;
- macOS application discovery;
- Homebrew / CLI tool discovery;
- namespaced identifiers such as bundle ID, Homebrew formula/cask identity, and package identity;
- install source and location metadata;
- purpose / “why installed” field;
- discovery snapshots kept separate from canonical asset state;
- manual confirmation / adoption of discovered software;
- exact external-reference matching before heuristic matching;
- duplicate/conflict reporting without implicit canonical merges;
- basic relations such as `installed_via`, `uses`, and `depends_on`;
- Software -> SearchDocument projection;
- portable Software module export/import;
- module migrations and fixtures;
- CLI commands or test harness covering all application services.

Explicitly deferred:

- desktop UI;
- graphical software inventory;
- runtime process monitoring;
- install/uninstall actions;
- automatic destructive reconciliation;
- durable background-job infrastructure unless discovery workloads prove it necessary.

Exit criteria: installed software and CLI tools can be discovered, reviewed, adopted into canonical AssetMesh records, searched, related to other assets, exported/imported, and rebuilt without requiring a desktop client.

## Phase 3 — Services and Subscriptions vertical slice

Deliverables:

- API / SaaS / local-service / VPS / domain asset kinds;
- service CRUD application services;
- endpoint/provider/account metadata without secret leakage;
- renewal/cost metadata where useful;
- namespaced provider/external references;
- relations between software, projects, accounts, domains, and services;
- search projection;
- portable module schema;
- CLI/test coverage.

Exit criteria: Media, Software, and Services coexist on the same shared Asset identity, search, activity, relation, migration, and portability contracts without cross-module table coupling.

## Phase 4 — Cross-module Relations and Library Core

Purpose: stabilize the capabilities needed by a unified asset library before committing to presentation-layer patterns.

Deliverables:

- relation query services;
- incoming/outgoing relation traversal;
- inverse and symmetric relation behavior;
- dependency / impact queries such as “what depends on this?”;
- cross-module asset lookup;
- global Search Projection queries;
- shared filtering / sorting / pagination contracts;
- duplicate and merge review application services;
- activity querying across modules;
- stable DTO/view-model boundaries suitable for CLI, desktop, HTTP, or agent adapters.

Explicitly deferred:

- graph visualization;
- desktop navigation;
- shared React list/detail components;
- visual relation explorer.

Exit criteria: the application layer exposes a stable unified-library contract over Media, Software, Services, Search, Relations, Activity, Import/Export, and Merge operations.

## Phase 5 — Application Shell / Desktop UI

Purpose: add the primary graphical client only after multiple asset domains and the unified library contract have proven which presentation patterns are genuinely shared.

Deliverables:

- Tauri desktop shell;
- global navigation;
- global asset library;
- global search using Search Projection;
- shared list/detail patterns;
- Media pages;
- Software pages;
- Services pages;
- relation explorer and graph visualization;
- activity page;
- import/export UI;
- duplicate/merge review UI;
- settings;
- typed Rust <-> UI transport contracts generated or shared where practical.

Architecture rule: the desktop client remains an adapter over the existing application layer. Business rules, identity semantics, migrations, discovery rules, search behavior, import matching, and merge behavior must not be reimplemented in the UI.

Exit criteria: all major AssetMesh V1 capabilities already available headlessly can be operated through the desktop application without duplicating domain or application logic.

## Phase 6 — Runtime enrichment

Optional capabilities for relevant software/service assets:

- running/stopped state;
- process/port mapping;
- Docker/OrbStack status;
- launchd awareness;
- logs/health checks;
- safe actions such as open/restart.

These capabilities remain subordinate to asset management.

If scans, metadata extraction, reindexing, or runtime inspection become long-running, introduce a background job boundary here or earlier when first required. Prefer a simple local implementation before adopting external queues.

## Phase 7 — Additional personal modules

Candidates:

- queue/tasks;
- learning records/cards;
- projects/workspaces;
- books/music;
- domains/VPS inventory;
- subscription lifecycle.

At this point reassess whether the internal module descriptor is mature enough to justify a third-party plugin API.

## Phase 8+ — Optional synchronization / remote access

Only after the local model is proven:

- optional HTTP/local daemon deployment mode;
- remote/mobile clients;
- synchronization protocol;
- tombstone/revision semantics hardened for multiple writers.

Do not introduce CRDTs or distributed sync infrastructure merely because the domain model reserves stable IDs/revisions.

## Explicit non-goals for V1

- cloud account/login system;
- multi-user collaboration;
- mobile native app;
- public plugin marketplace;
- AI-generated canonical data;
- complex live monitoring dashboard;
- automatic deletion/uninstallation of software;
- distributed sync or CRDT implementation;
- external search cluster.
