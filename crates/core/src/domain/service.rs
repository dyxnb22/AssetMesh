//! Services module domain: typed details owned by the Services module
//! (ADR 0003, docs/10).
//!
//! A `ServiceRecord` is typed detail of a shared `Asset`, not a competing
//! top-level identity. A subscription is modelled as commercial/lifecycle
//! attributes of the service it describes — there is deliberately no separate
//! Subscription asset (ADR 0010).
//!
//! The Services module carries no credential material. API keys, passwords,
//! tokens, cookies, SSH keys, and raw provider payloads never enter
//! canonical state, search, activity, or exports; the field set below is the
//! whole canonical vocabulary (ADR 0010 secret boundary).

use crate::domain::asset::AssetKind;
use crate::domain::ids::AssetId;
use crate::domain::validation::{bounded, optional_text};
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Services module data schema version (ADR 0008). Owned by the Services
/// module; independent of the database migration version and the portable
/// export format version.
pub const SCHEMA_VERSION: i64 = 1;

/// Recommended V1 text bounds (docs/10 "Text and URL invariants").
const PROVIDER_MAX: usize = 256;
const ACCOUNT_LABEL_MAX: usize = 256;
const URL_MAX: usize = 2048;
const DOMAIN_NAME_MAX: usize = 253;
const PLAN_MAX: usize = 256;
const NOTES_MAX: usize = 8192;

/// What kind of durable service an asset records. Maps 1:1 to an Asset kind:
/// a `service.saas` asset owns exactly a `SaaS` record (docs/10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    Saas,
    Api,
    Vps,
    Domain,
    Local,
}

impl ServiceType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            ServiceType::Saas => "saas",
            ServiceType::Api => "api",
            ServiceType::Vps => "vps",
            ServiceType::Domain => "domain",
            ServiceType::Local => "local",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "saas" => Some(ServiceType::Saas),
            "api" => Some(ServiceType::Api),
            "vps" => Some(ServiceType::Vps),
            "domain" => Some(ServiceType::Domain),
            "local" => Some(ServiceType::Local),
            _ => None,
        }
    }

    /// Human-oriented label used in search subtitles ("SaaS · OpenAI · Plus").
    pub const fn label(&self) -> &'static str {
        match self {
            ServiceType::Saas => "SaaS",
            ServiceType::Api => "API",
            ServiceType::Vps => "VPS",
            ServiceType::Domain => "Domain",
            ServiceType::Local => "Local",
        }
    }

    /// The asset kind that owns this service type. Kind and type must stay
    /// compatible: a `service.api` asset cannot hold a `SaaS` record.
    pub const fn asset_kind(&self) -> AssetKind {
        match self {
            ServiceType::Saas => AssetKind::ServiceSaas,
            ServiceType::Api => AssetKind::ServiceApi,
            ServiceType::Vps => AssetKind::ServiceVps,
            ServiceType::Domain => AssetKind::ServiceDomain,
            ServiceType::Local => AssetKind::ServiceLocal,
        }
    }

    pub fn from_asset_kind(kind: AssetKind) -> Option<Self> {
        match kind {
            AssetKind::ServiceSaas => Some(ServiceType::Saas),
            AssetKind::ServiceApi => Some(ServiceType::Api),
            AssetKind::ServiceVps => Some(ServiceType::Vps),
            AssetKind::ServiceDomain => Some(ServiceType::Domain),
            AssetKind::ServiceLocal => Some(ServiceType::Local),
            _ => None,
        }
    }
}

impl fmt::Display for ServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a service is billed. `usage_based` may carry no `cost_minor` when no
/// stable amount exists (docs/10 money rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingCadence {
    Monthly,
    Quarterly,
    Yearly,
    UsageBased,
    OneTime,
    Other,
}

impl BillingCadence {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BillingCadence::Monthly => "monthly",
            BillingCadence::Quarterly => "quarterly",
            BillingCadence::Yearly => "yearly",
            BillingCadence::UsageBased => "usage_based",
            BillingCadence::OneTime => "one_time",
            BillingCadence::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "monthly" | "month" => Some(BillingCadence::Monthly),
            "quarterly" | "quarter" => Some(BillingCadence::Quarterly),
            "yearly" | "year" | "annual" => Some(BillingCadence::Yearly),
            "usage_based" | "usage" => Some(BillingCadence::UsageBased),
            "one_time" | "one-time" | "once" => Some(BillingCadence::OneTime),
            "other" => Some(BillingCadence::Other),
            _ => None,
        }
    }

    /// Human-oriented label for the search body ("USD 20.00/month").
    pub const fn label(&self) -> &'static str {
        match self {
            BillingCadence::Monthly => "monthly",
            BillingCadence::Quarterly => "quarterly",
            BillingCadence::Yearly => "yearly",
            BillingCadence::UsageBased => "usage-based",
            BillingCadence::OneTime => "one-time",
            BillingCadence::Other => "other",
        }
    }
}

impl fmt::Display for BillingCadence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Services typed details owned by the Services module.
///
/// `cost_minor` is a non-negative integer in the currency's minor unit;
/// money is never floating point in canonical state (ADR 0010).
/// `renews_at` and `expires_at` are distinct caller-supplied facts — this
/// module never computes the next renewal date. `auto_renew = None` means
/// the setting is unknown, not false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceRecord {
    pub asset_id: AssetId,
    pub service_type: ServiceType,
    /// Human-readable provider name; non-secret.
    pub provider: Option<String>,
    /// Non-secret account label, e.g. "Personal" or "Work".
    pub account_label: Option<String>,
    /// Service/API endpoint; metadata, never identity and never a credential.
    pub endpoint_url: Option<String>,
    /// Non-secret management/dashboard URL.
    pub dashboard_url: Option<String>,
    /// Canonical domain text; only meaningful for a domain service.
    pub domain_name: Option<String>,
    /// Plan name, e.g. "Plus", "Pro", "Basic VPS".
    pub plan: Option<String>,
    /// Integer minor units (e.g. 1999 = USD 19.99).
    pub cost_minor: Option<i64>,
    /// Three-letter uppercase code, e.g. "USD".
    pub currency: Option<String>,
    pub billing_cadence: Option<BillingCadence>,
    /// Next expected renewal/charge boundary, when known.
    pub renews_at: Option<Timestamp>,
    /// Known end of entitlement/service if not extended.
    pub expires_at: Option<Timestamp>,
    /// User-known setting; `None` means unknown.
    pub auto_renew: Option<bool>,
    /// User-owned notes; never secrets.
    pub notes: Option<String>,
}

impl ServiceRecord {
    pub fn new(asset_id: AssetId, service_type: ServiceType) -> Self {
        ServiceRecord {
            asset_id,
            service_type,
            provider: None,
            account_label: None,
            endpoint_url: None,
            dashboard_url: None,
            domain_name: None,
            plan: None,
            cost_minor: None,
            currency: None,
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
        }
    }

    /// Record-level invariants, enforced and normalized on every write path
    /// (application services, repository upsert, future portable import).
    ///
    /// The single canonicalization point for optional text: every free-text
    /// field is trimmed (whitespace-only collapses to `None`) and control
    /// characters are rejected. Money, URLs, and the domain-specific
    /// `domain_name` rules are enforced here too. Callers pass a mutable
    /// record so the normalized values are what gets stored.
    pub fn validate(&mut self) -> AppResult<()> {
        for (value, name, max) in [
            (&mut self.provider, "provider", PROVIDER_MAX),
            (&mut self.account_label, "account_label", ACCOUNT_LABEL_MAX),
            (&mut self.endpoint_url, "endpoint_url", URL_MAX),
            (&mut self.dashboard_url, "dashboard_url", URL_MAX),
            (&mut self.plan, "plan", PLAN_MAX),
            (&mut self.notes, "notes", NOTES_MAX),
        ] {
            *value = optional_text(value, name)?;
            if let Some(text) = value.as_deref() {
                bounded(text, max, name)?;
            }
        }
        if let Some(url) = self.endpoint_url.as_deref() {
            validate_url_shape(url)?;
        }
        if let Some(url) = self.dashboard_url.as_deref() {
            validate_url_shape(url)?;
        }

        // `domain_name` is canonical domain metadata: it belongs to a domain
        // service only, is stored in its normalized comparison form, and is
        // never an Asset ID (docs/10).
        let domain = optional_text(&self.domain_name, "domain_name")?;
        self.domain_name = match domain {
            None => None,
            Some(_) if self.service_type != ServiceType::Domain => {
                return Err(AppError::validation(format!(
                    "domain_name is canonical metadata of a domain service and cannot be set on \
                     a {} service",
                    self.service_type
                )));
            }
            Some(value) => {
                let normalized = value.to_ascii_lowercase();
                bounded(&normalized, DOMAIN_NAME_MAX, "domain_name")?;
                validate_domain_name_shape(&normalized)?;
                Some(normalized)
            }
        };

        // Money: integer minor units, never floating point. `cost_minor` and
        // `currency` are paired — both present or both absent (ADR 0010).
        if let Some(cost) = self.cost_minor {
            if cost < 0 {
                return Err(AppError::validation(
                    "cost_minor must be a non-negative integer in minor units",
                ));
            }
        }
        match (&self.cost_minor, &self.currency) {
            (Some(_), None) | (None, Some(_)) => {
                return Err(AppError::validation(
                    "cost_minor and currency must be both present or both absent",
                ));
            }
            (Some(_), Some(raw)) => self.currency = Some(normalize_currency(raw)?),
            (None, None) => {}
        }

        Ok(())
    }
}

/// An asset together with its service details — the joined view the Service
/// repository returns for list/detail queries.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceEntry {
    pub asset: crate::domain::asset::Asset,
    pub record: ServiceRecord,
}

/// Requires a syntactically reasonable absolute http/https URL. This is not
/// an RFC parser and performs no network I/O (docs/10); it exists to keep
/// obvious garbage and credential-bearing URLs out of canonical state.
fn validate_url_shape(url: &str) -> AppResult<()> {
    // Whitespace or control characters ANYWHERE make the URL malformed — not
    // just in the host (`https://example.com/a b` is no more resolvable than
    // `https://exa mple.com`, and an embedded newline would survive into
    // exported JSON).
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::validation(
            "URL must not contain whitespace or control characters",
        ));
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| AppError::validation("URL must be an absolute http:// or https:// URL"))?;
    // Authority ends at the first path/query/fragment separator.
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return Err(AppError::validation("URL must have a host"));
    }
    // Credentials must not enter canonical data. A userinfo segment is
    // rejected outright rather than parsed out and stored somewhere: the
    // Services module has no field that may hold it (ADR 0010). Checked BEFORE
    // the host/port split, because a userinfo segment contains a ':' that
    // would otherwise be misread as a port separator.
    if authority.contains('@') {
        return Err(AppError::validation(
            "URL must not contain credentials; remove the user info segment",
        ));
    }
    // Split the host from the port. A bracketed authority is an IPv6 literal
    // and must be closed before any port; anything else after the host is a
    // malformed port.
    let (host, port) = if let Some(after_bracket) = authority.strip_prefix('[') {
        let close = after_bracket
            .find(']')
            .ok_or_else(|| AppError::validation("URL has an unterminated IPv6 host literal"))?;
        let host = &after_bracket[..close];
        let after = &after_bracket[close + ']'.len_utf8()..];
        let port = after.strip_prefix(':');
        if port.is_none() && !after.is_empty() {
            return Err(AppError::validation(
                "URL has unexpected characters after the IPv6 host literal",
            ));
        }
        (host, port)
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return Err(AppError::validation("URL must have a host"));
    }
    // A bracketed host is a real IPv6 literal — parse it rather than eyeballing
    // the character set, so `[1:::2]` and `[::::]` are rejected the same way
    // `[x]` is. `Ipv6Addr::from_str` accepts a zone id (`%eth0`) which URLs do
    // not, so reject one explicitly.
    if authority.starts_with('[') {
        if host.contains('%') || host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(AppError::validation(
                "URL has a malformed IPv6 host literal",
            ));
        }
    } else {
        // A hostname is dot-separated labels: non-empty, ASCII
        // letters/digits/hyphens, and no label may start or end with a
        // hyphen (`https://-bad.com`) or be empty (`https://example..com`).
        for label in host.split('.') {
            if label.is_empty()
                || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                || label.starts_with('-')
                || label.ends_with('-')
            {
                return Err(AppError::validation(format!(
                    "URL host {host:?} is not a valid host name"
                )));
            }
        }
    }
    // A port must be a decimal number in range.
    if let Some(port) = port {
        let valid = !port.is_empty()
            && port.len() <= 5
            && port.chars().all(|c| c.is_ascii_digit())
            && port.parse::<u16>().is_ok();
        if !valid {
            return Err(AppError::validation(
                "URL port must be a decimal number between 0 and 65535",
            ));
        }
    }
    Ok(())
}

/// Conservative host shape for the canonical domain text: ASCII letters,
/// digits, hyphens, and dots, with non-empty labels. No DNS resolution or
/// ownership facts are claimed (docs/10).
fn validate_domain_name_shape(value: &str) -> AppResult<()> {
    let malformed = || {
        AppError::validation(format!(
            "domain_name must be a host name of dot-separated labels (letters, digits, '-'): \
             {value:?}"
        ))
    };
    if value.starts_with('.') || value.starts_with('-') || value.ends_with('-') {
        return Err(malformed());
    }
    for label in value.split('.') {
        if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(malformed());
        }
    }
    Ok(())
}

/// Normalizes a currency code to trimmed uppercase ASCII and requires
/// exactly three alphabetic characters (docs/10 money rules).
pub fn normalize_currency(value: &str) -> AppResult<String> {
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.len() != 3 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::validation(format!(
            "currency must be exactly three alphabetic characters, got {value:?}"
        )));
    }
    Ok(normalized)
}

/// Formats an integer-minor-unit amount for display ("USD 19.99"). Display
/// only: the canonical value stays an integer, so this never introduces
/// floating-point drift (ADR 0010).
pub fn format_money(cost_minor: i64, currency: &str) -> String {
    format!("{} {}.{:02}", currency, cost_minor / 100, cost_minor % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_type_parsing_and_kind_mapping() {
        assert_eq!(ServiceType::parse("SaaS"), Some(ServiceType::Saas));
        assert_eq!(ServiceType::parse("api"), Some(ServiceType::Api));
        assert_eq!(ServiceType::parse("VPS"), Some(ServiceType::Vps));
        assert_eq!(ServiceType::parse("domain"), Some(ServiceType::Domain));
        assert_eq!(ServiceType::parse("local"), Some(ServiceType::Local));
        assert_eq!(ServiceType::parse("nonsense"), None);
        assert_eq!(ServiceType::Saas.asset_kind(), AssetKind::ServiceSaas);
        assert_eq!(
            ServiceType::from_asset_kind(AssetKind::ServiceDomain),
            Some(ServiceType::Domain)
        );
        assert_eq!(ServiceType::from_asset_kind(AssetKind::MediaGame), None);
        assert_eq!(ServiceType::Domain.label(), "Domain");
    }

    #[test]
    fn billing_cadence_parsing_and_labels() {
        assert_eq!(
            BillingCadence::parse("monthly"),
            Some(BillingCadence::Monthly)
        );
        assert_eq!(
            BillingCadence::parse("YEARLY"),
            Some(BillingCadence::Yearly)
        );
        assert_eq!(
            BillingCadence::parse("usage_based"),
            Some(BillingCadence::UsageBased)
        );
        assert_eq!(
            BillingCadence::parse("one-time"),
            Some(BillingCadence::OneTime)
        );
        assert_eq!(BillingCadence::parse("nonsense"), None);
        assert_eq!(BillingCadence::Monthly.label(), "monthly");
    }

    fn record(service_type: ServiceType) -> ServiceRecord {
        ServiceRecord::new(AssetId::generate(), service_type)
    }

    #[test]
    fn validation_normalizes_text_in_place() {
        let mut r = record(ServiceType::Saas);
        r.provider = Some("  OpenAI  ".into());
        r.account_label = Some("   ".into());
        r.notes = Some("line\nbreak".into());
        // A newline is a control character and must be rejected.
        assert!(r.validate().is_err());

        let mut r = record(ServiceType::Saas);
        r.provider = Some("  OpenAI  ".into());
        r.account_label = Some("  Personal  ".into());
        r.notes = Some("  team plan  ".into());
        assert!(r.validate().is_ok());
        assert_eq!(r.provider.as_deref(), Some("OpenAI"));
        assert_eq!(r.account_label.as_deref(), Some("Personal"));
        assert_eq!(r.notes.as_deref(), Some("team plan"));
    }

    #[test]
    fn validation_rejects_overlong_fields() {
        let mut r = record(ServiceType::Saas);
        r.provider = Some("x".repeat(PROVIDER_MAX + 1));
        assert!(r.validate().is_err());

        let mut r = record(ServiceType::Saas);
        r.notes = Some("x".repeat(NOTES_MAX + 1));
        assert!(r.validate().is_err());
    }

    #[test]
    fn validation_rejects_money_misuse() {
        // Negative cost.
        let mut r = record(ServiceType::Vps);
        r.cost_minor = Some(-1);
        r.currency = Some("usd".into());
        assert!(r.validate().is_err());

        // Cost without currency.
        let mut r = record(ServiceType::Vps);
        r.cost_minor = Some(1999);
        assert!(r.validate().is_err());

        // Currency without cost.
        let mut r = record(ServiceType::Vps);
        r.currency = Some("USD".into());
        assert!(r.validate().is_err());

        // Malformed currency.
        let mut r = record(ServiceType::Vps);
        r.cost_minor = Some(1999);
        r.currency = Some("DOLLAR".into());
        assert!(r.validate().is_err());

        let mut r = record(ServiceType::Vps);
        r.cost_minor = Some(1999);
        r.currency = Some("usd".into());
        assert!(r.validate().is_ok());
        // Normalized to uppercase ASCII.
        assert_eq!(r.currency.as_deref(), Some("USD"));

        // usage_based billing legitimately has no cost.
        let mut r = record(ServiceType::Api);
        r.billing_cadence = Some(BillingCadence::UsageBased);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn validation_checks_url_shape_without_network_io() {
        let mut r = record(ServiceType::Saas);
        r.endpoint_url = Some("https://api.openai.com/v1".into());
        r.dashboard_url = Some("http://localhost:8080/".into());
        assert!(r.validate().is_ok());

        let mut r = record(ServiceType::Saas);
        r.endpoint_url = Some("ftp://example.com".into());
        assert!(r.validate().is_err());

        let mut r = record(ServiceType::Saas);
        r.endpoint_url = Some("https://".into());
        assert!(r.validate().is_err());

        // Credentials in a URL are rejected, never parsed into canonical
        // state (ADR 0010).
        let mut r = record(ServiceType::Saas);
        r.endpoint_url = Some("https://user:pass@example.com".into());
        assert!(r.validate().is_err());

        // An '@' inside the path is not userinfo and must survive.
        let mut r = record(ServiceType::Saas);
        r.endpoint_url = Some("https://example.com/team@org".into());
        assert!(r.validate().is_ok());
    }

    #[test]
    fn domain_name_is_domain_service_metadata_only() {
        // Canonical, normalized form on a domain service.
        let mut r = record(ServiceType::Domain);
        r.domain_name = Some("AssetMesh.Dev".into());
        assert!(r.validate().is_ok());
        assert_eq!(r.domain_name.as_deref(), Some("assetmesh.dev"));

        // The same field on a SaaS service is canonical-metadata misuse.
        let mut r = record(ServiceType::Saas);
        r.domain_name = Some("assetmesh.dev".into());
        assert!(r.validate().is_err());

        let mut r = record(ServiceType::Domain);
        r.domain_name = Some("not a host".into());
        assert!(r.validate().is_err());

        let mut r = record(ServiceType::Domain);
        r.domain_name = Some(".leading".into());
        assert!(r.validate().is_err());
    }

    #[test]
    fn money_formats_from_integer_minor_units() {
        assert_eq!(format_money(1999, "USD"), "USD 19.99");
        assert_eq!(format_money(500, "USD"), "USD 5.00");
        assert_eq!(format_money(5, "USD"), "USD 0.05");
        assert_eq!(format_money(0, "HKD"), "HKD 0.00");
    }

    #[test]
    fn currency_normalization_is_strict() {
        assert_eq!(normalize_currency(" usd ").unwrap(), "USD");
        assert!(normalize_currency("eu").is_err());
        assert!(normalize_currency("euro").is_err());
        assert!(normalize_currency("US1").is_err());
    }

    #[test]
    fn record_carries_no_credential_fields() {
        // The canonical DTO vocabulary is exactly the non-secret field set
        // (ADR 0010). A serialized record must not expose any key that could
        // carry a credential, so a future field addition is caught by name.
        let serialized = serde_json::to_value(record(ServiceType::Saas)).unwrap();
        for key in serialized.as_object().unwrap().keys() {
            assert!(
                ![
                    "password",
                    "token",
                    "secret",
                    "credential",
                    "cookie",
                    "ssh_key",
                    "api_key"
                ]
                .iter()
                .any(|forbidden| key.contains(forbidden)),
                "canonical ServiceRecord exposes a secret-shaped field: {key}"
            );
        }
    }
}
