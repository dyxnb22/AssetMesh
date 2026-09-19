# Foundation Design

## Objective

The foundation should let future modules reuse boring infrastructure without forcing them into the same domain model.

## Foundation capabilities

### Identity

- UUID/ULID-style stable IDs generated in the application layer.
- IDs must survive export/import round trips.
- External provider IDs are aliases, never canonical primary keys.

### Time

- Store timestamps in UTC.
- Display in the user's locale/timezone.
- Use an injectable clock in application services where useful for testing.

### Transactions

A use case that changes canonical state and writes its corresponding activity event should commit atomically when practical.

Example:

```text
Complete media
  1. update media status
  2. set completed_at
  3. append media.completed activity
  4. commit
```

### Search

V1 should support deterministic local search without requiring an external search service.

Start with SQLite FTS5 if available and useful; otherwise begin with indexed normalized fields and introduce FTS once the query model is stable.

### Import

Import must be:

- previewable;
- validated before commit;
- idempotent where possible;
- able to report duplicates and conflicts;
- backed up before destructive migration of legacy data.

### Export

Portable export is a first-class product feature, not a debugging tool.

See `05-storage-and-portability.md`.

### Activity

Modules publish semantic activity events through a shared application-level interface.

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

## Error model

Application errors should be typed enough for UI/CLI adapters to present useful feedback:

```text
ValidationError
NotFound
Conflict
ImportConflict
ProviderUnavailable
PermissionDenied
StorageFailure
```

Infrastructure-specific errors should be mapped at adapter boundaries rather than leaking SQL driver or OS error details throughout the domain.

## Testing strategy

Prioritize:

1. domain invariants;
2. application use cases with in-memory/fake ports;
3. SQLite repository contract tests;
4. import/export round-trip tests;
5. a small number of end-to-end vertical-slice tests.
