//! Software module write commands, discovery preview, and mutation receipts (P5-06).

use assetmesh_core::application::software_service::{
    AdoptOverrides, AdoptTarget, CreateSoftware, UpdateSoftwareMetadata,
};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::SoftwareCandidate;
use assetmesh_core::ports::providers::SoftwareDiscoveryProvider;
use assetmesh_providers::{CliToolsProvider, HomebrewProvider, MacosApplicationsProvider};
use tauri::{Manager, State};

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
pub async fn software_discover<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<DiscoveryReportDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        software_discover_impl(&app.state::<DesktopState>())
    })
    .await
    .map_err(|error| DesktopError::internal(error.to_string()))?
}

#[derive(Debug, serde::Serialize)]
pub struct DiscoveryFailureDto {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct DiscoveryReportDto {
    pub candidates: Vec<ClassifiedCandidateDto>,
    pub completed_sources: Vec<String>,
    pub failed_sources: Vec<DiscoveryFailureDto>,
}

pub fn software_discover_impl(state: &DesktopState) -> Result<DiscoveryReportDto, DesktopError> {
    let mut providers: Vec<Box<dyn SoftwareDiscoveryProvider>> = Vec::new();
    let mut failures = Vec::new();
    match MacosApplicationsProvider::system_default() {
        Ok(provider) => providers.push(Box::new(provider)),
        Err(error) => failures.push(DiscoveryFailureDto {
            source: "macos_applications".into(),
            message: error.to_string(),
        }),
    }
    providers.push(Box::new(HomebrewProvider::system_default()));
    providers.push(Box::new(CliToolsProvider::npm_default()));
    providers.push(Box::new(CliToolsProvider::pipx_default()));
    let sources: Vec<_> = providers.iter().map(|provider| provider.as_ref()).collect();
    let mut report = software_discover_sources_impl(state, &sources)?;
    report.failed_sources.extend(failures);
    Ok(report)
}

pub fn software_discover_sources_impl(
    state: &DesktopState,
    providers: &[&dyn SoftwareDiscoveryProvider],
) -> Result<DiscoveryReportDto, DesktopError> {
    state.with_modules(|modules| {
        let mut svc = modules.software();
        let mut result = DiscoveryReportDto::default();
        for provider in providers {
            match svc.discover(*provider) {
                Ok(report) => {
                    result.completed_sources.push(provider.name().into());
                    result.candidates.extend(
                        report
                            .candidates
                            .into_iter()
                            .map(ClassifiedCandidateDto::from),
                    );
                }
                Err(error) => result.failed_sources.push(DiscoveryFailureDto {
                    source: provider.name().into(),
                    message: error.to_string(),
                }),
            }
        }
        Ok(result)
    })
}

pub fn software_command_impl(
    command: SoftwareCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    match &command {
        SoftwareCommandDto::UpdateMetadata {
            expected_revision, ..
        }
        | SoftwareCommandDto::Archive {
            expected_revision, ..
        } => {
            super::required_revision(*expected_revision, "expected_revision")?;
        }
        _ => {}
    }
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
                && summary.is_leave()
                && version.is_leave()
                && install_location.is_leave()
                && executable_path.is_leave()
                && purpose.is_leave()
                && notes.is_leave()
                && architecture.is_leave()
            {
                return state.with_modules(|modules| {
                    let view = modules.software().get_software(id)?;
                    view.entry.asset.ensure_mutable()?;
                    let actual = view.entry.asset.revision;
                    if let Some(expected) = expected_revision {
                        if actual != expected {
                            return Err(DesktopError::from(
                                assetmesh_core::AppError::stale_revision(expected, actual),
                            ));
                        }
                    }
                    Ok(MutationReceiptDto {
                        operation: "software.update_metadata".into(),
                        asset_ids: vec![asset_id],
                        revision: Some(actual),
                        changed: false,
                        warnings: vec!["No-op: no fields were updated".into()],
                    })
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
            expected_revision,
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
                let outcome =
                    svc.adopt_candidate_with_revision(candidate, overrides, expected_revision)?;
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
