//! Global CLI tool discovery (npm, pipx).
//!
//! Reads provider-owned package metadata for globally installed CLI tools —
//! not a filesystem crawl. Both sources are advisory:
//!
//! - a tool that is not installed (command not found) is skipped silently:
//!   the machine simply has no candidates from that source;
//! - a tool that IS installed but fails or emits unusable output surfaces a
//!   typed [`AppError::ProviderUnavailable`].
//!
//! This avoids the noisy "everything in PATH becomes software" behavior
//! while still recording tools with real package ownership.

use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::ports::providers::{CandidateRef, SoftwareCandidate};
use assetmesh_core::{AppError, AppResult};
use serde::Deserialize;

use crate::{CommandError, CommandRunner};

pub const PROVIDER_NAME: &str = "cli_tools";

/// Discovers globally installed npm packages and pipx-managed tools.
#[derive(Debug)]
pub struct CliToolsProvider<R: CommandRunner>
where
    R: std::fmt::Debug,
{
    runner: R,
}

impl CliToolsProvider<crate::SystemCommandRunner> {
    pub fn system_default() -> Self {
        CliToolsProvider {
            runner: crate::SystemCommandRunner,
        }
    }
}

impl<R: CommandRunner> CliToolsProvider<R>
where
    R: std::fmt::Debug,
{
    pub fn with_runner(runner: R) -> Self {
        CliToolsProvider { runner }
    }
}

impl<R: CommandRunner> SoftwareDiscoveryProvider for CliToolsProvider<R>
where
    R: std::fmt::Debug,
{
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn description(&self) -> &'static str {
        "Globally installed CLI tools managed by npm or pipx"
    }

    fn scan(&self) -> AppResult<Vec<SoftwareCandidate>> {
        let mut candidates = Vec::new();
        let mut failed: Option<AppError> = None;

        match self.scan_npm() {
            Ok(mut found) => candidates.append(&mut found),
            Err(e) => failed = Some(e),
        }
        match self.scan_pipx() {
            Ok(mut found) => candidates.append(&mut found),
            Err(e) if failed.is_none() => failed = Some(e),
            Err(_) => {}
        }

        // A present-but-failing source must not silently look like "no
        // candidates"; surface the first failure instead.
        if let Some(error) = failed {
            return Err(error);
        }
        candidates.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        Ok(candidates)
    }
}

impl<R: CommandRunner> CliToolsProvider<R>
where
    R: std::fmt::Debug,
{
    fn scan_npm(&self) -> AppResult<Vec<SoftwareCandidate>> {
        let Some(stdout) = self.try_run_json("npm", &["ls", "-g", "--depth=0", "--json"])? else {
            return Ok(Vec::new());
        };
        let payload: NpmList = serde_json::from_str(&stdout).map_err(|e| {
            AppError::provider_unavailable(format!("npm output is not valid JSON: {e}"))
        })?;

        let mut candidates = Vec::new();
        for (name, package) in payload.dependencies {
            let reference = match CandidateRef::new("npm", &name) {
                Ok(reference) => reference,
                Err(_) => continue,
            };
            candidates.push(SoftwareCandidate {
                provider: PROVIDER_NAME.to_string(),
                display_name: reference.external_id.clone(),
                category: SoftwareCategory::Cli,
                install_source: InstallSource::NpmGlobal,
                version: package.and_then(|p| p.version),
                install_location: None,
                executable_path: None,
                external_refs: vec![reference],
                metadata: None,
            });
        }
        Ok(candidates)
    }

    fn scan_pipx(&self) -> AppResult<Vec<SoftwareCandidate>> {
        let Some(stdout) = self.try_run_json("pipx", &["list", "--json"])? else {
            return Ok(Vec::new());
        };
        let payload: PipxList = serde_json::from_str(&stdout).map_err(|e| {
            AppError::provider_unavailable(format!("pipx output is not valid JSON: {e}"))
        })?;

        let mut candidates = Vec::new();
        for (venv_name, venv) in payload.venvs {
            let main = venv.metadata.and_then(|m| m.main_package);
            let package_name = main
                .as_ref()
                .and_then(|p| p.package.clone())
                .unwrap_or_else(|| venv_name.clone());
            let version = main.and_then(|p| p.package_version);
            let reference = match CandidateRef::new("pipx", &package_name) {
                Ok(reference) => reference,
                Err(_) => continue,
            };
            candidates.push(SoftwareCandidate {
                provider: PROVIDER_NAME.to_string(),
                display_name: reference.external_id.clone(),
                category: SoftwareCategory::Cli,
                install_source: InstallSource::Pipx,
                version,
                install_location: None,
                executable_path: None,
                external_refs: vec![reference],
                metadata: Some(serde_json::json!({ "venv": venv_name })),
            });
        }
        Ok(candidates)
    }

    /// Runs a JSON command. `Ok(None)` means the tool is not installed
    /// (advisory skip); a present-but-failing tool is a typed error.
    fn try_run_json(&self, program: &str, args: &[&str]) -> AppResult<Option<String>> {
        let output = match self.runner.run(program, args) {
            Ok(output) => output,
            // Typed seam signal: the tool is absent, which for these
            // optional sources simply means no candidates.
            Err(CommandError::NotFound) => return Ok(None),
            Err(CommandError::Failed(e)) => {
                return Err(AppError::provider_unavailable(format!(
                    "failed to run {program}: {e}"
                )))
            }
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::provider_unavailable(format!(
                "{program} failed ({}): {}",
                output.status,
                stderr.trim()
            )));
        }
        String::from_utf8(output.stdout).map(Some).map_err(|e| {
            AppError::provider_unavailable(format!("{program} produced non-UTF-8 output: {e}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct NpmList {
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, Option<NpmPackage>>,
}

#[derive(Debug, Deserialize)]
struct NpmPackage {
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PipxList {
    #[serde(default)]
    venvs: std::collections::BTreeMap<String, PipxVenv>,
}

#[derive(Debug, Deserialize)]
struct PipxVenv {
    #[serde(default)]
    metadata: Option<PipxMetadata>,
}

#[derive(Debug, Deserialize)]
struct PipxMetadata {
    #[serde(default)]
    main_package: Option<PipxMainPackage>,
}

#[derive(Debug, Deserialize)]
struct PipxMainPackage {
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    package_version: Option<String>,
}
