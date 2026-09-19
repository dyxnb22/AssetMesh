# ADR 0007: SQLite concurrency policy

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh is local-first and may eventually have more than one local entry point touching the same data: desktop app, CLI, import tools, and potentially an MCP or HTTP adapter. SQLite is appropriate, but careless multi-process access can produce lock contention, long write transactions, and inconsistent connection configuration.

## Decision

SQLite remains the operational store, with an explicit concurrency policy.

### Connection configuration

Every AssetMesh SQLite connection should enable or verify:

```text
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = <bounded value>;
```

The implementation may also choose appropriate `synchronous` and checkpoint settings after measuring real workloads. They are not hard-coded in this ADR.

### Transaction policy

- Keep write transactions short.
- Do not perform network calls, filesystem scanning, metadata downloads, or user interaction while a write transaction is open.
- A use case may stage external work first, then open a transaction only for canonical writes.
- Canonical state changes and their semantic activity event should commit atomically when they belong to the same use case.
- Bulk imports should use bounded batches when necessary rather than one unbounded transaction.

### Write ownership

V1 may allow the desktop app and CLI to open the database directly through the same storage adapter.

If real contention, remote access, or background-worker coordination becomes significant, AssetMesh may introduce a local daemon as a single writer/coordinator:

```text
Desktop ─┐
CLI ─────┼─> local AssetMesh service ─> Core ─> SQLite
HTTP/MCP ┘
```

This must be an adapter/deployment change, not a rewrite of domain/application services.

### Connection discipline

All entry points must use the same migration version and connection initialization code. No adapter may create ad-hoc SQLite connections with different pragmas or bypass repository contracts for canonical writes.

### Failure behavior

Lock timeouts and storage contention are typed storage/application failures. Callers should receive a useful retryable error rather than raw driver strings.

## Consequences

- Desktop and CLI can initially share one local database safely for normal personal workloads.
- The system has a migration path to a coordinating daemon without changing the core model.
- Long-running provider/discovery work cannot accidentally hold database locks.

## Non-goals

- distributed database access;
- network filesystem support guarantees;
- multi-user server concurrency in V1.
