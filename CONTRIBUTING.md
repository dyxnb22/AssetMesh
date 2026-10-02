# Contributing

AssetMesh is a personal local-first application with a working desktop client and
CLI. Start with [README.md](README.md) for the product and
[DEVELOPMENT.md](DEVELOPMENT.md) for setup, commands and test ownership.

Development principles:

1. Prefer a complete small workflow over broad scaffolding.
2. Keep domain/application logic independent of UI frameworks and storage drivers.
3. Desktop and CLI call application services; they do not assemble cross-module
   views by joining repositories or SQLite tables.
4. Keep identity, matching, merge, traversal and lifecycle semantics in the core.
5. Use typed application DTOs at adapter boundaries.
6. Add migration and round-trip coverage when changing user-owned data formats.
7. Treat portability and backward compatibility as product features.
8. Keep discovery advisory; canonical writes remain explicit and explainable.
9. Introduce abstractions and infrastructure only for a concrete implemented need.
10. Run checks appropriate to the change. Avoid tests that merely freeze constants,
    copy, CSS details or fake fixture counts.

The [Architecture](docs/02-architecture.md), [Domain Model](docs/03-domain-model.md),
[Storage & Portability](docs/05-storage-and-portability.md), module contracts and
[ADRs](docs/adr/README.md) define lasting boundaries. The
[Desktop Contract](docs/12-desktop-contract.md) describes adapter behavior.

The [Roadmap](docs/07-roadmap.md) lists optional follow-ups. Do not automatically
start runtime enrichment, add a new module or build HTTP/MCP servers, sync,
plugins, vector search or job infrastructure merely because an earlier phase plan
mentioned them.
