use crate::format::print_activity_page;
use assetmesh_core::application::activity_service::{ActivityQuery, ActivityService};
use assetmesh_core::application::library_service::PageRequest;
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::{AppError, AppResult};
use clap::Subcommand;

use crate::commands::asset::resolve_asset_id;
use crate::commands::service::parse_service_timestamp;
use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum ActivityCommand {
    /// Recent cross-module activity, newest first (Phase 4C).
    List {
        /// Only events about this asset.
        #[arg(long)]
        asset: Option<String>,
        /// Only these event types, e.g. media.completed (repeatable).
        #[arg(long = "type")]
        event_types: Vec<String>,
        /// Only these subsystems: asset, media, software, services, relation, import.
        #[arg(long = "module")]
        modules: Vec<String>,
        /// Only these asset kinds, e.g. media.anime (repeatable). Narrower than
        /// a module: a module owns several kinds.
        #[arg(long = "kind")]
        kinds: Vec<String>,
        /// Only these actors, e.g. user or import (repeatable).
        #[arg(long)]
        actor: Vec<String>,
        /// Inclusive lower bound (YYYY-MM-DD or RFC 3339).
        #[arg(long)]
        since: Option<String>,
        /// Inclusive upper bound (YYYY-MM-DD or RFC 3339).
        #[arg(long)]
        until: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        json: bool,
    },
}

/// Activity commands (Phase 4C): parse → query → format.
pub(crate) fn run_activity(factory: SharedFactory, cmd: ActivityCommand) -> Result<(), AppError> {
    let mut service = ActivityService::new(factory.clone());
    match cmd {
        ActivityCommand::List {
            asset,
            event_types,
            modules,
            kinds,
            actor,
            since,
            until,
            limit,
            offset,
            json,
        } => {
            let modules = modules
                .iter()
                .map(|value| {
                    assetmesh_core::domain::activity::ActivityModule::parse(value).ok_or_else(
                        || {
                            AppError::validation(format!(
                            "unknown activity module {value:?}; expected one of: asset, media, info, \
                             software, services, relation, import"
                        ))
                        },
                    )
                })
                .collect::<AppResult<Vec<_>>>()?;
            let query = ActivityQuery {
                asset_id: match asset.as_deref() {
                    Some(value) => Some(resolve_asset_id(&factory, value)?),
                    None => None,
                },
                event_types,
                modules,
                kinds: kinds
                    .into_iter()
                    .map(|value| {
                        AssetKind::parse(&value).ok_or_else(|| {
                            AppError::validation(format!(
                                "unknown asset kind {value:?}; see `assetmesh capabilities` for the \
                                 list"
                            ))
                        })
                    })
                    .collect::<AppResult<Vec<_>>>()?,
                actors: actor,
                since: since.as_deref().map(parse_service_timestamp).transpose()?,
                until: until.as_deref().map(parse_service_timestamp).transpose()?,
                page: PageRequest::new(limit, offset),
            };
            let page = service.query(&query)?;
            print_activity_page(&page, json);
        }
    }
    Ok(())
}
