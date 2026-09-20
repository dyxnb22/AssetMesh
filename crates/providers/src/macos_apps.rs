//! macOS application discovery.
//!
//! Scans conventional application directories for `.app` bundles and reads
//! each bundle's `Contents/Info.plist` for stable identity (bundle
//! identifier) and metadata. No privileged APIs; everything comes from the
//! filesystem. Scanning configurable roots (instead of hard-coded paths)
//! keeps the provider deterministic and testable: tests point the roots at
//! fixture directories.
//!
//! System applications under `/System` are deliberately not scanned: the
//! default roots are `/Applications` and `~/Applications` only.

use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::ports::providers::{CandidateRef, SoftwareCandidate};
use assetmesh_core::{AppError, AppResult};
use std::path::{Path, PathBuf};

pub const PROVIDER_NAME: &str = "macos_applications";

/// Discovers installed macOS `.app` applications.
#[derive(Debug, Clone)]
pub struct MacosApplicationsProvider {
    roots: Vec<PathBuf>,
}

impl MacosApplicationsProvider {
    /// Default scan roots: `/Applications` and `~/Applications`.
    pub fn system_default() -> AppResult<Self> {
        let home = std::env::var("HOME").map_err(|e| {
            AppError::provider_unavailable(format!("cannot resolve home directory: {e}"))
        })?;
        Ok(MacosApplicationsProvider {
            roots: vec![
                PathBuf::from("/Applications"),
                Path::new(&home).join("Applications"),
            ],
        })
    }

    /// Explicit scan roots (used by tests and by callers scanning other
    /// volumes).
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        MacosApplicationsProvider { roots }
    }
}

impl SoftwareDiscoveryProvider for MacosApplicationsProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn description(&self) -> &'static str {
        "Installed macOS applications (.app bundles in the conventional application folders)"
    }

    fn scan(&self) -> AppResult<Vec<SoftwareCandidate>> {
        // Deterministic order: roots then entries sorted by path.
        let mut bundle_paths: Vec<PathBuf> = Vec::new();
        for root in &self.roots {
            let entries = match std::fs::read_dir(root) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => continue,
                Err(e) => {
                    return Err(AppError::provider_unavailable(format!(
                        "cannot read application directory {}: {e}",
                        root.display()
                    )))
                }
            };
            let mut paths = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| {
                    AppError::provider_unavailable(format!(
                        "cannot enumerate {}: {e}",
                        root.display()
                    ))
                })?;
                let path = entry.path();
                if is_app_bundle(&path) {
                    paths.push(path);
                }
            }
            paths.sort();
            bundle_paths.extend(paths);
        }

        // Same bundle id in multiple roots (or the same app linked twice):
        // first path in deterministic order wins.
        let mut candidates = Vec::new();
        let mut seen_bundle_ids = std::collections::BTreeSet::new();
        for path in bundle_paths {
            let Some(candidate) = normalize_app_bundle(&path)? else {
                continue; // malformed metadata: skipped, never canonical
            };
            let bundle_id = candidate
                .external_refs
                .iter()
                .find(|r| r.namespace == "bundle_id")
                .map(|r| r.external_id.clone());
            if let Some(id) = bundle_id {
                if !seen_bundle_ids.insert(id) {
                    continue;
                }
            }
            candidates.push(candidate);
        }
        Ok(candidates)
    }
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension().map(|e| e == "app").unwrap_or(false)
}

/// Normalizes one `.app` bundle into a candidate. Returns `Ok(None)` when
/// the bundle has unreadable or malformed metadata — discovery is advisory
/// and must never fail the whole scan because one app is broken.
fn normalize_app_bundle(bundle_path: &Path) -> AppResult<Option<SoftwareCandidate>> {
    let display_name = bundle_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let plist_path = bundle_path.join("Contents").join("Info.plist");
    let value = match plist::Value::from_file(&plist_path) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let dict = match value.as_dictionary() {
        Some(dict) => dict,
        None => return Ok(None),
    };
    let plist_string = |key: &str| dict.get(key).and_then(|v| v.as_string()).map(String::from);

    let bundle_id = plist_string("CFBundleIdentifier");
    let version =
        plist_string("CFBundleShortVersionString").or_else(|| plist_string("CFBundleVersion"));
    let executable = plist_string("CFBundleExecutable");

    let mut refs = Vec::new();
    if let Some(bundle_id) = bundle_id {
        // A bundle id that cannot be a deterministic external ref (e.g. it
        // contains whitespace) leaves the candidate unmatchable and its
        // metadata suspect: skip the app rather than fail the whole scan.
        match CandidateRef::new("bundle_id", bundle_id) {
            Ok(reference) => refs.push(reference),
            Err(_) => return Ok(None),
        }
    }

    Ok(Some(SoftwareCandidate {
        provider: PROVIDER_NAME.to_string(),
        display_name,
        category: SoftwareCategory::Application,
        install_source: InstallSource::MacosApp,
        version,
        install_location: Some(bundle_path.to_string_lossy().to_string()),
        executable_path: executable.map(|name| {
            bundle_path
                .join("Contents")
                .join("MacOS")
                .join(name)
                .to_string_lossy()
                .to_string()
        }),
        external_refs: refs,
        metadata: None,
    }))
}
