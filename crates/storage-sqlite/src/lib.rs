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

use assetmesh_core::application::portable::MEDIA_SCHEMA_VERSION;
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
    let version: Option<i64> = conn
        .query_row(
            "SELECT schema_version FROM module_metadata WHERE module_id = 'media'",
            [],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(map_error)?;

    match version {
        None => Err(AppError::storage(
            "database is missing media module metadata; refusing to open",
        )),
        // Only the current version is accepted: older semantics must go
        // through an explicit module migration (ADR 0008), zero/negative
        // values are corruption, and newer versions need newer code.
        Some(v) if v != MEDIA_SCHEMA_VERSION => Err(AppError::unsupported_schema_version(
            "media module",
            v,
            format!("schema_version {MEDIA_SCHEMA_VERSION}"),
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
