//! SQLite storage adapter for AssetMesh.
//!
//! Implements the core ports over SQLite with the shared concurrency policy
//! from ADR 0007: `foreign_keys = ON`, verified `journal_mode = WAL` for file
//! databases, bounded `busy_timeout`, short write transactions, snapshot
//! read scopes, and one initialization path shared by every entry point.

mod connection;
mod migrations;
mod repos;
mod uow;

pub use migrations::latest_db_version;
pub use uow::{SharedSqlite, SqliteFactory};

use assetmesh_core::application::portable::{MEDIA_SCHEMA_VERSION, SOFTWARE_SCHEMA_VERSION};
use assetmesh_core::AppError;
use rusqlite::Connection;

/// Opens (or creates) a database file, applies pending migrations, verifies
/// persisted module versions, and returns a factory implementing the core
/// ports. Safe to call concurrently from multiple processes: migrations run
/// under an immediate transaction.
pub fn open(path: &str) -> Result<SqliteFactory, AppError> {
    let mut conn = connection::connect(path)?;
    migrations::migrate(&mut conn)?;
    validate_module_versions(&conn)?;
    Ok(uow::SqliteFactory::new(path.to_string(), conn))
}

/// Opens a private in-memory database. Useful for tests and throwaway use.
pub fn open_in_memory() -> Result<SqliteFactory, AppError> {
    let mut conn = connection::connect(":memory:")?;
    migrations::migrate(&mut conn)?;
    validate_module_versions(&conn)?;
    Ok(uow::SqliteFactory::new(":memory:".to_string(), conn))
}

/// Verifies the persisted module schema versions against what this binary
/// supports (ADR 0008). A database written by a newer module version must
/// fail loudly instead of being silently read or upgraded.
fn validate_module_versions(conn: &Connection) -> Result<(), AppError> {
    let versions: Vec<(String, i64)> = {
        let mut stmt = conn
            .prepare("SELECT module_id, schema_version FROM module_metadata")
            .map_err(map_error)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(map_error)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(map_error)?);
        }
        out
    };

    let expected: &[(&str, i64)] = &[
        ("media", MEDIA_SCHEMA_VERSION),
        ("software", SOFTWARE_SCHEMA_VERSION),
    ];
    for (module_id, current) in expected {
        check_module_version(&versions, module_id, *current)?;
    }
    Ok(())
}

/// Checks one module's persisted schema version against the running binary.
fn check_module_version(
    versions: &[(String, i64)],
    module_id: &str,
    current: i64,
) -> Result<(), AppError> {
    let found = versions
        .iter()
        .find(|(id, _)| id == module_id)
        .map(|(_, v)| *v);
    match found {
        None => Err(AppError::storage(format!(
            "database is missing {module_id} module metadata; refusing to open"
        ))),
        // Only the current version is accepted: older semantics must go
        // through an explicit module migration (ADR 0008), zero/negative
        // values are corruption, and newer versions need newer code.
        Some(v) if v != current => Err(AppError::unsupported_schema_version(
            format!("{module_id} module"),
            v,
            format!("schema_version {current}"),
        )),
        Some(_) => Ok(()),
    }
}

/// Maps a rusqlite error into the typed application error model, keeping
/// driver specifics inside this crate (ADR 0007 failure behavior).
pub(crate) fn map_error(err: rusqlite::Error) -> AppError {
    use rusqlite::ffi::ErrorCode as FfiCode;
    match &err {
        rusqlite::Error::SqliteFailure(ffi, details)
            if ffi.code == FfiCode::DatabaseLocked || ffi.code == FfiCode::DatabaseBusy =>
        {
            // SQLITE_LOCKED / SQLITE_BUSY are lock contention: retryable.
            AppError::storage_busy(format!(
                "sqlite is busy or locked: {}",
                details.clone().unwrap_or_default()
            ))
        }
        rusqlite::Error::SqliteFailure(ffi, _) if ffi.code == FfiCode::ConstraintViolation => {
            AppError::conflict(format!("constraint violated: {err}"))
        }
        _ => AppError::storage(format!("sqlite failure: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(code: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None)
    }

    #[test]
    fn busy_errors_map_to_retryable_storage_busy() {
        assert!(map_error(failure(rusqlite::ffi::SQLITE_BUSY)).is_retryable());
        assert!(map_error(failure(rusqlite::ffi::SQLITE_LOCKED)).is_retryable());
        assert!(
            !map_error(failure(rusqlite::ffi::SQLITE_IOERR)).is_retryable(),
            "I/O failure is not contention"
        );
        assert!(
            matches!(
                map_error(failure(rusqlite::ffi::SQLITE_CONSTRAINT)),
                AppError::Conflict { .. }
            ),
            "constraint violations surface as conflicts"
        );
    }
}
