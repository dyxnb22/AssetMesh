//! Software module write commands, discovery preview, and mutation receipts (P5-06).

use assetmesh_core::application::software_service::{
    AdoptOverrides, AdoptTarget, CreateSoftware, UpdateSoftwareMetadata,
};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareCandidate;
use assetmesh_providers::{CliToolsProvider, HomebrewProvider, MacosApplicationsProvider};
use tauri::State;

use crate::dto::{ClassifiedCandidateDto, MutationReceiptDto, SoftwareCommandDto};
use crate::error::DesktopError;
use crate::state::DesktopState;

#[tauri::command]
pub fn software_command(
    command: SoftwareCommandDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    software_command_impl(command, &state)
}

#[tauri::command]
pub fn software_discover(
    state: State<'_, DesktopState>,
) -> Result<Vec<ClassifiedCandidateDto>, DesktopError> {
    software_discover_impl(&state)
}

pub fn software_discover_impl(
    state: &DesktopState,
) -> Result<Vec<ClassifiedCandidateDto>, DesktopError> {
    state.with_modules(|modules| {
        let mut svc = modules.software();

        let mut all_classified = Vec::new();
        if let Ok(macos_prov) = MacosApplicationsProvider::system_default() {
            if let Ok(report) = svc.discover(&macos_prov) {
                all_classified.extend(
                    report
                        .candidates
                        .into_iter()
                        .map(ClassifiedCandidateDto::from),
                );
            }
        }
        let brew_prov = HomebrewProvider::system_default();
        if let Ok(report) = svc.discover(&brew_prov) {
            all_classified.extend(
                report
                    .candidates
                    .into_iter()
                    .map(ClassifiedCandidateDto::from),
            );
        }
        let cli_prov = CliToolsProvider::system_default();
        if let Ok(report) = svc.discover(&cli_prov) {
            all_classified.extend(
                report
                    .candidates
                    .into_iter()
                    .map(ClassifiedCandidateDto::from),
            );
        }

        Ok(all_classified)
    })
}

pub fn software_command_impl(
    command: SoftwareCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    match command {
        SoftwareCommandDto::Create {
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
            tags,
        } => {
            let cat = SoftwareCategory::parse(&category).ok_or_else(|| {
                DesktopError::invalid_input(format!("unknown software category: {category}"))
            })?;

            let src = if let Some(s) = install_source.as_deref() {
                Some(InstallSource::parse(s).ok_or_else(|| {
                    DesktopError::invalid_input(format!("unknown install source: {s}"))
                })?)
            } else {
                None
            };

            let cmd = CreateSoftware {
                name,
                category: cat,
                summary,
                install_source: src,
                version,
                install_location,
                executable_path,
                purpose,
                notes,
                architecture,
                installed_at: None,
                tags,
                external_refs: Vec::new(),
            };

            state.with_modules(|modules| {
                let mut svc = modules.software();
                let view = svc.create_software(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "software.create".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
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

            state.with_modules(|modules| {
                let mut svc = modules.software();
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
        SoftwareCommandDto::AdoptCandidate {
            candidate: candidate_dto,
            target,
            purpose,
            notes,
            tags,
        } => {
            let candidate: SoftwareCandidate = candidate_dto.try_into()?;

            let adopt_target = match target.as_deref() {
                Some("create_new") => AdoptTarget::CreateNew,
                Some(id_str) if id_str != "auto" => {
                    let parsed_id = uuid::Uuid::parse_str(id_str)
                        .map(AssetId::from_uuid)
                        .map_err(|e| {
                            DesktopError::invalid_input(format!("invalid target asset ID: {e}"))
                        })?;
                    AdoptTarget::Existing(parsed_id)
                }
                _ => AdoptTarget::Auto,
            };

            let overrides = AdoptOverrides {
                target: adopt_target,
                name: None,
                category: None,
                install_source: None,
                version: None,
                install_location: None,
                executable_path: None,
                purpose,
                notes,
                architecture: None,
                tags,
            };

            state.with_modules(|modules| {
                let mut svc = modules.software();
                let outcome = svc.adopt_candidate(candidate, overrides)?;
                Ok(MutationReceiptDto {
                    operation: "software.adopt".into(),
                    asset_ids: vec![outcome.asset_id.to_string()],
                    revision: None,
                    changed: outcome.created || !outcome.updated_fields.is_empty(),
                    warnings: outcome.skipped_refs,
                })
            })
        }
        SoftwareCommandDto::Archive {
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
