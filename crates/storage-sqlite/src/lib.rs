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

/// Statement accounting, used to prove a read is batched.
///
/// A repository method that loads tags per row costs one statement per row, so
/// a page gets slower as the library grows. Nothing in the API can see that —
/// the caller gets the same rows back either way — so the count has to be
/// observed. `rusqlite::Connection::trace` is the seam: it reports every
/// statement the connection prepares.
///
/// It lives in the library rather than in the tests because `trace` is a
/// method on `Connection`, which the factory owns. `rusqlite`'s `trace` feature
/// is additive (`trace = []` adds no dependency and changes no behavior), so
/// enabling it costs nothing unless this module arms a counter.
///
/// Nothing in application code calls [`arm`]; a production path that counts its
/// own statements would be measuring the wrong thing.
pub mod statement_accounting {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static STATEMENTS: AtomicUsize = AtomicUsize::new(0);

    fn observe(_sql: &str) {
        STATEMENTS.fetch_add(1, Ordering::SeqCst);
    }

    /// Starts counting statements on the factory's write connection, which is
    /// also the read connection for an in-memory database.
    ///
    /// Resets the counter to zero, so only statements executed after this call
    /// are measured. Installing a trace callback cannot fail, but a fault here
    /// would otherwise silently report "no statements" forever, so it panics
    /// rather than lying.
    pub fn arm(factory: &super::SqliteFactory) {
        let installed = factory.set_tracer(Some(observe));
        assert!(
            installed.is_ok(),
            "the trace callback could not be installed"
        );
        STATEMENTS.store(0, Ordering::SeqCst);
    }

    /// Statements executed since [`arm`].
    pub fn take() -> usize {
        STATEMENTS.swap(0, Ordering::SeqCst)
    }

    /// Statements executed since [`arm`], without resetting — for reporting a
    /// measurement while leaving it inspectable.
    pub fn peek() -> usize {
        STATEMENTS.load(Ordering::SeqCst)
    }
}

use assetmesh_core::application::portable::{
    MEDIA_SCHEMA_VERSION, SERVICES_SCHEMA_VERSION, SOFTWARE_SCHEMA_VERSION,
};
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
        ("services", SERVICES_SCHEMA_VERSION),
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
///
/// The message becomes user-facing text, so rusqlite's own rendering is not
/// forwarded: its `Display` for a failure is
/// `SqliteFailure(Error { code: DatabaseCorruption, extended_code: 11 }, Some("…"))`,
/// which leaks the FFI enum shape into the desktop UI. Only the SQLite error
/// code and the driver's human detail sentence are kept.
pub(crate) fn map_error(err: rusqlite::Error) -> AppError {
    use rusqlite::ffi::ErrorCode as FfiCode;
    match &err {
        rusqlite::Error::SqliteFailure(ffi, _)
            if ffi.code == FfiCode::DatabaseLocked || ffi.code == FfiCode::DatabaseBusy =>
        {
            // SQLITE_LOCKED / SQLITE_BUSY are lock contention: retryable.
            AppError::storage_busy("sqlite database is busy or locked")
        }
        rusqlite::Error::SqliteFailure(ffi, _) if ffi.code == FfiCode::DatabaseCorrupt => {
            AppError::corrupt_data("database disk image is malformed or corrupted")
        }
        rusqlite::Error::SqliteFailure(ffi, _) if ffi.code == FfiCode::ConstraintViolation => {
            AppError::conflict("database constraint violation")
        }
        rusqlite::Error::SqliteFailure(ffi, _) => {
            AppError::storage(format!("sqlite error code {}", ffi.extended_code))
        }
        _ => AppError::storage("internal sqlite database error"),
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
        assert!(
            matches!(
                map_error(failure(rusqlite::ffi::SQLITE_CORRUPT)),
                AppError::CorruptData { .. }
            ),
            "corruption surfaces as CorruptData"
        );
    }
}
