# Developer Setup & Implementation Notes

This document covers how to build, test, and use the implemented core, CLI,
and Phase 5 Tauri desktop application, and records the concrete contracts the implementation
established on top of the architecture docs and ADRs.

**Next implementation target:** Phase 6 — Runtime Enrichment.
Phase 4 — Unified Library Core and Phase 5 — Desktop are complete; see
`docs/11-unified-library-core.md` for the implementation contract and
`docs/07-roadmap.md` for phase boundaries.

## Requirements

- Rust 1.85+ (2024-era stable; developed on 1.97)
- No system SQLite required — `rusqlite` compiles a bundled copy

## Build & test

```bash
cargo build                       # workspace: core, providers, storage-sqlite, cli, desktop
cargo test --workspace            # domain, use cases, providers, sqlite, e2e
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
npm ci && npm run typecheck && npm run lint && npm test && npm run build
npm run test:frontend-workflow # jsdom cross-workspace workflow
# Linux only: install tauri-driver + webkit2gtk-driver, then:
cargo build -p assetmesh-desktop
xvfb-run -a npm run test:e2e # real Tauri/WebKitGTK/SQLite runtime
```

## Desktop run & package

```bash
npm run --workspace=apps/desktop tauri dev    # vite + window, reloads on UI edits
npm run --workspace=apps/desktop tauri build  # .app + .dmg under target/release/bundle/macos
```

A bare `cargo run -p assetmesh-desktop` has no Dock icon: macOS reads the icon from the
`.app` bundle, so only the packaged build shows it. The icon source of truth is
`apps/desktop/src-tauri/icons/icon.svg`; regenerate the full set with
`npx tauri icon <abs>/icon.svg --output <abs>/icons` (absolute paths — the CLI resolves
relative ones against `apps/desktop`).

The desktop database defaults to `assetmesh.db` inside the Tauri application data
directory (`~/Library/Application Support/com.assetmesh.desktop` on macOS); `ASSETMESH_DB`
overrides it. Never a working-directory-relative path: a `.app` launched from a file
manager runs with the CWD at `/`, which is read-only. When the open fails the window
still appears, the card shows a generic hint, and the real reason plus the path go to
stderr (`< /Applications/AssetMesh.app/Contents/MacOS/assetmesh-desktop 2> log`).

## Workspace layout

```text
AssetMesh/
├── Cargo.toml                    # workspace
├── apps/desktop/                 # Tauri + React desktop application and real Linux E2E
├── migrations/
│   ├── 0001_core_media_v1.sql         # database migration v1 (embedded at build time)
│   ├── 0002_software_relations_v1.sql # software details + shared relations
│   ├── 0003_services_v1.sql           # services details (module schema v1)
│   └── 0004_service_relations_v1.sql  # service relation types in the stored-type CHECK
├── crates/
│   ├── core/                     # assetmesh-core: headless kernel (ADR 0001)
│   │   └── src/
│   │       ├── domain/           # Asset, MediaRecord, SoftwareRecord, ServiceRecord, Relation, Refs, Activity, Tag, SearchDocument
│   │       ├── ports/            # repositories, UnitOfWork, SearchIndex, discovery providers, Clock, IdGenerator
│   │       ├── application/      # use cases: media, software + discovery/adoption, services, relations, asset/merge, unified library, search, import, portable export
│   │       └── error.rs          # typed AppError model
│   ├── providers/                # assetmesh-providers: macOS / Homebrew / npm+pipx discovery (seam-tested)
│   ├── storage-sqlite/           # assetmesh-storage-sqlite: SQLite adapter
│   │   └── src/
│   │       ├── connection.rs     # shared pragma policy (foreign_keys, WAL, busy_timeout)
│   │       ├── migrations.rs     # ordered, checksummed migration runner
│   │       ├── uow.rs            # transaction boundary (IMMEDIATE tx, commit/rollback)
│   │       └── repos/            # Asset/Media/Software/Service/ExternalRef/Relation/Tag/Activity repos, FTS5 SearchIndex
│   └── cli/                      # assetmesh binary (thin adapter)
└── docs/
```

Dependency rule (verified by tests + crate boundaries): `cli/desktop → storage-sqlite →
core` and `providers → core`; `core` depends only on
chrono/csv/serde/serde_json/thiserror/uuid — no database driver, UI framework,
OS API, or transport. Provider I/O (process execution, plist parsing) is
confined to `assetmesh-providers` behind a `CommandRunner` seam.

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

# Services and subscriptions (Phase 3). Cost is a decimal on the CLI and is
# stored as integer minor units; --cost and --currency are always paired.
assetmesh service add --name "OpenAI" --type saas --provider OpenAI \
    --plan Plus --cost 19.99 --currency USD --billing monthly \
    --renews-at 2026-10-01 --auto-renew true --tag ai --ref openai:org-work
assetmesh service list --type saas --provider openai --sort renews
assetmesh service get <id>
assetmesh service update <id> --plan Pro         # other fields left unchanged
assetmesh service update <id> --notes ""         # empty clears a text field
assetmesh service update <id> --clear-cost       # removes cost AND currency
assetmesh service renew <id> --renewed-at 2026-10-01 --cost 29.99 \
    --currency USD --next-renewal 2026-11-01      # explicit facts only
assetmesh service search openai

# Relations (inverse types are accepted and resolved at view time)
assetmesh relation add <api-id> hosted_on <vps-id>
assetmesh relation add <domain-id> points_to <api-id>
assetmesh relation list <vps-id>               # shows `hosts`

# Unified library (Phase 4A) — one application contract across every module
assetmesh library list
assetmesh library list --module media --sort name --limit 20 --offset 40
assetmesh library list --kind media.anime --kind service.saas --tag ai
assetmesh library list --lifecycle active-or-archived   # archived is opt-in
assetmesh library get <id-or-prefix>          # typed Media/Software/Service details
assetmesh library search frieren              # same summary vocabulary as the list
assetmesh library search 腾讯 --module services
assetmesh library list --json                 # the application DTOs, unmapped

# Relation graph queries (Phase 4B) — read-only, bounded, inverse-resolved
assetmesh relation neighbors <id>             # every directly connected asset
assetmesh relation dependencies <id>          # what it needs (depends_on/installed_via/hosted_on)
assetmesh relation dependents <vps-id>        # what needs it
assetmesh relation impact <vps-id>            # transitive dependents + paths
assetmesh relation impact <vps-id> --depth 1  # bounded; reports when the bound hid more
assetmesh relation traverse <id> --direction both --type depends_on --depth 4 \
                                       [--include-archived]

# Cross-module activity (Phase 4C)
assetmesh activity list
assetmesh activity list --asset <id> --type media.completed
assetmesh activity list --module services --since 2026-01-01 --limit 50

# Duplicate review (Phase 4C) — advisory only; merging stays an explicit command
assetmesh duplicates list
assetmesh duplicates list --kind software.cli --active-only

assetmesh export <dir>              # portable bundle (see below)
assetmesh import <dir>              # restore canonical data by canonical ID
```

IDs may be given in full or as a unique prefix (≥4 chars).

Every Phase 4 command is a thin adapter: it parses arguments into an
application query (`LibraryQuery`, `LibrarySearchQuery`, `TraversalOptions`,
`ActivityQuery`, `DuplicateQuery`), calls the service, and formats the result.
Module dispatch, lifecycle rules, ordering, pagination, traversal, inverse
resolution, duplicate matching, and event-type classification all live in the
application layer — the CLI never joins module repositories, never walks a
graph, and never decides what a duplicate is.

## Version axes (ADR 0008)

| Axis | Location | Current value |
| --- | --- | --- |
| Database migration version | `assetmesh_migrations` table (checksummed) | 4 |
| Portable export format version | `manifest.json → version` | 1 (`EXPORT_VERSION`) |
| Media module data schema version | `module_metadata` + `manifest.json → modules.media.schema_version` | 1 (`MEDIA_SCHEMA_VERSION`) |
| Software module data schema version | `module_metadata` + `manifest.json → modules.software.schema_version` | 1 (`SOFTWARE_SCHEMA_VERSION`) |
| Services module data schema version | `module_metadata` + `manifest.json → modules.services.schema_version` | 1 (`SERVICES_SCHEMA_VERSION`) |

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
├── relations.jsonl
└── modules/
    ├── media.jsonl
    ├── software.jsonl
    └── services.jsonl
```

- Rows are dedicated `*V1` wire DTOs (in `application/portable.rs`), not the
  mutable domain structs — internal refactors cannot silently change format
  version 1. A checked-in fixture in `core/tests/export_tests.rs` pins the
  historical representation.
- Search projections, provider cache, and secrets are never exported.
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
  `merged_into`. **Services merge field by field instead** (docs/10 merge
  rules 3-6): the kind-equality preflight already guarantees both records are
  the same `ServiceType`, so equal normalized values deduplicate, an absent
  field is filled from the loser, and money is compared as the
  `(cost_minor, currency)` pair. Anything still disagreeing makes the merge
  fail with a reviewable conflict listing every field and both values — never
  a silent survivor pick — and the transaction rolls back leaving both records
  intact. Because a successful merge discards nothing, there is no
  `loser_service_details` payload (unlike Media/Software, whose survivor-wins
  rule really does drop the loser's record). Relations are re-pointed to the
  winner in canonical form with duplicate/self-loop cleanup; collection and
  attachment merge handling remains deferred until those subsystems exist
  (ADR 0005).
- **Asset kind vs module discriminator**: the typed detail's own discriminator
  (media type / software category / service type) is only consistent with the
  kind assigned at creation, and no application write path ever re-types an
  asset. The repository boundary therefore refuses ANY Asset kind change
  while the old module's detail row remains, in both directions and including
  within-module changes (`service.saas` → `service.api` would strand a `saas`
  record); remove the record first if a re-type is genuinely required.
  Mirrored by the in-memory test double.
- **Software**: category ↔ asset kind are 1:1 and enforced on every write
  path INCLUDING the repository boundary (`software.app|cli|package|runtime|tool`)
  — even a direct UnitOfWork write cannot attach a software record to an
  asset of another module. `version` ≤ 128 chars;
  `install_location`/`executable_path`/`architecture` ≤ 1024; optional text
  fields are trimmed and never contain control characters. `install_source`
  is a closed enum (`macos_app`, `homebrew_formula`, `homebrew_cask`,
  `npm_global`, `pipx`, `manual`, `system`, `unknown`).
- **Purpose / "why installed"**: `purpose` and `notes` are user-owned.
  Discovery/adoption never writes them from provider data; only explicit
  user overrides do. Adoption's update path only FILLS missing fields —
  existing version/location/architecture is never overwritten.
- **Discovery**: providers are read-only adapters (macOS apps, Homebrew,
  npm/pipx) staging advisory candidates; scans run outside transactions and
  emit no activity. Classification follows ADR 0005: exact namespaced
  external refs → `ExactMatch`, normalized-name similarity → review-only
  `PotentialDuplicate`, anything ambiguous → `Conflict`. Adoption re-matches
  inside the commit transaction and is the only route to canonical state;
  candidates are ephemeral (no persisted snapshots). Namespaces in use:
  `bundle_id`, `homebrew_formula`, `homebrew_cask`, `npm`, `pipx` (no
  `path:` namespace — external IDs cannot contain whitespace and paths are
  unstable).
- **Relations**: shared `relations` table over Asset IDs. Registry:
  `depends_on ↔ dependency_of`, `uses ↔ used_by`, `installed_via ↔ installs`,
  `hosted_on ↔ hosts`, `points_to ↔ pointed_to_by`, and `related_to`
  (symmetric). Every fact has exactly one canonical row: inverse-pair types
  are never stored (a fact stated via an inverse type is stored in its primary
  direction with endpoints swapped; `related_to` is stored in canonical
  endpoint order). Migration 0002 established the original stored-type CHECK;
  migration 0004 widens that CHECK for the Phase 3 service relation types
  without modifying the already-applied migration. Re-stating a fact from
  either endpoint is a conflict. Relation IDs are application-generated
  (injected generator). Merge re-points relations to the winner in canonical
  form, dropping duplicates and self-loops.
- **Activity events**: `asset.created`, `asset.archived`, `asset.merged`,
  `media.created`, `media.started`, `media.paused`, `media.dropped`,
  `media.completed`, `media.progress_changed`, `media.rating_changed`,
  `media.imported` (actor `import`), `software.created`,
  `software.adopted`, `service.created`, `service.renewed`,
  `relation.created`, `relation.removed`. Metadata-only edits deliberately
  emit no event; discovery scans emit no event. Renewal history contains only
  caller-supplied factual renewal data; it is not an invoice ledger.
- **Search**: FTS5 (`unicode61`) over the projected `SearchDocument`; queries
  fall back to substring `LIKE` when tokenization cannot serve them (e.g. CJK,
  short substrings). The projection is rebuildable via the `SearchService`
  rebuild use case and is tested to survive deletion. Media, Software, and
  Services all project into this shared derived index.

## Phase 4 — Unified Library Core (complete)

Phase 4 consolidated the existing module capabilities rather than adding a
fourth asset domain. The full contract is in
`docs/11-unified-library-core.md`; this section records the implementation
details that are not obvious from the docs.

| Sub-phase | Service | Status |
| --- | --- | --- |
| 4A — Unified Library Query | `application/library_service.rs` | complete |
| 4B — Relation Traversal & Impact | `application/relation_query_service.rs` | complete |
| 4C — Activity & Duplicate Review | `application/activity_service.rs`, `application/duplicate_review_service.rs` | complete |
| 4D — Contract Hardening | CLI + cross-module E2E | complete |

Phase 4 introduced **no database migration, no schema change, and no new
index**: traversal, activity querying, and duplicate review are all derived
from existing canonical rows. The only port addition is
`RelationReader::list_for_assets`, so a BFS frontier is one batch query instead
of one query per node.

### Unified library query contract (Phase 4A)

`LibraryService::new(factory)` exposes:

- `get_asset(id) -> AssetDetailView` — one consistent read snapshot containing
  the asset, its **typed** module details (`AssetDetails::Media | Software |
  Service`), tags, and external refs. Nothing collapses into
  `serde_json::Value`.
- `list_assets(&LibraryQuery) -> Page<AssetSummary>`
- `search_assets(&LibrarySearchQuery) -> Page<AssetSummary>` — over the
  existing Search Projection, returning the same `AssetSummary` vocabulary.
- `resolve_merge_redirect(id) -> AssetId` — follows `merged_into` with a
  visited set, so adapters never hand-roll redirect chains.

`AssetSummary { id, kind, name, lifecycle, subtitle, tags, updated_at }`.
`subtitle` comes from the module's own summary helper in
`application/projection.rs`, which the search projection also uses — list,
search, and FTS subtitles cannot drift.

Query rules:

- `LibraryQuery { lifecycle, modules, kinds, tags, sort, page }`;
  `LibrarySearchQuery { text, lifecycle, modules, kinds, tags, page }`.
- Lifecycle defaults to `LifecycleFilter::Active`. Archived is opt-in
  (`ActiveOrArchived` / `All`); merged tombstones are redirects and are never
  listed or searchable.
- `modules` and `kinds` compose (both must match). A contradictory pair (e.g.
  `--module media --kind software.cli`) yields an empty page without reading
  storage.
- Tag filters require every requested tag, case-insensitively.
- Sorts: `UpdatedDesc` (default), `UpdatedAsc`, `NameAsc`, `NameDesc`,
  `KindAsc` — every one with `AssetId` ascending as the secondary key.
- `PageRequest { limit, offset }`: `limit = 0` selects
  `DEFAULT_PAGE_LIMIT` (50) and is clamped to `MAX_PAGE_LIMIT` (200).
  `Page.total` is `Some(exact)` for the list and `None` for search (relevance
  ordering exposes no count); search pages can also be shorter than `limit`
  because filters are applied after hydration. The search hydration window
  (`offset + limit`) is bounded, so an absurd offset returns an empty page
  instead of asking the index for everything.

Detail semantics:

| Case | Result |
| --- | --- |
| unknown id | `not_found("asset", id)` |
| merged tombstone | `conflict` naming the surviving asset |
| archived | readable |
| no module details | `not_found("module details", id)` |

Composition and the deliberate trade-off:

- the unified list is composed from the existing module readers — **no new port
  method, no new SQL, no migration**;
- each selected module reader is called exactly once, and module list rows
  already carry their tags, so there is no per-asset asset/detail/tag lookup;
  a `core` test asserts one unified read opens exactly one `QueryUnitOfWork`
  and uses no repository capability twice;
- filtering/sorting/paging run in the application layer over the selected
  modules' rows rather than as a SQL `LIMIT`. For a local personal library
  that is the right trade and keeps Phase 4A out of storage; if it ever
  matters, the follow-up is a repository-level paged cross-module query (port
  in core, SQLite + test double, contract tests) — never an adapter-side join.

### Relation traversal and impact (Phase 4B)

`RelationQueryService::new(factory)` provides `neighbors`, `outgoing`,
`incoming`, `dependencies`, `dependents`, `traverse`, and `impact`. All seven
share one BFS engine:

- one `QueryUnitOfWork` per traversal, one batch edge query per frontier;
- visited-set cycle protection; `max_depth = 0` reaches nothing beyond the
  root and values above `MAX_TRAVERSAL_DEPTH` (32) are clamped;
- the relation-type filter matches the row's **stored** type, so
  `Incoming + [DependsOn]` works; every reported type is the **effective** type
  from the expanding node, so adapters never derive an inverse;
- `DEPENDENCY_RELATION_TYPES` (`depends_on`, `installed_via`, `hosted_on`) is
  the single definition of dependency semantics — `uses`, `points_to`, and
  `related_to` are deliberately not dependencies;
- `dependencies` / `dependents` / `impact` fix their own direction and type
  set and keep only the caller's `max_depth` and `include_archived`;
- `TraversalView.truncated` re-applies the query's own filters, so it never
  claims the depth bound hid an edge the walk would have skipped;
- merged tombstones are never nodes and are refused as a root with the same
  redirect-naming conflict the library uses; archived nodes are opt-in and are
  never traversed through; an asset with no module details is still a node,
  because relations are shared infrastructure;
- one-hop queries hydrate only the neighbour by identity — they do not load the
  library index that traversal needs.

### Activity and duplicate review (Phase 4C)

`ActivityService::query` takes an `ActivityQuery` (asset, event types, modules,
actors, time range, page) and returns `Page<ActivityView>`, newest first with
the event id as the tie-breaker. `ActivityModule::of_event_type` in
`domain/activity.rs` is the only event-type → subsystem mapping.

`DuplicateReviewService::candidates` returns `Page<DuplicateCandidate>` where
each candidate is a canonically-ordered pair plus typed `DuplicateEvidence`.
Bucketing is by `(kind, normalized name)` and by module-specific deterministic
keys, so cost is bounded by the largest bucket. Detection never merges and
never scores; merging stays `AssetService::merge_assets`. Archived assets are
reviewable by default (an archived loser is a legitimate merge, ADR 0005);
merged tombstones never are. There is no dismissal storage.

Documented trade-offs, same framing as the library list above:

- activity and duplicate queries compose in the application layer over
  `ActivityReader::list_all` and the module readers rather than pushing the
  filter into SQL. One query, no N+1; the follow-up if the activity log ever
  grows large enough to matter is a repository-level filtered page, not an
  adapter-side scan;
- duplicate bucketing is bounded by the largest bucket, so a very large bucket
  of identical names is still quadratic *within that bucket*. Real personal
  inventories do not produce that shape, and the alternative (a SQL GROUP BY)
  would move detection rules into storage.

Tests: `core/tests/relation_query_use_cases.rs`,
`core/tests/activity_duplicate_use_cases.rs`,
`storage-sqlite/tests/relation_query_contracts.rs`,
`storage-sqlite/tests/activity_duplicate_contracts.rs`, and the cross-module
`unified_core_works_end_to_end_across_modules` in `cli/tests/cli_e2e.rs`.

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
- docs/03 sketches dotted namespace names (`homebrew.formula`); the kernel
  validation rule forbids dots, so the implemented namespaces use underscores
  (`homebrew_formula`, `homebrew_cask`, `bundle_id`, `npm`, `pipx`).
- Discovery snapshots are ephemeral by decision (docs/09): no persistence,
  no background scan subsystem in Phase 2.
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
  semantics mirror SQLite transactions). Software coverage adds discovery
  classification, adoption (create/update/purpose-preservation/rollback),
  and scan-never-writes assertions.
- Providers: `providers/tests/` — fixture `.app` bundles, fake command
  runners for Homebrew/npm/pipx outputs (missing tool, failure, malformed
  JSON), read-only command assertions. No test depends on the host machine's
  installed software.
- SQLite contract tests: `storage-sqlite/tests/` — pragmas, migrations
  (fresh/reopen/tamper/1→2→3→4 upgrades with Media/Software/Relation data
  intact), CRUD, FK/unique enforcement, transaction atomicity, search +
  rebuild, portable round trip on a real database, relation constraints.
  The upgrade tests run against **real historical databases**:
  `tests/fixtures/phase1_media_only.db` was produced by the Phase 1 binary
  (commit `cdb0710`) through the CLI, and
  `tests/fixtures/phase2_software.db` by the Phase 2 binary
  (commit `4ff3119`), so each carries that release's actual layout,
  pragmas, migration ledger, and FTS5 shadow tables rather than a
  reconstruction from today's SQL. Their ledger checksums must equal
  `PHASE1_MIGRATION_0001_CHECKSUM` and `PHASE2_MIGRATION_0002_CHECKSUM`;
  regenerate either by checking out its commit in a worktree, building the
  CLI, and recreating the records with the same commands.
- CLI end-to-end: `cli/tests/cli_e2e.rs` drives the compiled binary through
  media, software, and service lifecycles: create → query → discover
  (fixture root) → adopt → re-adopt → relations → export → restore →
  idempotent re-import, plus dry-run/conflict/error behavior and read-only
  discovery. The export → restore → re-import chain covers Media, Software,
  and Services (including `modules/services.jsonl`); service coverage is
  CRUD, subscription billing (decimal → integer minor units), explicit
  patch/clear semantics, `service renew`, `hosted_on`/`points_to` relations
  and their view-time inverses, credential rejection, and
  archive-read-only behavior.
