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
    pub const RELATION_CREATED: &str = "relation.created";
    pub const RELATION_REMOVED: &str = "relation.removed";
}

/// Well-known actors. Adapters may pass their own names, e.g. `cli`.
pub mod actors {
    pub const USER: &str = "user";
    pub const IMPORT: &str = "import";
    pub const SYSTEM: &str = "system";
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
