use assetmesh_core::application::info_service::{CreateInfo, UpdateInfo};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::info::InfoType;
use serde::Deserialize;
use tauri::State;

use crate::dto::MutationReceiptDto;
use crate::error::DesktopError;
use crate::state::DesktopState;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum InfoCommandDto {
    Create {
        name: String,
        info_type: String,
        value: String,
        notes: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    },
    BatchCreate {
        items: Vec<InfoImportItemDto>,
    },
    Update {
        asset_id: String,
        expected_revision: i64,
        name: String,
        info_type: String,
        value: String,
        notes: Option<String>,
        tags: Option<Vec<String>>,
    },
    Archive {
        asset_id: String,
        expected_revision: i64,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfoImportItemDto {
    name: String,
    info_type: String,
    value: String,
    notes: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

fn parse_type(value: &str) -> Result<InfoType, DesktopError> {
    InfoType::parse(value)
        .ok_or_else(|| DesktopError::invalid_input(format!("unknown information type: {value}")))
}

fn parse_id(value: &str) -> Result<AssetId, DesktopError> {
    uuid::Uuid::parse_str(value)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))
}

#[tauri::command]
pub fn info_command(
    command: InfoCommandDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    info_command_impl(command, &state)
}

pub fn info_command_impl(
    command: InfoCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    state.with_modules(|modules| {
        let (operation, assets) = match command {
            InfoCommandDto::Create {
                name,
                info_type,
                value,
                notes,
                tags,
            } => {
                let asset = modules.info().create(CreateInfo {
                    name,
                    info_type: parse_type(&info_type)?,
                    value,
                    notes,
                    tags,
                })?;
                ("info.create", vec![asset])
            }
            InfoCommandDto::BatchCreate { items } => {
                let inputs = items
                    .into_iter()
                    .map(|item| {
                        Ok(CreateInfo {
                            name: item.name,
                            info_type: parse_type(&item.info_type)?,
                            value: item.value,
                            notes: item.notes,
                            tags: item.tags,
                        })
                    })
                    .collect::<Result<Vec<_>, DesktopError>>()?;
                state.recovery_point("before_import")?;
                let assets = modules.info().create_many(inputs)?;
                ("info.batch_create", assets)
            }
            InfoCommandDto::Update {
                asset_id,
                expected_revision,
                name,
                info_type,
                value,
                notes,
                tags,
            } => {
                let asset = modules.info().update(UpdateInfo {
                    asset_id: parse_id(&asset_id)?,
                    expected_revision,
                    name,
                    info_type: parse_type(&info_type)?,
                    value,
                    notes,
                    tags,
                })?;
                ("info.update", vec![asset])
            }
            InfoCommandDto::Archive {
                asset_id,
                expected_revision,
            } => {
                let id = parse_id(&asset_id)?;
                let detail = modules.library().get_asset(id)?;
                if detail.asset.kind != assetmesh_core::domain::asset::AssetKind::InfoItem {
                    return Err(DesktopError::invalid_input(
                        "asset is not an information item",
                    ));
                }
                let asset = modules
                    .asset()
                    .archive_asset_with_revision(id, Some(expected_revision))?;
                ("info.archive", vec![asset])
            }
        };
        Ok(MutationReceiptDto {
            operation: operation.into(),
            asset_ids: assets.iter().map(|asset| asset.id.to_string()).collect(),
            revision: (assets.len() == 1).then(|| assets[0].revision),
            changed: true,
            warnings: Vec::new(),
        })
    })
}
