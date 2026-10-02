//! Recovery storage: consistent snapshots, validation, and staged restores.
use crate::{
    application::portable::{write_bundle_to_directory, PortableBundle, PortableManifest},
    domain::Timestamp,
    AppResult,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type BackupPreferences = BTreeMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupEntry {
    pub id: String,
    pub source_dir: String,
    pub created_at: Timestamp,
    pub kind: String,
    pub reason: String,
    pub asset_count: u64,
    pub contains_api_keys: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupIssue {
    pub source_dir: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct BackupInventory {
    pub entries: Vec<BackupEntry>,
    /// Unreadable packages are preserved and never considered for eviction.
    pub issues: Vec<BackupIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreReceipt {
    pub db_path: String,
    pub preferences: BackupPreferences,
    pub restart_required: bool,
}

/// The service chooses useful recovery points; the adapter applies the space
/// budget and filesystem cleanup without evicting the protected latest points.
pub struct BackupRetention {
    pub keep_ids: Vec<String>,
    pub protected_ids: Vec<String>,
    pub max_bytes: u64,
    pub now: Timestamp,
}

/// Filesystem and database mechanics remain behind this port. A restore must
/// create a new library and persist its selection without replacing the source.
pub trait BackupStore: Send + Sync {
    fn directory(&self) -> String;
    fn change_token(&self) -> AppResult<String>;
    fn inventory(&self) -> AppResult<BackupInventory>;
    fn list(&self) -> AppResult<Vec<BackupEntry>> {
        Ok(self.inventory()?.entries)
    }
    fn snapshot(&self, now: Timestamp, reason: &str) -> AppResult<BackupEntry>;
    fn portable(&self, now: Timestamp, bundle: &PortableBundle) -> AppResult<BackupEntry> {
        self.portable_stream(now, &mut |directory| {
            write_bundle_to_directory(bundle, directory)?;
            Ok(bundle.manifest.clone())
        })
    }
    fn portable_stream(
        &self,
        now: Timestamp,
        build: &mut dyn FnMut(&std::path::Path) -> AppResult<PortableManifest>,
    ) -> AppResult<BackupEntry>;
    fn preview(&self, source: &str) -> AppResult<BackupEntry>;
    fn restore(&self, source: &str, fingerprint: &str) -> AppResult<RestoreReceipt>;
    fn copy_to(&self, source: &str, target: &str) -> AppResult<String>;
    fn storage_bytes(&self) -> AppResult<u64>;
    fn prune(&self, retention: &BackupRetention) -> AppResult<()>;
    fn preferences(&self) -> AppResult<BackupPreferences>;
    fn save_preferences(&self, preferences: &BackupPreferences) -> AppResult<()>;
}
