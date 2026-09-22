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

    #[error("stale revision: expected {expected}, found {found}")]
    StaleRevision { expected: i64, found: i64 },

    #[error("setup required: {message}")]
    SetupRequired { message: String },

    #[error("corrupt data: {message}")]
    CorruptData { message: String },
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

    pub fn stale_revision(expected: i64, found: i64) -> Self {
        AppError::StaleRevision { expected, found }
    }

    pub fn setup_required(message: impl Into<String>) -> Self {
        AppError::SetupRequired {
            message: message.into(),
        }
    }

    pub fn corrupt_data(message: impl Into<String>) -> Self {
        AppError::CorruptData {
            message: message.into(),
        }
    }

    /// Stable error category for adapters (CLI, Desktop UI, HTTP).
    /// Prevents leaking raw driver or internal details to presentation.
    pub fn category(&self) -> &'static str {
        match self {
            AppError::Validation { .. } => "invalid_input",
            AppError::NotFound { .. } => "not_found",
            AppError::Conflict { .. } | AppError::ImportConflict { .. } => "conflict",
            AppError::StaleRevision { .. } => "stale_revision",
            AppError::SetupRequired { .. } => "setup_required",
            AppError::ProviderUnavailable { .. } => "unavailable",
            AppError::PermissionDenied { .. } => "permission_denied",
            AppError::StorageBusy { .. } => "unavailable",
            AppError::UnsupportedSchemaVersion { .. } => "unsupported",
            AppError::CorruptData { .. } => "corrupt_data",
            AppError::Storage { .. } => "internal",
        }
    }

    /// Returns true when the error describes transient storage contention that
    /// a caller may retry (ADR 0007 failure behavior).
    pub fn is_retryable(&self) -> bool {
        matches!(self, AppError::StorageBusy { .. })
    }
}

pub type AppResult<T> = Result<T, AppError>;
