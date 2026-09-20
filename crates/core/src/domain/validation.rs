//! Text invariants shared by every module.
//!
//! Modules own their typed details (ADR 0003) and must not reach into each
//! other's domain types. Free-text handling is a kernel-level concern, so it
//! lives here: every module's optional text field is trimmed the same way and
//! control characters are rejected the same way.

use crate::{AppError, AppResult};

/// Trims an optional free-text field; `None`/whitespace-only collapse to
/// `None` so empty strings never become meaningful state.
pub fn optional_text(value: &Option<String>, field: &str) -> AppResult<Option<String>> {
    match value {
        None => Ok(None),
        Some(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().any(|c| c.is_control()) {
                return Err(AppError::validation(format!(
                    "{field} must not contain control characters"
                )));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

/// Rejects a value longer than `max` bytes. Shared so bound checks read
/// identically across modules.
pub fn bounded(text: &str, max: usize, field: &str) -> AppResult<()> {
    if text.len() > max {
        return Err(AppError::validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(())
}
