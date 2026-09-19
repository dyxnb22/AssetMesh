# Architecture Decision Records

This directory records decisions that should remain explainable as AssetMesh evolves.

An ADR is used when changing the decision later would materially affect data ownership, module boundaries, portability, or deployment architecture.

## Accepted decisions

| ADR | Decision |
| --- | --- |
| [0001](0001-local-first-and-ui-independent-core.md) | Local-first product with a UI-independent core |
| [0002](0002-sqlite-as-operational-store.md) | SQLite is the operational store, not the only portable representation |
| [0003](0003-assets-plus-typed-module-details.md) | Shared Asset identity plus module-owned typed details |
| [0004](0004-deterministic-canonical-data.md) | AI/discovery cannot silently own canonical data |
| [0005](0005-external-references-and-merge-semantics.md) | AssetMesh owns identity; external refs are namespaced aliases; merges are explicit |
| [0006](0006-search-projection.md) | Global search uses a rebuildable shared Search Projection |
| [0007](0007-sqlite-concurrency-policy.md) | SQLite uses a shared concurrency/connection policy and short transactions |
| [0008](0008-module-schema-versioning.md) | DB, portable-export, and module schema versions are independent |
| [0009](0009-canonical-attachments-vs-provider-cache.md) | Canonical attachments are durable; provider/derived cache is rebuildable |

## Deferred decisions

The following are intentionally **not** architecture commitments yet:

- durable background-job implementation;
- multi-device synchronization protocol;
- CRDT model;
- mandatory local daemon;
- third-party plugin ABI/API;
- semantic/vector search;
- external search service;
- cloud account system.

These should receive their own ADR only when a real product requirement and at least one concrete implementation use case exist.

## ADR lifecycle

Preferred statuses:

- Proposed
- Accepted
- Superseded
- Rejected

Do not silently edit history to make an old decision appear as though it never existed. If a substantial accepted decision changes, create a new ADR and mark the previous decision as superseded where appropriate.
