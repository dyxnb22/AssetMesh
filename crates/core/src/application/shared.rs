//! Kernel-level application helpers shared by every module.
//!
//! Module services must not reach into each other's implementations (ADR
//! 0003): `service_service` has no business depending on `media_service`.
//! Capabilities that every module needs — external-ref commands, tag
//! normalization, the active-asset precondition — live here, next to
//! [`crate::domain::validation`].

use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::ports::uow::UnitOfWork;
use crate::{AppError, AppResult};

/// One external reference a command asks to attach to an asset. An adapter
/// concern: the namespace/id pair is validated by [`AssetExternalRef`] before
/// it reaches canonical state, and no field here may hold a secret.
#[derive(Debug, Clone)]
pub struct ExternalRefInput {
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
}

/// Loads an asset and requires it be mutable (not archived, not merged).
/// Every mutating use case shares this precondition.
pub fn load_active_asset(uow: &mut dyn UnitOfWork, asset_id: AssetId) -> AppResult<Asset> {
    let asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    asset.ensure_mutable()?;
    Ok(asset)
}

/// Enforces optimistic concurrency on canonical assets.
/// Returns [`AppError::StaleRevision`] when the actual revision diverges.
pub fn check_asset_revision(asset: &Asset, expected: i64) -> AppResult<()> {
    if asset.revision != expected {
        return Err(AppError::stale_revision(expected, asset.revision));
    }
    Ok(())
}

/// A namespaced reference must be free, or already owned by the same asset,
/// before it is written. Called inside the caller's transaction so the check
/// and the insert cannot be separated.
pub fn ensure_ref_available(
    uow: &mut dyn UnitOfWork,
    reference: &AssetExternalRef,
) -> AppResult<()> {
    if let Some(existing) = uow
        .external_refs()
        .find_asset_by_ref(&reference.namespace, &reference.external_id)?
    {
        if existing != reference.asset_id {
            return Err(AppError::conflict(format!(
                "external ref {}:{} is already attached to asset {}",
                reference.namespace, reference.external_id, existing
            )));
        }
    }
    Ok(())
}

/// Trims, drops empties, and case-insensitively de-duplicates tag names.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for tag in tags {
        let name = tag.trim();
        if name.is_empty() {
            continue;
        }
        if !normalized
            .iter()
            .any(|n: &String| n.eq_ignore_ascii_case(name))
        {
            normalized.push(name.to_string());
        }
    }
    normalized
}

/// Standard receipt returned by mutating commands (docs/12 Section 6.3 & P5-05).
/// Contains the operation name, affected canonical asset IDs, the resulting revision,
/// and any advisory warnings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MutationReceipt {
    pub operation: String,
    pub asset_ids: Vec<AssetId>,
    pub revision: Option<i64>,
    pub warnings: Vec<String>,
}

impl MutationReceipt {
    pub fn new(operation: impl Into<String>, asset_id: AssetId, revision: Option<i64>) -> Self {
        Self {
            operation: operation.into(),
            asset_ids: vec![asset_id],
            revision,
            warnings: Vec::new(),
        }
    }

    pub fn multiple(
        operation: impl Into<String>,
        asset_ids: Vec<AssetId>,
        revision: Option<i64>,
    ) -> Self {
        Self {
            operation: operation.into(),
            asset_ids,
            revision,
            warnings: Vec::new(),
        }
    }

    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}
