//! Read the inventory without allowing one damaged entry to hide the others.
use super::files::{corrupt, storage_error};
use super::manifest::read_manifest_header;
use assetmesh_core::ports::backup::{BackupInventory, BackupIssue};
use assetmesh_core::AppResult;
use std::{fs, path::Path};

pub(super) fn read_inventory(root: &Path) -> AppResult<BackupInventory> {
    if !root.exists() {
        return Ok(BackupInventory::default());
    }
    let mut inventory = BackupInventory::default();
    for item in fs::read_dir(root).map_err(storage_error)? {
        let item = item.map_err(storage_error)?;
        if !item.file_type().map_err(storage_error)?.is_dir()
            || uuid::Uuid::parse_str(&item.file_name().to_string_lossy()).is_err()
        {
            continue;
        }
        let path = item.path();
        let result = read_manifest_header(&path).and_then(|manifest| {
            if manifest.entry.id != item.file_name().to_string_lossy() {
                return Err(corrupt());
            }
            Ok(manifest.entry)
        });
        match result {
            Ok(mut entry) => {
                entry.source_dir = path.to_string_lossy().into_owned();
                inventory.entries.push(entry);
            }
            Err(error) => inventory.issues.push(BackupIssue {
                source_dir: path.to_string_lossy().into_owned(),
                message: error.to_string(),
            }),
        }
    }
    inventory.entries.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    inventory
        .issues
        .sort_by(|a, b| a.source_dir.cmp(&b.source_dir));
    Ok(inventory)
}
