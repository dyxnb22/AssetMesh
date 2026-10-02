//! Private, checksummed recovery folders with one lock per write operation.
mod create;
mod database;
mod files;
mod inventory;
mod manifest;
mod recovery;
mod retention;

use crate::SharedSqlite;
use assetmesh_core::{
    application::portable::PortableManifest,
    domain::Timestamp,
    ports::backup::{
        BackupEntry, BackupInventory, BackupPreferences, BackupRetention, BackupStore,
        RestoreReceipt,
    },
    AppResult,
};
pub(crate) use create::before_migration;
use files::{atomic_json, backup_lock, directory_bytes, read_bounded_json};
use manifest::read_manifest;
pub use recovery::selected_database;
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
};

pub struct SqliteBackupStore {
    factory: SharedSqlite,
    root: PathBuf,
    original_database: PathBuf,
}

pub fn backup_directory(database: &Path) -> PathBuf {
    let mut name = database.file_name().unwrap_or_default().to_os_string();
    name.push(".backups");
    database.with_file_name(name)
}

impl SqliteBackupStore {
    pub fn new(factory: SharedSqlite, original_database: &Path) -> Self {
        Self {
            factory,
            root: backup_directory(original_database),
            original_database: original_database.to_path_buf(),
        }
    }

    fn lock(&self) -> AppResult<File> {
        backup_lock(&self.root)
    }
}

impl BackupStore for SqliteBackupStore {
    fn directory(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn change_token(&self) -> AppResult<String> {
        let preferences = self.preferences()?;
        self.factory
            .0
            .with_raw_connection(|conn| database::database_change_token(conn, &preferences))?
    }

    fn storage_bytes(&self) -> AppResult<u64> {
        directory_bytes(&self.root)
    }

    fn preferences(&self) -> AppResult<BackupPreferences> {
        let path = self.root.join("preferences.json");
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        read_bounded_json(&path)
    }

    fn save_preferences(&self, preferences: &BackupPreferences) -> AppResult<()> {
        let _lock = self.lock()?;
        atomic_json(&self.root.join("preferences.json"), preferences)
    }

    fn inventory(&self) -> AppResult<BackupInventory> {
        inventory::read_inventory(&self.root)
    }
    fn snapshot(&self, now: Timestamp, reason: &str) -> AppResult<BackupEntry> {
        let _lock = self.lock()?;
        self.create_snapshot(now, reason)
    }
    fn portable_stream(
        &self,
        now: Timestamp,
        build: &mut dyn FnMut(&Path) -> AppResult<PortableManifest>,
    ) -> AppResult<BackupEntry> {
        let _lock = self.lock()?;
        self.create_portable(now, build)
    }
    fn preview(&self, source: &str) -> AppResult<BackupEntry> {
        let mut entry = read_manifest(source)?.entry;
        entry.source_dir = source.into();
        Ok(entry)
    }
    fn restore(&self, source: &str, expected: &str) -> AppResult<RestoreReceipt> {
        let _lock = self.lock()?;
        self.restore_library(source, expected)
    }
    fn copy_to(&self, source: &str, target: &str) -> AppResult<String> {
        let _lock = self.lock()?;
        self.copy_backup(source, target)
    }
    fn prune(&self, retention: &BackupRetention) -> AppResult<()> {
        let _lock = self.lock()?;
        self.prune_owned(retention)
    }
}
