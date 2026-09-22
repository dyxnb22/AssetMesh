//! Media module write commands and mutation receipts (P5-06).

use assetmesh_core::application::media_service::{CreateMedia, UpdateMediaMetadata};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use tauri::State;

use crate::dto::{MediaCommandDto, MutationReceiptDto};
use crate::error::DesktopError;
use crate::state::DesktopState;

#[tauri::command]
pub fn media_command(
    command: MediaCommandDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    media_command_impl(command, &state)
}

pub fn media_command_impl(
    command: MediaCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    match command {
        MediaCommandDto::Create {
            title,
            media_type,
            summary,
            status,
            rating,
            year,
            platform,
            progress_unit,
            progress_current,
            progress_total,
            notes,
            tags,
        } => {
            let m_type = MediaType::parse(&media_type).ok_or_else(|| {
                DesktopError::invalid_input(format!("unknown media type: {media_type}"))
            })?;

            let m_status = if let Some(s) = status.as_deref() {
                Some(MediaStatus::parse(s).ok_or_else(|| {
                    DesktopError::invalid_input(format!("unknown media status: {s}"))
                })?)
            } else {
                None
            };

            let progress = match (progress_unit, progress_current, progress_total) {
                (Some(unit), current, total) => Progress {
                    unit: Some(unit),
                    current,
                    total,
                },
                (None, None, None) => Progress::default(),
                _ => {
                    return Err(DesktopError::invalid_input(
                        "progress requires unit when current or total is supplied",
                    ));
                }
            };

            let cmd = CreateMedia {
                title,
                media_type: m_type,
                summary,
                status: m_status,
                rating,
                year,
                platform,
                progress,
                notes,
                tags,
                external_refs: Vec::new(),
                started_at: None,
                completed_at: None,
            };

            state.with_modules(|modules| {
                let mut svc = modules.media();
                let view = svc.create_media(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "media.create".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::UpdateMetadata {
            asset_id,
            expected_revision,
            title,
            summary,
            year,
            platform,
            notes,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            if title.is_none()
                && summary.is_none()
                && year.is_none()
                && platform.is_none()
                && notes.is_none()
            {
                return Ok(MutationReceiptDto {
                    operation: "media.update_metadata".into(),
                    asset_ids: vec![asset_id],
                    revision: expected_revision,
                    changed: false,
                    warnings: vec!["No-op: no fields were updated".into()],
                });
            }

            let cmd = UpdateMediaMetadata {
                asset_id: id,
                title,
                summary,
                year,
                platform,
                notes,
                expected_revision,
            };

            state.with_modules(|modules| {
                let mut svc = modules.media();
                let view = svc.update_metadata(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "media.update_metadata".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::TransitionStatus {
            asset_id,
            status,
            expected_revision,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            let target_status = MediaStatus::parse(&status).ok_or_else(|| {
                DesktopError::invalid_input(format!("unknown media status: {status}"))
            })?;

            if target_status == MediaStatus::Planned {
                return Err(DesktopError::invalid_input(
                    "transition to planned is not supported",
                ));
            }

            state.with_modules(|modules| {
                let mut svc = modules.media();
                let view =
                    svc.transition_status_with_revision(id, target_status, expected_revision)?;
                Ok(MutationReceiptDto {
                    operation: format!("media.transition.{}", target_status.as_str()),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::UpdateProgress {
            asset_id,
            unit,
            current,
            total,
            expected_revision,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            let progress = Progress {
                unit,
                current,
                total,
            };

            state.with_modules(|modules| {
                let mut svc = modules.media();
                let view = svc.update_progress_with_revision(id, progress, expected_revision)?;
                Ok(MutationReceiptDto {
                    operation: "media.update_progress".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::Rate {
            asset_id,
            rating,
            expected_revision,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            state.with_modules(|modules| {
                let mut svc = modules.media();
                let view = svc.rate_media_with_revision(id, rating, expected_revision)?;
                Ok(MutationReceiptDto {
                    operation: "media.rate".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::Archive {
            asset_id,
            expected_revision,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            state.with_modules(|modules| {
                let mut svc = modules.asset();
                let asset = svc.archive_asset_with_revision(id, expected_revision)?;
                Ok(MutationReceiptDto {
                    operation: "asset.archive".into(),
                    asset_ids: vec![asset_id],
                    revision: Some(asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
    }
}
