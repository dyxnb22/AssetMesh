# Storage & Portability

## Storage philosophy

SQLite is the operational database, not the only representation of the user's data.

AssetMesh must avoid creating a new proprietary data trap.

## SQLite responsibilities

SQLite stores:

- assets;
- typed module details;
- relations;
- tags and collections;
- activity events;
- provider aliases/cache metadata where appropriate;
- application settings that are not secrets.

## Portable export

A portable export should contain user-owned canonical data in an inspectable format.

Suggested layout:

```text
assetmesh-export/
├── manifest.json
├── assets.jsonl
├── relations.jsonl
├── collections.json
├── activity.jsonl
└── modules/
    ├── media.jsonl
    ├── software.jsonl
    └── services.jsonl
```

Provider caches should be excluded or explicitly separated from canonical user data.

## Manifest

Example fields:

```json
{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "...",
  "app_version": "...",
  "modules": ["media"],
  "record_counts": {}
}
```

## Schema migrations

Database schema and portable export format versions are separate concerns.

Rules:

- database migrations may change frequently;
- portable export versions should change conservatively;
- importers should preserve unknown metadata where feasible;
- migration code must be testable on real legacy fixtures.

## Backups

Before importing or migrating legacy personal data, create a timestamped backup of the original source when AssetMesh is authorized to modify it.

For read-only imports, never mutate the source.

## Secrets

Credentials, API keys, and tokens are not digital assets in the same sense as asset metadata. Store references to secrets, not plaintext secrets, in portable exports.

Use the OS keychain/secure store for secrets when implemented.

## Provider cache separation

External metadata such as posters, API responses, and discovery snapshots must be distinguishable from user-authored canonical data.

The user should be able to delete/rebuild caches without losing their library.
