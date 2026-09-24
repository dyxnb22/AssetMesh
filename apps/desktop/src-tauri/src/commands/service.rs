//! Services module write commands, subscription updates, and renewals (P5-06).

use assetmesh_core::application::service_service::{
    parse_money_pair, parse_money_patch, CreateService, Patch, RecordRenewal, UpdateService,
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
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            if name.is_none()
                && summary.is_none()
                && provider.is_none()
                && account_label.is_none()
                && endpoint_url.is_none()
                && dashboard_url.is_none()
                && domain_name.is_none()
                && plan.is_none()
                && cost.is_none()
                && currency.is_none()
                && billing_cadence.is_none()
                && renews_at.is_none()
                && expires_at.is_none()
                && auto_renew.is_none()
                && notes.is_none()
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

            let (cost_patch, currency_patch) =
                parse_money_patch(cost.as_deref(), currency.as_deref())?;

            let cadence_patch = match billing_cadence.as_deref() {
                None => Patch::Leave,
                Some(c) if c.trim().is_empty() => Patch::Clear,
                Some(c) => {
                    let parsed = BillingCadence::parse(c).ok_or_else(|| {
                        DesktopError::invalid_input(format!("unknown billing cadence: {c}"))
                    })?;
                    Patch::Set(parsed)
                }
            };

            let renews_patch = match renews_at.as_deref() {
                None => Patch::Leave,
                Some(r) if r.trim().is_empty() => Patch::Clear,
                Some(r) => Patch::Set(parse_timestamp(r)?),
            };

            let expires_patch = match expires_at.as_deref() {
                None => Patch::Leave,
                Some(e) if e.trim().is_empty() => Patch::Clear,
                Some(e) => Patch::Set(parse_timestamp(e)?),
            };

            let auto_renew_patch = match auto_renew {
                None => Patch::Leave,
                Some(val) => Patch::Set(val),
            };

            let cmd = UpdateService {
                asset_id: id,
                name,
                summary: Patch::from_text(summary),
                provider: Patch::from_text(provider),
                account_label: Patch::from_text(account_label),
                endpoint_url: Patch::from_text(endpoint_url),
                dashboard_url: Patch::from_text(dashboard_url),
                domain_name: Patch::from_text(domain_name),
                plan: Patch::from_text(plan),
                cost_minor: cost_patch,
                currency: currency_patch,
                billing_cadence: cadence_patch,
                renews_at: renews_patch,
                expires_at: expires_patch,
                auto_renew: auto_renew_patch,
                notes: Patch::from_text(notes),
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
