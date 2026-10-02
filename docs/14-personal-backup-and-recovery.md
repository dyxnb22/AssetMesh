# Personal Backup and Recovery

AssetMesh provides automatic local recovery points and manual external copies.
Recovery policy lives in `BackupService`; consistent SQLite snapshots, private
filesystem writes, checksums and staged database selection live in the SQLite
backup adapter. Desktop commands call the application service.

## Schedule and retention

The desktop checks once at startup and every minute while open. Startup checks a
durable transaction revision, database identity/schema and saved preferences
against the latest snapshot. When unchanged, it validates that recovery point
and reuses it without another database copy. A missing old-style marker or a
damaged payload requires a new capture. Rollbacks do not advance the revision.
Subsequent changes, including writes from another SQLite connection and backed-up
preference changes, coalesce into a snapshot every 30 minutes. A normal application
exit flushes outstanding changes. A forced quit or power loss can therefore lose
changes since the last successful backup.

Keep the most recent 5 automatic snapshots plus the newest snapshot on each of
the last 7 calendar dates, deduplicating overlaps. Keep the most recent 3
manual/operation recovery points and 2 portable exports. Dates are UTC. The desktop records points
before applying a portable import, an Info CSV batch or an asset merge. Existing
file databases are snapshotted before pending migrations in the shared storage
opener, including CLI opens. A failed pre-operation backup blocks the operation.

The backup folder has a 256 MiB space budget. After applying the count/date policy,
evict the oldest unprotected packages until usage is within budget. Always retain
the latest automatic snapshot, latest manual/operation point and latest portable
copy. If these essential points or preserved libraries exceed the budget, keep
them and show the usage warning in Settings. This is a soft budget, not a reason
to delete the last usable recovery point. Successful creation needs temporary
space for one additional full snapshot or export before old copies are removed.

The first automatic pass creates a portable copy. Thereafter, a portable copy is
made at most once per week when a newer snapshot exists. An unchanged relaunch
does not create a newer snapshot or trigger another weekly export. Manual external
copies can create additional portable versions. No scheduler runs while the app
is closed; the next launch resumes checks. Maintenance also runs when there are no
new writes, at most once an hour while idle, so older policies and abandoned
staging do not persist indefinitely. Creating a new copy applies retention immediately.

Routine media state/progress/rating history retains the most recent 10,000 rows
from the last 180 days. A daily check deletes only these known routine event types,
after a complete \u0060before_history_cleanup\u0060 recovery point succeeds. Imported,
creation, merge, renewal, relation and unknown event types are preserved.
Database compaction requires at least 16 MiB and 25% free pages; contention postpones
it until a later daily check. Canonical records remain the source of current state.

CLI writes are observed while the desktop is open. The CLI itself has no periodic
scheduler or pre-import/merge hooks; its existing portable export remains usable.

## Recovery folder

Each originally configured database has a sibling `<database filename>.backups`
directory. Its location is shown in Settings. Every completed UUID folder contains
`backup.json` and either `library.db` or a `portable/` export directory. The backup
manifest includes time, reason, record count, the presence of API keys, checksums
and saved-filter/theme/language preferences. Incomplete `.partial-*` folders are
not offered as recovery points. Files are staged, synced, and renamed before they
are published. Package retention touches only completed UUID backup folders.

Under the same folder lock used by snapshot, export, restore and pre-migration
creation, remove owned staging directories and atomic JSON temporary files older
than 24 hours. Unknown names and symlinks are left alone. Restore-created databases
have a separate lifecycle: protect the selected database, the running database and
the most recently modified previous library. Other managed UUID databases become
eligible after 30 days without changes to the database, WAL or migration-backup
folder. Only remove them when SQLite can obtain an exclusive lock immediately;
open libraries are skipped. Remove their sidecars and owned migration recovery
folders too. The originally configured database and manually saved external copies
are outside this cleanup policy.

Snapshot databases are generated using SQLite's online backup API, including
committed WAL state. Copying a live `.db` file with filesystem copy is not used.
Snapshot restore preview verifies hashes, SQLite integrity and foreign keys;
portable preview verifies every exported file and the core import preflight.

Directory inspection isolates packages with missing, oversized, invalid or
mismatched manifests. They are reported individually in Settings and preserved;
other packages remain available for backup creation, retention and recovery.
Listing reads at most 256 KiB of each manifest, and does not replace the full
payload validation required for preview or restoration. A directory-wide I/O
failure still reports an error. Unreadable packages count toward space usage
and are never silently deleted by retention.

Unix recovery folders use 0700 and database/manifest/preference files use 0600.
External-copy contents use the same private permissions. A private enclosing
folder protects generated portable files. Permissions on other operating systems
follow that operating system's filesystem defaults.

## Restore workflow

Select a listed recovery point or choose a copied backup directory. Preview shows
its date, asset count and whether API key values are included. Applying requires
the preview fingerprint and explicit confirmation. The source is validated again;
a changed or corrupt backup cannot silently replace the library.

A usable current library receives a `before_restore` point. If initialization
failed, recovery still works and leaves the original database untouched. Recovery
creates a new UUID database in `backups/restored/`, exercises current migrations
and module validation on the new copy, then persists `active-library.json`. The
running factory stops accepting canonical writes. Quit and reopen AssetMesh to
switch to the restored library. The current source library is preserved immediately;
older restored libraries and backup packages follow the retention policy above.

Desktop initialization resolves the marker against the originally configured
path, including `ASSETMESH_DB`. The CLI does not resolve this desktop selection;
use the restored database path explicitly when accessing it from the CLI.

Saved filters, theme and language are restored from the package. Old migration
points without preference metadata cannot reconstruct earlier UI preferences.
Standalone portable export/import continues to carry canonical assets only;
personal recovery packages wrap that format with preferences and checksums.

## External copies and credential boundary

Settings shows the last successful backup, copy count and folder size. Appearance
and language are compact independent preferences; environment diagnostics are not
loaded by this page. History is collapsed behind Restore backup, which opens by
default when recovering from an unusable library. Manual creation and the folder
path are under Backup details. Errors and pending recovery remain visible.

Settings → Save a backup copy creates a complete portable recovery folder in the
chosen destination. It can be selected on another installation without depending
on the old folder's path. Local retention never deletes these external copies.
Use another disk or an existing backup service to protect against device loss;
AssetMesh does not upload or continuously synchronize files.

API key values remain user-owned Info data and are preserved in complete recovery
snapshots and recovery exports. Ordinary desktop/CLI portable exports exclude API
key assets, redirects to them, associated rows and history containing typed key
snapshots by default; including them requires the desktop choice or CLI
\u0060--include-api-keys\u0060 flag. This is not general redaction of text, notes or commands.
Detail views mask keys until explicitly revealed, but current databases
and backups are **not encrypted**. The Settings page identifies backups containing
keys. Keep external copies on encrypted storage. Keychain migration and password
protected archive export are separate future changes. No credentials are removed
or changed by this policy. Save credentials in project environment files instead
of service launch/stop commands, and review imported commands before execution.

On Unix, the desktop-owned data directory is 0700; the database, WAL/SHM and
ordinary export files are 0600. Export folders are 0700. Explicit user-selected
database parent directories are not chmodded.

## Verification

Real SQLite contracts cover WAL recovery, source preservation, unchanged identity,
UI preferences, damaged/stale backup rejection, coalescing, external writers,
normal-exit flushing, count/date and space retention, unchanged relaunches,
preference-only changes, durable revision rollback/reopen behavior, abandoned
staging, protected restored libraries, backed-up routine history cleanup,
failed-backup retry, legacy migration points and recovery into another installation.
Desktop contracts cover recovery from a
migration-checksum failure. UI workflows cover preview/confirmation, cancellation,
external copies, error visibility and default API key masking.
