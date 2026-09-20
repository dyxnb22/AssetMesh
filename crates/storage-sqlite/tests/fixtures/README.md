# Historical migration fixtures

These databases are **binary artifacts produced by real past releases**, not
reconstructions from today's migration SQL. The upgrade tests in
`../sqlite_contracts.rs` open them with the current binary to prove that a
database written by an older AssetMesh migrates forward without losing or
corrupting historical data.

A fixture carries that release's actual layout, pragma settings, migration
ledger (with its frozen checksums), FTS5 shadow tables, and row values — none
of which can be reproduced by re-running the current migration SQL, because
the SQL has since changed shape or the schema it describes no longer matches
what that release wrote.

| Fixture | Produced by | Ledger | Proves |
| --- | --- | --- | --- |
| `phase1_media_only.db` | Phase 1 binary, commit `cdb0710` | v1 | migration 0002 preserves Media data |
| `phase2_software.db` | Phase 2 binary, commit `4ff3119` | v1, v2 | migration 0003 preserves Media/Software/Relation data and adds Services |

## Why the checksums matter

Each applied migration records a SHA-256 of its SQL. On open, the runner
refuses to start if an already-applied migration's checksum changed, so
editing `migrations/0001_core_media_v1.sql` today would silently break every
existing installation's upgrade path. The tests assert the fixtures carry
`PHASE1_MIGRATION_0001_CHECKSUM` / `PHASE2_MIGRATION_0002_CHECKSUM`: if a
fixture is ever regenerated from *current* SQL, those assertions fail loudly,
because a regenerated file would not carry the historical checksums.

## Regenerating `phase2_software.db`

From the repository root:

```bash
# 1. Check out the Phase 2 release commit in a worktree (leaves main alone).
git worktree add /tmp/assetmesh-phase2 4ff3119

# 2. Build that release's binary.
cargo build --manifest-path /tmp/assetmesh-phase2/Cargo.toml --bin assetmesh

# 3. Create the records through the CLI, exactly as the fixture holds them.
BIN=/tmp/assetmesh-phase2/target/debug/assetmesh
DB=/tmp/phase2_software.db
rm -f "$DB"

# Media (Phase 1 module).
$BIN --db "$DB" media add --title "Frieren: Beyond Journey's End" --media-type anime \
    --status in-progress --progress-current 72 --progress-total 100 --progress-unit episodes \
    --rating 9 --tag anime --tag fantasy --ref tmdb:123456 \
    --summary "An elf mage's journey after the hero's party disbands."
$BIN --db "$DB" media add --title "Oshi no Ko" --media-type anime --status planned \
    --tag anime --tag mystery --ref tmdb:204423
$BIN --db "$DB" media add --title "Hades" --media-type game --status completed \
    --rating 10 --tag game --tag roguelite --ref steam:1145360 \
    --summary "A rogue-like about escaping the underworld."

# Software (Phase 2 module).
$BIN --db "$DB" software add --name Homebrew --category package --install-source system \
    --version 4.4.0 --purpose "Package manager for the other tools" \
    --ref homebrew_formula:brew --tag toolchain
$BIN --db "$DB" software add --name Ollama --category runtime --install-source homebrew-formula \
    --version 0.3.6 --purpose "Local LLM runtime for offline models" \
    --executable-path /opt/homebrew/bin/ollama --architecture arm64 \
    --ref homebrew_formula:ollama --tag ai --tag local
$BIN --db "$DB" software add --name "Visual Studio Code" --category application \
    --install-source homebrew-cask --version 1.93.1 --purpose "Primary editor" \
    --install-location /Applications --ref bundle_id:com.microsoft.vscode --tag editor
$BIN --db "$DB" software add --name ripgrep --category cli --install-source homebrew-formula \
    --version 14.1.0 --purpose "Fast recursive search" \
    --ref homebrew_formula:ripgrep --tag toolchain

# Relations, resolved to the asset IDs printed above.
$BIN --db "$DB" relation add <ollama>  installed_via <brew> --note "Installed through Homebrew formula"
$BIN --db "$DB" relation add <vscode>  installed_via <brew>
$BIN --db "$DB" relation add <ripgrep> installed_via <brew>
$BIN --db "$DB" relation add <vscode>  uses <ollama> --note "Continue extension talks to the local Ollama server"

# A metadata update and one archived asset, so the fixture covers the shared
# Asset lifecycle (revision bump + archived_at) alongside the happy path.
$BIN --db "$DB" software update <ripgrep> --purpose "Fast recursive search used everywhere in shell scripts"
$BIN --db "$DB" asset archive <vscode>

# 4. Merge the WAL into the main file so the fixture is a single self-contained
#    database, then verify before copying it in.
sqlite3 "$DB" "PRAGMA wal_checkpoint(TRUNCATE);"
sqlite3 "$DB" "PRAGMA integrity_check; PRAGMA foreign_key_check;"
cp "$DB" crates/storage-sqlite/tests/fixtures/phase2_software.db

git worktree remove /tmp/assetmesh-phase2
```

After copying, run the contract tests. If the ledger checksums no longer match
the constants in `sqlite_contracts.rs`, the fixture was regenerated from the
wrong commit or from current SQL — fix the source, do not relax the constants.
