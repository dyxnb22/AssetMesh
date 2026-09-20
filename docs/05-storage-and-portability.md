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

## Portable export

A portable export contains user-owned canonical data in an inspectable format.

Suggested layout:

```text
assetmesh-export/
├── manifest.json
├── assets.jsonl
├── external-refs.jsonl
├── relations.jsonl
├── collections.json
├── activity.jsonl
├── attachments.jsonl
├── blobs/                  # included for a complete portable-library export
└── modules/
    ├── media.jsonl
    ├── software.jsonl
    └── services.jsonl
```

Provider caches, generated search indexes, discovery snapshots, and other rebuildable data are excluded by default.

The implemented bundle (see DEVELOPMENT.md) uses underscores in file names
(`external_refs.jsonl`), and module sections are governed by manifest
declaration: `modules/media.jsonl`, `modules/software.jsonl`, and
`modules/services.jsonl` each carry their `modules.<name>.schema_version`;
`relations.jsonl` is declared by its `record_counts` entry. An undeclared
section means the bundle predates that capability and imports as empty, leaving
destination data of that kind untouched (docs/09 portable data policy).

AssetMesh may later provide lighter export modes that omit canonical blobs, but such exports must be clearly labeled as metadata-only and must not pretend to be a complete portable copy.

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

Credentials, API keys, and tokens are not digital assets in the same sense as asset metadata. Store references to secrets, not plaintext secrets, in portable exports.

Use the OS keychain/secure store for secrets when implemented.

## Provider cache separation

External metadata such as posters, API responses, extracted application icons, generated thumbnails, and discovery snapshots must be distinguishable from user-authored/canonical data.

The user should be able to delete/rebuild caches without losing their library.

If the user explicitly chooses to keep provider content as durable personal data, the application promotes it into a canonical attachment rather than silently changing cache semantics.

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
