//! Media and Software -> SearchDocument projections (ADR 0006).
//!
//! Modules own the transformation from their typed details into the shared
//! search representation. These projectors are pure and used by both
//! synchronous updates and full rebuilds.

use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::media::MediaRecord;
use crate::domain::search::SearchDocument;
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
