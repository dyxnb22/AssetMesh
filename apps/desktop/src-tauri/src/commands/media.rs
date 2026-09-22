//! Media module write commands and mutation receipts (P5-06).

use std::sync::Arc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::media_service::{CreateMedia, MediaService, UpdateMediaMetadata};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
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

            let progress = Progress {
                current: progress_current,
                total: progress_total,
                unit: progress_unit,
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

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = MediaService::new(factory.clone(), clock, id_gen);
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

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = MediaService::new(factory.clone(), clock, id_gen);
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
        MediaCommandDto::TransitionStatus { asset_id, status } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            let target_status = MediaStatus::parse(&status).ok_or_else(|| {
                DesktopError::invalid_input(format!("unknown media status: {status}"))
            })?;

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = MediaService::new(factory.clone(), clock, id_gen);
                let view = match target_status {
                    MediaStatus::InProgress => svc.start_media(id)?,
                    MediaStatus::Paused => svc.pause_media(id)?,
                    MediaStatus::Dropped => svc.drop_media(id)?,
                    MediaStatus::Completed => svc.complete_media(id)?,
                    MediaStatus::Planned => {
                        return Err(DesktopError::invalid_input(
                            "transition to planned is not supported",
                        ));
                    }
                };
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
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            let progress = Progress {
                unit,
                current,
                total,
            };

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = MediaService::new(factory.clone(), clock, id_gen);
                let view = svc.update_progress(id, progress)?;
                Ok(MutationReceiptDto {
                    operation: "media.update_progress".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::Rate { asset_id, rating } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = MediaService::new(factory.clone(), clock, id_gen);
                let view = svc.rate_media(id, rating)?;
                Ok(MutationReceiptDto {
                    operation: "media.rate".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        MediaCommandDto::Archive { asset_id } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = AssetService::new(factory.clone(), clock, id_gen);
                svc.archive_asset(id)?;
                Ok(MutationReceiptDto {
                    operation: "asset.archive".into(),
                    asset_ids: vec![asset_id],
                    revision: None,
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
    }
}
