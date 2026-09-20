//! Media and Software -> SearchDocument projections (ADR 0006).
//!
//! Modules own the transformation from their typed details into the shared
//! search representation. These projectors are pure and used by both
//! synchronous updates and full rebuilds.

use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::media::MediaRecord;
use crate::domain::search::SearchDocument;
use crate::domain::service::{format_money, BillingCadence, ServiceRecord};
use crate::domain::software::SoftwareRecord;
use crate::domain::tag::Tag;

/// Projects one media asset into a `SearchDocument`.
///
/// - title: canonical asset name
/// - subtitle: "Anime · 2023"
/// - body: platform + notes
/// - keywords: tags and external aliases (`steam:1091500`)
pub fn project_media(
    asset: &Asset,
    record: &MediaRecord,
    tags: &[Tag],
    refs: &[AssetExternalRef],
) -> SearchDocument {
    let subtitle = match record.year {
        Some(year) => Some(format!("{} · {}", record.media_type.label(), year)),
        None => Some(record.media_type.label().to_string()),
    };

    let mut body_parts: Vec<String> = Vec::new();
    if let Some(platform) = record.platform.as_deref().map(str::trim) {
        if !platform.is_empty() {
            body_parts.push(platform.to_string());
        }
    }
    if let Some(notes) = record.notes.as_deref().map(str::trim) {
        if !notes.is_empty() {
            body_parts.push(notes.to_string());
        }
    }

    let mut keywords: Vec<String> = Vec::new();
    for tag in tags {
        let name = tag.name.trim();
        if !name.is_empty() && !keywords.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            keywords.push(name.to_string());
        }
    }
    for reference in refs {
        let keyword = format!("{}:{}", reference.namespace, reference.external_id);
        if !keywords.contains(&keyword) {
            keywords.push(keyword);
        }
    }

    SearchDocument {
        asset_id: asset.id,
        kind: asset.kind.as_str().to_string(),
        title: asset.name.clone(),
        subtitle,
        body: if body_parts.is_empty() {
            None
        } else {
            Some(body_parts.join(" — "))
        },
        keywords,
        updated_at: asset.updated_at,
    }
}

/// Projects one software asset into a `SearchDocument` (ADR 0006 software
/// example).
///
/// - title: canonical asset name
/// - subtitle: "CLI Tool · 14.1.0" (category label, plus version when known)
/// - body: purpose + install source + install location + notes
/// - keywords: tags and external aliases (`homebrew_cask:visual-studio-code`)
pub fn project_software(
    asset: &Asset,
    record: &SoftwareRecord,
    tags: &[Tag],
    refs: &[AssetExternalRef],
) -> SearchDocument {
    let subtitle = match &record.version {
        Some(version) if !version.trim().is_empty() => {
            Some(format!("{} · {}", record.category.label(), version.trim()))
        }
        _ => Some(record.category.label().to_string()),
    };

    let mut body_parts: Vec<String> = Vec::new();
    if let Some(purpose) = record.purpose.as_deref().map(str::trim) {
        if !purpose.is_empty() {
            body_parts.push(purpose.to_string());
        }
    }
    body_parts.push(record.install_source.label().to_string());
    if let Some(location) = record.install_location.as_deref().map(str::trim) {
        if !location.is_empty() {
            body_parts.push(location.to_string());
        }
    }
    if let Some(notes) = record.notes.as_deref().map(str::trim) {
        if !notes.is_empty() {
            body_parts.push(notes.to_string());
        }
    }

    let mut keywords: Vec<String> = Vec::new();
    for tag in tags {
        let name = tag.name.trim();
        if !name.is_empty() && !keywords.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            keywords.push(name.to_string());
        }
    }
    for reference in refs {
        let keyword = format!("{}:{}", reference.namespace, reference.external_id);
        if !keywords.contains(&keyword) {
            keywords.push(keyword);
        }
    }

    SearchDocument {
        asset_id: asset.id,
        kind: asset.kind.as_str().to_string(),
        title: asset.name.clone(),
        subtitle,
        body: Some(body_parts.join(" — ")),
        keywords,
        updated_at: asset.updated_at,
    }
}

/// Projects one service asset into a `SearchDocument` (docs/10).
///
/// - title: canonical asset name
/// - subtitle: "SaaS · OpenAI · Plus" (type label + provider + plan; only
///   fields that have values, in a stable order)
/// - body: account label + endpoint/domain metadata + billing summary +
///   renewal/expiry + notes
/// - keywords: tags, external aliases, provider, and domain name
///
/// Nothing derived from a provider payload is indexed, and no credential
/// field exists in the record to leak: the projection is built entirely from
/// the non-secret canonical vocabulary (ADR 0010).
pub fn project_service(
    asset: &Asset,
    record: &ServiceRecord,
    tags: &[Tag],
    refs: &[AssetExternalRef],
) -> SearchDocument {
    let mut subtitle_parts: Vec<String> = vec![record.service_type.label().to_string()];
    if let Some(provider) = record.provider.as_deref().map(str::trim) {
        if !provider.is_empty() {
            subtitle_parts.push(provider.to_string());
        }
    }
    if let Some(plan) = record.plan.as_deref().map(str::trim) {
        if !plan.is_empty() {
            subtitle_parts.push(plan.to_string());
        }
    }

    let mut body_parts: Vec<String> = Vec::new();
    match (record.cost_minor, record.currency.as_deref()) {
        (Some(cost), Some(currency)) => {
            let mut summary = format_money(cost, currency);
            if let Some(cadence) = record.billing_cadence {
                summary.push('/');
                summary.push_str(cadence.label());
            }
            body_parts.push(summary);
        }
        (None, None) if matches!(record.billing_cadence, Some(BillingCadence::UsageBased)) => {
            body_parts.push("usage-based".to_string());
        }
        _ => {}
    }
    if let Some(renews_at) = record.renews_at {
        body_parts.push(format!("renews {}", renews_at.format("%Y-%m-%d")));
    }
    if let Some(expires_at) = record.expires_at {
        body_parts.push(format!("expires {}", expires_at.format("%Y-%m-%d")));
    }
    if let Some(label) = record.account_label.as_deref().map(str::trim) {
        if !label.is_empty() {
            body_parts.push(label.to_string());
        }
    }
    if let Some(domain) = record.domain_name.as_deref().map(str::trim) {
        if !domain.is_empty() {
            body_parts.push(domain.to_string());
        }
    } else if let Some(url) = record.endpoint_url.as_deref().map(str::trim) {
        if !url.is_empty() {
            body_parts.push(url.to_string());
        }
    }
    if let Some(notes) = record.notes.as_deref().map(str::trim) {
        if !notes.is_empty() {
            body_parts.push(notes.to_string());
        }
    }

    let mut keywords: Vec<String> = Vec::new();
    for tag in tags {
        let name = tag.name.trim();
        if !name.is_empty() && !keywords.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            keywords.push(name.to_string());
        }
    }
    for reference in refs {
        let keyword = format!("{}:{}", reference.namespace, reference.external_id);
        if !keywords.contains(&keyword) {
            keywords.push(keyword);
        }
    }
    // Provider and domain labels are useful search terms but never identity.
    for label in [record.provider.as_deref(), record.domain_name.as_deref()] {
        if let Some(label) = label.map(str::trim) {
            if !label.is_empty() && !keywords.iter().any(|k| k.eq_ignore_ascii_case(label)) {
                keywords.push(label.to_string());
            }
        }
    }

    SearchDocument {
        asset_id: asset.id,
        kind: asset.kind.as_str().to_string(),
        title: asset.name.clone(),
        subtitle: Some(subtitle_parts.join(" · ")),
        body: if body_parts.is_empty() {
            None
        } else {
            Some(body_parts.join(" · "))
        },
        keywords,
        updated_at: asset.updated_at,
    }
}
