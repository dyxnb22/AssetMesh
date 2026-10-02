use crate::format::{print_import_report, print_media_detail, print_media_list, print_search_hits};
use assetmesh_core::application::import_media::{ImportFormatHint, MediaImportService};
use assetmesh_core::application::media_service::{CreateMedia, MediaService, UpdateMediaMetadata};
use assetmesh_core::application::patch::Patch;
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::ports::repos::{MediaFilter, MediaSort};
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use clap::{Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::commands::asset::{parse_ref_input, resolve_asset_id};
use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum MediaCommand {
    /// Add a media record.
    Add {
        #[arg(long)]
        title: String,
        #[arg(long, value_enum)]
        media_type: CliMediaType,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long, value_enum)]
        status: Option<CliMediaStatus>,
        #[arg(long)]
        rating: Option<f64>,
        #[arg(long)]
        year: Option<i32>,
        #[arg(long)]
        platform: Option<String>,
        #[arg(long)]
        progress_current: Option<f64>,
        #[arg(long)]
        progress_total: Option<f64>,
        #[arg(long)]
        progress_unit: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        /// External reference as `namespace:external_id` (repeatable).
        #[arg(long = "ref")]
        refs: Vec<String>,
    },
    /// Show one media record with details and activity.
    Get { id: String },
    /// List media records with typed filters.
    List {
        #[arg(long = "type", value_enum)]
        media_type: Option<CliMediaType>,
        #[arg(long, value_enum)]
        status: Option<CliMediaStatus>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, value_enum, default_value_t = CliMediaSort::Updated)]
        sort: CliMediaSort,
        #[arg(long)]
        json: bool,
    },
    /// Update editable metadata.
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long, conflicts_with = "clear_year")]
        year: Option<i32>,
        /// Remove the release year.
        #[arg(long)]
        clear_year: bool,
        #[arg(long)]
        platform: Option<String>,
        #[arg(long)]
        notes: Option<String>,
    },
    /// Mark as in progress.
    Start { id: String },
    /// Update structured progress.
    Progress {
        id: String,
        #[arg(long)]
        current: Option<f64>,
        #[arg(long)]
        total: Option<f64>,
        #[arg(long)]
        unit: Option<String>,
    },
    /// Pause.
    Pause { id: String },
    /// Drop.
    Drop { id: String },
    /// Complete.
    Complete { id: String },
    /// Rate (0..=10).
    Rate {
        id: String,
        #[arg(long)]
        rating: f64,
    },
    /// Full-text search over the projection.
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Import legacy JSON/CSV media records with preview support.
    Import {
        file: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum)]
        format: Option<CliImportFormat>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliMediaType {
    Movie,
    Tv,
    Anime,
    Game,
}

impl From<CliMediaType> for MediaType {
    fn from(value: CliMediaType) -> Self {
        match value {
            CliMediaType::Movie => MediaType::Movie,
            CliMediaType::Tv => MediaType::Tv,
            CliMediaType::Anime => MediaType::Anime,
            CliMediaType::Game => MediaType::Game,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliMediaStatus {
    Planned,
    InProgress,
    Completed,
    Paused,
    Dropped,
}

impl From<CliMediaStatus> for MediaStatus {
    fn from(value: CliMediaStatus) -> Self {
        match value {
            CliMediaStatus::Planned => MediaStatus::Planned,
            CliMediaStatus::InProgress => MediaStatus::InProgress,
            CliMediaStatus::Completed => MediaStatus::Completed,
            CliMediaStatus::Paused => MediaStatus::Paused,
            CliMediaStatus::Dropped => MediaStatus::Dropped,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliMediaSort {
    Updated,
    Title,
    Rating,
    Completed,
}

impl From<CliMediaSort> for MediaSort {
    fn from(value: CliMediaSort) -> Self {
        match value {
            CliMediaSort::Updated => MediaSort::UpdatedDesc,
            CliMediaSort::Title => MediaSort::TitleAsc,
            CliMediaSort::Rating => MediaSort::RatingDesc,
            CliMediaSort::Completed => MediaSort::CompletedDesc,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliImportFormat {
    Json,
    Csv,
}

pub(crate) fn run_media(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: MediaCommand,
) -> Result<(), AppError> {
    let mut media = MediaService::new(factory.clone(), clock.clone(), ids.clone());

    match cmd {
        MediaCommand::Add {
            title,
            media_type,
            summary,
            status,
            rating,
            year,
            platform,
            progress_current,
            progress_total,
            progress_unit,
            notes,
            tag,
            refs,
        } => {
            let external_refs = refs
                .iter()
                .map(|raw| parse_ref_input(raw))
                .collect::<Result<Vec<_>, AppError>>()?;
            let view = media.create_media(CreateMedia {
                title,
                media_type: media_type.into(),
                summary,
                status: status.map(Into::into),
                rating,
                year,
                platform,
                progress: Progress {
                    current: progress_current,
                    total: progress_total,
                    unit: progress_unit,
                },
                notes,
                tags: tag,
                external_refs,
                started_at: None,
                completed_at: None,
            })?;
            println!("created {}", view.entry.asset.id);
            print_media_detail(&view);
        }
        MediaCommand::Get { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.get_media(asset_id)?;
            print_media_detail(&view);
        }
        MediaCommand::List {
            media_type,
            status,
            tag,
            sort,
            json,
        } => {
            let filter = MediaFilter {
                media_type: media_type.map(Into::into),
                status: status.map(Into::into),
                tag,
                platform: None,
                sort: sort.into(),
            };
            let rows = media.list_media(&filter)?;
            print_media_list(&rows, json);
        }
        MediaCommand::Update {
            id,
            title,
            summary,
            year,
            clear_year,
            platform,
            notes,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.update_metadata(UpdateMediaMetadata {
                asset_id,
                title,
                summary: summary.into(),
                year: if clear_year {
                    Patch::Clear
                } else {
                    year.into()
                },
                platform: platform.into(),
                notes: notes.into(),
                ..Default::default()
            })?;
            println!("updated {}", view.entry.asset.id);
        }
        MediaCommand::Start { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.start_media(asset_id)?;
            println!(
                "started {} ({})",
                view.entry.asset.id, view.entry.record.status
            );
        }
        MediaCommand::Progress {
            id,
            current,
            total,
            unit,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.update_progress(
                asset_id,
                Progress {
                    current,
                    total,
                    unit,
                },
            )?;
            println!("progress updated: {:?}", view.entry.record.progress);
        }
        MediaCommand::Pause { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.pause_media(asset_id)?;
            println!("paused {}", view.entry.asset.id);
        }
        MediaCommand::Drop { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.drop_media(asset_id)?;
            println!("dropped {}", view.entry.asset.id);
        }
        MediaCommand::Complete { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.complete_media(asset_id)?;
            println!("completed {}", view.entry.asset.id);
        }
        MediaCommand::Rate { id, rating } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.rate_media(asset_id, rating)?;
            println!("rated {} → {rating}", view.entry.asset.id);
        }
        MediaCommand::Search { query, limit } => {
            let mut search = SearchService::new(factory);
            let hits = search.search(&query, limit)?;
            print_search_hits(&hits);
        }
        MediaCommand::Import {
            file,
            dry_run,
            format,
        } => {
            let content = std::fs::read_to_string(&file).map_err(|e| {
                AppError::validation(format!("cannot read {}: {e}", file.display()))
            })?;
            let hint = match format {
                Some(CliImportFormat::Json) => ImportFormatHint::Json,
                Some(CliImportFormat::Csv) => ImportFormatHint::Csv,
                None => ImportFormatHint::Auto,
            };
            let mut importer = MediaImportService::new(factory, clock, ids);
            let report = importer.import(&content, hint, dry_run)?;
            print_import_report(&report, dry_run);
            if let Some(failure) = &report.failed {
                return Err(AppError::storage(format!(
                    "import failed after {} committed record(s): {}; earlier batches remain committed",
                    failure.records_committed, failure.error
                )));
            }
        }
    }
    Ok(())
}
