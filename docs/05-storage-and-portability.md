# Storage & Portability

## Storage philosophy

SQLite is the operational database, not the only representation of the user's data.

AssetMesh must avoid creating a new proprietary data trap.

## SQLite responsibilities

SQLite stores:

- assets;
- typed module details;
- external references/aliases;
- relations;
- tags and collections;
- activity events;
- attachment metadata and blob references;
- search projections/index metadata;
- provider aliases/cache metadata where appropriate;
- application settings that are not secrets.

Provider cache and generated indexes are explicitly rebuildable and are not canonical merely because they happen to live in SQLite.

## SQLite concurrency contract

SQLite connections must share one initialization policy and migration level.

Baseline expectations:

```text
foreign_keys = ON
journal_mode = WAL
busy_timeout = bounded value
```

Transactions must stay short; external I/O must happen outside write transactions. See ADR 0007.

## Concurrency & consistency

- Write scopes (`UnitOfWorkFactory::transact`) run on a single writer
  connection in an IMMEDIATE transaction and commit/rollback atomically.
- Read scopes (`UnitOfWorkFactory::read`) expose only reader capabilities
  (mutation methods are unreachable at the type level) and run in a deferred,
  `PRAGMA query_only` transaction on a pooled reader connection: every read
  scope sees one consistent snapshot, concurrent writers cannot tear
  multi-query reads, and WAL lets readers and the writer proceed without
  blocking each other. (In-memory databases have a single connection, so
  their reads serialize with writes.)
- Share the adapter with `assetmesh_storage_sqlite::SharedSqlite` (an
  `Arc` handle) — each scope locks only its own connection.
- Migrations run inside one immediate transaction, so concurrent first
  launches (desktop + CLI) serialize safely instead of racing on the
  migration ledger. `journal_mode = WAL` is verified on file databases, and
  `SQLITE_BUSY`/`SQLITE_LOCKED` map to the retryable `StorageBusy` error.
- Opening a database whose module schema version is not exactly the current
  one fails loudly (`module_metadata` is validated on open); older semantics
  require an explicit module migration, never silent interpretation.
- Connection initialization installs the busy timeout before the WAL switch
  and retries bounded initialization contention, so concurrent first launch
  (desktop + CLI) is reliable under test with 16 simultaneous openers.

## Portable export

```text
assetmesh-export/
├── manifest.json
├── assets.jsonl
├── external_refs.jsonl
├── activity.jsonl
├── tags.json            # JSON array (tags are the only non-JSONL file)
├── asset_tags.jsonl
├── relations.jsonl
└── modules/
    ├── media.jsonl
    ├── software.jsonl
    ├── services.jsonl
    └── info.jsonl
```

- Rows are dedicated `*V1` wire DTOs (in `application/portable.rs`), not the
  mutable domain structs — internal refactors cannot silently change format
  version 1. A checked-in fixture in `core/tests/export_tests.rs` pins the
  historical representation.
- Search projections and provider cache are excluded. Ordinary exports omit Info
  API key assets and associated rows unless explicitly requested; complete
  recovery copies include them. See [Secrets](#secrets).
- Module sections are governed by manifest declaration: a section the
  manifest declares is required, count-checked, and authoritative on import
  (destination state for bundled assets that the bundle no longer carries is
  reconciled away). A Phase 1 bundle predates Software/Relations: absent
  sections import cleanly as empty and leave destination data of that kind
  untouched, while a section file present WITHOUT its manifest declaration
  fails loudly. The software section additionally enforces
  kind/category compatibility; relations enforce endpoint existence,
  no self-relations, and unique triples (symmetric `related_to` is stored in
  canonical endpoint order); services enforce kind ↔ service type
  compatibility.
- Every import (dry-run or commit) runs the same preflight: all declared
  files must be present, decoded row counts must match the manifest,
  identities (asset/media/software/ref/event/tag/relation ids, ref pairs,
  relation triples) must be unique, and the reference graph must be
  internally consistent (module details and refs point at bundled assets,
  kind/media-type/category compatibility, no dangling memberships, events, or
  relations, merge redirects exist, are not self-referential and are
  acyclic).
- Restore is deterministic and **authoritative for every asset the bundle
  contains**: rows upsert by canonical AssetMesh ID, and destination module
  state the bundle no longer carries (e.g. a merged tombstone's old media
  details and search document) is removed, keeping the destination
  equivalent to the bundle. External refs cannot be re-pointed at a
  different asset (fails loudly); a second restore is a no-op.
- Dry-run computes its dispositions from the same destination snapshot and
  the same checks as commit: identical counts (media create/update is judged
  by media-record existence, refs/activity/tags against destination ids and
  names), and it fails exactly when commit would — including destination
  ref-ID collisions and identity conflicts.
- Projections after import are built from the destination's actual canonical
  state (post-remap tags, pre-existing refs), so remapped tags and retained
  aliases are searchable without a rebuild.
- Writing a bundle uses a crash-recoverable swap protocol with deterministic
  names (`<target>.swap/` staging): an interrupted export leaves the
  previous complete bundle recoverable, and the next read or write performs
  the recovery. Files are fsynced before the swap (std cannot fsync
  directories; power-loss durability is therefore best-effort, process-crash
  recovery is guaranteed).
Filesystem bundle I/O (`write_bundle_to_directory` / `read_bundle_from_directory`)
lives in core as a std-only reference adapter. Its extraction is deferred until a
real additional consumer needs a separate adapter. Attachments/blobs are not yet
implemented; their ownership contract remains in ADR 0009.

Production exports stream ordered rows from one SQLite snapshot into private
staging files, sync them, then replace the bundle atomically. Limits are 256 KiB
for the manifest, 32 MiB per section, 128 MiB per bundle, 1 MiB per JSONL row and
100,000 records across sections. Import checks metadata before reading, enforces
the limits again while reading/parsing, rejects nonregular/outside-symlink files,
and validates counts/identities/references before mutation. Preflight retains a
bounded bundle in memory. Destination probes inspect only incoming identities;
relation reconciliation examines at most 100,000 related destination rows.
Section-presence reconciliation uses sets rather than repeated scans.

## Manifest

Example fields:

```json
{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "...",
  "app_version": "...",
  "modules": {
    "media": { "schema_version": 1 },
    "software": { "schema_version": 2 }
  },
  "record_counts": {},
  "attachments_included": true
}
```

Top-level export format version and module schema versions are independent. See ADR 0008.

## Schema migrations

Database schema, portable export format, and module data versions are separate concerns.

Rules:

- database migrations may change frequently;
- portable export versions should change conservatively;
- module semantic schemas evolve independently through explicit versions;
- importers should preserve unknown optional metadata where feasible;
- unsupported future versions fail clearly before canonical mutation;
- migration code must be testable on real legacy fixtures;
- module migrations must not directly mutate another module's private tables without an explicit coordinated migration.

## Stable identity

AssetMesh IDs survive export/import round trips.

External provider identifiers are exported as namespaced references, not substituted for canonical IDs. This supports deterministic re-import and cross-provider matching without coupling ownership to third parties.

Explicit merge redirects/tombstones should be exported when necessary to preserve identity history.

See ADR 0005.

## Backups

Before importing or migrating legacy personal data, create a timestamped backup of the original source when AssetMesh is authorized to modify it.

For read-only imports, never mutate the source.

Before applying risky database migrations, the implementation should support a recoverable local backup strategy appropriate for SQLite.

## Attachments and blobs

Canonical attachments are part of the user's durable data. Attachment metadata refers to stored blobs through a shared blob abstraction.

Suggested logical split:

```text
Attachment
  -> asset identity + role + filename + mime metadata

Blob
  -> content identity/storage key + size/integrity metadata
```

Content-addressed blob storage is preferred when implemented, because it enables integrity checking and deduplication without exposing arbitrary module-specific file paths as the storage contract.

Deleting attachment metadata must not blindly delete a blob that is still referenced elsewhere. Blob cleanup should be a safe garbage-collection operation.

See ADR 0009.

## Secrets

Services store only non-secret metadata. Info currently supports user-entered API
key values and preserves those values in complete SQLite/portable recovery
copies (docs/13). Ordinary exports require an explicit choice to include keys.
Key values are omitted from search and masked by default in
the desktop detail view, but they are **not encrypted at rest**.

Recovery directories use private filesystem permissions on Unix. External copies
containing keys belong on encrypted storage. Keychain references or application
encryption remain a separate future credential-storage change; neither masking
nor private permissions should be described as encryption.

## Provider cache separation

External metadata such as posters, API responses, extracted application icons, generated thumbnails, and discovery snapshots must be distinguishable from user-authored/canonical data.

The user should be able to delete/rebuild caches without losing their library.

If the user explicitly chooses to keep provider content as durable personal data, the application promotes it into a canonical attachment rather than silently changing cache semantics.

`AssetExternalRef.metadata` is local provider cache. Portable exports omit it,
and older bundles that include it are accepted with that cache value discarded.
Canonical external reference identifiers and source URLs still round-trip.

## Search index separation

Search documents and FTS indexes are derived projections. They are not included as canonical portable data because they can be rebuilt from assets, module details, tags, aliases, and related canonical records.

See ADR 0006.

## Recovery principle

A durable AssetMesh installation should be reconstructable from:

```text
canonical portable export
+ canonical attachment blobs
+ optional secrets reconfiguration
```

It must not require provider cache, search indexes, runtime discovery snapshots, or a specific historical SQLite page layout to recover the user's library.
