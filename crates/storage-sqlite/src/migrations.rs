//! Deterministic database migrations.
//!
//! Migrations are ordered SQL files embedded at compile time from the
//! repository's `migrations/` directory. Each applied migration is recorded
//! with its checksum; an edited already-applied migration fails loudly
//! instead of silently diverging (docs/05 schema migration rules).
//!
//! The entire migration step — ledger creation, state read, and every
//! pending migration — runs inside ONE immediate transaction. Concurrent
//! processes therefore serialize: the second opener blocks on the write lock
//! (bounded by busy_timeout), then sees the ledger already populated and
//! applies nothing.
//!
//! This database migration version is independent of the portable export
//! format version and of module schema versions (ADR 0008).

use assetmesh_core::AppError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

pub(crate) const MIGRATIONS: &[(i64, &str, &str)] = &[(
    1,
    "0001_core_media_v1",
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations/0001_core_media_v1.sql"
    )),
)];

/// Latest database migration version.
pub fn latest_db_version() -> i64 {
    MIGRATIONS
        .last()
        .map(|(version, _, _)| *version)
        .unwrap_or(0)
}

pub(crate) fn migrate(conn: &mut Connection) -> Result<(), AppError> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(crate::map_error)?;

    // Bookkeeping table lives outside the versioned migrations themselves.
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS assetmesh_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            checksum TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(crate::map_error)?;

    let applied: Vec<(i64, String)> = {
        let mut stmt = tx
            .prepare("SELECT version, checksum FROM assetmesh_migrations ORDER BY version")
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(crate::map_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(crate::map_error)?
    };

    // Fail loudly if an already-applied migration changed on disk.
    for (version, _, sql) in MIGRATIONS {
        if let Some((_, recorded)) = applied.iter().find(|(v, _)| v == version) {
            let expected = checksum(sql);
            if *recorded != expected {
                return Err(AppError::storage(format!(
                    "migration {version} was modified after being applied (checksum mismatch); \
                     refusing to start on a diverged database"
                )));
            }
        }
    }

    // Refuse a database migrated by a newer AssetMesh.
    let max_applied = applied.iter().map(|(v, _)| *v).max().unwrap_or(0);
    if max_applied > latest_db_version() {
        return Err(AppError::unsupported_schema_version(
            "database",
            max_applied,
            format!("<= {}", latest_db_version()),
        ));
    }

    for (version, name, sql) in MIGRATIONS {
        if applied.iter().any(|(v, _)| v == version) {
            continue;
        }
        tx.execute_batch(sql)
            .map_err(|e| AppError::storage(format!("migration {version} ({name}) failed: {e}")))?;
        tx.execute(
            "INSERT INTO assetmesh_migrations (version, name, checksum, applied_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                version,
                name,
                checksum(sql),
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .map_err(crate::map_error)?;
    }

    tx.commit().map_err(crate::map_error)?;
    Ok(())
}

fn checksum(sql: &str) -> String {
    let digest = Sha256::digest(sql.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_stable() {
        assert_eq!(checksum("CREATE TABLE t;"), checksum("CREATE TABLE t;"));
        assert_ne!(checksum("CREATE TABLE t;"), checksum("CREATE TABLE u;"));
    }
}
