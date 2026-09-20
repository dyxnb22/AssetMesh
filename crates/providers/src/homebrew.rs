//! Homebrew discovery (formulae + casks).
//!
//! Runs `brew info --json=v2 --installed` — Homebrew's stable
//! machine-readable output for installed packages — through the injectable
//! [`CommandRunner`] seam. Read-only: no package-changing commands are ever
//! invoked. Unavailable/failed commands and malformed output are typed
//! errors, never partial canonical state.

use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::ports::providers::{CandidateRef, SoftwareCandidate};
use assetmesh_core::{AppError, AppResult};
use serde::Deserialize;

use crate::{CommandError, CommandRunner};

pub const PROVIDER_NAME: &str = "homebrew";

/// Discovers installed Homebrew formulae and casks.
#[derive(Debug)]
pub struct HomebrewProvider<R: CommandRunner>
where
    R: std::fmt::Debug,
{
    brew_path: String,
    runner: R,
}

impl HomebrewProvider<crate::SystemCommandRunner> {
    pub fn system_default() -> Self {
        HomebrewProvider {
            brew_path: "brew".to_string(),
            runner: crate::SystemCommandRunner,
        }
    }
}

impl<R: CommandRunner> HomebrewProvider<R>
where
    R: std::fmt::Debug,
{
    pub fn with_runner(brew_path: impl Into<String>, runner: R) -> Self {
        HomebrewProvider {
            brew_path: brew_path.into(),
            runner,
        }
    }

    fn run_installed(&self) -> AppResult<String> {
        let output = match self
            .runner
            .run(&self.brew_path, &["info", "--json=v2", "--installed"])
        {
            Ok(output) => output,
            Err(CommandError::NotFound) => {
                return Err(AppError::provider_unavailable(format!(
                    "{} is not installed or not on PATH",
                    self.brew_path
                )))
            }
            Err(CommandError::Failed(e)) => {
                return Err(AppError::provider_unavailable(format!(
                    "failed to run {}: {e}",
                    self.brew_path
                )))
            }
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::provider_unavailable(format!(
                "brew info failed ({}): {}",
                output.status,
                stderr.trim()
            )));
        }
        String::from_utf8(output.stdout).map_err(|e| {
            AppError::provider_unavailable(format!("brew produced non-UTF-8 output: {e}"))
        })
    }
}

impl<R: CommandRunner> SoftwareDiscoveryProvider for HomebrewProvider<R>
where
    R: std::fmt::Debug,
{
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn description(&self) -> &'static str {
        "Installed Homebrew formulae and casks"
    }

    fn scan(&self) -> AppResult<Vec<SoftwareCandidate>> {
        let stdout = self.run_installed()?;
        let payload: BrewInfo = serde_json::from_str(&stdout).map_err(|e| {
            AppError::provider_unavailable(format!("brew output is not valid JSON: {e}"))
        })?;

        // Deterministic order: formulae then casks, each by name/token.
        let mut candidates: Vec<SoftwareCandidate> = Vec::new();
        for formula in payload.formulae {
            if let Some(candidate) = formula.into_candidate() {
                candidates.push(candidate);
            }
        }
        for cask in payload.casks {
            if let Some(candidate) = cask.into_candidate() {
                candidates.push(candidate);
            }
        }
        candidates.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        Ok(candidates)
    }
}

#[derive(Debug, Deserialize)]
struct BrewInfo {
    #[serde(default)]
    formulae: Vec<BrewFormula>,
    #[serde(default)]
    casks: Vec<BrewCask>,
}

#[derive(Debug, Deserialize)]
struct BrewFormula {
    name: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    versions: Option<BrewVersions>,
    #[serde(default)]
    installed: Vec<BrewInstalledVersion>,
}

#[derive(Debug, Deserialize)]
struct BrewVersions {
    #[serde(default)]
    stable: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrewInstalledVersion {
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrewCask {
    token: String,
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    desc: Option<String>,
}

impl BrewFormula {
    /// Normalizes one formula. Returns `None` when the identity fields are
    /// unusable — malformed entries are skipped, never canonical.
    fn into_candidate(self) -> Option<SoftwareCandidate> {
        // Prefer the actually-installed version; fall back to the tracked
        // stable version. Partial fields simply stay absent.
        let version = self
            .installed
            .iter()
            .find_map(|i| i.version.clone())
            .or_else(|| self.versions.and_then(|v| v.stable));
        let reference = CandidateRef::new("homebrew_formula", self.name).ok()?;
        Some(SoftwareCandidate {
            provider: PROVIDER_NAME.to_string(),
            display_name: reference.external_id.clone(),
            category: SoftwareCategory::Package,
            install_source: InstallSource::HomebrewFormula,
            version,
            install_location: None,
            executable_path: None,
            external_refs: vec![reference],
            metadata: self.desc.map(serde_json::Value::String),
        })
    }
}

impl BrewCask {
    /// Normalizes one cask. Returns `None` when the identity fields are
    /// unusable — malformed entries are skipped, never canonical.
    fn into_candidate(self) -> Option<SoftwareCandidate> {
        let reference = CandidateRef::new("homebrew_cask", self.token).ok()?;
        let display_name = self
            .name
            .into_iter()
            .next()
            .unwrap_or_else(|| reference.external_id.clone());
        Some(SoftwareCandidate {
            provider: PROVIDER_NAME.to_string(),
            display_name,
            category: SoftwareCategory::Application,
            install_source: InstallSource::HomebrewCask,
            version: self.version,
            install_location: None,
            executable_path: None,
            external_refs: vec![reference],
            metadata: self.desc.map(serde_json::Value::String),
        })
    }
}
