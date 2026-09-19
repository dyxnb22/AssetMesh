//! Shared kernel domain types.
//!
//! The kernel stays small (ADR 0003): stable asset identity, namespaced
//! external references, activity contract, tags, and the search projection
//! contract. Module semantics (media, later software/services) live in
//! module-owned types such as [`media::MediaRecord`].

pub mod activity;
pub mod asset;
pub mod external_ref;
pub mod ids;
pub mod media;
pub mod search;
pub mod tag;

pub use activity::ActivityEvent;
pub use asset::{Asset, AssetKind, LifecycleState};
pub use external_ref::AssetExternalRef;
pub use ids::{ActivityId, AssetId, ExternalRefId, TagId};
pub use media::{MediaEntry, MediaRecord, MediaStatus, MediaType};
pub use search::{SearchDocument, SearchHit};
pub use tag::Tag;

/// UTC timestamp used across canonical data. Stored/exported as RFC 3339.
pub type Timestamp = chrono::DateTime<chrono::Utc>;
