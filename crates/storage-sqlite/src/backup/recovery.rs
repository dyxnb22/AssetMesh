//! Validated restore destinations, active-library selection, and export copies.
use super::database::{inspect_database, online_copy, read_only};
use super::files::{
    atomic_json, corrupt, private_dir, private_file, read_bounded_json, storage_error,
};
use super::manifest::read_manifest;
use super::{backup_directory, SqliteBackupStore};
use crate::SharedSqlite;
use assetmesh_core::{
    application::portable::{read_bundle_from_directory, PortableImportService},
    ports::backup::{BackupPreferences, RestoreReceipt},
};
use assetmesh_core::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Serialize, Deserialize)]
struct Selection {
    db_path: String,
    preferences: BackupPreferences,
}

pub fn selected_database(database: &Path) -> AppResult<PathBuf> {
    let root = backup_directory(database);
    let marker = root.join("active-library.json");
    if !marker.exists() {
        return Ok(database.to_path_buf());
    }
    let selection: Selection = read_bounded_json(&marker)?;
    let path = PathBuf::from(selection.db_path);
    let canonical = path.canonicalize().map_err(storage_error)?;
    let restored = root
        .join("restored")
        .canonicalize()
        .map_err(storage_error)?;
    if !canonical.starts_with(restored) || canonical.extension().is_none_or(|e| e != "db") {
        return Err(corrupt());
    }
    Ok(canonical)
}

impl SqliteBackupStore {
    pub(super) fn restore_library(
        &self,
        source: &str,
        expected: &str,
    ) -> AppResult<RestoreReceipt> {
        let manifest = read_manifest(source)?;
        if manifest.entry.fingerprint != expected {
            return Err(AppError::conflict(
                "backup changed since preview; preview it again",
            ));
        }
        let restored = self.root.join("restored");
        private_dir(&restored)?;
        let target = restored.join(format!("{}.db", uuid::Uuid::now_v7()));
        let result = (|| {
            if manifest.entry.kind == "snapshot" {
                online_copy(&read_only(&Path::new(source).join("library.db"))?, &target)?;
            } else {
                drop(private_file(&target)?);
                let factory =
                    SharedSqlite(Arc::new(crate::open(target.to_str().ok_or_else(corrupt)?)?));
                let bundle = read_bundle_from_directory(&Path::new(source).join("portable"))?;
                PortableImportService::new(factory).import_bundle(&bundle, false)?;
            }
            // Recheck source after copying, then exercise migrations and module
            // compatibility on the NEW database before selecting it for launch.
            if read_manifest(source)?.entry.fingerprint != expected {
                return Err(AppError::conflict("backup changed during restore"));
            }
            let _ = crate::open(target.to_str().ok_or_else(corrupt)?)?;
            inspect_database(&target)?;
            let path = target
                .canonicalize()
                .map_err(storage_error)?
                .to_string_lossy()
                .into_owned();
            atomic_json(&self.root.join("preferences.json"), &manifest.preferences)?;
            atomic_json(
                &self.root.join("active-library.json"),
                &Selection {
                    db_path: path.clone(),
                    preferences: manifest.preferences.clone(),
                },
            )?;
            self.factory.0.disable_writes()?;
            Ok(RestoreReceipt {
                db_path: path,
                preferences: manifest.preferences,
                restart_required: true,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(target);
        }
        result
    }

    pub(super) fn copy_backup(&self, source: &str, target: &str) -> AppResult<String> {
        let manifest = read_manifest(source)?;
        let parent = Path::new(target).canonicalize().map_err(storage_error)?;
        if !parent.is_dir() {
            return Err(AppError::validation("choose a destination directory"));
        }
        let name = format!("assetmesh-backup-{}", uuid::Uuid::now_v7());
        let staging = parent.join(format!(".partial-{name}"));
        let final_dir = parent.join(name);
        private_dir(&staging)?;
        let result = (|| {
            for relative in manifest
                .checksums
                .keys()
                .map(String::as_str)
                .chain(std::iter::once("backup.json"))
            {
                let destination = staging.join(relative);
                private_dir(destination.parent().ok_or_else(corrupt)?)?;
                let mut to = private_file(&destination)?;
                let mut from =
                    File::open(Path::new(source).join(relative)).map_err(storage_error)?;
                std::io::copy(&mut from, &mut to).map_err(storage_error)?;
                to.sync_all().map_err(storage_error)?;
            }
            read_manifest(staging.to_str().ok_or_else(corrupt)?)?;
            fs::rename(&staging, &final_dir).map_err(storage_error)?;
            File::open(&parent)
                .and_then(|f| f.sync_all())
                .map_err(storage_error)?;
            Ok(final_dir.to_string_lossy().into_owned())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(staging);
        }
        result
    }
}
