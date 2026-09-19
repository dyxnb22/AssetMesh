# Media Records V1

## Why Media first

Media Records is a strong first vertical slice because it has real historical data, meaningful CRUD/search/filter workflows, and low-risk domain behavior. It can validate AssetMesh's core without requiring privileged macOS integration.

Media V1 should validate the long-lived foundation contracts while remaining intentionally small. It is not the place to implement sync, jobs, plugin APIs, runtime discovery, or the desktop application shell.

## Supported media types

Initial types:

- movie
- tv
- anime
- game

Additional types can be added later without redesigning the base Asset model.

## Suggested model

```text
Asset
- id
- kind
- name
- summary?
- lifecycle_state
- created_at
- updated_at
- archived_at?
- revision?

MediaRecord
- asset_id
- media_type
- status
- rating?
- year?
- platform?
- progress_current?
- progress_total?
- progress_unit?
- notes?
- started_at?
- completed_at?
```

Tags use the shared tag system.

External provider/import identifiers use `AssetExternalRef`, never fields hard-coded into `MediaRecord` merely for one provider.

## Status

Keep status vocabulary small and normalized:

```text
planned
in_progress
completed
paused
dropped
```

Future UI labels may differ by media type (“Watching”, “Playing”), but storage should avoid unnecessary type-specific status enums unless behavior genuinely differs.

## Progress

Avoid storing progress only as a formatted string.

Suggested representation:

```text
current: 18
total: 28
unit: episode
```

For games where numeric progress is unknown, allow progress to be omitted and rely on status + notes.

## External references

The importer should preserve stable external identifiers when legacy data contains them.

Examples:

```text
steam:<app-id>
igdb:<id>
tmdb:<id>
douban:<id>
```

Matching priority:

1. canonical AssetMesh ID for AssetMesh-native imports;
2. exact namespaced external ref;
3. deterministic module key if one exists;
4. normalized title + type + year heuristic.

Heuristic candidates require review; they do not silently merge assets.

## Search projection

Media V1 must implement the shared `SearchDocument` projection.

Example:

```text
SearchDocument
- asset_id: <id>
- kind: media.anime
- title: 葬送的芙莉莲
- subtitle: Anime · 2023
- body: <notes + platform where appropriate>
- keywords: [Frieren, Sousou no Frieren, tags, provider aliases]
- updated_at: ...
```

The search projection is rebuildable from canonical data.

Initial structured filters remain typed Media queries:

- media type;
- status;
- rating range;
- tag;
- platform where useful.

Full-text search should not replace these filters.

## Core use cases

- add media;
- edit metadata;
- start;
- update progress;
- pause/drop;
- complete;
- rate;
- search/filter/sort;
- add/remove tags;
- import legacy records;
- preview/resolve import conflicts;
- export portable data;
- rebuild Media search projection.

## Transaction examples

Completing media should be one short canonical transaction:

```text
1. update media status/progress
2. set completed_at
3. increment revision if used
4. append media.completed activity
5. update synchronous search projection
6. commit
```

No provider/network request should occur while this transaction is open.

## Activity examples

- `media.created`
- `media.started`
- `media.progress_changed`
- `media.completed`
- `media.rating_changed`
- `asset.merged` when an explicit duplicate merge occurs

Do not write noisy activity for every trivial text edit unless it provides future value.

## Import contract

Legacy import should support a dry-run summary:

```text
Input: 292 records
Valid: 290
Exact matches: 0
Potential duplicates: 2
Rejected: 0
```

The importer should conceptually operate as an import session:

```text
parse
  ↓
validate
  ↓
normalize
  ↓
match by IDs/external refs
  ↓
produce create/update/conflict candidates
  ↓
user reviews uncertain conflicts
  ↓
commit canonical changes
  ↓
report
```

Duplicate matching may use normalized title + type + year as a heuristic, but the importer must allow explicit review rather than silently merging uncertain matches.

The first implementation does not need a fully generic durable `ImportSession` table if a simpler in-memory preview flow safely handles the historical migration. The application boundary should still reflect the stages above.

## Module schema version

Media portable data starts at an explicit schema version, for example:

```text
media schema_version = 1
```

The exact SQLite migration number is separate from this semantic module version.

Portable import must inspect the Media schema version before canonical mutation.

## SQLite expectations

Media repository tests should verify that the shared storage adapter configures SQLite consistently, including foreign keys, WAL mode, and bounded busy timeout according to ADR 0007.

The implementation should use short write transactions and avoid holding a transaction across import-file parsing or user conflict review.

## Attachments

Attachment/blob implementation is **not required** for Media V1 unless the legacy dataset contains durable user-owned binary data that must migrate.

If posters or provider artwork are added later, they are provider cache by default. User-promoted artwork may become a canonical attachment under ADR 0009.

## Portable export

Media export must include:

- stable Asset IDs;
- Media typed details;
- relevant tags/collections;
- external refs;
- meaningful activity according to export policy;
- top-level export format version;
- Media module schema version.

Search indexes/provider caches are excluded because they are rebuildable.

## Deferred desktop presentation

The Media desktop experience is intentionally deferred to the shared Application Shell / Desktop UI phase rather than being part of the Media V1 exit criteria.

A future Media surface is expected to expose capabilities such as:

```text
Media
├── Search
├── Filters: type / status / rating / tag
├── Sort: updated / title / rating / completed date
├── List or card view
└── Detail panel/page
```

A future detail view may include:

```text
Title
Type · Year
Status · Progress · Rating
Platform
Tags
Started / Completed
Notes
Activity
Edit / Complete / Archive
```

These are presentation expectations, not Media-domain requirements. The desktop adapter must call existing application use cases and must not implement import matching, canonical merge behavior, domain invariants, or SQL search joins itself.

## Required tests before declaring Media V1 complete

1. domain invariant tests for status/progress/rating;
2. application use-case tests with fake ports;
3. SQLite repository contract tests;
4. legacy JSON/CSV import dry-run fixtures;
5. duplicate/external-ref matching fixtures;
6. portable export/import round-trip;
7. Media schema migration fixture;
8. search projection rebuild test;
9. activity + canonical write atomicity test.

UI polish is not a prerequisite for Media V1 completion.

## Explicitly deferred from Media V1

- desktop/application-shell implementation;
- metadata-provider integration;
- poster/thumbnail system unless migration requires it;
- durable background job queue;
- sync/CRDT;
- third-party plugin support;
- semantic/vector search;
- mandatory local daemon.

## Definition of done

Media V1 is done when real historical data can replace the old host-specific implementation without losing information or creating a new data lock-in, and when:

- import preview is safe and repeatable;
- exact external references prevent duplicate re-import where available;
- uncertain matches are reviewable rather than auto-merged;
- search can be rebuilt from canonical data;
- portable export/import preserves stable identity and Media schema version;
- CLI and future desktop/HTTP/agent adapters can use the same application services without domain duplication;
- no desktop client is required to satisfy the Media V1 exit criteria.
