# ADR 0009: Canonical attachments vs provider cache

- Status: Accepted
- Date: 2026-09-19

## Context

Digital assets may acquire files such as user notes, documents, screenshots, app icons, posters, cover art, thumbnails, and provider responses. Some of these files are user-owned durable data; others are derived or downloadable caches.

Treating both classes the same would make backup/export semantics unclear and risks either losing user data or bloating portable exports with rebuildable cache files.

## Decision

AssetMesh distinguishes canonical attachments from provider/derived cache.

### Canonical attachment

A canonical attachment is intentionally attached to an asset and is part of the user's durable library.

Examples:

- a user-added PDF;
- a manually selected cover image that should survive provider removal;
- a screenshot or document the user explicitly keeps with an asset.

Suggested logical model:

```text
Attachment
- id
- asset_id
- blob_id
- role
- filename
- mime_type
- size
- created_at
- metadata?
```

### Blob storage

Binary content is addressed separately from attachment metadata.

Suggested direction:

```text
Blob
- id or content_hash
- size
- storage_key
- created_at
```

Content-addressed storage is preferred when practical so identical content can be deduplicated and blobs can be integrity-checked. The exact hash algorithm and on-disk layout are implementation details to decide when attachments are implemented.

### Provider/derived cache

Rebuildable data is cache, not canonical attachment data.

Examples:

- TMDB/IGDB poster downloaded only as provider metadata;
- generated thumbnail;
- software icon extracted from an installed application;
- raw provider API response;
- discovery snapshot.

Cache must be deletable/rebuildable without damaging the canonical library.

### Promotion

The application may let a user promote provider content into a canonical attachment. Promotion is an explicit canonical write.

### Export/backup

Portable export includes canonical attachment metadata and, when the export mode promises a complete portable library, the referenced canonical blobs.

Provider cache is excluded by default.

### Deletion and reference counting

Deleting an attachment does not imply immediate unsafe blob deletion. Blob cleanup must account for all references and should be implemented as a safe garbage-collection operation.

## Consequences

- Backups and portable exports have clear semantics.
- Cache can be aggressively rebuilt or cleared.
- Future media/software modules can share one attachment system.
- Binary lifecycle is more complex than storing arbitrary file paths, but long-term ownership is explicit.

## Non-goals

- implementing blob sync in V1;
- storing large media libraries themselves;
- turning AssetMesh into a general filesystem or cloud-drive replacement.
