//! AssetMesh CLI — a thin adapter over the same application services the
//! desktop UI uses (ADR 0001). Business rules live in
//! `assetmesh-core`; this crate only translates arguments and formats output.

mod commands;
mod format;

use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use assetmesh_storage_sqlite::SharedSqlite;
use clap::{Parser, Subcommand};
use commands::{
    run_activity, run_asset, run_duplicates, run_export, run_import, run_info, run_library,
    run_media, run_relation, run_service, run_software, ActivityCommand, AssetCommand,
    DuplicatesCommand, InfoCommand, LibraryCommand, MediaCommand, RelationCommand, ServiceCommand,
    SharedFactory, SoftwareCommand,
};
use std::path::PathBuf;
use std::sync::Arc;

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
    /// Reusable email addresses, URLs, API keys and text.
    Info {
        #[command(subcommand)]
        cmd: InfoCommand,
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
    /// Cross-module activity history (Phase 4C).
    Activity {
        #[command(subcommand)]
        cmd: ActivityCommand,
    },
    /// Review likely duplicate pairs (Phase 4C). Never merges.
    Duplicates {
        #[command(subcommand)]
        cmd: DuplicatesCommand,
    },
    /// The unified library across Media, Software, and Services (Phase 4).
    Library {
        #[command(subcommand)]
        cmd: LibraryCommand,
    },
    /// Write a portable export bundle to a directory.
    Export {
        /// Target directory (created if missing).
        dir: PathBuf,
        /// Include API key assets and their history in this unencrypted export.
        #[arg(long)]
        include_api_keys: bool,
    },
    /// Import a portable export bundle (restores canonical data by ID).
    Import {
        /// Directory containing manifest.json.
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
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
        Command::Info { cmd } => run_info(factory, clock, ids, cmd),
        Command::Asset { cmd } => run_asset(factory, clock, cmd),
        Command::Relation { cmd } => run_relation(factory.clone(), clock, ids, cmd),
        Command::Library { cmd } => run_library(factory, cmd),
        Command::Activity { cmd } => run_activity(factory, cmd),
        Command::Duplicates { cmd } => run_duplicates(factory, cmd),
        Command::Export {
            dir,
            include_api_keys,
        } => run_export(factory, clock, dir, include_api_keys),
        Command::Import { path, dry_run } => run_import(factory, path, dry_run),
    }
}
