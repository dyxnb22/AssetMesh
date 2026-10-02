//! Input normalization before any matching or write transaction.
use crate::application::import_parse::{
    parse_timestamp, ImportCandidate, PendingExternalRef, RawMediaInput,
};
use crate::application::shared::normalize_tags;
use crate::domain::ids::AssetId;
use crate::domain::media::{MediaRecord, MediaStatus, MediaType, Progress};

/// Validates and normalizes one raw record into a candidate. Returns the
/// rejection reason on failure.
pub(super) fn normalize_candidate(
    raw: RawMediaInput,
    index: usize,
) -> Result<ImportCandidate, String> {
    let title = raw
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("record {index} is missing a title"))?;

    let media_type = raw
        .media_type
        .as_deref()
        .and_then(MediaType::parse)
        .ok_or_else(|| {
            format!(
                "record {index} has an unsupported media_type {:?} (expected movie, tv, anime, game)",
                raw.media_type
            )
        })?;

    let status = match raw.status.as_deref() {
        None => None,
        Some(s) => Some(
            MediaStatus::parse(s)
                .ok_or_else(|| format!("record {index} has an unknown status {s:?}"))?,
        ),
    };

    let started_at = match raw.started_at.as_deref() {
        None => None,
        Some(s) => Some(
            parse_timestamp(s)
                .ok_or_else(|| format!("record {index} has an unparseable started_at {s:?}"))?,
        ),
    };
    let completed_at = match raw.completed_at.as_deref() {
        None => None,
        Some(s) => Some(
            parse_timestamp(s)
                .ok_or_else(|| format!("record {index} has an unparseable completed_at {s:?}"))?,
        ),
    };

    let asset_id = match raw
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(s) => Some(
            uuid::Uuid::parse_str(s)
                .map(AssetId::from_uuid)
                .map_err(|_| format!("record {index} has an invalid asset_id {s:?}"))?,
        ),
    };

    let mut warnings = Vec::new();

    let progress = Progress {
        current: raw.progress_current,
        total: raw.progress_total,
        unit: raw
            .progress_unit
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase),
    };

    // Validate record-level invariants before matching. For rows without an
    // explicit status (update candidates), pick the most permissive status
    // consistent with the provided timestamps so legitimate enrichment is
    // not rejected; the commit path re-validates against the real target and
    // discloses any failure in the report.
    let effective_status = match (status, completed_at, started_at) {
        (Some(s), _, _) => s,
        (None, Some(_), _) => MediaStatus::Completed,
        (None, None, Some(_)) => MediaStatus::InProgress,
        (None, None, None) => MediaStatus::Planned,
    };
    let mut probe = MediaRecord::new(
        AssetId::from_uuid(uuid::Uuid::nil()),
        media_type,
        effective_status,
    );
    probe.rating = raw.rating;
    probe.year = raw.year;
    probe.platform = raw.platform.clone();
    probe.progress = progress.clone();
    probe.notes = raw.notes.clone();
    probe.started_at = started_at;
    probe.completed_at = completed_at;
    if let Err(e) = probe.validate() {
        return Err(format!("record {index}: {e}"));
    }

    let mut refs: Vec<PendingExternalRef> = Vec::new();
    if let Some(raw_refs) = raw.external_refs {
        let flattened: Vec<(String, String, Option<String>)> = match raw_refs {
            crate::application::import_parse::RawExternalRefs::Map(map) => {
                map.into_iter().map(|(ns, id)| (ns, id, None)).collect()
            }
            crate::application::import_parse::RawExternalRefs::List(items) => items
                .into_iter()
                .map(|r| (r.namespace, r.external_id, r.source_url))
                .collect(),
        };
        for (namespace, external_id, source_url) in flattened {
            let reference = PendingExternalRef {
                namespace: namespace.trim().to_lowercase(),
                external_id: external_id.trim().to_string(),
                source_url,
            };
            if let Err(e) = reference.validate() {
                return Err(format!("record {index}: {e}"));
            }
            if refs.iter().any(|r| r.key() == reference.key()) {
                warnings.push(format!(
                    "duplicate external ref {} ignored",
                    reference.key().0
                ));
                continue;
            }
            refs.push(reference);
        }
    }

    Ok(ImportCandidate {
        index,
        asset_id,
        title: title.to_string(),
        media_type,
        status,
        rating: raw.rating,
        year: raw.year,
        platform: raw.platform,
        progress,
        notes: raw.notes,
        summary: raw.summary,
        tags: normalize_tags(&raw.tags.unwrap_or_default()),
        refs,
        started_at,
        completed_at,
        warnings,
        planned_asset_id: None,
    })
}
