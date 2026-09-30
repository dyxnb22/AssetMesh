use crate::format::{print_asset_detail, print_asset_list};
use assetmesh_core::application::library_service::{
    LibraryModule, LibraryQuery, LibrarySearchQuery, LibraryService, LibrarySort, PageRequest,
};
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::{AppError, AppResult};
use clap::{Subcommand, ValueEnum};

use crate::commands::asset::resolve_asset_id;
use crate::commands::{CliLifecycle, SharedFactory};

#[derive(Subcommand)]
pub(crate) enum LibraryCommand {
    /// List the whole library as one page, across every module.
    List {
        /// Which lifecycle states are visible. `all` still never shows merged
        /// tombstones: they are redirects, not library entries.
        #[arg(long, value_enum, default_value_t = CliLifecycle::Active)]
        lifecycle: CliLifecycle,
        /// Restrict to modules (repeatable): media, software, or services.
        #[arg(long = "module", value_enum)]
        modules: Vec<CliLibraryModule>,
        /// Restrict to asset kinds such as `media.anime` (repeatable).
        #[arg(long = "kind")]
        kinds: Vec<String>,
        /// Require a tag (repeatable; every one must match).
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long, value_enum, default_value_t = CliLibrarySort::Updated)]
        sort: CliLibrarySort,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show one asset's unified detail view (typed module details included).
    Get {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Search the whole library through the shared search projection.
    Search {
        query: String,
        /// Archived assets are searchable but opt-in, as in `library list`.
        #[arg(long, value_enum, default_value_t = CliLifecycle::Active)]
        lifecycle: CliLifecycle,
        /// Restrict to modules (repeatable): media, software, or services.
        #[arg(long = "module", value_enum)]
        modules: Vec<CliLibraryModule>,
        /// Require a tag (repeatable; every one must match).
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        json: bool,
    },
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliLibraryModule {
    Media,
    Software,
    Services,
    Info,
}

impl From<CliLibraryModule> for LibraryModule {
    fn from(value: CliLibraryModule) -> Self {
        match value {
            CliLibraryModule::Media => LibraryModule::Media,
            CliLibraryModule::Software => LibraryModule::Software,
            CliLibraryModule::Services => LibraryModule::Services,
            CliLibraryModule::Info => LibraryModule::Info,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliLibrarySort {
    Updated,
    UpdatedAsc,
    Name,
    NameDesc,
    Kind,
}

impl From<CliLibrarySort> for LibrarySort {
    fn from(value: CliLibrarySort) -> Self {
        match value {
            CliLibrarySort::Updated => LibrarySort::UpdatedDesc,
            CliLibrarySort::UpdatedAsc => LibrarySort::UpdatedAsc,
            CliLibrarySort::Name => LibrarySort::NameAsc,
            CliLibrarySort::NameDesc => LibrarySort::NameDesc,
            CliLibrarySort::Kind => LibrarySort::KindAsc,
        }
    }
}

/// Unified library commands (Phase 4A).
///
/// This handler only translates arguments into an application query and
/// formats the result: module dispatch, lifecycle rules, ordering, and
/// pagination all live in `LibraryService`. It needs no clock and no id
/// generator because the unified library is read-only.
pub(crate) fn run_library(factory: SharedFactory, cmd: LibraryCommand) -> Result<(), AppError> {
    let mut library = LibraryService::new(factory.clone());

    match cmd {
        LibraryCommand::List {
            lifecycle,
            modules,
            kinds,
            tags,
            sort,
            limit,
            offset,
            json,
        } => {
            let kinds = kinds
                .iter()
                .map(|kind| {
                    AssetKind::parse(kind).ok_or_else(|| {
                        AppError::validation(format!(
                            "unknown asset kind {kind:?}; expected one of: media.movie, media.tv, \
                             media.anime, media.game, software.app, software.cli, \
                             software.package, software.runtime, software.tool, service.saas, \
                             service.api, service.vps, service.domain, service.local"
                        ))
                    })
                })
                .collect::<AppResult<Vec<_>>>()?;
            let query = LibraryQuery {
                lifecycle: lifecycle.into(),
                modules: modules.into_iter().map(Into::into).collect(),
                kinds,
                tags,
                media_status: None,
                sort: sort.into(),
                page: PageRequest::new(limit, offset),
            };
            let page = library.list_assets(&query)?;
            print_asset_list(&page, json);
        }
        LibraryCommand::Get { id, json } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = library.get_asset(asset_id)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&view).unwrap_or_else(|_| "{}".to_string())
                );
            } else {
                print_asset_detail(&view);
            }
        }
        LibraryCommand::Search {
            query,
            lifecycle,
            modules,
            tags,
            limit,
            offset,
            json,
        } => {
            let query = LibrarySearchQuery {
                text: query,
                lifecycle: lifecycle.into(),
                modules: modules.into_iter().map(Into::into).collect(),
                kinds: Vec::new(),
                tags,
                page: PageRequest::new(limit, offset),
            };
            let page = library.search_assets(&query)?;
            print_asset_list(&page, json);
        }
    }
    Ok(())
}
