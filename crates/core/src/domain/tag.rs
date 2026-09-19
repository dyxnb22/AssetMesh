//! Shared tags: lightweight labels on assets (ADR 0003, 06-module-system).
//! Tags are shared asset infrastructure, not media-only columns.

use crate::domain::ids::TagId;
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    pub created_at: Timestamp,
}

impl Tag {
    pub fn new(name: impl Into<String>, now: Timestamp) -> Self {
        Tag {
            id: TagId::generate(),
            name: name.into(),
            created_at: now,
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        let name = self.name.trim();
        if name.is_empty() || name.len() > 64 {
            return Err(AppError::validation("tag name must be 1..=64 characters"));
        }
        Ok(())
    }
}
