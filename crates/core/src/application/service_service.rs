//! Services use cases: CRUD/query for durable services and their
//! subscription lifecycle metadata (docs/10).
//!
//! A subscription is not a separate asset: commercial attributes (plan,
//! cost, currency, cadence, renewal/expiry, auto-renew) describe the Service
//! asset itself (ADR 0010). Every mutation opens ONE short transaction that
//! commits the canonical change, its activity event, and the synchronous
//! search projection together (ADR 0007). There is no provider I/O in this
//! module — discovery, health checks, and renewal automation are explicitly
//! out of scope for Phase 3.
//!
//! Money reaches this layer already parsed into integer minor units; decimal
//! parsing is an adapter concern (ADR 0010). No field of the command DTOs may
//! hold credential material: the canonical vocabulary is closed (ADR 0010).

use crate::application::projection::project_service;
use crate::application::shared::{ensure_ref_available, normalize_tags, ExternalRefInput};
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::service::{BillingCadence, ServiceEntry, ServiceRecord, ServiceType};
use crate::domain::Timestamp;
use crate::ports::repos::{ServiceFilter, ServiceListRow};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde_json::json;

/// Application-facing view of one service asset with its module details,
/// external references, tags, and recent activity.
#[derive(Debug, Clone)]
pub struct ServiceView {
    pub entry: ServiceEntry,
    pub external_refs: Vec<AssetExternalRef>,
    pub tags: Vec<String>,
    pub activity: Vec<crate::domain::activity::ActivityEvent>,
}

#[derive(Debug, Clone)]
pub struct CreateService {
    pub name: String,
    pub service_type: ServiceType,
    pub summary: Option<String>,
    pub provider: Option<String>,
    pub account_label: Option<String>,
    pub endpoint_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub domain_name: Option<String>,
    pub plan: Option<String>,
    /// Integer minor units (e.g. 1999 = USD 19.99); paired with `currency`.
    pub cost_minor: Option<i64>,
    pub currency: Option<String>,
    pub billing_cadence: Option<BillingCadence>,
    pub renews_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
    pub auto_renew: Option<bool>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub external_refs: Vec<ExternalRefInput>,
}

/// Explicit patch semantics (docs/10): an update must distinguish "leave
/// unchanged" from "clear this optional field", so an omitted CLI/UI value
/// can never be deserialized as null and erase canonical data.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Patch<T> {
    /// Leave the canonical value untouched.
    #[default]
    Leave,
    /// Replace the canonical value.
    Set(T),
    /// Remove the canonical value.
    Clear,
}

impl<T> Patch<T> {
    /// Borrows so a patch held by an `FnMut` closure (a transaction body)
    /// can be applied without moving out of a captured command.
    fn apply_to(&self, field: &mut Option<T>)
    where
        T: Clone,
    {
        match self {
            Patch::Leave => {}
            Patch::Set(value) => *field = Some(value.clone()),
            Patch::Clear => *field = None,
        }
    }
}

impl Patch<String> {
    /// CLI/UI text mapping: an absent argument leaves the field alone; an
    /// explicit empty/whitespace value clears it; anything else sets it.
    /// Normalization (trim, control-character rejection, bounds) happens in
    /// [`ServiceRecord::validate`], the single canonicalization point.
    pub fn from_text(value: Option<String>) -> Self {
        match value {
            None => Patch::Leave,
            Some(text) if text.trim().is_empty() => Patch::Clear,
            Some(text) => Patch::Set(text),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateService {
    pub asset_id: AssetId,
    /// Setting the name to an empty string is an error, never a clear.
    pub name: Option<String>,
    pub summary: Patch<String>,
    pub provider: Patch<String>,
    pub account_label: Patch<String>,
    pub endpoint_url: Patch<String>,
    pub dashboard_url: Patch<String>,
    pub domain_name: Patch<String>,
    pub plan: Patch<String>,
    /// `cost_minor` and `currency` are a pair: they must be set together or
    /// cleared together.
    pub cost_minor: Patch<i64>,
    pub currency: Patch<String>,
    pub billing_cadence: Patch<BillingCadence>,
    pub renews_at: Patch<Timestamp>,
    pub expires_at: Patch<Timestamp>,
    pub auto_renew: Patch<bool>,
    pub notes: Patch<String>,
}

pub struct ServiceService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> ServiceService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        ServiceService {
            factory,
            clock,
            ids,
        }
    }

    pub fn factory(&mut self) -> &mut F {
        &mut self.factory
    }

    pub fn get_service(&mut self, asset_id: AssetId) -> AppResult<ServiceView> {
        self.factory.read(&mut |q| build_view(q, asset_id))
    }

    pub fn list_services(&mut self, filter: &ServiceFilter) -> AppResult<Vec<ServiceListRow>> {
        self.factory.read(&mut |uow| uow.services().list(filter))
    }

    /// Creates a service asset: Asset + ServiceRecord + refs + tags + activity
    /// + search projection, committed in one short transaction.
    pub fn create_service(&mut self, cmd: CreateService) -> AppResult<ServiceView> {
        let now = self.clock.now();

        let name = cmd.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::validation("service name must not be empty"));
        }

        let asset_id = AssetId::from_uuid(self.ids.new_id());
        let mut record = ServiceRecord::new(asset_id, cmd.service_type);
        record.provider = cmd.provider;
        record.account_label = cmd.account_label;
        record.endpoint_url = cmd.endpoint_url;
        record.dashboard_url = cmd.dashboard_url;
        record.domain_name = cmd.domain_name;
        record.plan = cmd.plan;
        record.cost_minor = cmd.cost_minor;
        record.currency = cmd.currency;
        record.billing_cadence = cmd.billing_cadence;
        record.renews_at = cmd.renews_at;
        record.expires_at = cmd.expires_at;
        record.auto_renew = cmd.auto_renew;
        record.notes = cmd.notes;
        // The domain layer is the single canonicalization point: text,
        // money pairing, URL shape, and domain-specific rules are all
        // normalized/rejected here, before any write.
        record.validate()?;

        let asset = Asset::new(
            asset_id,
            cmd.service_type.asset_kind(),
            name,
            cmd.summary,
            now,
        )?;

        let refs: Vec<AssetExternalRef> = cmd
            .external_refs
            .into_iter()
            .map(|input| {
                let r = AssetExternalRef::new(
                    asset_id,
                    input.namespace,
                    input.external_id,
                    input.source_url,
                    now,
                );
                r.validate()?;
                Ok(r)
            })
            .collect::<AppResult<Vec<_>>>()?;

        let tag_names = normalize_tags(&cmd.tags);

        self.factory.transact(&mut |uow| {
            for reference in &refs {
                ensure_ref_available(uow, reference)?;
            }
            uow.assets().insert(&asset)?;
            uow.services().upsert(&record)?;

            let mut tags = Vec::new();
            for name in &tag_names {
                let tag = uow.tags().ensure(name)?;
                uow.tags().attach(asset_id, tag.id)?;
                tags.push(tag);
            }
            for reference in &refs {
                uow.external_refs().insert(reference)?;
            }

            uow.activity().append(&ActivityEvent::new(
                event_types::ASSET_CREATED,
                Some(asset_id),
                actors::USER,
                json!({ "kind": asset.kind.as_str(), "name": asset.name.clone() }),
                now,
            ))?;
            uow.activity().append(&ActivityEvent::new(
                event_types::SERVICE_CREATED,
                Some(asset_id),
                actors::USER,
                json!({
                    "service_type": record.service_type.as_str(),
                    "provider": record.provider.clone(),
                    "plan": record.plan.clone(),
                }),
                now,
            ))?;

            let document = project_service(&asset, &record, &tags, &refs);
            uow.search_index().upsert(&document)?;
            Ok(())
        })?;

        self.get_service(asset_id)
    }

    /// Applies an explicit patch to a service asset. Only the fields the
    /// caller named change; a `Clear` removes a value and a `Leave` keeps it.
    /// Metadata-only edits emit no activity event, matching the Media and
    /// Software policy (docs/10 activity).
    pub fn update_service(&mut self, cmd: UpdateService) -> AppResult<ServiceView> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = crate::application::shared::load_active_asset(uow, cmd.asset_id)?;
            let mut record = load_service_record(uow, cmd.asset_id)?;

            // A cross-module guard: the record must belong to this asset's
            // kind. The repository boundary enforces it too, but failing
            // here keeps the error message about the asset, not the row.
            if record.service_type.asset_kind() != asset.kind {
                return Err(AppError::conflict(format!(
                    "service record type {} requires asset kind {}, but asset {} has kind {}",
                    record.service_type,
                    record.service_type.asset_kind(),
                    asset.id,
                    asset.kind
                )));
            }

            if let Some(name) = cmd.name.as_deref().map(str::trim) {
                if name.is_empty() {
                    return Err(AppError::validation("service name must not be empty"));
                }
                asset.name = name.to_string();
            }
            cmd.summary.apply_to(&mut asset.summary);

            for (patch, field) in [
                (&cmd.provider, &mut record.provider),
                (&cmd.account_label, &mut record.account_label),
                (&cmd.endpoint_url, &mut record.endpoint_url),
                (&cmd.dashboard_url, &mut record.dashboard_url),
                (&cmd.domain_name, &mut record.domain_name),
                (&cmd.plan, &mut record.plan),
                (&cmd.notes, &mut record.notes),
            ] {
                patch.apply_to(field);
            }

            // Money is a pair: set together, clear together, or leave alone.
            match (&cmd.cost_minor, &cmd.currency) {
                (Patch::Leave, Patch::Leave) => {}
                (Patch::Clear, Patch::Clear) => {
                    record.cost_minor = None;
                    record.currency = None;
                }
                (Patch::Set(cost), Patch::Set(currency)) => {
                    record.cost_minor = Some(*cost);
                    record.currency = Some(currency.clone());
                }
                _ => {
                    return Err(AppError::validation(
                        "cost and currency must be set together or cleared together",
                    ));
                }
            }

            cmd.billing_cadence.apply_to(&mut record.billing_cadence);
            cmd.renews_at.apply_to(&mut record.renews_at);
            cmd.expires_at.apply_to(&mut record.expires_at);
            cmd.auto_renew.apply_to(&mut record.auto_renew);

            asset.validate()?;
            record.validate()?;
            asset.touch(now);
            uow.assets().update(&asset)?;
            uow.services().upsert(&record)?;

            update_service_projection(uow, &asset, &record)?;
            Ok(())
        })?;

        self.get_service(cmd.asset_id)
    }
}

pub(crate) fn load_service_record(
    uow: &mut dyn UnitOfWork,
    asset_id: AssetId,
) -> AppResult<ServiceRecord> {
    uow.services()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("service record", asset_id))
}

/// Rebuilds and stores the search projection for one service asset from
/// canonical state, inside the caller's transaction.
pub(crate) fn update_service_projection(
    uow: &mut dyn UnitOfWork,
    asset: &Asset,
    record: &ServiceRecord,
) -> AppResult<crate::domain::search::SearchDocument> {
    let tags = uow.tags().list_for_asset(asset.id)?;
    let refs = uow.external_refs().list_for_asset(asset.id)?;
    let document = project_service(asset, record, &tags, &refs);
    uow.search_index().upsert(&document)?;
    Ok(document)
}

pub(crate) fn build_view(
    uow: &mut dyn QueryUnitOfWork,
    asset_id: AssetId,
) -> AppResult<ServiceView> {
    let asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    let record = uow
        .services()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("service record", asset_id))?;
    let external_refs = uow.external_refs().list_for_asset(asset_id)?;
    let tags = uow
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let activity = uow.activity().list_for_asset(asset_id, 50)?;
    Ok(ServiceView {
        entry: ServiceEntry { asset, record },
        external_refs,
        tags,
        activity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_text_mapping_distinguishes_leave_set_and_clear() {
        assert!(matches!(Patch::<String>::from_text(None), Patch::Leave));
        assert!(matches!(
            Patch::<String>::from_text(Some("".into())),
            Patch::Clear
        ));
        assert!(matches!(
            Patch::<String>::from_text(Some("   ".into())),
            Patch::Clear
        ));
        match Patch::<String>::from_text(Some(" OpenAI ".into())) {
            Patch::Set(value) => assert_eq!(value, " OpenAI "),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn patch_applies_set_and_clear_only() {
        let mut field = Some("old".to_string());
        Patch::<String>::Leave.apply_to(&mut field);
        assert_eq!(field, Some("old".into()));
        Patch::Set("new".into()).apply_to(&mut field);
        assert_eq!(field, Some("new".into()));
        Patch::<String>::Clear.apply_to(&mut field);
        assert_eq!(field, None);
    }
}
