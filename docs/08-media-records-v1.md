# Media Records V1

## Why Media first

Media Records is a strong first vertical slice because it has real historical data, meaningful CRUD/search/filter workflows, and low-risk domain behavior. It can validate AssetMesh's core without requiring privileged macOS integration.

## Supported media types

Initial types:

- movie
- tv
- anime
- game

Additional types can be added later without redesigning the base Asset model.

## Suggested model

```text
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

## Status

Keep status vocabulary small and normalized:

```text
planned
in_progress
completed
paused
dropped
```

UI labels may differ by media type (“Watching”, “Playing”), but storage should avoid unnecessary type-specific status enums unless behavior genuinely differs.

## Progress

Avoid storing progress only as a formatted string.

Suggested representation:

```text
current: 18
total: 28
unit: episode
```

For games where numeric progress is unknown, allow progress to be omitted and rely on status + notes.

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
- export portable data.

## Activity examples

- `media.created`
- `media.started`
- `media.progress_changed`
- `media.completed`
- `media.rating_changed`

Do not write noisy activity for every trivial text edit unless it provides future value.

## Import contract

Legacy import should support a dry-run summary:

```text
Input: 292 records
Valid: 290
Potential duplicates: 2
Rejected: 0
```

Duplicate matching may use normalized title + type + year as a heuristic, but the importer must allow explicit review rather than silently merging uncertain matches.

## V1 UI

```text
Media
├── Search
├── Filters: type / status / rating / tag
├── Sort: updated / title / rating / completed date
├── List or card view
└── Detail panel/page
```

Detail view:

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

## Definition of done

Media V1 is done when real historical data can replace the old host-specific implementation without losing information or creating a new data lock-in.
