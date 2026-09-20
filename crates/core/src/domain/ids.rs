//! Stable application-generated identifiers.
//!
//! AssetMesh owns canonical identity (ADR 0005). IDs are UUIDv7: sortable by
//! creation time, globally unique, and stable across export/import round
//! trips. External provider IDs never become primary keys; they are stored as
//! namespaced [`crate::domain::external_ref::AssetExternalRef`] aliases.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a fresh UUIDv7 identifier.
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::generate()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }
    };
}

define_id!(
    /// Canonical identity of an asset. Survives export/import round trips.
    AssetId
);
define_id!(
    /// Identity of a single external-reference row.
    ExternalRefId
);
define_id!(
    /// Identity of a single activity event.
    ActivityId
);
define_id!(
    /// Identity of a shared tag.
    TagId
);
define_id!(
    /// Identity of a single relation row between two assets.
    RelationId
);
