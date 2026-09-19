//! Shared `Asset` identity (ADR 0003).
//!
//! The base asset stays small: identity, kind, name, summary, lifecycle, and
//! bookkeeping. Module-specific fields (episodes, binary paths, endpoints)
//! belong to module-owned typed details, never here.

use crate::domain::ids::AssetId;
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The owning module/type of an asset, for example `media.movie`.
///
/// Only the kinds required by Media V1 exist here. Future modules add their
/// own kinds; the base `Asset` shape does not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetKind {
    #[serde(rename = "media.movie")]
    MediaMovie,
    #[serde(rename = "media.tv")]
    MediaTv,
    #[serde(rename = "media.anime")]
    MediaAnime,
    #[serde(rename = "media.game")]
    MediaGame,
}

impl AssetKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            AssetKind::MediaMovie => "media.movie",
            AssetKind::MediaTv => "media.tv",
            AssetKind::MediaAnime => "media.anime",
            AssetKind::MediaGame => "media.game",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "media.movie" => Some(AssetKind::MediaMovie),
            "media.tv" => Some(AssetKind::MediaTv),
            "media.anime" => Some(AssetKind::MediaAnime),
            "media.game" => Some(AssetKind::MediaGame),
            _ => None,
        }
    }

    /// The module that owns typed details for this kind.
    pub const fn module(&self) -> &'static str {
        match self {
            AssetKind::MediaMovie
            | AssetKind::MediaTv
            | AssetKind::MediaAnime
            | AssetKind::MediaGame => "media",
        }
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle of an asset. `Merged` is a redirect/tombstone: the identity
/// remains explainable after an explicit merge (ADR 0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Active,
    Archived,
    Merged,
}

impl LifecycleState {
    pub const fn as_str(&self) -> &'static str {
        match self {
            LifecycleState::Active => "active",
            LifecycleState::Archived => "archived",
            LifecycleState::Merged => "merged",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(LifecycleState::Active),
            "archived" => Some(LifecycleState::Archived),
            "merged" => Some(LifecycleState::Merged),
            _ => None,
        }
    }
}

/// The shared canonical asset identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub kind: AssetKind,
    pub name: String,
    pub summary: Option<String>,
    pub lifecycle_state: LifecycleState,
    /// Local optimistic-concurrency/evolution counter. Not a sync primitive.
    pub revision: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub archived_at: Option<Timestamp>,
    /// Set when this asset was explicitly merged into another asset.
    pub merged_into: Option<AssetId>,
}

impl Asset {
    pub fn new(
        id: AssetId,
        kind: AssetKind,
        name: impl Into<String>,
        summary: Option<String>,
        now: Timestamp,
    ) -> AppResult<Self> {
        let asset = Asset {
            id,
            kind,
            name: name.into(),
            summary,
            lifecycle_state: LifecycleState::Active,
            revision: 1,
            created_at: now,
            updated_at: now,
            archived_at: None,
            merged_into: None,
        };
        asset.validate()?;
        Ok(asset)
    }

    /// Small, boring invariants enforced on every construction/update.
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::validation("asset name must not be empty"));
        }
        if self.name.len() > 512 {
            return Err(AppError::validation(
                "asset name must be at most 512 characters",
            ));
        }
        if self.revision < 1 {
            return Err(AppError::validation("asset revision must be >= 1"));
        }
        if self.lifecycle_state == LifecycleState::Archived && self.archived_at.is_none() {
            return Err(AppError::validation(
                "archived asset must have archived_at set",
            ));
        }
        if self.lifecycle_state == LifecycleState::Merged && self.merged_into.is_none() {
            return Err(AppError::validation(
                "merged asset must point at its merge target (merged_into)",
            ));
        }
        Ok(())
    }

    /// Only active assets accept normal mutations. Archived/merged assets are
    /// historical; only archive/merge flows may have touched them.
    pub fn is_mutable(&self) -> bool {
        self.lifecycle_state == LifecycleState::Active
    }

    pub fn ensure_mutable(&self) -> AppResult<()> {
        match self.lifecycle_state {
            LifecycleState::Active => Ok(()),
            LifecycleState::Archived => Err(AppError::conflict(
                "asset is archived and no longer accepts mutations",
            )),
            LifecycleState::Merged => Err(AppError::conflict(
                "asset is merged into another asset and no longer accepts mutations",
            )),
        }
    }

    /// Bumps the revision and updates `updated_at` after a canonical change.
    pub fn touch(&mut self, now: Timestamp) {
        self.revision += 1;
        self.updated_at = now;
    }

    pub fn archive(&mut self, now: Timestamp) {
        self.lifecycle_state = LifecycleState::Archived;
        self.archived_at = Some(now);
        self.touch(now);
    }

    pub fn mark_merged(&mut self, target: AssetId, now: Timestamp) {
        self.lifecycle_state = LifecycleState::Merged;
        self.merged_into = Some(target);
        self.touch(now);
    }
}
