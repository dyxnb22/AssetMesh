use assetmesh_core::application::media_service::ExternalRefInput;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, SharedClock};
use clap::Subcommand;

use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum AssetCommand {
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
pub(crate) enum RefCommand {
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

pub(crate) fn run_asset(
    factory: SharedFactory,
    clock: SharedClock,
    cmd: AssetCommand,
) -> Result<(), AppError> {
    let mut assets =
        assetmesh_core::application::asset_service::AssetService::new(factory.clone(), clock);

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
pub(crate) fn resolve_asset_id(factory: &SharedFactory, input: &str) -> Result<AssetId, AppError> {
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

pub(crate) fn parse_ref_input(raw: &str) -> Result<ExternalRefInput, AppError> {
    let (namespace, external_id) = raw.split_once(':').ok_or_else(|| {
        AppError::validation(format!("--ref must be namespace:external_id, got {raw:?}"))
    })?;
    Ok(ExternalRefInput {
        namespace: namespace.trim().to_lowercase(),
        external_id: external_id.trim().to_string(),
        source_url: None,
    })
}
