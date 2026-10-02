# Developer Setup & Implementation Notes

This document covers how to build, test, and use the implemented core, CLI,
and Phase 5 Tauri desktop application, and records the concrete contracts the implementation
established on top of the architecture docs and ADRs.

The implemented library and desktop contracts are documented in
[Unified Library Core](docs/11-unified-library-core.md) and
[Desktop Contract](docs/12-desktop-contract.md). Choose follow-up work from an
actual personal-use need; the [Roadmap](docs/07-roadmap.md) records optional ideas.

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

## Daily checks and release checks

Development/test profiles disable incremental compilation to avoid retaining
large per-artifact incremental graphs. Keep build flags stable. After builds
and checks finish, \u0060npm run clean:dev-cache\u0060 removes reproducible debug/test
artifacts, preserving release bundles and the latest previous application.
The next development build will take longer because it rebuilds those artifacts.

Routine pushes and pull requests run frontend lint, typecheck, all Vitest tests,
the production build and Rust workspace contracts on macOS. `npm test` already
includes the frontend workflow suite; running it separately is optional.
Version tags (`v*`) and manual workflow dispatch additionally run Linux checks,
real Tauri WebDriver E2E and the 50k scale benchmark (`npm run test:scale`).
Scale tests are explicitly ignored by the ordinary workspace test run. These checks remain available
locally; they no longer run on every personal change.

## Test ownership

Keep each assertion at the boundary it verifies:

| Concern | Owning tests |
| --- | --- |
| Domain rules, matching, merge policy, traversal | Core use-case tests |
| Transactions, migrations, persisted queries, backup integrity | SQLite contracts |
| Desktop DTOs, command errors and real service wiring | Desktop Rust contracts against SQLite |
| Form payloads, canonical read-back and mutation receipts | Feature workflows; one representative successful metadata save in SoftwareWorkflow |
| Failure drafts, explicit retries and conflict reload | MutationPattern plus feature-specific progress/rating and Todo regressions |
| Cross-workspace navigation and interoperability | Frontend workflow gate; included once in `npm test` |

`FakeDesktopTransport` provides stateful CRUD for UI workflows and canned DTOs
for traversal, duplicate review, merge previews and portable imports. Configure
these responses explicitly (`query-fixtures.ts`); do not reproduce the core's
matching, graph walking, merge conflict rules or file validation in this double.
A fake passing is evidence about the UI, not about storage or domain correctness.
When testing read-back, use a returned canonical value that differs from the
draft instead of asserting an exact number of reads. Retain call counts where
the count *is* the behavior: duplicate-submit prevention, debounce and bounded
retries. Test technical disclosures through their visible summary controls.

Avoid tests that freeze copy, CSS choices, provider counts or an arbitrary minimum
number of translation keys. Cover defaults and read-only states through real UI
workflows instead of separate constant/banner tests. Keep dictionary completeness
and placeholder parity, currency formatting, failure recovery and data integrity
checks: these protect behavior and user-owned data rather than implementation text.

## Personal workflows

Media statuses allow direct correction, with timestamps managed by the core.
The frontend presents choices without mirroring a restrictive transition matrix.
Maintenance navigation lives under Tools; graph analyses, activity diagnostics,
record identifiers, revisions and runtime settings are collapsed by default.
Portable import previews a picked directory automatically and requires one Apply
click; preview fingerprints still bind the apply to the validated bundle. Merge
still requires choosing the survivor and reviewing impact, then one Merge click.
Software discovery supports checked adoption of new candidates. Each candidate
is reclassified transactionally; without an observed existing revision it cannot
update a newly matching asset. Partial failures remain selected for retry.
Preferences save after changes settle (250 ms), with at most three retries after
failure, with a final save on page hide or unmount. Scroll writes settle after 300 ms and flush on view change or page hide.

## Personal backup and recovery

See `docs/14-personal-backup-and-recovery.md` for scheduling, retention, recovery
format, restore selection and credential limitations. The desktop Settings page
now shows backup health and space usage, with manual snapshots and restore history
under their respective disclosure controls. The storage policy bounds counts and
space, reuses unchanged relaunch snapshots and cleans aged owned recovery residue.
`cargo test -p assetmesh-storage-sqlite --test backup_contracts` exercises real WAL,
portable recovery, retention, migration preservation and damaged backup rejection;
`desktop_backup_contracts` covers recovery when the source cannot be opened.

## Desktop run & package

### Startup measurements

The desktop records native setup, database readiness, WebView load, frontend
evaluation, shell paint, workspace mount and first-list paint. The latest launch
report is `startup-performance.json` in the application data directory. It contains
only version, UTC launch time, stage names and elapsed milliseconds. Report writes
run after workspace paint on background threads. The first-list marker applies to
launches that open a library or Todo view; a saved maintenance view instead reports
workspace mount. Native timings start when the application code begins, so they do
not measure LaunchServices or pre-main loading. Browser timings use that page's
`performance.now()` origin and should be compared within the same clock.

Set `ASSETMESH_STARTUP_TRACE=1` to also print each stage as JSON on stderr.
`ASSETMESH_STARTUP_PROFILE_DIR` overrides the report directory for measurements
against private database copies. Optional views and dialogs load on demand;
preference recovery runs alongside the initial library read, and preference saves
wait for recovery so a late read cannot overwrite new choices.

Library lists keep up to eight pages in memory for immediate return navigation;
each visit revalidates its page in the background. Media status counts load
independently and are reused across sorting and paging within a lifecycle/kind/tag
scope. Returning to a scope revalidates counts older than one minute. Mutations
invalidate other cached pages and relevant counts.
Media status/progress read-back can patch a fully loaded, non-search result
without another list query. Partial pages also avoid a query when the row still
matches and its place among unseen rows is certain (unchanged name/kind order,
or moving a known first-page row ahead of the old newest row). Other partial
pages and searches revalidate after the immediate patch; requests started before
a mutation cannot restore old values.

Local-service status and log polling is serialized and paused while the document
is hidden. Visible running/transitional processes use a 1.5-second interval;
idle processes use five seconds. Returning to the window and start/stop actions
refresh immediately. Unchanged snapshots skip React updates, and log tails
remain incremental and bounded to 1,000 lines.

```bash
npm run --workspace=apps/desktop tauri dev    # vite + window, reloads on UI edits
npm run --workspace=apps/desktop tauri build  # .app + .dmg under target/release/bundle/macos
```

No-terminal workflows: the repo root has two double-clickable launchers.
`1-打开AssetMesh.command` opens the installed app; `2-编译最新版并替换.command`
builds the app bundle (`tauri build --bundles app`, no DMG) and swaps it into
`/Applications`, pausing with the build log tail on failure.

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
│   ├── 0004_service_relations_v1.sql  # service relation types in the stored-type CHECK
│   ├── 0005_info_items_v1.sql         # reusable information details
│   └── 0006_local_service_launch_v1.sql # local-service launch fields (services schema v2)
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

## Implementation contracts

| Concern | Maintained source |
| --- | --- |
| Identity, merge, relations and shared model | [Domain Model](docs/03-domain-model.md), [ADRs](docs/adr/README.md) |
| Versions, concurrency, bundle format and recovery | [Storage & Portability](docs/05-storage-and-portability.md), [Backup](docs/14-personal-backup-and-recovery.md) |
| Media status/progress/rating and legacy import | [Media](docs/08-media-records-v1.md) |
| Discovery, classification and adoption | [Software](docs/09-software-inventory-v1.md) |
| Billing, renewals, credential boundaries and Service merge | [Services](docs/10-services-subscriptions-v1.md) |
| Reusable values, CSV and API key exports | [Info](docs/13-reusable-information-v1.md) |
| List/search, traversal, activity and duplicate evidence | [Unified Library](docs/11-unified-library-core.md) |
| IPC, receipts and frontend mutation behavior | [Desktop Contract](docs/12-desktop-contract.md) |

Update the owning contract when behavior changes instead of adding a second
version here. Current schema values live in migration files and module constants;
DB/export/module version axes are defined in ADR 0008.

## Historical migration fixtures

Keep the real Phase 1 and Phase 2 database fixtures. They prove upgrades against
past binaries, not reconstructed schemas. Provenance, frozen checksums and
regeneration commands live in the
[fixture README](crates/storage-sqlite/tests/fixtures/README.md).

## Implementation notes

`TagRepository::ensure` generates a UUIDv7 tag ID in the storage adapter. External
IDs are never used as canonical IDs. The other layer exceptions and data-format
constraints are documented in their owning contracts above.
