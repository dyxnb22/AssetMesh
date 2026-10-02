//! Snapshot and portable recovery creation, including pre-migration points.
use super::database::{database_change_token, inspect_database, online_copy};
use super::files::{
    atomic_json, backup_lock, checksum, corrupt, private_dir, read_bounded_json, storage_error,
};
use super::manifest::{
    fingerprint, publish, read_manifest, read_manifest_header, Manifest, FORMAT,
};
use super::{backup_directory, SqliteBackupStore};
use assetmesh_core::AppResult;
use assetmesh_core::{
    application::portable::{PortableManifest, V1_FILE_PATHS},
    domain::Timestamp,
    ports::backup::{BackupEntry, BackupStore},
};
use rusqlite::{Connection, OpenFlags};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::BufRead,
    path::Path,
};

impl SqliteBackupStore {
    pub(super) fn create_snapshot(&self, now: Timestamp, reason: &str) -> AppResult<BackupEntry> {
        let preferences = self.preferences()?;
        let previous = if reason == "automatic" {
            self.list()?
                .into_iter()
                .find(|entry| entry.kind == "snapshot")
        } else {
            None
        };
        if let Some(previous) = &previous {
            let token = self.change_token()?;
            if read_manifest_header(Path::new(&previous.source_dir))
                .is_ok_and(|manifest| manifest.change_token.as_ref() == Some(&token))
            {
                // Reuse across launches only after validating the complete
                // recovery point. A damaged payload must cause a fresh copy.
                if read_manifest(&previous.source_dir).is_ok() {
                    return Ok(previous.clone());
                }
            }
        }
        let id = uuid::Uuid::now_v7().to_string();
        let staging = self.root.join(format!(".partial-{id}"));
        private_dir(&staging)?;
        let result = (|| {
            self.factory
                .0
                .with_raw_connection(|conn| online_copy(conn, &staging.join("library.db")))??;
            let (asset_count, contains_api_keys) = inspect_database(&staging.join("library.db"))?;
            let checksums =
                BTreeMap::from([("library.db".into(), checksum(&staging.join("library.db"))?)]);
            // Record the copied snapshot's revision, never a later live commit.
            let snapshot = Connection::open_with_flags(
                staging.join("library.db"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(crate::map_error)?;
            let change_token = Some(database_change_token(&snapshot, &preferences)?);
            if let Some(previous) = previous {
                if let Ok(manifest) = read_manifest(&previous.source_dir) {
                    if manifest.checksums == checksums && manifest.preferences == preferences {
                        fs::remove_dir_all(&staging).map_err(storage_error)?;
                        return Ok(previous);
                    }
                }
            }
            let entry = BackupEntry {
                id,
                source_dir: String::new(),
                created_at: now,
                kind: "snapshot".into(),
                reason: reason.into(),
                asset_count,
                contains_api_keys,
                fingerprint: String::new(),
            };
            publish(
                &self.root,
                &staging,
                Manifest {
                    format: FORMAT.into(),
                    entry,
                    checksums,
                    preferences,
                    change_token,
                },
            )
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(staging);
        }
        result
    }

    pub(super) fn create_portable(
        &self,
        now: Timestamp,
        build: &mut dyn FnMut(&Path) -> AppResult<PortableManifest>,
    ) -> AppResult<BackupEntry> {
        let id = uuid::Uuid::now_v7().to_string();
        let staging = self.root.join(format!(".partial-{id}"));
        private_dir(&staging)?;
        let result = (|| {
            let manifest = build(&staging.join("portable"))?;
            let mut checksums = BTreeMap::new();
            for key in V1_FILE_PATHS
                .into_iter()
                .filter(|key| staging.join("portable").join(key).is_file())
                .chain(std::iter::once("manifest.json"))
            {
                let relative = format!("portable/{key}");
                checksums.insert(relative.clone(), checksum(&staging.join(&relative))?);
            }
            let asset_count = *manifest.record_counts.get("assets").unwrap_or(&0) as u64;
            let info_path = staging.join("portable/modules/info.jsonl");
            let mut contains_api_keys = false;
            if info_path.exists() {
                for line in
                    std::io::BufReader::new(File::open(&info_path).map_err(storage_error)?).lines()
                {
                    let line = line.map_err(storage_error)?;
                    let row: serde_json::Value =
                        serde_json::from_str(&line).map_err(|_| corrupt())?;
                    if row["info_type"] == "api_key" {
                        contains_api_keys = true;
                        break;
                    }
                }
            }
            let entry = BackupEntry {
                id,
                source_dir: String::new(),
                created_at: now,
                kind: "portable".into(),
                reason: "portable_export".into(),
                asset_count,
                contains_api_keys,
                fingerprint: String::new(),
            };
            publish(
                &self.root,
                &staging,
                Manifest {
                    format: FORMAT.into(),
                    entry,
                    checksums,
                    preferences: self.preferences()?,
                    change_token: None,
                },
            )
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(staging);
        }
        result
    }
}

/// A failed pre-migration recovery point leaves the source database intact.
pub(crate) fn before_migration(path: &str, conn: &Connection) -> AppResult<()> {
    if path == ":memory:" {
        return Ok(());
    }
    let has_ledger: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'assetmesh_migrations')",
            [],
            |r| r.get(0),
        )
        .map_err(crate::map_error)?;
    if !has_ledger {
        return Ok(());
    }
    let version: i64 = conn
        .query_row(
            "SELECT coalesce(max(version), 0) FROM assetmesh_migrations",
            [],
            |r| r.get(0),
        )
        .map_err(crate::map_error)?;
    if version < 1 || version >= crate::latest_db_version() {
        return Ok(());
    }
    let root = backup_directory(Path::new(path));
    let _lock = backup_lock(&root)?;
    let id = uuid::Uuid::now_v7().to_string();
    let staging = root.join(format!(".partial-{id}"));
    private_dir(&staging)?;
    let result = (|| {
        online_copy(conn, &staging.join("library.db"))?;
        let (asset_count, contains_api_keys) = inspect_database(&staging.join("library.db"))?;
        let final_dir = root.join(&id);
        let mut manifest = Manifest {
            format: FORMAT.into(),
            entry: BackupEntry {
                id,
                source_dir: final_dir.to_string_lossy().into_owned(),
                created_at: chrono::Utc::now(),
                kind: "snapshot".into(),
                reason: "before_migration".into(),
                asset_count,
                contains_api_keys,
                fingerprint: String::new(),
            },
            checksums: BTreeMap::from([(
                "library.db".into(),
                checksum(&staging.join("library.db"))?,
            )]),
            preferences: if root.join("preferences.json").exists() {
                read_bounded_json(&root.join("preferences.json"))?
            } else {
                BTreeMap::new()
            },
            change_token: None,
        };
        manifest.entry.fingerprint = fingerprint(&manifest)?;
        atomic_json(&staging.join("backup.json"), &manifest)?;
        fs::rename(&staging, final_dir).map_err(storage_error)?;
        File::open(&root)
            .and_then(|f| f.sync_all())
            .map_err(storage_error)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}
