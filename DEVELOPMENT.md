# Developer Setup & Implementation Notes

This document covers how to build, test, and use the Phase 1 (Media Records)
implementation, and records the concrete contracts the implementation
established on top of the architecture docs and ADRs.

## Requirements

- Rust 1.85+ (2024-era stable; developed on 1.97)
- No system SQLite required — `rusqlite` compiles a bundled copy

## Build & test

```bash
cargo build                       # workspace: core, storage-sqlite, cli
cargo test --workspace            # 63 tests: domain, use cases, sqlite, e2e
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
```

## Workspace layout

```text
AssetMesh/
├── Cargo.toml                    # workspace
├── migrations/
│   └── 0001_core_media_v1.sql    # database migration v1 (embedded at build time)
├── crates/
│   ├── core/                     # assetmesh-core: headless kernel (ADR 0001)
│   │   └── src/
│   │       ├── domain/           # Asset, MediaRecord, AssetExternalRef, Activity, Tag, SearchDocument
│   │       ├── ports/            # repositories, UnitOfWork, SearchIndex, Clock, IdGenerator
│   │       ├── application/      # use cases: media, asset/merge, search, import, portable export
│   │       └── error.rs          # typed AppError model
│   ├── storage-sqlite/           # assetmesh-storage-sqlite: SQLite adapter
│   │   └── src/
│   │       ├── connection.rs     # shared pragma policy (foreign_keys, WAL, busy_timeout)
│   │       ├── migrations.rs     # ordered, checksummed migration runner
│   │       ├── uow.rs            # transaction boundary (IMMEDIATE tx, commit/rollback)
│   │       └── repos/            # AssetRepository, MediaRepository, ..., FTS5 SearchIndex
│   └── cli/                      # assetmesh binary (thin adapter)
└── docs/
```

Dependency rule (verified by tests + crate boundaries): `cli → storage-sqlite →
core`; `core` depends only on chrono/csv/serde/serde_json/thiserror/uuid — no
database driver, UI framework, or transport.

## CLI usage

The CLI defaults to `./assetmesh.db`, overridable with `--db <path>` or
`ASSETMESH_DB`. Every command runs pending migrations on open.

```bash
assetmesh media add --title "Frieren" --media-type anime --year 2023 \
    --status completed --rating 9.5 \
    --progress-current 28 --progress-total 28 --progress-unit episode \
    --tag healing --ref tmdb:209867
assetmesh media list --type anime --sort rating
assetmesh media get <id-or-prefix>
assetmesh media start <id>          # also: pause, drop, complete
assetmesh media progress <id> --current 18 --total 28 --unit episode
assetmesh media rate <id> --rating 9.0
assetmesh media search frieren      # FTS5 + substring fallback (works for CJK)
assetmesh media update <id> --notes "..."

assetmesh media import legacy.json --dry-run   # or .csv; sniffed by content
assetmesh media import legacy.json

assetmesh asset archive <id>
assetmesh asset merge <loser> <winner>          # explicit; tombstones the loser
assetmesh asset ref add <id> steam 1091500
assetmesh asset ref remove <id> steam 1091500

assetmesh export <dir>              # portable bundle (see below)
assetmesh import <dir>              # restore canonical data by canonical ID
```

IDs may be given in full or as a unique prefix (≥4 chars).

## Version axes (ADR 0008)

| Axis | Location | Current value |
| --- | --- | --- |
| Database migration version | `assetmesh_migrations` table (checksummed) | 1 |
| Portable export format version | `manifest.json → version` | 1 (`EXPORT_VERSION`) |
| Media module data schema version | `module_metadata` table + `manifest.json → modules.media.schema_version` | 1 (`MEDIA_SCHEMA_VERSION`) |

A portable bundle with an unsupported format or module version is rejected
before any canonical mutation. An already-applied database migration whose
checksum changed makes startup fail loudly.

## Portable export format

```text
assetmesh-export/
├── manifest.json
├── assets.jsonl
├── external_refs.jsonl
├── activity.jsonl
├── tags.json            # JSON array (tags are the only non-JSONL file)
├── asset_tags.jsonl
└── modules/
    └── media.jsonl
```

- Rows are dedicated `*V1` wire DTOs (in `application/portable.rs`), not the
  mutable domain structs — internal refactors cannot silently change format
  version 1. A checked-in fixture in `core/tests/export_tests.rs` pins the
  historical representation.
- Search projections, provider cache, and secrets are never exported.
- Every import (dry-run or commit) runs the same preflight: all six declared
  files must be present, decoded row counts must match the manifest,
  identities (asset/media/ref/event/tag ids, ref pairs) must be unique, and
  the reference graph must be internally consistent (module details and refs
  point at bundled assets, kind/media-type compatibility, no dangling
  memberships or events, merge redirects exist, are not self-referential and
  are acyclic).
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
- Note: file names use underscores (`external_refs.jsonl`); the older
  `docs/05` sketch showed dashes — underscores are the implemented contract.

## Legacy media import

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
matched asset — never a placeholder owner). Status is set directly on update
(interactive transition-matrix rules apply to use cases, not to import
normalization), but all record-level invariants (finite rating/progress,
ranges, timestamps) are still enforced. Commits run in bounded 200-record
transaction batches; if a batch fails, the report discloses it in
`report.failed` with `records_committed` — earlier batches remain committed
and the CLI exits non-zero. Parsing and matching always run outside write
transactions.

## Established domain contracts

- **Rating scale**: 0.0–10.0, always finite.
- **Progress**: structured `current` / `total` / `unit` (unit is a lowercase
  label such as `episode` or `percent`). Numbers may be absent (games); a unit
  without any number is invalid; `current ≤ total`; nothing negative; all
  present values must be finite (NaN/infinity would not survive SQLite/JSON
  round trips and are rejected on every write path).
- **Status transitions** (interactive use cases):

  ```text
  planned     → in_progress | completed | dropped
  in_progress → planned | paused | completed | dropped
  paused      → planned | in_progress | completed | dropped
  dropped     → planned | in_progress | paused | completed
  completed   → in_progress | paused | dropped        (never straight back to planned)
  ```

  Entering `in_progress` sets `started_at` (if unset); entering `completed`
  sets `completed_at` and `started_at` if unset; leaving `completed` clears
  `completed_at`; returning to `planned` clears `started_at`. `completed_at ≥
  started_at` is enforced on every write path.
- **Merge**: explicit use case. Loser may be active or archived; winner must be
  active; kinds must match. Refs move (winner-owned duplicates are dropped),
  tags union, media details move if the winner has none — if both exist the
  survivor wins and the loser's record is preserved inside the `asset.merged`
  activity payload. The loser becomes a `merged` tombstone with
  `merged_into`. Relations/collections/attachment merges are deferred until
  those subsystems exist (ADR 0005).
- **Activity events**: `asset.created`, `asset.archived`, `asset.merged`,
  `media.created`, `media.started`, `media.paused`, `media.dropped`,
  `media.completed`, `media.progress_changed`, `media.rating_changed`,
  `media.imported` (actor `import`). Metadata-only edits deliberately emit no
  event.
- **Search**: FTS5 (`unicode61`) over the projected `SearchDocument`; queries
  fall back to substring `LIKE` when tokenization cannot serve them (e.g. CJK,
  short substrings). The projection is rebuildable via the `SearchService`
  rebuild use case and is tested to survive deletion.

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

## Known deviations / clarifications vs. docs

- `TagRepository::ensure` generates the tag ID in the adapter (still an
  application-generated UUIDv7, never an external ID).
- Filesystem bundle I/O (`write_bundle_to_directory` /
  `read_bundle_from_directory`) lives in core as a std-only reference
  adapter; moving it behind a dedicated adapter crate is deferred until a
  second consumer exists.
- Doc 05 shows `external-refs.jsonl`; the implemented bundle uses
  `external_refs.jsonl` (matching the Phase 1 tasking).
- Archived assets stay searchable; merged tombstones are excluded from search
  and from import matching (only active assets are match targets).

## Testing strategy

- Domain invariants: unit tests in `core/src/domain/*`.
- Use cases against in-memory port doubles: `core/tests/` (rollback
  semantics mirror SQLite transactions).
- SQLite contract tests: `storage-sqlite/tests/` — pragmas, migrations
  (fresh/reopen/tamper), CRUD, FK/unique enforcement, transaction atomicity,
  search + rebuild, portable round trip on a real database.
- CLI end-to-end: `cli/tests/cli_e2e.rs` drives the compiled binary through
  create → start → progress → complete → rate → search → merge → export →
  restore → idempotent re-import, plus dry-run/conflict/error behavior.
