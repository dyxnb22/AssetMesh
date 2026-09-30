//! Reusable pieces of information owned by the Info module.

use crate::domain::ids::AssetId;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfoType {
    Email,
    Url,
    ApiKey,
    Text,
}

impl InfoType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Url => "url",
            Self::ApiKey => "api_key",
            Self::Text => "text",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "email" => Some(Self::Email),
            "url" => Some(Self::Url),
            "api_key" => Some(Self::ApiKey),
            "text" => Some(Self::Text),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Email => "Email",
            Self::Url => "URL",
            Self::ApiKey => "API key",
            Self::Text => "Text",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoRecord {
    pub asset_id: AssetId,
    pub info_type: InfoType,
    pub value: String,
    pub notes: Option<String>,
}

impl InfoRecord {
    pub fn validate(&mut self) -> AppResult<()> {
        self.value = self.value.trim().to_string();
        if self.value.is_empty() {
            return Err(AppError::validation("information value must not be empty"));
        }
        if self.value.len() > 16_384 {
            return Err(AppError::validation(
                "information value must be at most 16384 bytes",
            ));
        }
        self.notes = self.notes.take().and_then(|notes| {
            let notes = notes.trim().to_string();
            (!notes.is_empty()).then_some(notes)
        });
        if self.notes.as_ref().is_some_and(|notes| {
            notes.chars().any(|character| {
                character.is_control()
                    && character != '\n'
                    && character != '\r'
                    && character != '\t'
            })
        }) {
            return Err(AppError::validation(
                "notes must not contain control characters",
            ));
        }
        if self.notes.as_ref().is_some_and(|notes| notes.len() > 8192) {
            return Err(AppError::validation("notes must be at most 8192 bytes"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InfoEntry {
    pub asset: crate::domain::asset::Asset,
    pub record: InfoRecord,
    pub tags: Vec<String>,
}
