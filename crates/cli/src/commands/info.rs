use assetmesh_core::application::info_service::{CreateInfo, InfoService, UpdateInfo};
use assetmesh_core::application::library_service::{LibraryQuery, LibraryService};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::info::InfoType;
use assetmesh_core::ports::repos::{LibraryModule, PageRequest};
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use clap::{Subcommand, ValueEnum};

use crate::commands::SharedFactory;
use crate::format::{print_asset_detail, print_asset_list};

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliInfoType {
    Email,
    Url,
    ApiKey,
    Text,
}

impl From<CliInfoType> for InfoType {
    fn from(value: CliInfoType) -> Self {
        match value {
            CliInfoType::Email => Self::Email,
            CliInfoType::Url => Self::Url,
            CliInfoType::ApiKey => Self::ApiKey,
            CliInfoType::Text => Self::Text,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum InfoCommand {
    /// Save one reusable value.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long = "type", value_enum)]
        info_type: CliInfoType,
        #[arg(long)]
        value: String,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
    },
    /// Change a saved value by its asset ID.
    Update {
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long = "type", value_enum)]
        info_type: CliInfoType,
        #[arg(long)]
        value: String,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long, conflicts_with = "tags")]
        clear_tags: bool,
    },
    Get {
        id: String,
    },
    List {
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        json: bool,
    },
}

fn parse_id(raw: &str) -> Result<AssetId, AppError> {
    uuid::Uuid::parse_str(raw)
        .map(AssetId::from_uuid)
        .map_err(|e| AppError::validation(format!("invalid asset ID: {e}")))
}

pub(crate) fn run_info(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    command: InfoCommand,
) -> Result<(), AppError> {
    match command {
        InfoCommand::Add {
            name,
            info_type,
            value,
            notes,
            tag,
        } => {
            let asset = InfoService::new(factory, clock, ids).create(CreateInfo {
                name,
                info_type: info_type.into(),
                value,
                notes,
                tags: tag,
            })?;
            println!("{}", asset.id);
        }
        InfoCommand::Update {
            id,
            name,
            info_type,
            value,
            notes,
            tags,
            clear_tags,
        } => {
            let id = parse_id(&id)?;
            let revision = LibraryService::new(factory.clone())
                .get_asset(id)?
                .asset
                .revision;
            let asset = InfoService::new(factory, clock, ids).update(UpdateInfo {
                asset_id: id,
                expected_revision: revision,
                name,
                info_type: info_type.into(),
                value,
                notes,
                tags: if clear_tags {
                    Some(Vec::new())
                } else {
                    (!tags.is_empty()).then_some(tags)
                },
            })?;
            println!("{}", asset.id);
        }
        InfoCommand::Get { id } => {
            let view = LibraryService::new(factory).get_asset(parse_id(&id)?)?;
            if view.asset.kind != assetmesh_core::domain::asset::AssetKind::InfoItem {
                return Err(AppError::validation("asset is not an information item"));
            }
            print_asset_detail(&view);
        }
        InfoCommand::List {
            limit,
            offset,
            json,
        } => {
            let page = LibraryService::new(factory).list_assets(&LibraryQuery {
                modules: vec![LibraryModule::Info],
                page: PageRequest::new(limit, offset),
                ..LibraryQuery::default()
            })?;
            print_asset_list(&page, json);
        }
    }
    Ok(())
}
