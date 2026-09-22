//! Namespaced external references (ADR 0005).
//!
//! External identifiers (steam, igdb, tmdb, bundle_id, ...) are aliases of a
//! canonical `Asset`, never replacements for AssetMesh identity. The pair
//! `(namespace, external_id)` is globally unique.

use crate::domain::ids::{AssetId, ExternalRefId};
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetExternalRef {
    pub id: ExternalRefId,
    pub asset_id: AssetId,
    /// Stable machine identifier of the source system, e.g. `steam`, `igdb`,
    /// `tmdb`, `bundle_id`, `homebrew_cask`. Not a UI label.
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
    /// Optional free-form provider/import metadata (JSON text). Never a
    /// substitute for typed fields.
    pub metadata: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AssetExternalRef {
    pub fn new(
        asset_id: AssetId,
        namespace: impl Into<String>,
        external_id: impl Into<String>,
        source_url: Option<String>,
        now: Timestamp,
    ) -> Self {
        AssetExternalRef {
            id: ExternalRefId::generate(),
            asset_id,
            namespace: namespace.into(),
            external_id: external_id.into(),
            source_url,
            metadata: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn key(&self) -> (&str, &str) {
        (&self.namespace, &self.external_id)
    }

    pub fn validate(&self) -> AppResult<()> {
        validate_namespace(&self.namespace)?;
        validate_external_id(&self.external_id)?;
        // A source URL is optional, but when present it lands in canonical
        // state and is exported verbatim, so it gets the same shape check as
        // a Services URL: no userinfo and no credential query parameter.
        if let Some(url) = &self.source_url {
            crate::domain::validation::url_shape(url)?;
        }
        Ok(())
    }
}

pub fn validate_namespace(namespace: &str) -> AppResult<()> {
    if namespace.is_empty() || namespace.len() > 64 {
        return Err(AppError::validation(
            "external-ref namespace must be 1..=64 characters",
        ));
    }
    if !namespace
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(AppError::validation(
            "external-ref namespace must be lowercase ASCII letters, digits, '_' or '-'",
        ));
    }
    Ok(())
}

pub fn validate_external_id(external_id: &str) -> AppResult<()> {
    if external_id.is_empty() || external_id.len() > 512 {
        return Err(AppError::validation(
            "external-ref external_id must be 1..=512 characters",
        ));
    }
    if external_id.chars().any(|c| c.is_whitespace()) {
        return Err(AppError::validation(
            "external-ref external_id must not contain whitespace",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_validation() {
        assert!(validate_namespace("steam").is_ok());
        assert!(validate_namespace("homebrew_cask").is_ok());
        assert!(validate_namespace("bundle-id").is_ok());
        assert!(validate_namespace("").is_err());
        assert!(validate_namespace("Steam").is_err());
        assert!(validate_namespace("has space").is_err());
    }

    #[test]
    fn external_id_validation() {
        assert!(validate_external_id("1091500").is_ok());
        assert!(validate_external_id("com.microsoft.VSCode").is_ok());
        assert!(validate_external_id("").is_err());
        assert!(validate_external_id("has whitespace").is_err());
    }

    #[test]
    fn ref_validation() {
        let mut r = AssetExternalRef::new(
            AssetId::generate(),
            "steam",
            "1091500",
            None,
            chrono::Utc::now(),
        );
        assert!(r.validate().is_ok());
        r.namespace = "Steam".into();
        assert!(r.validate().is_err());
    }

    #[test]
    fn source_url_validation() {
        let mk = |url: Option<&str>| {
            AssetExternalRef::new(
                AssetId::generate(),
                "steam",
                "1091500",
                url.map(|s| s.to_string()),
                chrono::Utc::now(),
            )
        };
        assert!(mk(None).validate().is_ok());
        assert!(mk(Some("https://store.steampowered.com/app/1091500"))
            .validate()
            .is_ok());
        assert!(mk(Some("not a url")).validate().is_err());
        assert!(mk(Some("https://store.example.com/app?api_key=SECRET"))
            .validate()
            .is_err());
        assert!(mk(Some("https://user:pass@store.example.com/app"))
            .validate()
            .is_err());
    }
}
