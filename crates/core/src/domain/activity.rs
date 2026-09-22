//! Semantic activity events.
//!
//! Activity records meaningful user-asset lifecycle changes (media started,
//! asset merged, ...). It is append-oriented, it is NOT operational logging,
//! and it does not make AssetMesh event-sourced: canonical state stays in
//! normal tables.

use crate::domain::ids::{ActivityId, AssetId};
use crate::domain::Timestamp;
use crate::AppResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Known event types. Kept as constants (not an enum) so modules can add
/// their own dotted event names without changing the kernel.
pub mod event_types {
    pub const ASSET_CREATED: &str = "asset.created";
    pub const ASSET_ARCHIVED: &str = "asset.archived";
    pub const ASSET_MERGED: &str = "asset.merged";
    pub const MEDIA_CREATED: &str = "media.created";
    pub const MEDIA_STARTED: &str = "media.started";
    pub const MEDIA_PROGRESS_CHANGED: &str = "media.progress_changed";
    pub const MEDIA_COMPLETED: &str = "media.completed";
    pub const MEDIA_PAUSED: &str = "media.paused";
    pub const MEDIA_DROPPED: &str = "media.dropped";
    pub const MEDIA_RATING_CHANGED: &str = "media.rating_changed";
    pub const MEDIA_IMPORTED: &str = "media.imported";
    pub const SOFTWARE_CREATED: &str = "software.created";
    pub const SOFTWARE_ADOPTED: &str = "software.adopted";
    pub const SERVICE_CREATED: &str = "service.created";
    /// An explicit renewal was recorded (docs/10 renewal use case). Historical
    /// provenance only — AssetMesh keeps no invoice ledger.
    pub const SERVICE_RENEWED: &str = "service.renewed";
    pub const RELATION_CREATED: &str = "relation.created";
    pub const RELATION_REMOVED: &str = "relation.removed";
}

/// Well-known actors. Adapters may pass their own names, e.g. `cli`.
pub mod actors {
    pub const USER: &str = "user";
    pub const IMPORT: &str = "import";
    pub const SYSTEM: &str = "system";
}

/// Which subsystem an activity event belongs to.
///
/// Derived from the event type by exactly one mapping, so an adapter never
/// string-matches `event_type` itself (docs/11 4C). A module adds its events
/// by extending [`ActivityModule::of_event_type`], not by teaching every
/// consumer a new prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityModule {
    /// Kernel asset lifecycle: created, archived, merged.
    Asset,
    Media,
    Software,
    Services,
    Relation,
    /// Legacy import (`media.imported` carries actor `import`).
    Import,
}

impl ActivityModule {
    /// The subsystem that owns `event_type`, or `None` for an unrecognized
    /// name. Unknown names are a forward-compatibility case, not an error: a
    /// newer module's events still appear, just unclassified.
    pub fn of_event_type(event_type: &str) -> Option<Self> {
        // Import is an actor-shaped subsystem that reuses the `media.` prefix,
        // so it is matched on the whole name before the prefix mapping.
        if event_type == event_types::MEDIA_IMPORTED {
            return Some(ActivityModule::Import);
        }
        let (prefix, _) = event_type.split_once('.')?;
        match prefix {
            "asset" => Some(ActivityModule::Asset),
            "media" => Some(ActivityModule::Media),
            "software" => Some(ActivityModule::Software),
            "service" => Some(ActivityModule::Services),
            "relation" => Some(ActivityModule::Relation),
            _ => None,
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            ActivityModule::Asset => "asset",
            ActivityModule::Media => "media",
            ActivityModule::Software => "software",
            ActivityModule::Services => "services",
            ActivityModule::Relation => "relation",
            ActivityModule::Import => "import",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "asset" => Some(ActivityModule::Asset),
            "media" => Some(ActivityModule::Media),
            "software" => Some(ActivityModule::Software),
            "services" | "service" => Some(ActivityModule::Services),
            "relation" => Some(ActivityModule::Relation),
            "import" => Some(ActivityModule::Import),
            _ => None,
        }
    }

    /// Every module, for adapters that offer a filter list.
    pub const ALL: [ActivityModule; 6] = [
        ActivityModule::Asset,
        ActivityModule::Media,
        ActivityModule::Software,
        ActivityModule::Services,
        ActivityModule::Relation,
        ActivityModule::Import,
    ];
}

impl std::fmt::Display for ActivityModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One semantic, append-oriented activity event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: ActivityId,
    pub occurred_at: Timestamp,
    /// Dotted semantic name, e.g. `media.completed`.
    pub event_type: String,
    /// The asset this event is about, when applicable.
    pub asset_id: Option<AssetId>,
    /// Who performed it: `user`, `import`, `system`, or a named adapter.
    pub actor: String,
    /// Small JSON payload with event-specific details.
    pub payload: Value,
}

impl ActivityEvent {
    pub fn new(
        event_type: &str,
        asset_id: Option<AssetId>,
        actor: &str,
        payload: Value,
        now: Timestamp,
    ) -> Self {
        ActivityEvent {
            id: ActivityId::generate(),
            occurred_at: now,
            event_type: event_type.to_string(),
            asset_id,
            actor: actor.to_string(),
            payload,
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.event_type.trim().is_empty() {
            return Err(crate::AppError::validation(
                "activity event_type must not be empty",
            ));
        }
        if self.actor.trim().is_empty() {
            return Err(crate::AppError::validation(
                "activity actor must not be empty",
            ));
        }
        Ok(())
    }
}
