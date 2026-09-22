//! Module -> SearchDocument projections (ADR 0006), plus the concise
//! module-aware summary each module contributes to the unified library.
//!
//! Modules own the transformation from their typed details into the shared
//! search representation. These projectors are pure and used by both
//! synchronous updates and full rebuilds.
//!
//! The `*_subtitle` helpers are the single source of that one-line summary:
//! the search projection and the unified library list/search contract both
//! call them, so the two vocabularies cannot drift apart (docs/11).

use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::media::MediaRecord;
use crate::domain::search::SearchDocument;
use crate::domain::service::{format_money, BillingCadence, ServiceRecord};
use crate::domain::software::SoftwareRecord;
use crate::domain::tag::Tag;

/// Concise module-aware summary of a media record ("Anime · 2023").
///
/// Shared by [`project_media`] and the unified library summary contract
/// (docs/11) so both speak the same vocabulary.
pub fn media_subtitle(record: &MediaRecord) -> Option<String> {
    Some(match record.year {
        Some(year) => format!("{} · {}", record.media_type.label(), year),
        None => record.media_type.label().to_string(),
    })
}

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
    let subtitle = media_subtitle(record);

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

/// Concise module-aware summary of a software record ("CLI Tool · 14.1.0").
///
/// Shared by [`project_software`] and the unified library summary contract
/// (docs/11).
pub fn software_subtitle(record: &SoftwareRecord) -> Option<String> {
    Some(match &record.version {
        Some(version) if !version.trim().is_empty() => {
            format!("{} · {}", record.category.label(), version.trim())
        }
        _ => record.category.label().to_string(),
    })
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
    let subtitle = software_subtitle(record);

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

/// Concise module-aware summary of a service record ("SaaS · OpenAI · Plus"):
/// type label plus only the fields that have values, in a stable order.
///
/// Shared by [`project_service`] and the unified library summary contract
/// (docs/11).
pub fn service_subtitle(record: &ServiceRecord) -> Option<String> {
    let mut parts: Vec<String> = vec![record.service_type.label().to_string()];
    for value in [record.provider.as_deref(), record.plan.as_deref()] {
        if let Some(value) = value.map(str::trim) {
            if !value.is_empty() {
                parts.push(value.to_string());
            }
        }
    }
    Some(parts.join(" · "))
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
    let subtitle = service_subtitle(record);

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
        subtitle,
        body: if body_parts.is_empty() {
            None
        } else {
            Some(body_parts.join(" · "))
        },
        keywords,
        updated_at: asset.updated_at,
    }
}
