# Contributing

AssetMesh has completed its Phase 1 Media Records vertical slice and is now expanding the headless core across additional asset domains before building the desktop UI.

Development principles:

1. Prefer a complete vertical slice over broad scaffolding.
2. Keep domain/application logic independent of UI frameworks.
3. Treat CLI, desktop, HTTP, and agent integrations as adapters over the same application layer.
4. Do not introduce a new provider abstraction until at least one concrete use case needs it.
5. Add migration and import/export tests for user-owned data changes.
6. Treat data portability and backward compatibility as product features.
7. Keep AI/discovery advisory; canonical state must remain explainable.
8. Do not move business rules, identity semantics, matching logic, merge behavior, or search semantics into presentation code.

The completed first implementation target is `docs/08-media-records-v1.md`.

The current implementation target is **Phase 2 — Software Inventory vertical slice** in `docs/07-roadmap.md`. Phase 2 should remain usable through headless application services and CLI/test adapters; graphical presentation is intentionally deferred until Phase 5.
