//! Shared kernel domain types.
//!
//! The kernel stays small (ADR 0003): stable asset identity, namespaced
//! external references, relations, activity contract, tags, and the search
//! projection contract. Module semantics live in module-owned types such as
//! [`media::MediaRecord`], [`software::SoftwareRecord`], and
//! [`service::ServiceRecord`].

pub mod activity;
pub mod asset;
pub mod external_ref;
pub mod ids;
pub mod media;
pub mod relation;
pub mod search;
pub mod service;
pub mod software;
pub mod tag;
pub mod validation;

pub use activity::ActivityEvent;
pub use asset::{Asset, AssetKind, LifecycleState};
pub use external_ref::AssetExternalRef;
pub use ids::{ActivityId, AssetId, ExternalRefId, RelationId, TagId};
pub use media::{MediaEntry, MediaRecord, MediaStatus, MediaType};
pub use relation::{Relation, RelationProvenance, RelationType};
pub use search::{SearchDocument, SearchHit};
pub use service::{BillingCadence, ServiceEntry, ServiceRecord, ServiceType};
pub use software::{InstallSource, SoftwareCategory, SoftwareEntry, SoftwareRecord};
pub use tag::Tag;

/// UTC timestamp used across canonical data. Stored/exported as RFC 3339.
pub type Timestamp = chrono::DateTime<chrono::Utc>;
