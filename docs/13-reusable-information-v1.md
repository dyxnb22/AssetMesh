# Reusable Information V1

The Info module records small values that can be copied and reused: email addresses,
URLs, API keys, and arbitrary text. Each item is one `info.item` Asset with a name,
typed `InfoRecord` (`info_type`, `value`, optional `notes`), tags, activity, and a
search projection. The value is stored once; existing Asset relations can connect
the same item to multiple Services, Software assets, or other items. A changed value
is visible wherever that item is opened through its relation.

The desktop Library has an Information view for creating, finding, editing,
copying, archiving, and linking items. The CLI provides `info add`, `info update`,
`info get`, and `info list`; the shared `relation` commands attach links. The
existing relation explorer can attach and inspect the links in the desktop UI.
The editor can replace an item's tags after creation, and notes accept line breaks.

The Information view also accepts a CSV file or pasted CSV for bulk creation. Required headers
are `name,type,value` (or `info_type` in place of `type`); optional headers are
`notes,tags`. A `tags` cell separates multiple names with `|`. Standard CSV
quoting permits commas and line breaks in values. The desktop previews up to
500 items per batch before applying them; the core validates and commits the
whole batch in one transaction. Bulk import creates new items and does not
update existing items.

Library filters can be saved with a name and restored later. Saved filters are
stored in this desktop client's local preferences; they are not part of the
standalone portable asset bundle. Personal recovery packages additionally preserve
saved filters, theme and language; see `docs/14-personal-backup-and-recovery.md`.

The value is free text so a URL can include any syntax the user wants to remember,
and a key can contain line breaks. Names and tags are useful search terms. Email,
URL, and text values are indexed; API key values are omitted from the search
projection because searching by their name is the intended workflow.

The portable bundle declares `info` schema version 1 and writes
`modules/info.jsonl`. Import checks its record count, asset ownership, and value
invariants before committing. Bundles from before this module leave existing Info
records in the destination untouched. API key values are included in portable
exports by design: exporting the information library should preserve its data.

ADR 0010 continues to apply to Services: API keys do not become Service fields.
The Info module owns these user-entered values independently.
