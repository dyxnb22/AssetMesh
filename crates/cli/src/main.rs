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
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::{
    CreateService, Patch, RecordRenewal, ServiceService, UpdateService,
};
use assetmesh_core::application::software_discovery::ClassifiedCandidate;
use assetmesh_core::application::software_service::{
    AdoptOverrides, AdoptTarget, CreateSoftware, SoftwareService, UpdateSoftwareMetadata,
};
use assetmesh_core::domain::ids::{AssetId, RelationId};
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::service::{BillingCadence, ServiceType};
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter, MediaFilter, MediaSort};
use assetmesh_core::ports::repos::{ServiceFilter, ServiceSort};
use assetmesh_core::ports::repos::{SoftwareFilter, SoftwareSort};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, AppResult, SharedClock, SharedIdGenerator};
use assetmesh_providers::{CliToolsProvider, HomebrewProvider, MacosApplicationsProvider};
use assetmesh_storage_sqlite::SharedSqlite;
use clap::{Parser, Subcommand, ValueEnum};
use format::{
    fmt_time, print_adoption_outcome, print_import_report, print_media_detail, print_media_list,
    print_relation_views, print_scan_report, print_search_hits, print_service_detail,
    print_service_list, print_software_detail, print_software_list,
};
use std::path::PathBuf;
use std::sync::Arc;

type SharedFactory = SharedSqlite;

#[derive(Parser)]
#[command(
    name = "assetmesh",
    version,
    about = "Local-first personal digital asset manager (media, software, and services inventory)"
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
    /// Software inventory (Phase 2 domain).
    Software {
        #[command(subcommand)]
        cmd: SoftwareCommand,
    },
    /// Services and subscriptions (Phase 3 domain).
    Service {
        #[command(subcommand)]
        cmd: ServiceCommand,
    },
    /// Asset-level operations: archive, merge, external refs.
    Asset {
        #[command(subcommand)]
        cmd: AssetCommand,
    },
    /// Relations between assets.
    Relation {
        #[command(subcommand)]
        cmd: RelationCommand,
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
enum SoftwareCommand {
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

#[derive(Subcommand)]
enum ServiceCommand {
    /// Add a service record manually.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long = "type", value_enum)]
        service_type: CliServiceType,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        account_label: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        dashboard: Option<String>,
        /// Canonical domain text (domain services only).
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        plan: Option<String>,
        /// Decimal amount, e.g. 19.99 (two decimal places; parsed into
        /// integer minor units). Requires --currency.
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long, value_enum)]
        billing: Option<CliBillingCadence>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        renews_at: Option<String>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        expires_at: Option<String>,
        /// Auto-renew is known to be on. Use --no-auto-renew for off.
        #[arg(long, conflicts_with = "no_auto_renew")]
        auto_renew: Option<bool>,
        #[arg(long, conflicts_with = "auto_renew")]
        no_auto_renew: Option<bool>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        /// External reference as `namespace:external_id` (repeatable).
        #[arg(long = "ref")]
        refs: Vec<String>,
    },
    /// Show one service record with details and activity.
    Get { id: String },
    /// List service records with typed filters.
    List {
        #[arg(long = "type", value_enum)]
        service_type: Option<CliServiceType>,
        /// Substring match on the provider name.
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, value_enum, default_value_t = CliServiceSort::Updated)]
        sort: CliServiceSort,
        #[arg(long)]
        json: bool,
    },
    /// Update service metadata. Omitted fields are left unchanged; an empty
    /// text value clears the field.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        account_label: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        dashboard: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        plan: Option<String>,
        /// Decimal amount (with --currency); use --clear-cost to remove both.
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long, value_enum)]
        billing: Option<CliBillingCadence>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        renews_at: Option<String>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long, conflicts_with = "no_auto_renew")]
        auto_renew: Option<bool>,
        #[arg(long, conflicts_with = "auto_renew")]
        no_auto_renew: Option<bool>,
        #[arg(long)]
        notes: Option<String>,
        /// Remove cost and currency together.
        #[arg(long)]
        clear_cost: bool,
        /// Remove the billing cadence.
        #[arg(long)]
        clear_billing: bool,
        /// Remove the renewal date.
        #[arg(long)]
        clear_renews_at: bool,
        /// Remove the expiry date.
        #[arg(long)]
        clear_expires_at: bool,
        /// Set auto-renew back to unknown.
        #[arg(long, conflicts_with = "auto_renew", conflicts_with = "no_auto_renew")]
        clear_auto_renew: bool,
    },
    /// Full-text search over the projection.
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Record that a subscription renewed. The next renewal/expiry boundary is
    /// whatever you pass — AssetMesh never computes one.
    Renew {
        id: String,
        /// When the renewal happened (YYYY-MM-DD or RFC 3339). Required: a
        /// renewal without a renewal moment is a caller error (docs/10).
        #[arg(long)]
        renewed_at: String,
        /// Decimal amount charged (with --currency); updates the canonical cost.
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        /// Next renewal boundary, when known.
        #[arg(long)]
        next_renewal: Option<String>,
        /// Next expiry boundary, when known.
        #[arg(long)]
        next_expiry: Option<String>,
    },
}

#[derive(Subcommand)]
enum RelationCommand {
    /// Attach a relation between two assets.
    Add {
        source: String,
        /// Relation type, e.g. depends_on, uses, hosted_on, points_to, related_to.
        relation_type: String,
        target: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// List relations touching an asset (inverse semantics resolved).
    List { asset: String },
    /// Remove a relation by its id (see `relation list`).
    Remove { relation_id: String },
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

#[derive(ValueEnum, Clone, Copy)]
enum CliSoftwareCategory {
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
enum CliInstallSource {
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
enum CliSoftwareSort {
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

#[derive(ValueEnum, Clone, Copy)]
enum CliServiceType {
    Saas,
    Api,
    Vps,
    Domain,
    Local,
}

impl From<CliServiceType> for ServiceType {
    fn from(value: CliServiceType) -> Self {
        match value {
            CliServiceType::Saas => ServiceType::Saas,
            CliServiceType::Api => ServiceType::Api,
            CliServiceType::Vps => ServiceType::Vps,
            CliServiceType::Domain => ServiceType::Domain,
            CliServiceType::Local => ServiceType::Local,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
enum CliBillingCadence {
    Monthly,
    Quarterly,
    Yearly,
    UsageBased,
    OneTime,
    Other,
}

impl From<CliBillingCadence> for BillingCadence {
    fn from(value: CliBillingCadence) -> Self {
        match value {
            CliBillingCadence::Monthly => BillingCadence::Monthly,
            CliBillingCadence::Quarterly => BillingCadence::Quarterly,
            CliBillingCadence::Yearly => BillingCadence::Yearly,
            CliBillingCadence::UsageBased => BillingCadence::UsageBased,
            CliBillingCadence::OneTime => BillingCadence::OneTime,
            CliBillingCadence::Other => BillingCadence::Other,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
enum CliServiceSort {
    Updated,
    Title,
    Renews,
}

impl From<CliServiceSort> for ServiceSort {
    fn from(value: CliServiceSort) -> Self {
        match value {
            CliServiceSort::Updated => ServiceSort::UpdatedDesc,
            CliServiceSort::Title => ServiceSort::TitleAsc,
            CliServiceSort::Renews => ServiceSort::RenewsAsc,
        }
    }
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
        Command::Software { cmd } => run_software(factory, clock, ids, cmd),
        Command::Service { cmd } => run_service(factory, clock, ids, cmd),
        Command::Asset { cmd } => run_asset(factory, clock, ids, cmd),
        Command::Relation { cmd } => run_relation(factory.clone(), clock, ids, cmd),
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
                "software: {} created, {} updated",
                report.software_created, report.software_updated
            );
            println!(
                "services: {} created, {} updated",
                report.services_created, report.services_updated
            );
            println!(
                "relations: {} created, {} updated",
                report.relations_created, report.relations_updated
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

fn run_service(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: ServiceCommand,
) -> Result<(), AppError> {
    let mut services = ServiceService::new(factory.clone(), clock.clone(), ids.clone());

    match cmd {
        ServiceCommand::Add {
            name,
            service_type,
            summary,
            provider,
            account_label,
            endpoint,
            dashboard,
            domain,
            plan,
            cost,
            currency,
            billing,
            renews_at,
            expires_at,
            auto_renew,
            no_auto_renew,
            notes,
            tag,
            refs,
        } => {
            let external_refs = refs
                .iter()
                .map(|raw| parse_ref_input(raw))
                .collect::<Result<Vec<_>, AppError>>()?;
            let (cost_minor, currency) = parse_money_pair(cost, currency)?;
            let view = services.create_service(CreateService {
                name,
                service_type: service_type.into(),
                summary,
                provider,
                account_label,
                endpoint_url: endpoint,
                dashboard_url: dashboard,
                domain_name: domain,
                plan,
                cost_minor,
                currency,
                billing_cadence: billing.map(Into::into),
                renews_at: renews_at
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                expires_at: expires_at
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                auto_renew: parse_auto_renew(auto_renew, no_auto_renew),
                notes,
                tags: tag,
                external_refs,
            })?;
            println!("created {}", view.entry.asset.id);
            print_service_detail(&view);
        }
        ServiceCommand::Get { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = services.get_service(asset_id)?;
            print_service_detail(&view);
        }
        ServiceCommand::List {
            service_type,
            provider,
            tag,
            sort,
            json,
        } => {
            let filter = ServiceFilter {
                service_type: service_type.map(Into::into),
                provider,
                tag,
                sort: sort.into(),
            };
            let rows = services.list_services(&filter)?;
            print_service_list(&rows, json);
        }
        ServiceCommand::Update {
            id,
            name,
            summary,
            provider,
            account_label,
            endpoint,
            dashboard,
            domain,
            plan,
            cost,
            currency,
            billing,
            renews_at,
            expires_at,
            auto_renew,
            no_auto_renew,
            notes,
            clear_cost,
            clear_billing,
            clear_renews_at,
            clear_expires_at,
            clear_auto_renew,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let (cost_minor, currency) =
                if clear_cost {
                    (Patch::<i64>::Clear, Patch::<String>::Clear)
                } else {
                    match (cost, currency) {
                        (None, None) => (Patch::Leave, Patch::Leave),
                        (Some(raw), Some(cur)) => {
                            (Patch::Set(parse_cost_minor(&raw)?), Patch::Set(cur))
                        }
                        _ => return Err(AppError::validation(
                            "--cost and --currency must be given together (or use --clear-cost)",
                        )),
                    }
                };
            let view = services.update_service(UpdateService {
                asset_id,
                name,
                summary: Patch::from_text(summary),
                provider: Patch::from_text(provider),
                account_label: Patch::from_text(account_label),
                endpoint_url: Patch::from_text(endpoint),
                dashboard_url: Patch::from_text(dashboard),
                domain_name: Patch::from_text(domain),
                plan: Patch::from_text(plan),
                cost_minor,
                currency,
                billing_cadence: match billing {
                    Some(cadence) => Patch::Set(cadence.into()),
                    None if clear_billing => Patch::Clear,
                    None => Patch::Leave,
                },
                renews_at: parse_date_patch(renews_at.as_deref(), clear_renews_at)?,
                expires_at: parse_date_patch(expires_at.as_deref(), clear_expires_at)?,
                auto_renew: {
                    if clear_auto_renew {
                        Patch::Clear
                    } else if auto_renew.unwrap_or(false) {
                        Patch::Set(true)
                    } else if no_auto_renew.unwrap_or(false) {
                        Patch::Set(false)
                    } else {
                        Patch::Leave
                    }
                },
                notes: Patch::from_text(notes),
            })?;
            println!("updated {}", view.entry.asset.id);
        }
        ServiceCommand::Search { query, limit } => {
            let mut search = SearchService::new(factory, clock);
            let hits = search.search(&query, limit)?;
            print_search_hits(&hits);
        }
        ServiceCommand::Renew {
            id,
            renewed_at,
            cost,
            currency,
            next_renewal,
            next_expiry,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let (cost_minor, currency) = parse_money_pair(cost, currency)?;
            // The renewal moment is required by the argument parser, never
            // invented here: a renewal the caller cannot date is not a fact
            // AssetMesh may timestamp on their behalf (docs/10).
            let renewed_at = parse_service_timestamp(&renewed_at)?;
            let view = services.record_renewal(RecordRenewal {
                asset_id,
                renewed_at,
                charged_cost_minor: cost_minor,
                currency,
                next_renews_at: next_renewal
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                next_expires_at: next_expiry
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
            })?;
            let record = &view.entry.record;
            println!("renewed {}", view.entry.asset.id);
            println!("Renews:        {}", fmt_time(record.renews_at));
            println!("Expires:       {}", fmt_time(record.expires_at));
        }
    }
    Ok(())
}

/// Maps the auto-renew flag pair to a known setting: `--auto-renew` → on,
/// `--no-auto-renew` → off, neither → unknown (`None`).
fn parse_auto_renew(on: Option<bool>, off: Option<bool>) -> Option<bool> {
    if on.unwrap_or(false) {
        Some(true)
    } else if off.unwrap_or(false) {
        Some(false)
    } else {
        None
    }
}

/// Maps an optional date argument plus its clear flag to an explicit patch.
fn parse_date_patch(raw: Option<&str>, clear: bool) -> AppResult<Patch<Timestamp>> {
    match raw {
        Some(value) => Ok(Patch::Set(parse_service_timestamp(value)?)),
        None if clear => Ok(Patch::Clear),
        None => Ok(Patch::Leave),
    }
}

/// Parses `--cost`/`--currency` into canonical integer minor units. The two
/// arguments are a pair: both present or both absent (ADR 0010).
fn parse_money_pair(
    cost: Option<String>,
    currency: Option<String>,
) -> AppResult<(Option<i64>, Option<String>)> {
    match (cost, currency) {
        (None, None) => Ok((None, None)),
        (Some(raw), Some(currency)) => Ok((Some(parse_cost_minor(&raw)?), Some(currency))),
        _ => Err(AppError::validation(
            "--cost and --currency must be given together",
        )),
    }
}

/// Parses a decimal amount into integer minor units (ADR 0010). V1 uses two
/// decimal places; anything that could not be represented without rounding
/// is rejected rather than silently rounded. No floating point is involved.
fn parse_cost_minor(raw: &str) -> AppResult<i64> {
    let raw = raw.trim();
    let invalid = || {
        AppError::validation(format!(
            "cost must be a decimal amount like 19.99, got {raw:?}"
        ))
    };
    let (integer, fraction) = match raw.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (raw, ""),
    };
    if integer.is_empty()
        || !integer.chars().all(|c| c.is_ascii_digit())
        || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return Err(invalid());
    }
    if fraction.len() > 2 {
        return Err(AppError::validation(format!(
            "cost has more than two decimal places and cannot be represented in minor units \
             without rounding: {raw:?}"
        )));
    }
    let integer_value: i64 = integer.parse().map_err(|_| invalid())?;
    let fraction_value: i64 = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().unwrap() * 10,
        _ => fraction.parse::<i64>().unwrap(),
    };
    integer_value
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction_value))
        .ok_or_else(|| AppError::validation(format!("cost is too large: {raw:?}")))
}

/// Parses `YYYY-MM-DD` (as UTC midnight) or an RFC 3339 timestamp. Renewal
/// and expiry boundaries are caller-supplied facts — nothing is computed.
fn parse_service_timestamp(raw: &str) -> AppResult<Timestamp> {
    let raw = raw.trim();
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(ts.with_timezone(&chrono::Utc));
    }
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
        AppError::validation(format!(
            "expected a date (YYYY-MM-DD) or RFC 3339 timestamp, got {raw:?}"
        ))
    })?;
    let naive = date.and_hms_opt(0, 0, 0).ok_or_else(|| {
        AppError::validation(format!("could not build a timestamp from date {raw:?}"))
    })?;
    Ok(naive.and_utc())
}

/// Provider name → provider instance. `--root` overrides only apply to the
/// macOS applications provider.
fn build_provider(
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

fn run_software(
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
fn run_discovery(software: &mut SoftwareService<SharedFactory>, provider: &str, roots: &[PathBuf]) {
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
fn find_candidate(
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

fn run_relation(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: RelationCommand,
) -> Result<(), AppError> {
    let mut relations = RelationService::new(factory.clone(), clock, ids);

    match cmd {
        RelationCommand::Add {
            source,
            relation_type,
            target,
            note,
        } => {
            let source_id = resolve_asset_id(&factory, &source)?;
            let target_id = resolve_asset_id(&factory, &target)?;
            let relation_type = RelationType::parse(&relation_type).ok_or_else(|| {
                AppError::validation(format!(
                    "unknown relation type {relation_type:?}; expected one of: depends_on, \
                     dependency_of, uses, used_by, installed_via, installs, hosted_on, hosts, \
                     points_to, pointed_to_by, related_to"
                ))
            })?;
            let relation = relations.attach(
                source_id,
                relation_type,
                target_id,
                note,
                RelationProvenance::Manual,
            )?;
            println!(
                "attached {} {} → {}",
                relation.relation_type, relation.source_asset_id, relation.target_asset_id
            );
        }
        RelationCommand::List { asset } => {
            let asset_id = resolve_asset_id(&factory, &asset)?;
            let views = relations.list_for_asset(asset_id)?;
            print_relation_views(&views);
        }
        RelationCommand::Remove { relation_id } => {
            let id = RelationId::from_uuid(
                uuid::Uuid::parse_str(relation_id.trim())
                    .map_err(|_| AppError::validation("relation id must be a UUID"))?,
            );
            relations.remove(id)?;
            println!("removed {id}");
        }
    }
    Ok(())
}
