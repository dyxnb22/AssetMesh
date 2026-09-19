//! Search projection contract (ADR 0006).
//!
//! `SearchDocument` is derived, rebuildable state projected from canonical
//! data by each module. It is never canonical and is excluded from portable
//! export.

use crate::domain::ids::AssetId;
use crate::domain::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchDocument {
    pub asset_id: AssetId,
    /// Asset kind string, e.g. `media.anime`.
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub body: Option<String>,
    /// Additional searchable terms: tags, external aliases, alternate names.
    pub keywords: Vec<String>,
    pub updated_at: Timestamp,
}

/// A search result: identity plus presentation fields. Search results are
/// not a substitute for typed application queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub asset_id: AssetId,
    pub kind: String,
    pub title: String,
    pub subtitle: Option<String>,
}
