# AssetMesh

AssetMesh is a local-first personal library for media, software, services and
reusable information. It keeps records searchable, connects related assets, and
preserves your data through portable exports and local recovery copies.

## Everyday use

- Track media status, progress and ratings. The Todo list shows media in progress;
  completing a Todo completes the media record, and cancelling returns it to planned.
- Record installed software and why you use it. Discovery scans macOS apps,
  Homebrew and npm/pipx tools; reviewed candidates can be adopted explicitly.
- Keep service and subscription details, costs and renewal history.
- Start and stop configured local services. On macOS, the menu bar shows their
  status and provides quick controls; closing the window keeps them running,
  while quitting stops the services AssetMesh started.
- Store reusable email addresses, URLs, API keys and text, with copy/edit flows and
  links to assets. API key values are masked in the UI but are not encrypted at rest.
- Search and filter the library, save filters and reopen the previous view.
- Use Tools for relations, activity, duplicate review and portable import/export.
  Advanced controls and technical settings are collapsed by default.
- Recover through automatic snapshots, pre-operation recovery points, external
  backup copies and validated restoration into a new library. See
  [Personal Backup & Recovery](docs/14-personal-backup-and-recovery.md).

## Implementation

```text
AssetMesh/
├── apps/desktop/          # Tauri 2 + React + TypeScript
├── crates/core/           # domain, application services and ports
├── crates/providers/      # read-only software discovery
├── crates/storage-sqlite/ # SQLite, migrations and rebuildable search
├── crates/cli/            # CLI over the same application services
└── migrations/            # immutable, checksummed SQL migrations
```

Desktop and CLI call the same application layer. Canonical writes, matching,
merge rules and cross-module queries stay in the core. AssetMesh owns stable
asset IDs; external IDs are namespaced aliases. Discovery is advisory and merges
are explicit. Search and provider cache can be rebuilt; portable exports preserve
canonical data independently of the SQLite layout.

The app runs locally without a backend server. HTTP/MCP servers, runtime
monitoring, sync, dynamic plugins and durable job queues are not implemented.
Optional directions are recorded in the [Roadmap](docs/07-roadmap.md), rather than
being automatic next tasks.

## Build and verify

```bash
cargo build
cargo test --workspace
npm ci
npm run typecheck
npm run lint
npm test
npm run build
```

Frontend tests use jsdom and a fake transport; desktop Rust contracts use real
SQLite. Linux window tests and scale benchmarks run on manual CI or version tags.
See [DEVELOPMENT.md](DEVELOPMENT.md) for setup, commands and test ownership.

## Documentation

- [Architecture](docs/02-architecture.md) and [Domain Model](docs/03-domain-model.md)
- [Storage & Portability](docs/05-storage-and-portability.md)
- [Roadmap](docs/07-roadmap.md)
- [Media Records](docs/08-media-records-v1.md)
- [Software Inventory](docs/09-software-inventory-v1.md)
- [Services & Subscriptions](docs/10-services-subscriptions-v1.md)
- [Unified Library Core](docs/11-unified-library-core.md)
- [Desktop Contract](docs/12-desktop-contract.md)
- [Reusable Information](docs/13-reusable-information-v1.md)
- [Personal Backup & Recovery](docs/14-personal-backup-and-recovery.md)
- [Architecture Decisions](docs/adr/README.md)
- [Developer Setup](DEVELOPMENT.md) and [Contributing](CONTRIBUTING.md)
