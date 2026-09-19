# Foundation Design

## Objective

The foundation should let future modules reuse boring infrastructure without forcing them into the same domain model.

AssetMesh should keep the kernel small. Shared infrastructure exists only for concerns that multiple modules genuinely need.

## Foundation capabilities

### Identity

- UUID/ULID-style stable IDs generated in the application layer.
- IDs must survive export/import round trips.
- External provider IDs are aliases, never canonical primary keys.
- External aliases/references use a namespace + external ID contract.
- Asset merges are explicit use cases; heuristic similarity alone never silently rewrites identity.

See ADR 0005.

### Time

- Store timestamps in UTC.
- Display in the user's locale/timezone.
- Use an injectable clock in application services where useful for testing.

### Revisions

A lightweight monotonic `revision` or equivalent may be maintained on canonical assets where useful for optimistic concurrency, stale-write detection, and future evolution.

This is **not** a commitment to multi-device sync, CRDTs, or event sourcing in V1.

### Transactions

A use case that changes canonical state and writes its corresponding activity event should commit atomically when practical.

Example:

```text
Complete media
  1. update media status
  2. set completed_at
  3. append media.completed activity
  4. update search projection if synchronous
  5. commit
```

Do not hold database transactions open during:

- network/provider requests;
- filesystem or application scans;
- user prompts/confirmation;
- expensive rendering or parsing.

Bulk operations should use bounded transactions/batches when appropriate.

### SQLite concurrency

All SQLite entry points share one initialization policy:

```text
foreign_keys = ON
journal_mode = WAL
busy_timeout = bounded value
```

Write transactions should remain short. Desktop and CLI may directly share the local database in V1. If real contention or background-worker coordination later requires a single writer, a local daemon may be introduced as another adapter without changing domain/application logic.

See ADR 0007.

### Search

Search is a rebuildable projection over canonical module data, not a substitute for domain repositories.

Each module produces a shared `SearchDocument` containing fields such as:

```text
asset_id
kind
title
subtitle
body
keywords
updated_at
```

V1 should use deterministic local search without requiring an external search service.

Recommended progression:

1. normalized indexed fields for exact filters;
2. SQLite FTS5 for text search;
3. trigram/sub-string support when needed for languages or aliases that do not tokenize well with ordinary word boundaries.

Structured filters such as media status or software install source remain typed application queries.

See ADR 0006.

### Import

Import must be:

- previewable;
- validated before commit;
- idempotent where possible;
- able to report duplicates and conflicts;
- backed up before destructive migration of legacy data.

A durable import design should treat import as a session rather than a blind loop:

```text
ImportSession
  -> parse
  -> candidates
  -> match existing assets
  -> decisions/conflicts
  -> canonical commit
  -> report
```

The first Media importer may implement only the minimum subset required, but the boundaries above should remain visible.

External refs should be preferred over fuzzy matching when identifying existing assets.

### Export

Portable export is a first-class product feature, not a debugging tool.

See `05-storage-and-portability.md`.

### Activity

Modules publish semantic activity events through a shared application-level interface.

Activity records meaningful user-asset lifecycle changes. It is not an operational debug log and does not make AssetMesh event-sourced.

### Attachments and blobs

The foundation distinguishes:

- canonical user-owned attachments;
- rebuildable provider/derived cache.

Canonical binary data is addressed through a blob abstraction so modules do not invent arbitrary storage paths. Cache can be deleted/rebuilt without affecting the user's durable library.

The attachment subsystem does not need to be implemented for Media V1 unless required by real migration data, but its ownership boundary is fixed by ADR 0009.

### Settings

Split settings into:

- application preferences;
- provider configuration;
- secrets/credentials.

Secrets should not live in ordinary SQLite metadata when platform keychain storage is available.

### Logging

Operational logs and user activity are different:

- **logs** diagnose software behavior;
- **activity events** describe meaningful changes to user assets.

Do not mix them.

### Jobs

Long-running work such as discovery, reindexing, provider metadata enrichment, thumbnail generation, large import/export, or runtime scans should eventually run through a background-job boundary rather than blocking UI requests.

V1 does **not** require a durable job queue. Do not implement one until a real long-running workflow needs it.

When introduced, a job system should be infrastructure/application support, not domain identity.

## Suggested workspace boundaries

```text
crates/
├── core/
│   ├── domain/
│   ├── application/
│   └── ports/
├── storage-sqlite/
├── providers/
│   ├── macos/
│   └── metadata/
├── cli/
└── server/            # optional later
```

For the first implementation, fewer crates are acceptable. Architectural boundaries matter more than maximizing package count.

## Module schema ownership

Database schema version, portable export version, and module semantic schema version are separate concerns.

Each module owns evolution of its typed details and module export representation. Cross-module migrations must go through explicit shared contracts rather than reaching into another module's private tables.

See ADR 0008.

## Error model

Application errors should be typed enough for UI/CLI adapters to present useful feedback:

```text
ValidationError
NotFound
Conflict
ImportConflict
ProviderUnavailable
PermissionDenied
StorageBusy
StorageFailure
UnsupportedSchemaVersion
```

Infrastructure-specific errors should be mapped at adapter boundaries rather than leaking SQL driver or OS error details throughout the domain.

## Testing strategy

Prioritize:

1. domain invariants;
2. application use cases with in-memory/fake ports;
3. SQLite repository contract tests;
4. SQLite connection/concurrency configuration tests;
5. import/export round-trip tests;
6. migration tests against real historical fixtures;
7. search projection rebuild/consistency tests;
8. a small number of end-to-end vertical-slice tests.
