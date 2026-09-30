use crate::format::{
    print_adoption_outcome, print_scan_report, print_search_hits, print_software_detail,
    print_software_list,
};
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::software_discovery::ClassifiedCandidate;
use assetmesh_core::application::software_service::{
    AdoptOverrides, AdoptTarget, CreateSoftware, SoftwareService, UpdateSoftwareMetadata,
};
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::ports::repos::{SoftwareFilter, SoftwareSort};
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use assetmesh_providers::{CliToolsProvider, HomebrewProvider, MacosApplicationsProvider};
use clap::{Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::commands::asset::{parse_ref_input, resolve_asset_id};
use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum SoftwareCommand {
    /// Add a software record manually.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long, value_enum)]
        category: CliSoftwareCategory,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long, value_enum)]
        install_source: Option<CliInstallSource>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        install_location: Option<String>,
        #[arg(long)]
        executable_path: Option<String>,
        /// Why this software exists in your inventory.
        #[arg(long)]
        purpose: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        architecture: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        /// External reference as `namespace:external_id` (repeatable).
        #[arg(long = "ref")]
        refs: Vec<String>,
    },
    /// Show one software record with details and activity.
    Get { id: String },
    /// List software records with typed filters.
    List {
        #[arg(long, value_enum)]
        category: Option<CliSoftwareCategory>,
        #[arg(long, value_enum)]
        install_source: Option<CliInstallSource>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, value_enum, default_value_t = CliSoftwareSort::Updated)]
        sort: CliSoftwareSort,
        #[arg(long)]
        json: bool,
    },
    /// Update editable metadata.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        install_location: Option<String>,
        #[arg(long)]
        executable_path: Option<String>,
        #[arg(long)]
        purpose: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        architecture: Option<String>,
    },
    /// Full-text search over the projection.
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Scan a discovery source and classify candidates. Never mutates
    /// canonical data.
    Discover {
        /// Discovery source: macos, homebrew, cli, or all.
        provider: String,
        /// Override scan roots for the macOS application provider
        /// (repeatable; useful for scanning other volumes).
        #[arg(long)]
        root: Vec<PathBuf>,
    },
    /// Explicitly adopt a discovered candidate into canonical data.
    Adopt {
        /// Discovery source to rescan: macos, homebrew, or cli.
        provider: String,
        /// The candidate to adopt: `namespace:external_id` or exact name.
        candidate: String,
        /// Override scan roots for the macOS application provider.
        #[arg(long)]
        root: Vec<PathBuf>,
        /// Create a separate record even when a match exists.
        #[arg(long, conflicts_with = "as_target")]
        new: bool,
        /// Adopt into an existing software asset instead.
        #[arg(long = "as")]
        as_target: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum)]
        category: Option<CliSoftwareCategory>,
        #[arg(long, value_enum)]
        install_source: Option<CliInstallSource>,
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        install_location: Option<String>,
        #[arg(long)]
        executable_path: Option<String>,
        #[arg(long)]
        purpose: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        architecture: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliSoftwareCategory {
    Application,
    Cli,
    Package,
    Runtime,
    Tool,
}

impl From<CliSoftwareCategory> for SoftwareCategory {
    fn from(value: CliSoftwareCategory) -> Self {
        match value {
            CliSoftwareCategory::Application => SoftwareCategory::Application,
            CliSoftwareCategory::Cli => SoftwareCategory::Cli,
            CliSoftwareCategory::Package => SoftwareCategory::Package,
            CliSoftwareCategory::Runtime => SoftwareCategory::Runtime,
            CliSoftwareCategory::Tool => SoftwareCategory::Tool,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliInstallSource {
    MacosApp,
    HomebrewFormula,
    HomebrewCask,
    NpmGlobal,
    Pipx,
    Manual,
    System,
    Unknown,
}

impl From<CliInstallSource> for InstallSource {
    fn from(value: CliInstallSource) -> Self {
        match value {
            CliInstallSource::MacosApp => InstallSource::MacosApp,
            CliInstallSource::HomebrewFormula => InstallSource::HomebrewFormula,
            CliInstallSource::HomebrewCask => InstallSource::HomebrewCask,
            CliInstallSource::NpmGlobal => InstallSource::NpmGlobal,
            CliInstallSource::Pipx => InstallSource::Pipx,
            CliInstallSource::Manual => InstallSource::Manual,
            CliInstallSource::System => InstallSource::System,
            CliInstallSource::Unknown => InstallSource::Unknown,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliSoftwareSort {
    Updated,
    Title,
}

impl From<CliSoftwareSort> for SoftwareSort {
    fn from(value: CliSoftwareSort) -> Self {
        match value {
            CliSoftwareSort::Updated => SoftwareSort::UpdatedDesc,
            CliSoftwareSort::Title => SoftwareSort::TitleAsc,
        }
    }
}

/// Provider name → provider instance. `--root` overrides only apply to the
/// macOS applications provider.
pub(crate) fn build_provider(
    name: &str,
    roots: &[PathBuf],
) -> Result<Box<dyn SoftwareDiscoveryProvider>, AppError> {
    match name {
        "macos" | "macos_applications" => {
            let provider = if roots.is_empty() {
                MacosApplicationsProvider::system_default()?
            } else {
                MacosApplicationsProvider::with_roots(roots.to_vec())
            };
            Ok(Box::new(provider))
        }
        "homebrew" => Ok(Box::new(HomebrewProvider::system_default())),
        "cli" | "cli_tools" => Ok(Box::new(CliToolsProvider::system_default())),
        other => Err(AppError::validation(format!(
            "unknown discovery provider {other:?}; expected one of: macos, homebrew, cli"
        ))),
    }
}

pub(crate) fn run_software(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: SoftwareCommand,
) -> Result<(), AppError> {
    let mut software = SoftwareService::new(factory.clone(), clock.clone(), ids.clone());

    match cmd {
        SoftwareCommand::Add {
            name,
            category,
            summary,
            install_source,
            version,
            install_location,
            executable_path,
            purpose,
            notes,
            architecture,
            tag,
            refs,
        } => {
            let external_refs = refs
                .iter()
                .map(|raw| parse_ref_input(raw))
                .collect::<Result<Vec<_>, AppError>>()?;
            let view = software.create_software(CreateSoftware {
                name,
                category: category.into(),
                summary,
                install_source: install_source.map(Into::into),
                version,
                install_location,
                executable_path,
                purpose,
                notes,
                architecture,
                installed_at: None,
                tags: tag,
                external_refs,
            })?;
            println!("created {}", view.entry.asset.id);
            print_software_detail(&view);
        }
        SoftwareCommand::Get { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = software.get_software(asset_id)?;
            print_software_detail(&view);
        }
        SoftwareCommand::List {
            category,
            install_source,
            tag,
            sort,
            json,
        } => {
            let filter = SoftwareFilter {
                category: category.map(Into::into),
                install_source: install_source.map(Into::into),
                tag,
                sort: sort.into(),
            };
            let rows = software.list_software(&filter)?;
            print_software_list(&rows, json);
        }
        SoftwareCommand::Update {
            id,
            name,
            summary,
            version,
            install_location,
            executable_path,
            purpose,
            notes,
            architecture,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = software.update_metadata(UpdateSoftwareMetadata {
                asset_id,
                name,
                summary,
                version,
                install_location,
                executable_path,
                purpose,
                notes,
                architecture,
                ..Default::default()
            })?;
            println!("updated {}", view.entry.asset.id);
        }
        SoftwareCommand::Search { query, limit } => {
            let mut search = SearchService::new(factory, clock);
            let hits = search.search(&query, limit)?;
            print_search_hits(&hits);
        }
        SoftwareCommand::Discover { provider, root } => {
            run_discovery(&mut software, &provider, &root);
        }
        SoftwareCommand::Adopt {
            provider,
            candidate,
            root,
            new,
            as_target,
            name,
            category,
            install_source,
            version,
            install_location,
            executable_path,
            purpose,
            notes,
            architecture,
            tag,
        } => {
            let target = if new {
                AdoptTarget::CreateNew
            } else if let Some(asset) = as_target {
                AdoptTarget::Existing(resolve_asset_id(&factory, &asset)?)
            } else {
                AdoptTarget::Auto
            };
            let discovered = find_candidate(&mut software, &provider, &root, &candidate)?;
            let outcome = software.adopt_candidate(
                discovered.candidate,
                AdoptOverrides {
                    target,
                    name,
                    category: category.map(Into::into),
                    install_source: install_source.map(Into::into),
                    version,
                    install_location,
                    executable_path,
                    purpose,
                    notes,
                    architecture,
                    tags: tag,
                },
            )?;
            print_adoption_outcome(&outcome);
        }
    }
    Ok(())
}

/// Scans one provider and prints the classified candidates. Discovery is
/// read-only by construction: the scan never touches canonical state.
pub(crate) fn run_discovery(
    software: &mut SoftwareService<SharedFactory>,
    provider: &str,
    roots: &[PathBuf],
) {
    let providers: Vec<Box<dyn SoftwareDiscoveryProvider>> = if provider == "all" {
        let mut all = Vec::new();
        for name in ["macos", "homebrew", "cli"] {
            match build_provider(name, roots) {
                Ok(p) => all.push(p),
                Err(e) => eprintln!("warning: {name} discovery unavailable: {e}"),
            }
        }
        all
    } else {
        match build_provider(provider, roots) {
            Ok(p) => vec![p],
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    };

    let mut any_report = false;
    for p in &providers {
        match software.discover(p.as_ref()) {
            Ok(report) => {
                any_report = true;
                print_scan_report(&report);
            }
            Err(e) => {
                eprintln!("warning: {} discovery failed: {e}", p.name());
            }
        }
    }
    // Successful-but-empty scans still produce a report; only when every
    // requested provider failed does the command fail.
    if !any_report {
        std::process::exit(1);
    }
}

/// Re-scans a provider and selects one candidate by namespaced external ref
/// (`namespace:external_id`) or exact display name. Candidates are ephemeral:
/// adoption always refers to a fresh scan, never to cached state.
pub(crate) fn find_candidate(
    software: &mut SoftwareService<SharedFactory>,
    provider: &str,
    roots: &[PathBuf],
    selector: &str,
) -> Result<ClassifiedCandidate, AppError> {
    let provider = build_provider(provider, roots)?;
    let report = software.discover(provider.as_ref())?;

    let (wanted_ns, wanted_id) = match selector.split_once(':') {
        Some((ns, id)) => (Some(ns.trim().to_lowercase()), id.trim().to_string()),
        None => (None, selector.trim().to_string()),
    };

    let mut matches: Vec<&ClassifiedCandidate> = Vec::new();
    for classified in &report.candidates {
        if let Some(ns) = &wanted_ns {
            if classified
                .candidate
                .external_refs
                .iter()
                .any(|r| r.namespace == *ns && r.external_id == wanted_id)
            {
                matches.push(classified);
            }
        } else if assetmesh_core::application::software_discovery::normalize_name(
            &classified.candidate.display_name,
        ) == assetmesh_core::application::software_discovery::normalize_name(&wanted_id)
        {
            matches.push(classified);
        }
    }

    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err(AppError::not_found(
            "candidate",
            format!(
                "{selector} among {} scanned candidates",
                report.candidates.len()
            ),
        )),
        _ => Err(AppError::conflict(format!(
            "candidate selector {selector:?} matches {} scanned candidates; be more specific",
            matches.len()
        ))),
    }
}
