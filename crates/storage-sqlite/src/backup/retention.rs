//! Managed residual cleanup and retention, preserving active and protected files.
use super::files::{directory_bytes, storage_error};
use super::{backup_directory, selected_database, SqliteBackupStore};
use assetmesh_core::AppResult;
use assetmesh_core::{
    domain::Timestamp,
    ports::backup::{BackupRetention, BackupStore},
};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn older_than(path: &Path, now: Timestamp, age: chrono::Duration) -> AppResult<bool> {
    let modified: Timestamp = fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(storage_error)?
        .into();
    Ok(now - modified >= age)
}

impl SqliteBackupStore {
    pub(super) fn clean_staging(&self, now: Timestamp) -> AppResult<()> {
        // The folder lock excludes live snapshots, exports, restores and
        // migrations. A grace period also preserves recently interrupted work.
        for item in fs::read_dir(&self.root).map_err(storage_error)? {
            let item = item.map_err(storage_error)?;
            let name = item.file_name().to_string_lossy().into_owned();
            let kind = item.file_type().map_err(storage_error)?;
            let owned_dir = name
                .strip_prefix(".partial-")
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                && kind.is_dir();
            let owned_file = ["preferences.partial-", "active-library.partial-"]
                .iter()
                .any(|prefix| {
                    name.strip_prefix(prefix)
                        .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                })
                && kind.is_file();
            if (owned_dir || owned_file)
                && older_than(&item.path(), now, chrono::Duration::hours(24))?
            {
                if owned_dir {
                    fs::remove_dir_all(item.path()).map_err(storage_error)?;
                } else {
                    fs::remove_file(item.path()).map_err(storage_error)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn clean_restored(&self, now: Timestamp) -> AppResult<()> {
        let directory = self.root.join("restored");
        if !fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_dir()) {
            return Ok(());
        }
        let selected = selected_database(&self.original_database)?;
        let running = self
            .factory
            .0
            .with_raw_connection(|conn| conn.path().map(PathBuf::from))?;
        let mut unused = Vec::new();
        for item in fs::read_dir(directory).map_err(storage_error)? {
            let item = item.map_err(storage_error)?;
            let path = item.path();
            if !item.file_type().map_err(storage_error)?.is_file()
                || path.extension().is_none_or(|extension| extension != "db")
                || path
                    .file_stem()
                    .is_none_or(|name| uuid::Uuid::parse_str(&name.to_string_lossy()).is_err())
            {
                continue;
            }
            let canonical = path.canonicalize().map_err(storage_error)?;
            if canonical == selected
                || running
                    .as_ref()
                    .is_some_and(|p| p.canonicalize().ok().as_ref() == Some(&canonical))
            {
                continue;
            }
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .map_err(storage_error)?;
            unused.push((modified, path));
        }
        unused.sort_by_key(|entry| std::cmp::Reverse((entry.0, entry.1.clone())));
        // Retain the latest previous library, plus any library used or modified
        // in the last month. Never treat the selected/running library as a backup.
        for (_, path) in unused.into_iter().skip(1) {
            let wal = PathBuf::from(format!("{}-wal", path.display()));
            let migration_backups = backup_directory(&path);
            if !older_than(&path, now, chrono::Duration::days(30))?
                || (wal.exists() && !older_than(&wal, now, chrono::Duration::days(30))?)
                || (migration_backups.exists()
                    && !older_than(&migration_backups, now, chrono::Duration::days(30))?)
            {
                continue;
            }
            let Ok(connection) = Connection::open_with_flags(
                &path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) else {
                continue;
            };
            connection
                .busy_timeout(Duration::ZERO)
                .map_err(crate::map_error)?;
            connection
                .pragma_update(None, "locking_mode", "EXCLUSIVE")
                .map_err(crate::map_error)?;
            if connection.execute_batch("BEGIN EXCLUSIVE; COMMIT").is_err() {
                continue;
            }
            // Close SQLite first so checkpoints and sidecar removal finish
            // before unlinking this expired, inactive managed library.
            if connection.close().is_err() {
                continue;
            }
            fs::remove_file(&path).map_err(storage_error)?;
            for suffix in ["-wal", "-shm", "-journal"] {
                let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
                if sidecar.exists() {
                    fs::remove_file(sidecar).map_err(storage_error)?;
                }
            }
            if fs::symlink_metadata(&migration_backups).is_ok_and(|metadata| metadata.is_dir()) {
                fs::remove_dir_all(migration_backups).map_err(storage_error)?;
            }
        }
        Ok(())
    }

    pub(super) fn prune_owned(&self, retention: &BackupRetention) -> AppResult<()> {
        self.clean_staging(retention.now)?;
        self.clean_restored(retention.now)?;
        let mut remaining = Vec::new();
        for entry in self.list()? {
            if !retention.keep_ids.contains(&entry.id)
                && !retention.protected_ids.contains(&entry.id)
            {
                fs::remove_dir_all(self.root.join(&entry.id)).map_err(storage_error)?;
            } else {
                remaining.push(entry);
            }
        }
        let mut bytes = self.storage_bytes()?;
        for entry in remaining.into_iter().rev() {
            if bytes <= retention.max_bytes {
                break;
            }
            if !retention.protected_ids.contains(&entry.id) {
                let path = self.root.join(&entry.id);
                let size = directory_bytes(&path)?;
                fs::remove_dir_all(path).map_err(storage_error)?;
                bytes = bytes.saturating_sub(size);
            }
        }
        Ok(())
    }
}
