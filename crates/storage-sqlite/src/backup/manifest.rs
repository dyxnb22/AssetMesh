//! Recovery metadata, checksum validation, and final publication.
use super::database::inspect_database;
use super::files::{atomic_json, checksum, corrupt, read_bounded_json, storage_error};
use crate::SharedSqlite;
use assetmesh_core::application::portable::{read_bundle_from_directory, PortableImportService};
use assetmesh_core::ports::backup::{BackupEntry, BackupPreferences};
use assetmesh_core::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::Path,
    sync::Arc,
};

pub(super) const FORMAT: &str = "assetmesh-backup-v1";

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub(super) format: String,
    pub(super) entry: BackupEntry,
    pub(super) checksums: BTreeMap<String, String>,
    pub(super) preferences: BackupPreferences,
    // Omit absent markers so fingerprints of older V1 backups remain valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) change_token: Option<String>,
}

pub(super) fn read_manifest_header(directory: &Path) -> AppResult<Manifest> {
    let base = directory.canonicalize().map_err(storage_error)?;
    let path = directory
        .join("backup.json")
        .canonicalize()
        .map_err(storage_error)?;
    if !path.starts_with(base) {
        return Err(corrupt());
    }
    let manifest: Manifest = read_bounded_json(&path)?;
    if manifest.format != FORMAT
        || manifest.entry.fingerprint != fingerprint(&manifest)?
        || uuid::Uuid::parse_str(&manifest.entry.id).is_err()
        || !["snapshot", "portable"].contains(&manifest.entry.kind.as_str())
    {
        return Err(corrupt());
    }
    Ok(manifest)
}

pub(super) fn fingerprint(manifest: &Manifest) -> AppResult<String> {
    let mut copy = manifest.clone();
    copy.entry.fingerprint.clear();
    // A copied backup directory is still the same backup.
    copy.entry.source_dir.clear();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&copy).map_err(storage_error)?)
    ))
}

pub(super) fn read_manifest(source: &str) -> AppResult<Manifest> {
    let dir = Path::new(source);
    if !dir.is_dir() {
        return Err(AppError::validation("choose a backup directory"));
    }
    let manifest = read_manifest_header(dir)?;
    let base = dir.canonicalize().map_err(storage_error)?;
    for (relative, expected) in &manifest.checksums {
        let path = Path::new(relative);
        if path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(corrupt());
        }
        let file = dir.join(path).canonicalize().map_err(storage_error)?;
        if !file.starts_with(&base) || checksum(&file)? != *expected {
            return Err(corrupt());
        }
    }
    if manifest.entry.kind == "snapshot" {
        if manifest.checksums.len() != 1 || !manifest.checksums.contains_key("library.db") {
            return Err(corrupt());
        }
        let (count, keys) = inspect_database(&dir.join("library.db"))?;
        if count != manifest.entry.asset_count || keys != manifest.entry.contains_api_keys {
            return Err(corrupt());
        }
    } else {
        let bundle = read_bundle_from_directory(&dir.join("portable"))?;
        let mut expected_files: Vec<_> = bundle
            .files
            .iter()
            .map(|file| format!("portable/{}", file.path))
            .collect();
        expected_files.push("portable/manifest.json".into());
        if expected_files.len() != manifest.checksums.len()
            || expected_files
                .iter()
                .any(|file| !manifest.checksums.contains_key(file))
        {
            return Err(corrupt());
        }
        // Dry run against an empty disposable destination validates the full
        // canonical bundle, including typed invariants, without modifying it.
        let memory = SharedSqlite(Arc::new(crate::open_in_memory()?));
        PortableImportService::new(memory).import_bundle(&bundle, true)?;
    }
    Ok(manifest)
}

pub(super) fn publish(
    root: &Path,
    staging: &Path,
    mut manifest: Manifest,
) -> AppResult<BackupEntry> {
    let final_dir = root.join(&manifest.entry.id);
    manifest.entry.source_dir = final_dir.to_string_lossy().into_owned();
    manifest.entry.fingerprint = fingerprint(&manifest)?;
    atomic_json(&staging.join("backup.json"), &manifest)?;
    fs::rename(staging, &final_dir).map_err(storage_error)?;
    File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(storage_error)?;
    Ok(manifest.entry)
}
