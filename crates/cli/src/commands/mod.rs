//! CLI command modules.
//!
//! One module per domain: it owns the clap subcommand enum, the value-enums it
//! parses into, the conversions onto core types, and the handler that calls the
//! application service. `main.rs` therefore only has to route a command.

pub(crate) mod activity;
pub(crate) mod asset;
pub(crate) mod duplicates;
pub(crate) mod info;
pub(crate) mod library;
pub(crate) mod media;
pub(crate) mod portable;
pub(crate) mod relation;
pub(crate) mod service;
pub(crate) mod software;

use assetmesh_core::ports::repos::LifecycleFilter;
use assetmesh_storage_sqlite::SharedSqlite;
use clap::ValueEnum;

pub(crate) type SharedFactory = SharedSqlite;

pub(crate) use activity::{run_activity, ActivityCommand};
pub(crate) use asset::{run_asset, AssetCommand};
pub(crate) use duplicates::{run_duplicates, DuplicatesCommand};
pub(crate) use info::{run_info, InfoCommand};
pub(crate) use library::{run_library, LibraryCommand};
pub(crate) use media::{run_media, MediaCommand};
pub(crate) use portable::{run_export, run_import};
pub(crate) use relation::{run_relation, RelationCommand};
pub(crate) use service::{run_service, ServiceCommand};
pub(crate) use software::{run_software, SoftwareCommand};

/// `--lifecycle` is offered by the list commands of several domains, so the
/// value enum is shared rather than owned by any one of them.
#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliLifecycle {
    Active,
    /// Active plus archived.
    ActiveOrArchived,
    /// Active plus archived (merged tombstones are redirects and never listed).
    All,
}

impl From<CliLifecycle> for LifecycleFilter {
    fn from(value: CliLifecycle) -> Self {
        match value {
            CliLifecycle::Active => LifecycleFilter::Active,
            CliLifecycle::ActiveOrArchived => LifecycleFilter::ActiveOrArchived,
            CliLifecycle::All => LifecycleFilter::All,
        }
    }
}
