# Contributing

AssetMesh has completed its Phase 1 (Media Records), Phase 2 (Software
Inventory), Phase 3 (Services and Subscriptions), and Phase 4 (Unified Library
Core) headless vertical slices and Phase 5 (Tauri desktop application shell).
The next planned target is Phase 6 runtime enrichment; the desktop remains an
adapter over the Phase 4 application contract.

Development principles:

1. Prefer a complete vertical slice or complete application contract over broad scaffolding.
2. Keep domain/application logic independent of UI frameworks.
3. Treat CLI, desktop, HTTP, and agent integrations as adapters over the same application layer.
4. Do not introduce a new provider abstraction until at least one concrete use case needs it.
5. Add migration and import/export tests for user-owned data changes.
6. Treat data portability and backward compatibility as product features.
7. Keep AI/discovery advisory; canonical state must remain explainable.
8. Do not move business rules, identity semantics, matching logic, merge behavior, search semantics, relation traversal semantics, or duplicate-review decisions into presentation code.
9. Keep cross-module reads inside stable application query services rather than making adapters join module repositories themselves.
10. Prefer typed application DTOs over unstructured JSON at the core boundary.

Completed implementation targets: `docs/08-media-records-v1.md`,
`docs/09-software-inventory-v1.md`, `docs/10-services-subscriptions-v1.md`, and
`docs/11-unified-library-core.md` (Phase 4A-4D).

The next implementation target is **Phase 6 — Runtime Enrichment**. Start with
`docs/12-phases-5-7-execution-plan.md`; `docs/07-roadmap.md` contains the
phase-level roadmap, while `docs/11-unified-library-core.md` defines the stable
application contract the desktop adapter consumes.

Keep Tauri and React in `apps/desktop`; do not add
HTTP/MCP servers, semantic/vector search, runtime monitoring, plugin
infrastructure, or sync as part of the same change unless the roadmap is
explicitly revised first. Desktop commands must call application services and
must not access repositories or SQLite directly.
