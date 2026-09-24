//! Shared SQLite connection initialization (ADR 0007).
//!
//! Every connection AssetMesh opens goes through here: bounded busy timeout
//! FIRST (before anything that can take a lock — the WAL switch itself can
//! contend), foreign keys on, WAL journal verified for file databases. No
//! adapter may create ad-hoc connections with different pragmas.

use assetmesh_core::AppError;
use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use std::time::Duration;

/// Bounded wait for locks before failing with a retryable error.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// Initialization can contend between processes during concurrent first
/// open; busy handling plus a bounded retry makes it reliable.
const INIT_ATTEMPTS: u32 = 8;

pub(crate) fn connect(path: &str) -> Result<Connection, AppError> {
    let conn = Connection::open(path).map_err(crate::map_error)?;
    // Must precede the journal-mode switch: PRAGMA journal_mode=WAL takes a
    // lock and fails immediately when another process holds it.
    conn.busy_timeout(BUSY_TIMEOUT).map_err(crate::map_error)?;
    configure_with_retry(&conn, expects_wal(path))?;
    Ok(conn)
}

/// In-memory databases cannot use WAL; every other database must.
fn expects_wal(path: &str) -> bool {
    path != ":memory:"
}

fn configure_with_retry(conn: &Connection, expect_wal: bool) -> Result<(), AppError> {
    for attempt in 0..INIT_ATTEMPTS {
        match configure(conn, expect_wal) {
            Ok(()) => return Ok(()),
            Err(error) if error.is_retryable() && attempt + 1 < INIT_ATTEMPTS => {
                std::thread::sleep(Duration::from_millis(25 * u64::from(attempt + 1)));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn configure(conn: &Connection, expect_wal: bool) -> Result<(), AppError> {
    conn.create_scalar_function(
        "assetmesh_casefold",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(ctx.get::<String>(0)?.to_lowercase()),
    )
    .map_err(crate::map_error)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(crate::map_error)?;

    if expect_wal {
        // Switch to WAL only when needed: reading the current mode avoids a
        // needless (and lock-taking) write on already-WAL databases.
        let current: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(crate::map_error)?;
        if !current.eq_ignore_ascii_case("wal") {
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(crate::map_error)?;
            let switched: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .map_err(crate::map_error)?;
            if !switched.eq_ignore_ascii_case("wal") {
                return Err(AppError::storage(format!(
                    "could not enable WAL journal mode (got {switched:?}); \
                     refusing to open an unverified database"
                )));
            }
        }
    }

    // Durable-enough default for a local personal app (ADR 0007 leaves this
    // to the implementation): NORMAL keeps WAL safety with less fsync load.
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(crate::map_error)?;
    Ok(())
}
