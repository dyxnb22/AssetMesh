//! Discovery provider port (docs/06 provider design).
//!
//! Providers discover candidates in the external environment; they never
//! write canonical data. Scans run entirely outside transactions (ADR 0007):
//! the application stages provider output first, then opens a transaction
//! only for an explicit adoption.
//!
//! The candidate DTO lives here at the port boundary — it is the payload the
//! provider port hands to the application layer, which consumes it for
//! classification and adoption. It is advisory and never canonical state.

use crate::domain::external_ref::{validate_external_id, validate_namespace};
use crate::domain::software::{InstallSource, SoftwareCategory};
use crate::{AppError, AppResult};
use serde::Serialize;
use serde_json::Value;
use std::fmt::Debug;
/// One discovered software candidate staged by a provider. Contains only
/// information useful for matching, review, and adoption; raw provider
/// payloads stay inside `metadata` and never enter canonical state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SoftwareCandidate {
    /// Stable provider identifier, e.g. `macos_applications`.
    pub provider: String,
    pub display_name: String,
    pub category: SoftwareCategory,
    /// How the provider knows it is installed, where that is meaningful.
    pub install_source: InstallSource,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub executable_path: Option<String>,
    /// Deterministic namespaced identifiers used for exact matching
    /// (ADR 0005), e.g. `bundle_id:com.apple.Safari`.
    pub external_refs: Vec<CandidateRef>,
    /// Provider-specific extra context. Advisory only; never promoted into
    /// canonical fields automatically.
    pub metadata: Option<Value>,
}

/// One namespaced external identifier on a candidate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CandidateRef {
    pub namespace: String,
    pub external_id: String,
}

impl CandidateRef {
    pub fn new(namespace: impl Into<String>, external_id: impl Into<String>) -> AppResult<Self> {
        let reference = CandidateRef {
            namespace: namespace.into(),
            external_id: external_id.into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> AppResult<()> {
        validate_namespace(&self.namespace)?;
        validate_external_id(&self.external_id)?;
        Ok(())
    }

    pub fn key(&self) -> (&str, &str) {
        (&self.namespace, &self.external_id)
    }
}

impl SoftwareCandidate {
    /// Validates the candidate invariants every provider must uphold.
    pub fn validate(&self) -> AppResult<()> {
        if self.provider.trim().is_empty() {
            return Err(AppError::validation("candidate provider must not be empty"));
        }
        if self.display_name.trim().is_empty() {
            return Err(AppError::validation(
                "candidate display_name must not be empty",
            ));
        }
        for reference in &self.external_refs {
            reference.validate()?;
        }
        Ok(())
    }
}

/// A source of discovered software candidates (macOS applications, Homebrew,
/// CLI tool registries, ...). Implementations live in infrastructure
/// adapters; scanning must be read-only and deterministic for a fixed
/// environment.
pub trait SoftwareDiscoveryProvider: Debug + Send + Sync {
    /// Stable machine identifier of this provider, e.g. `macos_applications`.
    /// Used by the CLI to address providers and recorded on adopted
    /// candidates for provenance.
    fn name(&self) -> &'static str;

    /// Human-readable description for command help and output.
    fn description(&self) -> &'static str;

    /// Scans the environment and returns normalized candidates. Failures are
    /// typed (e.g. [`crate::AppError::ProviderUnavailable`]); a provider must
    /// never partially corrupt state — it either returns candidates or an
    /// error.
    fn scan(&self) -> AppResult<Vec<SoftwareCandidate>>;
}
