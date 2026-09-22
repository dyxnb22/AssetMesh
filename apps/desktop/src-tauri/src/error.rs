//! Desktop adapter error types.
//!
//! Maps internal core errors into stable error categories without leaking
//! driver-specific details or raw stack traces.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopError {
    pub category: String,
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
        DesktopError {
            category: err.category().to_string(),
            message: err.to_string(),
        }
    }
}

impl DesktopError {
    pub fn setup_required(msg: impl Into<String>) -> Self {
        DesktopError {
            category: "setup_required".to_string(),
            message: msg.into(),
        }
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        DesktopError {
            category: "invalid_input".to_string(),
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        DesktopError {
            category: "internal".to_string(),
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        DesktopError {
            category: "not_found".to_string(),
            message: msg.into(),
        }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        DesktopError {
            category: "conflict".to_string(),
            message: msg.into(),
        }
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        DesktopError {
            category: "validation".to_string(),
            message: msg.into(),
        }
    }
}
