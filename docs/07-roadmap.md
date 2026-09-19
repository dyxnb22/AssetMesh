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
- CLI or minimal test harness;
- desktop Media page after core behavior is stable.

Not required unless real migration data needs them:

- binary attachment implementation;
- durable job queue;
- external metadata providers;
- merge UI beyond import conflict review.

Exit criteria: historical media data can be imported, edited, searched, exported, and round-tripped safely, and the search index can be rebuilt from canonical data.

## Phase 2 — Application shell

Deliverables:

- desktop shell;
- global navigation;
- global library search using Search Projection;
- shared list/detail patterns;
- settings;
- activity page;
- import/export UI;
- typed Rust <-> UI transport contracts rather than duplicated handwritten payload shapes where practical.

## Phase 3 — Software inventory

Deliverables:

- Software asset kind;
- macOS application discovery;
- Homebrew/CLI discovery;
- namespaced identifiers such as bundle ID / Homebrew identity;
- install source and location;
- purpose / “why installed” field;
- manual confirmation of discovered assets;
- exact external-ref matching before heuristic matching;
- basic relations such as `installed_via`, `uses`, and `depends_on`;
- search projection and module migration fixtures.

## Phase 4 — Services and subscriptions

Deliverables:

- API/SaaS/local-service/VPS/domain kinds;
- endpoint/provider/account metadata without secret leakage;
- renewal/cost metadata where useful;
- provider/external references;
- relations between software, projects, and services;
- search projection and portable module schema.

## Phase 5 — Relationship graph

Deliverables:

- relation explorer;
- graph visualization;
- relation registry/inverse rendering;
- filters by relation/asset kind;
- impact view (“what depends on this?”);
- relation suggestions from discovery;
- explicit duplicate/merge review where useful.

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
