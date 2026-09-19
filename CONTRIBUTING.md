# Contributing

AssetMesh is currently in architecture/bootstrap phase.

Development principles:

1. Prefer a complete vertical slice over broad scaffolding.
2. Keep domain/application logic independent of UI frameworks.
3. Do not introduce a new provider abstraction until at least one concrete use case needs it.
4. Add migration and import/export tests for user-owned data changes.
5. Treat data portability and backward compatibility as product features.
6. Keep AI/discovery advisory; canonical state must remain explainable.

The first implementation target is `docs/08-media-records-v1.md`.
