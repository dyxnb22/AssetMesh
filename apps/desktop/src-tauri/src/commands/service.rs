//! Services module write commands, subscription updates, and renewals (P5-06).

use assetmesh_core::application::service_service::{
    parse_money_pair, parse_money_update, CreateService, Patch, RecordRenewal, UpdateService,
};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::service::{BillingCadence, ServiceType};
use assetmesh_core::domain::Timestamp;
use tauri::State;

use crate::dto::{MutationReceiptDto, ServiceCommandDto};
use crate::error::DesktopError;
use crate::state::DesktopState;

#[tauri::command]
pub fn service_command(
    command: ServiceCommandDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    service_command_impl(command, &state)
}

pub fn parse_timestamp(s: &str) -> Result<Timestamp, DesktopError> {
    let s = s.trim();
    if let Ok(dt) = s.parse::<Timestamp>() {
        return Ok(dt);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(dt) = d.and_hms_opt(0, 0, 0) {
            return Ok(dt.and_utc());
        }
    }
    Err(DesktopError::invalid_input(format!(
        "invalid timestamp format: {s}"
    )))
}

pub fn service_command_impl(
    command: ServiceCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    state.with_service_operation(|| service_command_locked(command, state))
}

fn service_command_locked(
    command: ServiceCommandDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    match &command {
        ServiceCommandDto::Create { .. } => {}
        ServiceCommandDto::Update {
            expected_revision, ..
        }
        | ServiceCommandDto::RecordRenewal {
            expected_revision, ..
        }
        | ServiceCommandDto::Archive {
            expected_revision, ..
        } => {
            super::required_revision(*expected_revision, "expected_revision")?;
        }
    }
    match command {
        ServiceCommandDto::Create {
            name,
            service_type,
            summary,
            provider,
            account_label,
            endpoint_url,
            dashboard_url,
            domain_name,
            plan,
            cost,
            currency,
            billing_cadence,
            renews_at,
            expires_at,
            auto_renew,
            notes,
            project_dir,
            start_command,
            stop_command,
            tags,
        } => {
            let st = ServiceType::parse(&service_type).ok_or_else(|| {
                DesktopError::invalid_input(format!("unknown service type: {service_type}"))
            })?;

            let cadence = if let Some(c) = billing_cadence.as_deref() {
                Some(BillingCadence::parse(c).ok_or_else(|| {
                    DesktopError::invalid_input(format!("unknown billing cadence: {c}"))
                })?)
            } else {
                None
            };

            let (cost_minor, currency) = parse_money_pair(cost.as_deref(), currency.as_deref())?;

            let r_at = if let Some(r) = renews_at.as_deref() {
                Some(parse_timestamp(r)?)
            } else {
                None
            };

            let e_at = if let Some(e) = expires_at.as_deref() {
                Some(parse_timestamp(e)?)
            } else {
                None
            };

            let cmd = CreateService {
                name,
                service_type: st,
                summary,
                provider,
                account_label,
                endpoint_url,
                dashboard_url,
                domain_name,
                plan,
                cost_minor,
                currency,
                billing_cadence: cadence,
                renews_at: r_at,
                expires_at: e_at,
                auto_renew,
                notes,
                project_dir,
                start_command,
                stop_command,
                tags,
                external_refs: Vec::new(),
            };

            state.with_modules(|modules| {
                let mut svc = modules.service();
                let view = svc.create_service(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "service.create".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        ServiceCommandDto::Update {
            asset_id,
            expected_revision,
            name,
            summary,
            provider,
            account_label,
            endpoint_url,
            dashboard_url,
            domain_name,
            plan,
            cost,
            currency,
            billing_cadence,
            renews_at,
            expires_at,
            auto_renew,
            notes,
            project_dir,
            start_command,
            stop_command,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            if name.is_none()
                && summary.is_leave()
                && provider.is_leave()
                && account_label.is_leave()
                && endpoint_url.is_leave()
                && dashboard_url.is_leave()
                && domain_name.is_leave()
                && plan.is_leave()
                && cost.is_leave()
                && currency.is_leave()
                && billing_cadence.is_leave()
                && renews_at.is_leave()
                && expires_at.is_leave()
                && auto_renew.is_leave()
                && notes.is_leave()
                && project_dir.is_leave()
                && start_command.is_leave()
                && stop_command.is_leave()
            {
                return state.with_modules(|modules| {
                    let view = modules.service().get_service(id)?;
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
                        operation: "service.update".into(),
                        asset_ids: vec![asset_id],
                        revision: Some(actual),
                        changed: false,
                        warnings: vec!["No-op: no fields were updated".into()],
                    })
                });
            }

            let (cost_patch, currency_patch) = parse_money_update(cost, currency)?;
            let cadence_patch = billing_cadence.normalize_text().try_map(|value| {
                BillingCadence::parse(&value).ok_or_else(|| {
                    DesktopError::invalid_input(format!("unknown billing cadence: {value}"))
                })
            })?;
            let renews_patch = renews_at
                .normalize_text()
                .try_map(|value| parse_timestamp(&value))?;
            let expires_patch = expires_at
                .normalize_text()
                .try_map(|value| parse_timestamp(&value))?;

            // Launch configuration is frozen while AssetMesh holds a live
            // process for the service: editing what a running process was
            // started with would leave the record describing something else.
            if state.service_runtime().is_active(&asset_id) {
                state.with_modules(|modules| {
                    let view = modules.service().get_service(id)?;
                    if let Some(expected) = expected_revision {
                        if view.entry.asset.revision != expected {
                            return Err(DesktopError::from(assetmesh_core::AppError::stale_revision(
                                expected, view.entry.asset.revision)));
                        }
                    }
                    let changed = |patch: &Patch<String>, current: &Option<String>| {
                        match patch.clone().normalize_text() {
                            Patch::Leave => false,
                            Patch::Clear => current.is_some(),
                            Patch::Set(value) => Some(value.trim()) != current.as_deref(),
                        }
                    };
                    if changed(&project_dir, &view.entry.record.project_dir)
                        || changed(&start_command, &view.entry.record.start_command)
                        || changed(&stop_command, &view.entry.record.stop_command) {
                        return Err(DesktopError::conflict(
                            "the service is running; stop it before editing its launch configuration"));
                    }
                    Ok(())
                })?;
            }

            let cmd = UpdateService {
                asset_id: id,
                name,
                summary: summary.normalize_text(),
                provider: provider.normalize_text(),
                account_label: account_label.normalize_text(),
                endpoint_url: endpoint_url.normalize_text(),
                dashboard_url: dashboard_url.normalize_text(),
                domain_name: domain_name.normalize_text(),
                plan: plan.normalize_text(),
                cost_minor: cost_patch,
                currency: currency_patch,
                billing_cadence: cadence_patch,
                renews_at: renews_patch,
                expires_at: expires_patch,
                auto_renew,
                notes: notes.normalize_text(),
                project_dir: project_dir.normalize_text(),
                start_command: start_command.normalize_text(),
                stop_command: stop_command.normalize_text(),
                expected_revision,
            };

            state.with_modules(|modules| {
                let mut svc = modules.service();
                let view = svc.update_service(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "service.update".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        ServiceCommandDto::RecordRenewal {
            asset_id,
            renews_at,
            cost,
            currency,
            next_renews_at,
            next_expires_at,
            expected_revision,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            let (charged_cost_minor, currency) =
                parse_money_pair(cost.as_deref(), currency.as_deref())?;

            let renewed_at_ts = parse_timestamp(&renews_at)?;

            let next_r_ts = if let Some(r) = next_renews_at.as_deref() {
                Some(parse_timestamp(r)?)
            } else {
                None
            };

            let next_e_ts = if let Some(e) = next_expires_at.as_deref() {
                Some(parse_timestamp(e)?)
            } else {
                None
            };

            let cmd = RecordRenewal {
                asset_id: id,
                renewed_at: renewed_at_ts,
                charged_cost_minor,
                currency,
                next_renews_at: next_r_ts,
                next_expires_at: next_e_ts,
                expected_revision,
            };

            state.with_modules(|modules| {
                let mut svc = modules.service();
                let view = svc.record_renewal(cmd)?;
                Ok(MutationReceiptDto {
                    operation: "service.record_renewal".into(),
                    asset_ids: vec![view.entry.asset.id.to_string()],
                    revision: Some(view.entry.asset.revision),
                    changed: true,
                    warnings: Vec::new(),
                })
            })
        }
        ServiceCommandDto::Archive {
            asset_id,
            expected_revision,
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            // Rejected archive requests must have no process side effects.
            state.with_modules(|modules| {
                let view = modules.asset().get_asset(id)?;
                view.asset.ensure_mutable()?;
                if let Some(expected) = expected_revision {
                    if view.asset.revision != expected {
                        return Err(DesktopError::from(
                            assetmesh_core::AppError::stale_revision(expected, view.asset.revision),
                        ));
                    }
                }
                Ok(())
            })?;

            // Archiving a running local service must not orphan its process:
            // stop what AssetMesh started before the record becomes
            // read-only. Externally started processes are not touched —
            // they are not ours to stop.
            state
                .service_runtime()
                .stop_blocking(&asset_id, std::time::Duration::from_secs(8))?;

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
