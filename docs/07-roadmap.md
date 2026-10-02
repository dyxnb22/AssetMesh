# Roadmap

AssetMesh is maintained for personal use. This document records current coverage
and optional directions; a future idea is not an instruction to implement it.

## Implemented

| Area | Current behavior | Contract |
| --- | --- | --- |
| Shared core | Stable identity, typed details, tags, external references, explicit merges, activity and rebuildable search | [Domain](03-domain-model.md), [ADRs](adr/README.md) |
| Media | Create/edit, direct status correction, progress, ratings and legacy import | [Media](08-media-records-v1.md) |
| Software | Manual records, read-only discovery, classification and explicit adoption | [Software](09-software-inventory-v1.md) |
| Services | Subscription metadata, minor-unit costs, explicit renewals and field-by-field merge review | [Services](10-services-subscriptions-v1.md) |
| Unified library | List/detail/search, bounded relations, activity filters and duplicate evidence | [Library](11-unified-library-core.md) |
| Desktop | Tauri client over the core, editable forms, receipts, recovery flows and portable data tools | [Desktop](12-desktop-contract.md) |
| Reusable information | Email, URL, API key and text records, asset links and CSV creation | [Info](13-reusable-information-v1.md) |
| Personal workflows | Media-derived Todo list, revision-safe undo, saved filters, navigation/scroll restoration and collapsed maintenance controls | [Development](../DEVELOPMENT.md#personal-workflows) |
| Durability | Automatic snapshots, operation recovery points, external copies and validated restore with preferences | [Backup](14-personal-backup-and-recovery.md) |

The earlier Phase 0–5 implementation sequence is complete. Its completed task
cards and repeated exit checklists have been removed; the contracts above describe
what must remain compatible.

## Follow-up candidates

Choose these only when they solve a concrete personal-use problem:

- Faster capture and recurring maintenance of existing records.
- Actionable subscription reminders and attention items, with user-chosen timing.
- A legacy-import policy for older exports that would overwrite newer media status
  or start/completion timestamps. Progress counts already merge without decreasing;
  status/timestamp provenance still needs an explicit product decision.
- Media metadata, episode tracking, credits or collections if actual library use
  calls for them. These require their own small data/migration design.
- Runtime observations for selected software/services, or projects/learning records,
  when there is an actual data source and a useful workflow.

## Scope boundaries

Keep changes focused on existing workflows and durable personal data. Do not add
cloud accounts, multi-user collaboration, sync/CRDTs, a plugin marketplace,
HTTP/MCP servers, a mandatory daemon, vector-search infrastructure or a durable
job queue without a separate concrete requirement.

Do not automatically delete/uninstall software, renew subscriptions, merge
heuristic matches or promote provider output to canonical truth. Existing ADRs,
portable formats and migration fixtures remain compatibility requirements.
