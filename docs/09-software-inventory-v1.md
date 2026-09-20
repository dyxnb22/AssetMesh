# Software Inventory V1

Phase 2 delivers the **Software Inventory** vertical slice: a second, substantially
different asset domain proving the shared AssetMesh kernel — identity, persistence,
search, portability, activity, relations, and the application-layer contracts —
without introducing UI assumptions or weakening Media (docs/08).

Headless by design: everything in this document is operable through application
services and the CLI. No desktop UI is required (Phase 5 owns presentation).

## Canonical model

A software item is a shared `Asset` (ADR 0003) plus module-owned typed details:

```text
SoftwareRecord
- asset_id            # canonical identity; the Asset holds name/summary/lifecycle
- category            # application | cli | package | runtime | tool
- install_source      # macos_app | homebrew_formula | homebrew_cask | npm_global
                      # | pipx | manual | system | unknown
- version?            # ≤ 128 chars
- install_location?   # ≤ 1024 chars
- executable_path?    # ≤ 1024 chars
- purpose?            # USER-OWNED "why installed" field (see below)
- notes?              # USER-OWNED
- discovered_at?      # first observation by discovery
- installed_at?       # only when reliably known; never guessed from discovery
- architecture?       # ≤ 1024 chars, only when useful/reliable
```

Asset kinds map 1:1 to categories (`software.app`, `software.cli`,
`software.package`, `software.runtime`, `software.tool`) exactly like Media kinds
map to media types. Kind and category compatibility is enforced on every write
path.

The module data schema version is `software schema_version = 1`
(`domain::software::SCHEMA_VERSION`), independent of the SQLite migration version
and the portable export format version (ADR 0008).

## "Why installed" / purpose

`purpose` (and `notes`) are durable, user-controlled fields answering
*why do I have this?* — e.g. "Java development", "Required by AssetMesh",
"Installed for one old project".

Rules enforced in the adoption service and tested:

- discovery/adoption **never** writes `purpose` or `notes` from provider data;
- only explicit user overrides write them (an override always replaces);
- the update path only FILLS fields the canonical record lacks — an existing
  version, location, or architecture is never overwritten by a candidate.

## Discovery lifecycle

```text
provider scan (outside any transaction — ADR 0007)
        ↓
SoftwareCandidate (advisory DTO; never canonical)
        ↓
classify against canonical state (ADR 0005 precedence)
        ↓
ScanReport (New / ExactMatch / PotentialDuplicate / Conflict)
        ↓
review — no scan ever writes canonical data or activity
        ↓
adopt_candidate(candidate, user_overrides)  — ONE short transaction
        ↓
canonical Asset + SoftwareRecord + refs + activity + search projection
```

### Candidate model

A candidate carries only what matching/review/adoption needs:

```text
SoftwareCandidate
- provider            # e.g. "macos_applications"
- display_name
- category, install_source
- version?, install_location?, executable_path?
- external_refs[]     # deterministic namespaced identifiers
- metadata?           # provider-specific JSON; never promoted automatically
```

Candidates are **ephemeral**: scans produce in-memory reports, and the CLI
adopt command re-scans the provider to resolve its selector. No discovery
snapshot is persisted — Phase 2 has no concrete need for durable scan state, and
it would be rebuildable cache by definition (ADR 0009). This is a deliberate
decision, not an omission; revisit only if a real workflow needs scan history.

### Matching / classification (ADR 0005 precedence)

1. **Exact namespaced external references.** All candidate refs must agree on
   one owner; that owner must be a software-kind asset → `ExactMatch`.
   Refs pointing at different assets, or at a non-software asset, →
   `Conflict`.
2. **Deterministic module key.** Software's deterministic identity IS its
   external refs; no separate key exists.
3. **Heuristic** — normalized display-name similarity (casefolded,
   whitespace-collapsed) against active software assets →
   `PotentialDuplicate(asset_ids)`. Review-only.

Heuristics never silently merge or mutate canonical assets. `Auto` adoption of a
`PotentialDuplicate` or `Conflict` fails loudly; the caller must pass an explicit
target (`--new` to create a separate record, or `--as <asset_id>` to adopt into a
chosen asset).

### Adoption

`adopt_candidate` re-classifies the candidate INSIDE the commit transaction so
the decision and the write observe the same state, then:

- **create** (disposition `New`, or explicit `--new`): new Asset + SoftwareRecord,
  attach unowned deterministic refs, apply overrides, set `discovered_at`,
  append `asset.created` + `software.adopted`, update the projection;
- **update** (disposition `ExactMatch`, or explicit `--as`): fill missing
  fields only, attach missing refs (a ref owned by a third asset is a hard
  conflict), apply explicit overrides, append `software.adopted` with the
  updated-field list, update the projection.

Adoption is idempotent in the strict sense: re-adopting a candidate that
would change nothing is a true no-op — no revision bump, no timestamp churn,
no activity event, no orphan tags. Provider objects never touch repositories;
the only path to canonical state is this application use case.

## Discovery sources

| Provider | Name | Source | Ref namespace | Notes |
| --- | --- | --- | --- | --- |
| macOS applications | `macos_applications` | `.app` bundles under `/Applications` and `~/Applications` (configurable roots) | `bundle_id:<CFBundleIdentifier>` | Info.plist only, no privileged APIs; malformed bundles skipped; system apps under `/System` deliberately not scanned |
| Homebrew | `homebrew` | `brew info --json=v2 --installed` (read-only) | `homebrew_formula:<name>`, `homebrew_cask:<token>` | Formula → category `package`, cask → `application`; installed version preferred; unavailable/failed/malformed output is a typed `ProviderUnavailable` error, never partial state |
| CLI tools | `cli_tools` | `npm ls -g --depth=0 --json`, `pipx list --json` | `npm:<name>`, `pipx:<package>` | Category `cli`; a missing tool is skipped silently (advisory), a present-but-failing tool is a typed error |

All providers run behind a `CommandRunner`/root seam (crates/providers); tests
use fixtures and never depend on the host machine's installed software. No
package-changing command is ever executed — discovery is read-only.

### External-ref namespace choices

Namespace strings follow the kernel validation rule (lowercase ASCII letters,
digits, `_`, `-` — no dots), so the docs/03 sketch names were realized as
`bundle_id`, `homebrew_formula`, `homebrew_cask`, plus `npm` and `pipx`. There is
deliberately no `path:` namespace: the external-id rule forbids whitespace, and
paths are unstable — locations belong in the record's `install_location` /
`executable_path` fields instead. External IDs remain aliases; AssetMesh IDs are
the only primary keys (ADR 0005).

## Relations

Phase 2 adds minimal **shared** relation infrastructure (docs/03 registry):

```text
Relation
- id, source_asset_id, target_asset_id   # shared Asset IDs only
- relation_type                          # registry-checked
- note?
- provenance                             # manual | discovered | imported
- created_at
```

Registry (inverse ↔ inverse, symmetric reads identically both ways):

```text
depends_on    ↔ dependency_of
uses          ↔ used_by
installed_via ↔ installs
related_to    ↔ related_to      # symmetric; stored in canonical endpoint order
```

Every fact has exactly ONE canonical row representation, enforced at every
write path (service attach, merge re-pointing, portable import) AND at the
storage level:

- inverse-pair types are never stored. `dependency_of`, `used_by`, and
  `installs` are view-time derivations of their primary; a fact stated with an
  inverse type is stored as its primary with the endpoints swapped
  (`B -dependency_of-> A` and `A -depends_on-> B` are the same row);
- symmetric `related_to` stores the lexicographically smaller endpoint as
  source;
- migration 0002 therefore CHECK-constrains stored `relation_type` to
  `('depends_on', 'uses', 'installed_via', 'related_to')` and
  `UNIQUE(source, target, relation_type)` — a duplicate or inverse
  representation is unrepresentable even to a direct SQL write. The CHECK set
  must stay in sync with the registry.

Views resolve the effective type per endpoint (`A -depends_on-> B` reads as
`dependency_of` from B). Stating an existing fact again — from either
endpoint, via either type of the pair — is a conflict, not a second row.
Self-relations are rejected. Storage is one shared `relations` table — no
module-private coupling. Explicit merges re-point relations touching the loser
at the winner, re-normalize them to canonical form, and drop duplicates and
self-loops. The Phase 4 relation query/explorer layer is not implemented here.

## Activity

Conservative, matching the Media policy:

- `asset.created` + `software.created` — manual creation;
- `asset.created` + `software.adopted` — adoption (create path);
- `software.adopted` — adoption update path (payload lists updated fields);
- `relation.created` / `relation.removed` — relation lifecycle;
- metadata-only edits emit **no** event;
- discovery scans emit **no** event (one event per transient candidate would
  flood activity; canonical adoption is the meaningful moment).

## Search projection

Software participates in the shared `SearchDocument` projection (ADR 0006):

- title: asset name;
- subtitle: `CLI Tool · 14.1.0` (category label + version when known);
- body: purpose — install source label — install location — notes;
- keywords: tags and external aliases (`homebrew_cask:iterm2`).

The projection is rebuilt from canonical state by the shared rebuild use case;
deleting the index never loses software data. Raw provider payloads are never
indexed.

## SQLite layout (migration 0002)

```sql
software_records (asset_id PK → assets ON DELETE CASCADE,
                  category CHECK, install_source CHECK, version,
                  install_location, executable_path, purpose, notes,
                  discovered_at, installed_at, architecture)
module_metadata  += ('software', 1)
relations        (id PK, source/target → assets ON DELETE CASCADE,
                  relation_type CHECK, note, provenance CHECK,
                  created_at, CHECK (source <> target),
                  UNIQUE (source, target, type))
```

Opening a database validates both module schema versions; a Phase 1 database
migrates forward by applying 0002 (existing Media rows untouched), and an
already-applied migration with a changed checksum still fails loudly.

## Portable data

The bundle gains `modules/software.jsonl` (schema_version 1) and
`relations.jsonl`, both as `*V1` wire DTOs. Export always declares both
sections; import treats them as **declared → authoritative**:

- a declared section must be present, count-checked, and reconciles destination
  state for bundled assets (details the bundle no longer carries are removed);
- an **absent section means the bundle predates Software/Relations** (a Phase 1
  export): it imports cleanly, creates no software state, and leaves any
  pre-existing destination data of that kind untouched;
- a section file present WITHOUT its manifest declaration is corruption and
  fails loudly.

Compatibility policy: backward compatible by declaration rather than a format
bump — the top-level export format stays version 1, and historical Media-only
bundles remain readable. All Phase 1 properties hold: stable IDs, validation
before mutation, deterministic restore, idempotent re-import, no search
data/discovery cache/secrets exported, crash-recoverable bundle writes.

## CLI

```bash
assetmesh software add --name "ripgrep" --category cli \
    --install-source homebrew-formula --version 14.1.0 \
    --purpose "fast search" --tag dev --ref homebrew_formula:ripgrep
assetmesh software list [--category cli] [--install-source ...] [--tag ...] [--json]
assetmesh software get <id-or-prefix>
assetmesh software update <id> [--version --purpose --notes ...]
assetmesh software search <query>

assetmesh software discover macos [--root <dir>]   # read-only; prints classified candidates
assetmesh software discover homebrew
assetmesh software discover cli
assetmesh software discover all

assetmesh software adopt macos bundle_id:com.fixture.app [--root <dir>]
    [--new | --as <id>] [--name --version --purpose ... --tag ...]

assetmesh relation add <source> <type> <target> [--note ...]
assetmesh relation list <asset>
assetmesh relation remove <relation_id>
```

Discovery output clearly separates candidates (`[new] FixtureStudio …`) from
canonical records; a discovery-only command never mutates state. Exit codes and
ID-prefix handling follow the established CLI conventions.

## Required tests

Delivered alongside the implementation:

1. domain invariants (category/kind mapping, install-source parsing, field
   bounds, control-character rejection);
2. use cases over in-memory ports — CRUD, filters, scan-never-writes,
   classification (new/exact/duplicate/conflict), adoption create/update,
   purpose preservation, rollback, activity counts, strict no-op re-adoption,
   inverse-relation deduplication, direct-write category/kind rejection;
3. provider tests with fixtures — `.app` normalization, malformed metadata,
   bundle-id dedupe, Homebrew JSON (formula/cask/partial/malformed/unavailable),
   npm/pipx outputs, read-only command assertions;
4. SQLite contract tests — software CRUD, ref uniqueness, search + rebuild,
   migration 1 → latest with Media data intact, reopen, relation constraints,
   portable round trip including software;
5. portable tests — software/relations round trip, legacy Phase 1 bundle
   compatibility, undeclared-section rejection, unsupported schema version,
   malformed/dangling/duplicate rows, idempotent restore, dry-run/commit
   symmetry, rebuild-after-wipe;
6. CLI end-to-end — add → list → get → update → search → discover (fixture
   root) → adopt → re-adopt → relations → export → restore → verify, plus
   read-only discovery and error behavior.

## Known limitations / deferred

- No durable discovery snapshots, background scans, or job queue (Phase 6 may
  introduce a job boundary if workloads demand it).
- No runtime/process/port monitoring, install/uninstall actions, or automatic
  reconciliation (Phase 6; explicitly out of scope).
- Homebrew semantics: formulae are categorized `package` uniformly; casks
  `application`. Refining categories per formula is future work.
- `installed_at` is never inferred from discovery; only explicit input sets it.
- CLI-tool discovery covers npm/pipx only; PATH crawling is deliberately
  avoided.
- Relation query services, traversal, and impact analysis are Phase 4.
- Desktop UI for software is Phase 5.
