//! AssetMesh CLI — a thin adapter over the same application services the
//! future desktop UI will use (ADR 0001). Business rules live in
//! `assetmesh-core`; this crate only translates arguments and formats output.

mod format;

use assetmesh_core::application::import_media::{ImportFormatHint, MediaImportService};
use assetmesh_core::application::media_service::{
    CreateMedia, ExternalRefInput, MediaService, UpdateMediaMetadata,
};
use assetmesh_core::application::portable::{
    read_bundle_from_directory, write_bundle_to_directory, PortableExportService,
    PortableImportService,
};
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter, MediaFilter, MediaSort};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use assetmesh_storage_sqlite::SharedSqlite;
use clap::{Parser, Subcommand, ValueEnum};
use format::{print_import_report, print_media_detail, print_media_list, print_search_hits};
use std::path::PathBuf;
use std::sync::Arc;

type SharedFactory = SharedSqlite;

#[derive(Parser)]
#[command(
    name = "assetmesh",
    version,
    about = "Local-first personal digital asset manager (Phase 1: media records)"
)]
struct Cli {
    /// Path to the SQLite database (env: ASSETMESH_DB).
    #[arg(long, global = true, env = "ASSETMESH_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Media records (Phase 1 domain).
    Media {
        #[command(subcommand)]
        cmd: MediaCommand,
    },
    /// Asset-level operations: archive, merge, external refs.
    Asset {
        #[command(subcommand)]
        cmd: AssetCommand,
    },
    /// Write a portable export bundle to a directory.
    Export {
        /// Target directory (created if missing).
        dir: PathBuf,
    },
    /// Import a portable export bundle (restores canonical data by ID).
    Import {
        /// Directory containing manifest.json.
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum MediaCommand {
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
        #[arg(long)]
        year: Option<i32>,
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

#[derive(Subcommand)]
enum AssetCommand {
    /// Archive an asset (further mutations are rejected).
    Archive { id: String },
    /// Explicitly merge `loser` into `winner` (tombstone + redirect).
    Merge { loser: String, winner: String },
    /// External reference management.
    Ref {
        #[command(subcommand)]
        cmd: RefCommand,
    },
}

#[derive(Subcommand)]
enum RefCommand {
    Add {
        asset: String,
        namespace: String,
        external_id: String,
        #[arg(long)]
        url: Option<String>,
    },
    Remove {
        asset: String,
        namespace: String,
        external_id: String,
    },
    List {
        asset: String,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum CliMediaType {
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
enum CliMediaStatus {
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
enum CliMediaSort {
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
enum CliImportFormat {
    Json,
    Csv,
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), AppError> {
    let db_path = cli
        .db
        .unwrap_or_else(|| PathBuf::from("assetmesh.db"))
        .to_string_lossy()
        .to_string();
    let factory: SharedFactory = SharedSqlite(Arc::new(assetmesh_storage_sqlite::open(&db_path)?));
    let clock: SharedClock = Arc::new(assetmesh_core::ports::clock::SystemClock);
    let ids: SharedIdGenerator = Arc::new(assetmesh_core::ports::ids::UuidV7Generator);

    match cli.command {
        Command::Media { cmd } => run_media(factory, clock, ids, cmd),
        Command::Asset { cmd } => run_asset(factory, clock, ids, cmd),
        Command::Export { dir } => {
            let mut service = PortableExportService::new(factory, clock);
            let bundle = service.export(env!("CARGO_PKG_VERSION"))?;
            write_bundle_to_directory(&bundle, &dir)?;
            println!("exported portable bundle to {}", dir.display());
            for (name, count) in &bundle.manifest.record_counts {
                println!("  {name}: {count}");
            }
            Ok(())
        }
        Command::Import { path, dry_run } => {
            let bundle = read_bundle_from_directory(&path)?;
            let mut service = PortableImportService::new(factory);
            let report = service.import_bundle(&bundle, dry_run)?;
            if dry_run {
                println!("dry run — nothing written");
            }
            println!(
                "assets: {} created, {} updated",
                report.assets_created, report.assets_updated
            );
            println!(
                "media: {} created, {} updated",
                report.media_created, report.media_updated
            );
            println!(
                "external refs: {} created, {} deduplicated",
                report.external_refs_created, report.external_refs_deduplicated
            );
            println!(
                "activity: {} events, tags: {}",
                report.activity_created, report.tags_created
            );
            Ok(())
        }
    }
}

fn run_media(
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
            platform,
            notes,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = media.update_metadata(UpdateMediaMetadata {
                asset_id,
                title,
                summary,
                year,
                platform,
                notes,
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
            let mut search = SearchService::new(factory, clock);
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

fn run_asset(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: AssetCommand,
) -> Result<(), AppError> {
    let mut assets =
        assetmesh_core::application::asset_service::AssetService::new(factory.clone(), clock, ids);

    match cmd {
        AssetCommand::Archive { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            assets.archive_asset(asset_id)?;
            println!("archived {asset_id}");
        }
        AssetCommand::Merge { loser, winner } => {
            let loser_id = resolve_asset_id(&factory, &loser)?;
            let winner_id = resolve_asset_id(&factory, &winner)?;
            assets.merge_assets(loser_id, winner_id)?;
            println!("merged {loser_id} into {winner_id}");
        }
        AssetCommand::Ref { cmd } => match cmd {
            RefCommand::Add {
                asset,
                namespace,
                external_id,
                url,
            } => {
                let asset_id = resolve_asset_id(&factory, &asset)?;
                assets.attach_external_ref(asset_id, &namespace, &external_id, url)?;
                println!("attached {namespace}:{external_id} to {asset_id}");
            }
            RefCommand::Remove {
                asset,
                namespace,
                external_id,
            } => {
                let asset_id = resolve_asset_id(&factory, &asset)?;
                assets.remove_external_ref(asset_id, &namespace, &external_id)?;
                println!("removed {namespace}:{external_id} from {asset_id}");
            }
            RefCommand::List { asset } => {
                let asset_id = resolve_asset_id(&factory, &asset)?;
                let view = assets.get_asset(asset_id)?;
                if view.external_refs.is_empty() {
                    println!("(no external refs)");
                }
                for reference in &view.external_refs {
                    println!(
                        "{}:{}  {}",
                        reference.namespace,
                        reference.external_id,
                        reference.source_url.as_deref().unwrap_or("")
                    );
                }
            }
        },
    }
    Ok(())
}

/// Accepts a full UUID or a unique prefix of one.
fn resolve_asset_id(factory: &SharedFactory, input: &str) -> Result<AssetId, AppError> {
    if let Ok(uuid) = uuid::Uuid::parse_str(input.trim()) {
        return Ok(AssetId::from_uuid(uuid));
    }
    let prefix = input.trim().to_lowercase();
    if prefix.len() < 4 {
        return Err(AppError::validation(
            "asset id prefix must be at least 4 characters",
        ));
    }
    let mut shared = factory.clone();
    let matches = shared.read(&mut |uow| {
        Ok(uow
            .assets()
            .list(&AssetFilter {
                kind: None,
                lifecycle: Some(LifecycleFilter::All),
            })?
            .into_iter()
            .filter(|a| a.id.to_string().starts_with(&prefix))
            .map(|a| a.id)
            .collect::<Vec<_>>())
    })?;
    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err(AppError::not_found("asset", input)),
        _ => Err(AppError::conflict(format!(
            "asset id prefix {prefix} is ambiguous ({} matches)",
            matches.len()
        ))),
    }
}

fn parse_ref_input(raw: &str) -> Result<ExternalRefInput, AppError> {
    let (namespace, external_id) = raw.split_once(':').ok_or_else(|| {
        AppError::validation(format!("--ref must be namespace:external_id, got {raw:?}"))
    })?;
    Ok(ExternalRefInput {
        namespace: namespace.trim().to_lowercase(),
        external_id: external_id.trim().to_string(),
        source_url: None,
    })
}
