//! Software module write commands and mutation receipts (P5-05).

use std::sync::Arc;

use assetmesh_core::application::software_service::{SoftwareService, UpdateSoftwareMetadata};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use tauri::State;

use crate::dto::{MutationReceiptDto, SoftwareCommandDto};
use crate::error::DesktopError;
use crate::state::DesktopState;

#[tauri::command]
pub fn software_command(
    command: SoftwareCommandDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    software_command_impl(command, &state)
}

pub fn software_command_impl(
    command: SoftwareCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    match command {
        SoftwareCommandDto::UpdateMetadata {
            asset_id,
            expected_revision,
            name,
            summary,
            version,
            install_location,
            executable_path,
            purpose,
            notes,
            architecture,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            // No-op detection: if no updating field is supplied
            if name.is_none()
                && summary.is_none()
                && version.is_none()
                && install_location.is_none()
                && executable_path.is_none()
                && purpose.is_none()
                && notes.is_none()
                && architecture.is_none()
            {
                return Ok(MutationReceiptDto {
                    operation: "software.update_metadata".into(),
                    asset_ids: vec![asset_id],
                    revision: expected_revision,
                    changed: false,
                    warnings: vec!["No-op: no fields were updated".into()],
                });
            }

            let cmd = UpdateSoftwareMetadata {
                asset_id: id,
                name,
                summary,
                version,
                install_location,
                executable_path,
                purpose,
                notes,
                architecture,
                expected_revision,
            };

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = SoftwareService::new(factory.clone(), clock, id_gen);
                let view = svc.update_metadata(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "software.update_metadata".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
    }
}
