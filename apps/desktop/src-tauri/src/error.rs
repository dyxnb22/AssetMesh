//! Desktop adapter error types.
//!
//! Maps internal core errors into stable error categories without leaking
//! driver-specific details, SQL, raw paths, or stack traces.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopErrorCategory {
    InvalidInput,
    NotFound,
    Conflict,
    StaleRevision,
    SetupRequired,
    StorageBusy,
    Unavailable,
    PermissionDenied,
    Unsupported,
    CorruptData,
    Internal,
}

impl DesktopErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::StaleRevision => "stale_revision",
            Self::SetupRequired => "setup_required",
            Self::StorageBusy => "storage_busy",
            Self::Unavailable => "unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::Unsupported => "unsupported",
            Self::CorruptData => "corrupt_data",
            Self::Internal => "internal",
        }
    }
}

impl PartialEq<&str> for DesktopErrorCategory {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
            || (matches!(self, Self::InvalidInput)
                && (*other == "validation" || *other == "invalid_input"))
            || (matches!(self, Self::StorageBusy)
                && (*other == "storageBusy" || *other == "storage_busy"))
            || (matches!(self, Self::CorruptData)
                && (*other == "corruptData" || *other == "corrupt_data"))
            || (matches!(self, Self::NotFound) && (*other == "notFound" || *other == "not_found"))
    }
}

impl PartialEq<DesktopErrorCategory> for &str {
    fn eq(&self, other: &DesktopErrorCategory) -> bool {
        other == self
    }
}

impl PartialEq<str> for DesktopErrorCategory {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
            || (matches!(self, Self::InvalidInput)
                && (other == "validation" || other == "invalid_input"))
            || (matches!(self, Self::StorageBusy)
                && (other == "storageBusy" || other == "storage_busy"))
            || (matches!(self, Self::CorruptData)
                && (other == "corruptData" || other == "corrupt_data"))
            || (matches!(self, Self::NotFound) && (other == "notFound" || other == "not_found"))
    }
}

impl std::fmt::Display for DesktopErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopError {
    pub category: DesktopErrorCategory,
    pub message: String,
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.category, self.message)
    }
}

impl std::error::Error for DesktopError {}

impl From<assetmesh_core::AppError> for DesktopError {
    fn from(err: assetmesh_core::AppError) -> Self {
        use assetmesh_core::AppError;
        match err {
            AppError::Validation { message } => DesktopError {
                category: DesktopErrorCategory::InvalidInput,
                message,
            },
            AppError::NotFound { entity, id } => DesktopError {
                category: DesktopErrorCategory::NotFound,
                message: format!("{entity} {id}"),
            },
            AppError::Conflict { message } | AppError::ImportConflict { message } => DesktopError {
                category: DesktopErrorCategory::Conflict,
                message,
            },
            AppError::StaleRevision { expected, found } => DesktopError {
                category: DesktopErrorCategory::StaleRevision,
                message: format!("expected revision {expected}, found {found}"),
            },
            AppError::SetupRequired { message } => DesktopError {
                category: DesktopErrorCategory::SetupRequired,
                message,
            },
            AppError::ProviderUnavailable { .. } => DesktopError {
                category: DesktopErrorCategory::Unavailable,
                message: "Provider is currently unavailable; retry later.".to_string(),
            },
            AppError::PermissionDenied { .. } => DesktopError {
                category: DesktopErrorCategory::PermissionDenied,
                message: "Permission denied.".to_string(),
            },
            AppError::StorageBusy { .. } => DesktopError {
                category: DesktopErrorCategory::StorageBusy,
                message: "Storage is currently busy; retry later.".to_string(),
            },
            AppError::UnsupportedSchemaVersion {
                context,
                found,
                supported,
            } => DesktopError {
                category: DesktopErrorCategory::Unsupported,
                message: format!("{context}: found {found}, supported {supported}"),
            },
            AppError::CorruptData { .. } => DesktopError {
                category: DesktopErrorCategory::CorruptData,
                message: "Underlying data is corrupt.".to_string(),
            },
            AppError::Storage { .. } => DesktopError {
                category: DesktopErrorCategory::Internal,
                message: "An internal storage error occurred.".to_string(),
            },
        }
    }
}

impl DesktopError {
    pub fn setup_required(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::SetupRequired,
            message: msg.into(),
        }
    }

    pub fn storage_busy(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::StorageBusy,
            message: msg.into(),
        }
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::InvalidInput,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::Internal,
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::NotFound,
            message: msg.into(),
        }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::Conflict,
            message: msg.into(),
        }
    }

    pub fn unavailable(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::Unavailable,
            message: msg.into(),
        }
    }

    pub fn corrupt_data(msg: impl Into<String>) -> Self {
        DesktopError {
            category: DesktopErrorCategory::CorruptData,
            message: msg.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assetmesh_core::AppError;

    #[test]
    fn sentinel_errors_do_not_leak_internals_or_sql_or_secrets() {
        let sql_statement = format!(
            "{} * FROM passwords WHERE secret = 'TOP_SECRET_123' at /var/db/assetmesh.db",
            "SELECT"
        );
        let sentinels = [
            AppError::storage(&sql_statement),
            AppError::storage_busy(
                "sqlite3_step locked on table assets (/Users/diaoyuxuan/secret/assetmesh.db)",
            ),
            AppError::corrupt_data(
                "database disk image is malformed: /Users/diaoyuxuan/db.sqlite: row 42 corrupted",
            ),
            AppError::provider_unavailable(
                "curl https://api.example.com?key=SECRET failed: 401 Unauthorized",
            ),
            AppError::permission_denied("chmod 000 denied on /etc/shadow or /secret/key.pem"),
        ];

        for err in sentinels {
            let desktop_err = DesktopError::from(err);
            let json = serde_json::to_string(&desktop_err).expect("serialize desktop error");

            assert!(!json.contains("SELECT"), "leaked SQL in {json}");
            assert!(!json.contains("TOP_SECRET"), "leaked SECRET in {json}");
            assert!(!json.contains("/var/db"), "leaked path in {json}");
            assert!(!json.contains("/Users/"), "leaked path in {json}");
            assert!(!json.contains("SECRET"), "leaked SECRET in {json}");
            assert!(!json.contains("/etc/shadow"), "leaked path in {json}");
        }
    }

    #[test]
    fn desktop_error_category_serializes_to_snake_case() {
        let err = DesktopError::invalid_input("bad text");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["category"], "invalid_input");

        let err = DesktopError::setup_required("db missing");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["category"], "setup_required");

        let err = DesktopError::from(AppError::corrupt_data("checksum fail"));
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["category"], "corrupt_data");
        assert_eq!(json["message"], "Underlying data is corrupt.");
    }
}
