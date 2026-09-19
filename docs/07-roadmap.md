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

## Phase 1 — Media Records vertical slice

Deliverables:

- Asset + MediaRecord schema;
- SQLite migrations;
- CRUD application services;
- list/search/filter;
- tags;
- progress/status/rating;
- activity events;
- JSON/CSV legacy import;
- portable export;
- CLI or minimal test harness;
- desktop Media page after core behavior is stable.

Exit criteria: historical media data can be imported, edited, searched, exported, and round-tripped safely.

## Phase 2 — Application shell

Deliverables:

- desktop shell;
- global navigation;
- library search;
- shared list/detail patterns;
- settings;
- activity page;
- import/export UI.

## Phase 3 — Software inventory

Deliverables:

- Software asset kind;
- macOS application discovery;
- Homebrew/CLI discovery;
- install source and location;
- purpose / “why installed” field;
- manual confirmation of discovered assets;
- basic relations such as `installed_via` and `used_by`.

## Phase 4 — Services and subscriptions

Deliverables:

- API/SaaS/local-service/VPS/domain kinds;
- endpoint/provider/account metadata without secret leakage;
- renewal/cost metadata where useful;
- relations between software, projects, and services.

## Phase 5 — Relationship graph

Deliverables:

- relation explorer;
- graph visualization;
- filters by relation/asset kind;
- impact view (“what depends on this?”);
- relation suggestions from discovery.

## Phase 6 — Runtime enrichment

Optional capabilities for relevant software/service assets:

- running/stopped state;
- process/port mapping;
- Docker/OrbStack status;
- launchd awareness;
- logs/health checks;
- safe actions such as open/restart.

These capabilities remain subordinate to asset management.

## Phase 7 — Additional personal modules

Candidates:

- queue/tasks;
- learning records/cards;
- projects/workspaces;
- books/music;
- domains/VPS inventory;
- subscription lifecycle.

## Explicit non-goals for V1

- cloud account/login system;
- multi-user collaboration;
- mobile native app;
- public plugin marketplace;
- AI-generated canonical data;
- complex live monitoring dashboard;
- automatic deletion/uninstallation of software.
