use crate::format::{fmt_time, print_search_hits, print_service_detail, print_service_list};
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::{
    parse_money_pair, parse_money_patch, CreateService, Patch, RecordRenewal, ServiceService,
    UpdateService,
};
use assetmesh_core::domain::service::{BillingCadence, ServiceType};
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::repos::{ServiceFilter, ServiceSort};
use assetmesh_core::{AppError, AppResult, SharedClock, SharedIdGenerator};
use clap::{Subcommand, ValueEnum};

use crate::commands::asset::{parse_ref_input, resolve_asset_id};
use crate::commands::SharedFactory;

#[derive(Subcommand)]
pub(crate) enum ServiceCommand {
    /// Add a service record manually.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long = "type", value_enum)]
        service_type: CliServiceType,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        account_label: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        dashboard: Option<String>,
        /// Canonical domain text (domain services only).
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        plan: Option<String>,
        /// Decimal amount, e.g. 19.99 (two decimal places; parsed into
        /// integer minor units). Requires --currency.
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long, value_enum)]
        billing: Option<CliBillingCadence>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        renews_at: Option<String>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        expires_at: Option<String>,
        /// Auto-renew is known to be on. Use --no-auto-renew for off.
        #[arg(long, conflicts_with = "no_auto_renew")]
        auto_renew: Option<bool>,
        #[arg(long, conflicts_with = "auto_renew")]
        no_auto_renew: Option<bool>,
        #[arg(long)]
        notes: Option<String>,
        /// Project directory a local service is started from (local services
        /// only; the desktop runtime starts it from here).
        #[arg(long)]
        project_dir: Option<String>,
        /// The command saved and run when the service is started (local
        /// services only).
        #[arg(long)]
        start_command: Option<String>,
        #[arg(long)]
        stop_command: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        /// External reference as `namespace:external_id` (repeatable).
        #[arg(long = "ref")]
        refs: Vec<String>,
    },
    /// Show one service record with details and activity.
    Get { id: String },
    /// List service records with typed filters.
    List {
        #[arg(long = "type", value_enum)]
        service_type: Option<CliServiceType>,
        /// Substring match on the provider name.
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, value_enum, default_value_t = CliServiceSort::Updated)]
        sort: CliServiceSort,
        #[arg(long)]
        json: bool,
    },
    /// Update service metadata. Omitted fields are left unchanged; an empty
    /// text value clears the field.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        account_label: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        dashboard: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        plan: Option<String>,
        /// Decimal amount (with --currency); use --clear-cost to remove both.
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long, value_enum)]
        billing: Option<CliBillingCadence>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        renews_at: Option<String>,
        /// Date (YYYY-MM-DD) or RFC 3339 timestamp.
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long, conflicts_with = "no_auto_renew")]
        auto_renew: Option<bool>,
        #[arg(long, conflicts_with = "auto_renew")]
        no_auto_renew: Option<bool>,
        #[arg(long)]
        notes: Option<String>,
        /// Set the local launch directory (local services only); an empty
        /// value clears the field.
        #[arg(long)]
        project_dir: Option<String>,
        /// Set the start command (local services only); an empty value clears
        /// the field.
        #[arg(long)]
        start_command: Option<String>,
        #[arg(long)]
        stop_command: Option<String>,
        /// Remove cost and currency together.
        #[arg(long)]
        clear_cost: bool,
        /// Remove the billing cadence.
        #[arg(long)]
        clear_billing: bool,
        /// Remove the renewal date.
        #[arg(long)]
        clear_renews_at: bool,
        /// Remove the expiry date.
        #[arg(long)]
        clear_expires_at: bool,
        /// Set auto-renew back to unknown.
        #[arg(long, conflicts_with = "auto_renew", conflicts_with = "no_auto_renew")]
        clear_auto_renew: bool,
    },
    /// Full-text search over the projection.
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Record that a subscription renewed. The next renewal/expiry boundary is
    /// whatever you pass — AssetMesh never computes one.
    Renew {
        id: String,
        /// When the renewal happened (YYYY-MM-DD or RFC 3339). Required: a
        /// renewal without a renewal moment is a caller error (docs/10).
        #[arg(long)]
        renewed_at: String,
        /// Decimal amount charged (with --currency); updates the canonical cost.
        #[arg(long)]
        cost: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        /// Next renewal boundary, when known.
        #[arg(long)]
        next_renewal: Option<String>,
        /// Next expiry boundary, when known.
        #[arg(long)]
        next_expiry: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliServiceType {
    Saas,
    Api,
    Vps,
    Domain,
    Local,
}

impl From<CliServiceType> for ServiceType {
    fn from(value: CliServiceType) -> Self {
        match value {
            CliServiceType::Saas => ServiceType::Saas,
            CliServiceType::Api => ServiceType::Api,
            CliServiceType::Vps => ServiceType::Vps,
            CliServiceType::Domain => ServiceType::Domain,
            CliServiceType::Local => ServiceType::Local,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliBillingCadence {
    Monthly,
    Quarterly,
    Yearly,
    UsageBased,
    OneTime,
    Other,
}

impl From<CliBillingCadence> for BillingCadence {
    fn from(value: CliBillingCadence) -> Self {
        match value {
            CliBillingCadence::Monthly => BillingCadence::Monthly,
            CliBillingCadence::Quarterly => BillingCadence::Quarterly,
            CliBillingCadence::Yearly => BillingCadence::Yearly,
            CliBillingCadence::UsageBased => BillingCadence::UsageBased,
            CliBillingCadence::OneTime => BillingCadence::OneTime,
            CliBillingCadence::Other => BillingCadence::Other,
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum CliServiceSort {
    Updated,
    Title,
    Renews,
}

impl From<CliServiceSort> for ServiceSort {
    fn from(value: CliServiceSort) -> Self {
        match value {
            CliServiceSort::Updated => ServiceSort::UpdatedDesc,
            CliServiceSort::Title => ServiceSort::TitleAsc,
            CliServiceSort::Renews => ServiceSort::RenewsAsc,
        }
    }
}

pub(crate) fn run_service(
    factory: SharedFactory,
    clock: SharedClock,
    ids: SharedIdGenerator,
    cmd: ServiceCommand,
) -> Result<(), AppError> {
    let mut services = ServiceService::new(factory.clone(), clock.clone(), ids.clone());

    match cmd {
        ServiceCommand::Add {
            name,
            service_type,
            summary,
            provider,
            account_label,
            endpoint,
            dashboard,
            domain,
            plan,
            cost,
            currency,
            billing,
            renews_at,
            expires_at,
            auto_renew,
            no_auto_renew,
            notes,
            project_dir,
            start_command,
            stop_command,
            tag,
            refs,
        } => {
            let external_refs = refs
                .iter()
                .map(|raw| parse_ref_input(raw))
                .collect::<Result<Vec<_>, AppError>>()?;
            let (cost_minor, currency) = parse_money_pair(cost.as_deref(), currency.as_deref())?;
            let view = services.create_service(CreateService {
                name,
                service_type: service_type.into(),
                summary,
                provider,
                account_label,
                endpoint_url: endpoint,
                dashboard_url: dashboard,
                domain_name: domain,
                plan,
                cost_minor,
                currency,
                billing_cadence: billing.map(Into::into),
                renews_at: renews_at
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                expires_at: expires_at
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                auto_renew: parse_auto_renew(auto_renew, no_auto_renew),
                notes,
                project_dir,
                start_command,
                stop_command,
                tags: tag,
                external_refs,
            })?;
            println!("created {}", view.entry.asset.id);
            print_service_detail(&view);
        }
        ServiceCommand::Get { id } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let view = services.get_service(asset_id)?;
            print_service_detail(&view);
        }
        ServiceCommand::List {
            service_type,
            provider,
            tag,
            sort,
            json,
        } => {
            let filter = ServiceFilter {
                service_type: service_type.map(Into::into),
                provider,
                tag,
                sort: sort.into(),
            };
            let rows = services.list_services(&filter)?;
            print_service_list(&rows, json);
        }
        ServiceCommand::Update {
            id,
            name,
            summary,
            provider,
            account_label,
            endpoint,
            dashboard,
            domain,
            plan,
            cost,
            currency,
            billing,
            renews_at,
            expires_at,
            auto_renew,
            no_auto_renew,
            notes,
            project_dir,
            start_command,
            stop_command,
            clear_cost,
            clear_billing,
            clear_renews_at,
            clear_expires_at,
            clear_auto_renew,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let (cost_minor, currency) = if clear_cost {
                (Patch::<i64>::Clear, Patch::<String>::Clear)
            } else {
                parse_money_patch(cost.as_deref(), currency.as_deref())?
            };
            let view = services.update_service(UpdateService {
                asset_id,
                name,
                summary: Patch::from_text(summary),
                provider: Patch::from_text(provider),
                account_label: Patch::from_text(account_label),
                endpoint_url: Patch::from_text(endpoint),
                dashboard_url: Patch::from_text(dashboard),
                domain_name: Patch::from_text(domain),
                plan: Patch::from_text(plan),
                cost_minor,
                currency,
                billing_cadence: match billing {
                    Some(cadence) => Patch::Set(cadence.into()),
                    None if clear_billing => Patch::Clear,
                    None => Patch::Leave,
                },
                renews_at: parse_date_patch(renews_at.as_deref(), clear_renews_at)?,
                expires_at: parse_date_patch(expires_at.as_deref(), clear_expires_at)?,
                auto_renew: {
                    if clear_auto_renew {
                        Patch::Clear
                    } else if auto_renew.unwrap_or(false) {
                        Patch::Set(true)
                    } else if no_auto_renew.unwrap_or(false) {
                        Patch::Set(false)
                    } else {
                        Patch::Leave
                    }
                },
                notes: Patch::from_text(notes),
                project_dir: Patch::from_text(project_dir),
                start_command: Patch::from_text(start_command),
                stop_command: Patch::from_text(stop_command),
                ..Default::default()
            })?;
            println!("updated {}", view.entry.asset.id);
        }
        ServiceCommand::Search { query, limit } => {
            let mut search = SearchService::new(factory);
            let hits = search.search(&query, limit)?;
            print_search_hits(&hits);
        }
        ServiceCommand::Renew {
            id,
            renewed_at,
            cost,
            currency,
            next_renewal,
            next_expiry,
        } => {
            let asset_id = resolve_asset_id(&factory, &id)?;
            let current = services.get_service(asset_id)?;
            let (cost_minor, currency) = parse_money_pair(cost.as_deref(), currency.as_deref())?;
            // The renewal moment is required by the argument parser, never
            // invented here: a renewal the caller cannot date is not a fact
            // AssetMesh may timestamp on their behalf (docs/10).
            let renewed_at = parse_service_timestamp(&renewed_at)?;
            let view = services.record_renewal(RecordRenewal {
                asset_id,
                renewed_at,
                charged_cost_minor: cost_minor,
                currency,
                next_renews_at: next_renewal
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                next_expires_at: next_expiry
                    .as_deref()
                    .map(parse_service_timestamp)
                    .transpose()?,
                expected_revision: Some(current.entry.asset.revision),
            })?;
            let record = &view.entry.record;
            println!("renewed {}", view.entry.asset.id);
            println!("Renews:        {}", fmt_time(record.renews_at));
            println!("Expires:       {}", fmt_time(record.expires_at));
        }
    }
    Ok(())
}

/// Maps the auto-renew flag pair to a known setting: `--auto-renew` → on,
/// `--no-auto-renew` → off, neither → unknown (`None`).
pub(crate) fn parse_auto_renew(on: Option<bool>, off: Option<bool>) -> Option<bool> {
    if on.unwrap_or(false) {
        Some(true)
    } else if off.unwrap_or(false) {
        Some(false)
    } else {
        None
    }
}

/// Maps an optional date argument plus its clear flag to an explicit patch.
pub(crate) fn parse_date_patch(raw: Option<&str>, clear: bool) -> AppResult<Patch<Timestamp>> {
    match raw {
        Some(value) => Ok(Patch::Set(parse_service_timestamp(value)?)),
        None if clear => Ok(Patch::Clear),
        None => Ok(Patch::Leave),
    }
}

/// Parses `YYYY-MM-DD` (as UTC midnight) or an RFC 3339 timestamp. Renewal
/// and expiry boundaries are caller-supplied facts — nothing is computed.
pub(crate) fn parse_service_timestamp(raw: &str) -> AppResult<Timestamp> {
    let raw = raw.trim();
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(ts.with_timezone(&chrono::Utc));
    }
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
        AppError::validation(format!(
            "expected a date (YYYY-MM-DD) or RFC 3339 timestamp, got {raw:?}"
        ))
    })?;
    let naive = date.and_hms_opt(0, 0, 0).ok_or_else(|| {
        AppError::validation(format!("could not build a timestamp from date {raw:?}"))
    })?;
    Ok(naive.and_utc())
}
