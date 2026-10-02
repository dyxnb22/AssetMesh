//! SQLite snapshot copying, integrity inspection, and persistent change tokens.
use super::files::{corrupt, private_file, storage_error};
use assetmesh_core::ports::backup::BackupPreferences;
use assetmesh_core::{AppError, AppResult};
use rusqlite::{
    backup::{Backup, StepResult},
    Connection, OpenFlags,
};
use sha2::{Digest, Sha256};
use std::{fs::File, path::Path, time::Duration};

pub(super) fn database_change_token(
    conn: &Connection,
    preferences: &BackupPreferences,
) -> AppResult<String> {
    let (library, revision, schema): (String, i64, i64) = conn.query_row(
        "SELECT library_id,revision,(SELECT MAX(version) FROM assetmesh_migrations) FROM assetmesh_change_clock WHERE singleton=1",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).map_err(crate::map_error)?;
    let prefs = Sha256::digest(serde_json::to_vec(preferences).map_err(storage_error)?);
    Ok(format!("{library}:{schema}:{revision}:{prefs:x}"))
}

pub(super) fn online_copy(source: &Connection, target: &Path) -> AppResult<()> {
    // Create with private permissions BEFORE SQLite writes any user values.
    drop(private_file(target)?);
    let mut destination = Connection::open(target).map_err(crate::map_error)?;
    {
        let backup = Backup::new(source, &mut destination).map_err(crate::map_error)?;
        let mut busy = 0;
        loop {
            match backup.step(256).map_err(crate::map_error)? {
                StepResult::Done => break,
                StepResult::More => {
                    busy = 0;
                }
                StepResult::Busy | StepResult::Locked => {
                    busy += 1;
                    if busy >= 100 {
                        return Err(AppError::storage_busy("backup source is busy; try again"));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => return Err(AppError::storage("backup did not complete")),
            }
        }
    }
    destination
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(crate::map_error)?;
    drop(destination);
    File::open(target)
        .and_then(|f| f.sync_all())
        .map_err(storage_error)?;
    Ok(())
}

pub(super) fn read_only(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(crate::map_error)?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(crate::map_error)?;
    Ok(conn)
}

pub(super) fn inspect_database(path: &Path) -> AppResult<(u64, bool)> {
    let conn = read_only(path)?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(crate::map_error)?;
    if integrity != "ok" {
        return Err(corrupt());
    }
    let foreign_keys = conn
        .prepare("PRAGMA foreign_key_check")
        .map_err(crate::map_error)?
        .exists([])
        .map_err(crate::map_error)?;
    if foreign_keys {
        return Err(corrupt());
    }
    let version: i64 = conn
        .query_row("SELECT max(version) FROM assetmesh_migrations", [], |r| {
            r.get(0)
        })
        .map_err(|_| corrupt())?;
    if version < 1 || version > crate::latest_db_version() {
        return Err(corrupt());
    }
    let assets = conn
        .query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
        .map_err(|_| corrupt())?;
    let has_info: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'info_records')",
            [],
            |r| r.get(0),
        )
        .map_err(crate::map_error)?;
    let keys = has_info
        && conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM info_records WHERE info_type = 'api_key')",
                [],
                |r| r.get(0),
            )
            .map_err(crate::map_error)?;
    Ok((assets, keys))
}
