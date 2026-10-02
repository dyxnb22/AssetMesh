//! Shared input and output budgets for the portable product contract.
use super::format::{PortableBundle, PortableManifest, V1_FILE_PATHS};
use crate::{AppError, AppResult};
use std::collections::HashSet;

pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_BUNDLE_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_BUNDLE_RECORDS: usize = 100_000;
pub(super) const MAX_JSONL_LINE_BYTES: usize = 1024 * 1024;

pub(super) fn validate_manifest_budget(manifest: &PortableManifest) -> AppResult<()> {
    let total = manifest
        .record_counts
        .values()
        .try_fold(0usize, |total, count| total.checked_add(*count));
    if total.is_none_or(|total| total > MAX_BUNDLE_RECORDS) {
        return Err(AppError::validation(
            "portable bundle exceeds the 100000 record limit",
        ));
    }
    Ok(())
}

pub(super) fn validate_bundle_budget(bundle: &PortableBundle) -> AppResult<()> {
    validate_manifest_budget(&bundle.manifest)?;
    let manifest_bytes = bundle.manifest_json()?.len();
    if manifest_bytes > MAX_MANIFEST_BYTES {
        return Err(AppError::validation(
            "portable manifest exceeds the 256 KiB limit",
        ));
    }
    let mut total = manifest_bytes;
    let mut records = 0usize;
    let mut paths = HashSet::new();
    for file in &bundle.files {
        if !V1_FILE_PATHS.contains(&file.path.as_str()) || !paths.insert(&file.path) {
            return Err(AppError::validation(
                "portable bundle contains an unknown or repeated file path",
            ));
        }
        total = total.saturating_add(file.content.len());
        if file.content.len() > MAX_BUNDLE_FILE_BYTES || total > MAX_BUNDLE_BYTES {
            return Err(AppError::validation(
                "portable bundle exceeds the 32 MiB file or 128 MiB total size limit",
            ));
        }
        if file.path.ends_with(".jsonl") {
            for line in file.content.lines().filter(|line| !line.trim().is_empty()) {
                records += 1;
                if records > MAX_BUNDLE_RECORDS || line.len() > MAX_JSONL_LINE_BYTES {
                    return Err(AppError::validation(
                        "portable bundle exceeds the record or 1 MiB line size limit",
                    ));
                }
            }
        }
    }
    Ok(())
}
