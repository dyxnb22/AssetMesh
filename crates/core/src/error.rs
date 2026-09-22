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

    pub fn permission_denied(message: impl Into<String>) -> Self {
        AppError::PermissionDenied {
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

    /// Human-readable detail WITHOUT the error's template prefix.
    ///
    /// Adapters should pair `category()` with this, not with `to_string()`:
    /// `to_string()` renders the thiserror template, so a `Storage` failure
    /// would reach the UI as "storage failure: <detail>", repeating the
    /// category the adapter already derived from `category()`. Returning the
    /// detail separately also keeps one variant's template wording from
    /// becoming load-bearing — callers classify by variant, not by text.
    pub fn message(&self) -> String {
        match self {
            AppError::Validation { message }
            | AppError::Conflict { message }
            | AppError::ImportConflict { message }
            | AppError::ProviderUnavailable { message }
            | AppError::PermissionDenied { message }
            | AppError::StorageBusy { message }
            | AppError::Storage { message }
            | AppError::SetupRequired { message }
            | AppError::CorruptData { message } => message.clone(),
            AppError::NotFound { entity, id } => format!("{entity} {id}"),
            AppError::UnsupportedSchemaVersion {
                context,
                found,
                supported,
            } => format!("{context}: found {found}, supported {supported}"),
            AppError::StaleRevision { expected, found } => {
                format!("expected revision {expected}, found {found}")
            }
        }
    }

    /// Returns true when the error describes transient storage contention that
    /// a caller may retry (ADR 0007 failure behavior).
    pub fn is_retryable(&self) -> bool {
        matches!(self, AppError::StorageBusy { .. })
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_excludes_the_template_prefix() {
        // `category()` already carries the semantics, so `message()` must not
        // repeat the template wording a caller would otherwise see twice.
        let err = AppError::storage("disk gone");
        assert_eq!(err.category(), "internal");
        assert_eq!(err.message(), "disk gone");
        assert!(err.to_string().starts_with("storage failure: "));
    }

    #[test]
    fn message_renders_variants_without_a_plain_field() {
        let err = AppError::not_found("asset", "abc");
        assert_eq!(err.message(), "asset abc");

        let err = AppError::stale_revision(3, 1);
        assert_eq!(err.message(), "expected revision 3, found 1");

        let err = AppError::unsupported_schema_version("database", 4, "<= 3");
        assert_eq!(err.message(), "database: found 4, supported <= 3");
    }

    #[test]
    fn only_storage_busy_is_retryable() {
        assert!(AppError::storage_busy("locked").is_retryable());
        assert!(!AppError::storage("boom").is_retryable());
        assert!(!AppError::corrupt_data("bad").is_retryable());
    }
}
