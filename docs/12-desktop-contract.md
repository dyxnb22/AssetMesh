# Desktop Contract

This is the maintained contract for the implemented Tauri desktop adapter. The
completed Phase 5 task cards and speculative Phase 6–7 execution plans have been
removed. Current priorities live in the [Roadmap](07-roadmap.md).

## Application boundary

- Tauri commands call application services composed in `DesktopState`; adapters
  do not join repositories or SQLite tables themselves.
- Command DTOs use closed enums/tagged unions. Detail results use typed module
  variants; unknown detail formats have a readable fallback.
- The core owns matching, lifecycle validation, inverse relations, traversal,
  duplicate evidence, imports and merge policy. React presents returned results.
- Available capabilities control navigation; do not add placeholder pages for
  unimplemented modules.
- Library and activity queries support typed filters and bounded pagination.
  Activity kind filters remain distinct from module filters: a module can own
  several kinds. Relation queries retain explainable paths and truncation.
- Duplicate scans return an unknown total when the pair budget is reached;
  both the desktop and CLI label these results as partial. Provider equality
  alone never produces a duplicate candidate.

The concrete command registry, DTOs and error categories live in
[`src-tauri/src/lib.rs`](../apps/desktop/src-tauri/src/lib.rs),
[`src-tauri/src/dto`](../apps/desktop/src-tauri/src/dto/mod.rs) and
[`src-tauri/src/error.rs`](../apps/desktop/src-tauri/src/error.rs).
Frontend transport types are in
[`transport.ts`](../apps/desktop/src/features/library/transport.ts) and
[`types.ts`](../apps/desktop/src/features/library/types.ts).

## Mutations and recovery

- Mutations of existing assets carry `expected_revision`. A stale revision fails
  rather than silently overwriting newer data.
- A mutation returns a receipt with its operation, affected IDs, resulting revision
  and warnings where applicable. The UI invalidates/re-reads canonical data before
  displaying the saved result; drafts are not a substitute for read-back.
- Validation/availability failures preserve editable drafts and allow an explicit
  retry. Conflicts preserve the draft until the user requests a reload.
- Duplicate submission is blocked while a mutation is pending. Unchanged updates
  remain distinguishable from a successful change.
- Archive makes records read-only; merged records can navigate to their survivor.
- Merge requires an explicitly selected survivor and a preview. Import apply is
  bound to the inspected source and fingerprint; changing the source makes that
  preview stale. Operation recovery points follow the
  [backup contract](14-personal-backup-and-recovery.md).
- Adapter errors use a closed taxonomy and useful recovery actions, without
  exposing SQL or credential details.

## Local-service runtime

The desktop adapter owns a process runtime for `local` services, as commands
`service_runtime_start|stop|restart|status|statuses|logs` over an in-memory registry:

- Start runs the saved command via `/bin/bash -c` in the saved project
  directory, in its own process group. A second start (including a concurrent
  one) is refused with a conflict while a process is held; when the access
  address names a loopback port that is already listening and no process is
  held, the start is refused as an apparent external instance — external
  processes are never signalled by a probe or inferred PID. A user-configured
  stop command may explicitly control an existing service.
- Stop escalates SIGTERM → SIGKILL per process group, so the command's own
  subprocess tree is cleaned up. Only processes this registry spawned are
  ever signalled. The loopback address of an access URL is probed — never
  scanned — for exactly two read-only purposes: refusing a duplicate start
  and reporting the `external` observation below. A probe targets exactly
  the configured address (only `localhost` may try both stacks), is bounded
  by a per-attempt connect timeout, and never signals anything. Explicit
  default ports (`:80` / `:443`) are included; an address without an explicit
  port is not probed.
- Each TCP attempt waits at most 300 ms. List reads share a 1500 ms TCP-wait
  budget, starting after the database read. Remaining time is divided among
  remaining records and, for `localhost`, among remaining IP addresses, so
  earlier timeouts cannot leave later records unprobed. Each attempt is
  capped by its remaining share; thread scheduling and result processing are
  outside the TCP-wait budget. A budget exhausted before an attempt produces
  a retryable `unavailable` error, rather than a false `stopped` observation.
  The frontend retains its last successful snapshot on a failed poll and
  updates it when polling succeeds again. Canonical read errors also propagate.
- States (`stopped | starting | running | stopping | failed`) are reported by
  the backend from the actual child; `running` means the process is alive,
  not that a health check passed. One read-time addition: when no process is
  held for an active `local` service whose access address answers on
  loopback, status reads report `external` — an apparent instance running
  outside AssetMesh. `external` is recomputed on every read, never stored,
  carries no pid, and can be controlled through its configured stop command; a held registry entry (including a
  finished run) always wins over the probe. Captured stdout/stderr is bounded
  (line count and bytes) and exposed incrementally. Log responses carry a run
  ID; a cursor from an earlier run returns the current run from its beginning.
- An exited group leader remains unreaped while its descendants are cleaned
  up, reserving its PID against reuse. `stopping` remains managed until cleanup
  finishes; stop, archive and exit wait for that cleanup.
- Runtime state is never persisted. Quitting AssetMesh stops every process it
  started; restoring a backup or importing a bundle never starts anything.
- Stop executes optional `stop_command` via `/bin/bash -c` in `project_dir`,
  with a bounded 10-second command wait. A failed or timed-out command returns
  an actionable error. For managed services the owned group is then cleaned
  up; an unmanaged service requires an explicit stop command. No PID/name
  scanning is used. An unmanaged address must stop answering within 3 seconds.
- Restart validates launch configuration before stopping, waits for owned group
  cleanup and address release, then executes the saved start command. Failure
  during stopping never starts a replacement. Stop and restart run on blocking
  workers and serialize with canonical service mutations.
- Launch configuration (`project_dir`/`start_command`/`stop_command`) is frozen while the
  process is live. Equal launch values and unrelated metadata remain editable.
  Archiving validates the observed revision before stopping the process.
  Start and canonical service writes are serialized, so start cannot race an
  archive. Merge requires both services stopped; import requires all managed
  local services stopped before replacing library records.
- `service_open_page` resolves the canonical HTTP(S) access address and asks
  the operating system to open it in the default browser. Local-service edits
  expose the address without adding subscription billing fields.
- Where process groups do not exist the commands return `unsupported` and
  `features.local_service_runtime` is `false`, so the UI offers no controls
  on those platforms.

## Personal interface

### macOS menu bar

The native menu bar provides a service summary, up to 12 local-service menus,
access-page opening, start/stop/restart, service details, the main window and
explicit Quit. It lists active local records in stable name/ID order; View all
services opens the full list only when there are more than 12 services. The menu
shows an additional message only during an operation or after an error.
Running counts include observed external instances,
which are labelled separately and require a configured stop command for control.
Running means the process is alive, not that its application is healthy.

Native Rust refreshes status independently of the WebView, every five seconds
after each completed read and immediately after menu actions. Reads do not
overlap; unchanged service sets update the existing native menu items in place.
A failed read retains the last snapshot, marks it unavailable, disables runtime
actions and retries. Failed actions remain visible in the menu until the next
successful menu action; an exclamation mark on the icon draws attention to failed
services, actions or status reads. The template icon follows the system appearance,
and menu text follows the saved Chinese/English interface preference.

Closing the main window hides it while AssetMesh and its services keep running.
The menu's Open AssetMesh and the Dock reopen that same window. Explicit Quit
(including Cmd+Q) stops managed process groups and flushes the final backup;
external instances are left alone. Queued service operations are rejected once
shutdown begins. Both exit-request and final-exit events perform cleanup, including
native macOS termination paths that skip the request; the final backup runs once.
If creating the native menu fails, normal window closing behavior
is retained. Other platforms retain their existing window behavior.

### Main window

The primary views are the library, media Todo list, software, subscriptions,
services and reusable information. The services page manages locally started
projects (name, state, start/stop, access address, launch configuration,
recent logs); the subscriptions page keeps the billed services with their
plan, cost, cadence and renewal records. Both are views over the same
services module split by kind — `service.local` versus the rest — so asset,
search, archival and detail capabilities are shared, not duplicated. The local-services
workspace lists active records and offers runtime-state filters, search and refresh.
Archived and merged records remain accessible through the library's lifecycle filters.
Tools contains relations, activity, duplicates and portable data. Advanced
analyses, identifiers, revisions and technical settings stay collapsed.
The Todo list is derived from media status and has no separate task database.

The window uses a unified toolbar with sidebar and inspector toggles. On macOS,
the system window controls remain native and the titlebar overlays the toolbar;
the sidebar uses a native window material. Other platforms retain their system
titlebar. The macOS material uses Tauri's transparent-window support and its
`macos-private-api` feature; an App Store distribution would need to replace that
path. Solid content surfaces and reduced-transparency styling keep text legible.

Todo search, kind filtering and ordering apply before pagination, within active
media in progress, independently of remembered library filters. Completing a row
uses an explicit trailing action; returning it to planned lives in its action
menu. Completion supports undo and advances keyboard focus to the next row.
The inspector can close, reopen and resize without changing selection. Its media
progress editor uses revision-safe writes and canonical read-back; failed drafts
remain available for retry. Artwork is a placeholder because the current record
format has no cover field; previews never become library data. Search supports
Cmd/Ctrl+F and Cmd/Ctrl+K; new records support Cmd/Ctrl+N in library and Todo views.

Rows support selection, keyboard navigation and opening details. Empty views offer
an action; errors offer recovery. Appearance and language choices persist. Focus
remains visible and narrow windows keep the selected record accessible. Concrete
layout and color tokens belong in the UI source rather than a frozen task checklist.

## Verification

Frontend workflows verify payloads, returned canonical data, failure recovery and
user actions. Rust desktop contracts verify service wiring against real SQLite.
The fake transport does not prove domain/storage correctness. Daily and release
checks, and the ownership of each test layer, are documented once in
[DEVELOPMENT.md](../DEVELOPMENT.md#test-ownership).
