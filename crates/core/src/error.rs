//! Typed application error model.
//!
//! Domain and application code speak only in these variants. Infrastructure
//! adapters (SQLite, filesystem) map their native errors into this model at
//! the boundary so driver-specific details do not leak across the codebase
//! (ADR 0007: storage contention is a typed, retryable failure).

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum AppError {
    #[error("validation failed: {message}")]
    Validation { message: String },

    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error("import conflict: {message}")]
    ImportConflict { message: String },

    #[error("provider unavailable: {message}")]
    ProviderUnavailable { message: String },

    #[error("permission denied: {message}")]
    PermissionDenied { message: String },

    #[error("storage busy (retryable): {message}")]
    StorageBusy { message: String },

    #[error("storage failure: {message}")]
    Storage { message: String },

    #[error("unsupported schema version for {context}: found {found}, supported: {supported}")]
    UnsupportedSchemaVersion {
        context: String,
        found: String,
        supported: String,
    },
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        AppError::Validation {
            message: message.into(),
        }
    }

    pub fn not_found(entity: &'static str, id: impl std::fmt::Display) -> Self {
        AppError::NotFound {
            entity,
            id: id.to_string(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        AppError::Conflict {
            message: message.into(),
        }
    }

    pub fn import_conflict(message: impl Into<String>) -> Self {
        AppError::ImportConflict {
            message: message.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        AppError::Storage {
            message: message.into(),
        }
    }

    pub fn storage_busy(message: impl Into<String>) -> Self {
        AppError::StorageBusy {
            message: message.into(),
        }
    }

    pub fn provider_unavailable(message: impl Into<String>) -> Self {
        AppError::ProviderUnavailable {
            message: message.into(),
        }
    }

    pub fn unsupported_schema_version(
        context: impl Into<String>,
        found: impl std::fmt::Display,
        supported: impl Into<String>,
    ) -> Self {
        AppError::UnsupportedSchemaVersion {
            context: context.into(),
            found: found.to_string(),
            supported: supported.into(),
        }
    }

    /// Returns true when the error describes transient storage contention that
    /// a caller may retry (ADR 0007 failure behavior).
    pub fn is_retryable(&self) -> bool {
        matches!(self, AppError::StorageBusy { .. })
    }
}

pub type AppResult<T> = Result<T, AppError>;
