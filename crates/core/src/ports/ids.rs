//! Identifier generation. Application-generated, UUIDv7-based (sortable).

use std::fmt::Debug;
use uuid::Uuid;

pub trait IdGenerator: Debug + Send + Sync {
    fn new_id(&self) -> Uuid;
}

/// Production generator producing UUIDv7 identifiers.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn new_id(&self) -> Uuid {
        Uuid::now_v7()
    }
}
