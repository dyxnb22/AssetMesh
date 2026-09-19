# ADR 0002: SQLite as operational store

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh is primarily a single-user local application and needs transactions, indexing, migrations, filtering, and reliable storage without operating a database server.

## Decision

Use SQLite as the primary operational database.

Portable export remains a separate product contract so SQLite is not the only representation of user-owned data.

## Consequences

Positive:

- zero-service deployment;
- transactional local storage;
- strong fit for desktop and CLI;
- straightforward backup.

Costs:

- future multi-device concurrent writes would require additional synchronization design;
- schema migrations must be carefully tested.
