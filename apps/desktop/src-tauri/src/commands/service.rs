//! Services module write commands, subscription updates, and renewals (P5-06).

use std::sync::Arc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::service_service::{
    CreateService, Patch, RecordRenewal, ServiceService, UpdateService,
};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::service::{BillingCadence, ServiceType};
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
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

pub fn parse_money_to_minor(s: &str) -> Result<i64, DesktopError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(DesktopError::invalid_input("cost string is empty"));
    }
    let parts: Vec<&str> = s.split('.').collect();
    match parts.len() {
        1 => {
            let whole: i64 = parts[0]
                .parse()
                .map_err(|e| DesktopError::invalid_input(format!("invalid cost: {e}")))?;
            if whole < 0 {
                return Err(DesktopError::invalid_input("cost cannot be negative"));
            }
            whole
                .checked_mul(100)
                .ok_or_else(|| DesktopError::invalid_input("cost overflow"))
        }
        2 => {
            let whole: i64 = parts[0]
                .parse()
                .map_err(|e| DesktopError::invalid_input(format!("invalid cost: {e}")))?;
            if whole < 0 {
                return Err(DesktopError::invalid_input("cost cannot be negative"));
            }
            let dec = parts[1];
            let cents = match dec.len() {
                0 => 0,
                1 => {
                    let d = dec
                        .parse::<i64>()
                        .map_err(|e| DesktopError::invalid_input(format!("invalid cost: {e}")))?;
                    d.checked_mul(10)
                        .ok_or_else(|| DesktopError::invalid_input("cost overflow"))?
                }
                2 => dec
                    .parse::<i64>()
                    .map_err(|e| DesktopError::invalid_input(format!("invalid cost: {e}")))?,
                _ => {
                    return Err(DesktopError::invalid_input(
                        "cost cannot have more than 2 decimal places",
                    ))
                }
            };
            whole
                .checked_mul(100)
                .and_then(|w| w.checked_add(cents))
                .ok_or_else(|| DesktopError::invalid_input("cost overflow"))
        }
        _ => Err(DesktopError::invalid_input("invalid cost format")),
    }
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

            let cost_minor = match (cost.as_deref(), currency.as_deref()) {
                (None, None) => None,
                (Some(c), Some(_)) => Some(parse_money_to_minor(c)?),
                _ => {
                    return Err(DesktopError::invalid_input(
                        "cost and currency must be both present or both absent",
                    ))
                }
            };

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

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = ServiceService::new(factory.clone(), clock, id_gen);
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
                return Ok(MutationReceiptDto {
                    operation: "service.update".into(),
                    asset_ids: vec![asset_id],
                    revision: expected_revision,
                    changed: false,
                    warnings: vec!["No-op: no fields were updated".into()],
                });
            }

            let (cost_patch, currency_patch) = match (cost.as_deref(), currency.as_deref()) {
                (None, None) => (Patch::Leave, Patch::Leave),
                (Some(c), Some(cur)) if c.trim().is_empty() && cur.trim().is_empty() => {
                    (Patch::Clear, Patch::Clear)
                }
                (Some(c), Some(cur)) => {
                    let minor = parse_money_to_minor(c)?;
                    (Patch::Set(minor), Patch::Set(cur.trim().to_uppercase()))
                }
                _ => {
                    return Err(DesktopError::invalid_input(
                        "cost and currency must be set together or cleared together",
                    ))
                }
            };

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

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = ServiceService::new(factory.clone(), clock, id_gen);
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
        } => {
            let id = uuid::Uuid::parse_str(&asset_id)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

            let charged_cost_minor = match (cost.as_deref(), currency.as_deref()) {
                (None, None) => None,
                (Some(c), Some(_)) => Some(parse_money_to_minor(c)?),
                _ => {
                    return Err(DesktopError::invalid_input(
                        "cost and currency must be both present or both absent",
                    ))
                }
            };

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
                currency: currency.map(|c| c.trim().to_uppercase()),
                next_renews_at: next_r_ts,
                next_expires_at: next_e_ts,
            };

            state.with_factory(|factory| {
                let clock = Arc::new(SystemClock);
                let id_gen = Arc::new(UuidV7Generator);
                let mut svc = ServiceService::new(factory.clone(), clock, id_gen);
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
        ServiceCommandDto::Archive { asset_id } => {
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
