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

Users can correct any status directly to any other status; same-status commands
are no-ops. Entering `in_progress` initializes `started_at` when absent; entering
`completed` initializes both timestamps when absent. Leaving `completed` clears
`completed_at`, and returning to `planned` clears `started_at`. Completed time
must not precede started time. Display labels may differ by media type.

## Progress

Avoid storing progress only as a formatted string.

Suggested representation:

```text
current: 18
total: 28
unit: episode
```

Progress may be omitted when no numeric count is known. A unit without a current
or total value is invalid. Present values are finite and non-negative; current
must not exceed total. Ratings are finite numbers on the 0–10 scale.

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

Pipeline: parse → validate/normalize → match → plan → review (`--dry-run`) →
commit → report. JSON (array, or object with a `records`/`items`/`data` array)
and CSV are supported; format-specific parsing is isolated in
`application/import_parse.rs`.

Matching precedence (ADR 0005):

1. canonical `asset_id` (for AssetMesh-native data) — but every ref the row
   carries must agree with the named target: a ref owned by any other asset
   (committed, planned, active, archived or merged) makes the row a
   reviewable conflict instead of an update that would fail at commit;
2. exact namespaced external ref — including refs claimed by records
   committed or planned earlier in the same run, so batch duplicates and
   200-row boundary cases resolve to one asset. A ref owned by an
   *archived or merged* asset is an explicit conflict, never a create;
3. normalized key: casefolded/whitespace-collapsed title + media type + year
   — reported as a **potential duplicate**, never auto-applied (ADR 0005
   classifies title/type/year matching as heuristic; two distinct releases
   may share the tuple);
4. heuristic: same normalized title + type with a different/missing year —
   same: reported, never written.

Only canonical IDs and exact external references auto-update canonical data,
and only when every ref on the row agrees with the target. Report indexes
are physical source row numbers, stable across malformed rows.

Update policy on match: imported `Some` fields overwrite, `None` keeps the
existing value, tags and refs are unioned (new refs are inserted owned by the
matched asset — never a placeholder owner). Status is set directly on update, with the same timestamp invariants
as interactive corrections. All record-level invariants (finite rating/progress,
ranges, timestamps) are still enforced. Commits run in bounded 200-record
transaction batches; if a batch fails, the report discloses it in
`report.failed` with `records_committed` — earlier batches remain committed
and the CLI exits non-zero. Parsing and matching always run outside write
transactions.

### Implemented legacy update safeguards

Legacy import keeps the source read-only. Existing archived or merged assets are
not editable import targets. An incoming progress count merges with the stored
count using the greater value, so re-importing an older export cannot reduce it.
Incoming totals and units may correct the descriptive fields. If the resulting
progress is invalid, the stored progress remains intact and the activity records
`progress_merge_refused:<reason>`; counts are never silently clamped.

Status and start/completion timestamps still follow the legacy field-update
policy. Unlike counts they have no monotonic ordering; protection from older
exports requires a separate provenance decision recorded in the
[Roadmap](07-roadmap.md).

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

## Desktop and verification

The implemented desktop calls these application use cases; its interaction
contract is maintained in [Desktop Contract](12-desktop-contract.md). Test
ownership is maintained in [DEVELOPMENT.md](../DEVELOPMENT.md#test-ownership).
Migration fixtures and import/export compatibility remain required when changing
stored data. Optional extensions belong in the [Roadmap](07-roadmap.md).
