//! Application layer: use cases coordinating domain and ports.
//!
//! All state changes go through these services. Adapters (CLI, future UI)
//! call them instead of repositories directly, and never own business rules.

pub mod activity_service;
pub mod asset_service;
pub mod duplicate_review_service;
pub mod import_media;
pub mod import_parse;
pub mod library_service;
pub mod media_service;
pub mod merge_preview_service;
pub mod portable;
pub mod projection;
pub mod relation_query_service;
pub mod relation_service;
pub mod search_service;
pub mod service_service;
pub mod shared;
pub mod software_discovery;
pub mod software_service;

use std::sync::Arc;

use crate::ports::clock::Clock;
use crate::ports::ids::IdGenerator;

pub type SharedClock = Arc<dyn Clock>;
pub type SharedIdGenerator = Arc<dyn IdGenerator>;

/// Bundled production defaults for clock + id generation.
#[derive(Clone)]
pub struct SystemDefaults {
    pub clock: SharedClock,
    pub ids: SharedIdGenerator,
}

impl Default for SystemDefaults {
    fn default() -> Self {
        SystemDefaults {
            clock: Arc::new(crate::ports::clock::SystemClock),
            ids: Arc::new(crate::ports::ids::UuidV7Generator),
        }
    }
}
