//! Deterministic duplicate review — Phase 4C (docs/11).
//!
//! Detection is advisory and review-only. This service finds pairs of
//! canonical assets that look like the same thing and explains **why**, using
//! nothing but already-canonical fields. It never merges, never scores, and
//! never writes: the merge stays the explicit
//! [`crate::application::asset_service::AssetService::merge_assets`] command,
//! which is also where the real field-level conflict list comes from.
//!
//! Rules this module upholds:
//!
//! - **Detection ≠ merge.** No evidence strength, however high, causes a
//!   canonical write. There is no numeric confidence at all.
//! - **Only reachable conditions.** Global `UNIQUE(namespace, external_id)`
//!   means two live canonical assets can never share an external reference, so
//!   that is deliberately *not* a candidate signal here — it belongs to the
//!   import/discovery review path, which already reports it. Inventing a
//!   condition that cannot occur would only produce unreachable code.
//! - **Bucketed, not pairwise.** Candidates come from buckets keyed by
//!   (kind, normalized name) and by module-specific deterministic keys, so the
//!   work is bounded by bucket size rather than by N².
//! - **Deterministic pairs.** Every pair is ordered by canonical Asset id, so
//!   `A/B` and `B/A` are one candidate.

use std::collections::HashMap;

use crate::application::library_service::{
    load_library_rows, AssetDetails, AssetSummary, LibraryModule, Page, PageRequest,
};
use crate::application::software_discovery::normalize_name;
use crate::domain::asset::LifecycleState;
use crate::domain::ids::AssetId;
use crate::ports::uow::UnitOfWorkFactory;
use crate::AppResult;
use serde::Serialize;

/// Why two assets look like the same thing.
///
/// Every variant is derived from canonical, non-secret fields. Nothing here is
/// a probability, a ranking, or a decision.
/// Serialized as `{"evidence": "same_normalized_name", …}` so a transport
/// consumer sees a self-describing union. The tag is `evidence` rather than
/// `kind` because one variant already carries an asset `kind` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum DuplicateEvidence {
    /// Same normalized (casefolded, whitespace-collapsed) name **and** the
    /// same asset kind. Different kinds are never duplicates: a film and a CLI
    /// tool may legitimately share a name.
    SameNormalizedName {
        normalized_name: String,
        kind: crate::domain::asset::AssetKind,
    },
    /// Same non-secret provider label (Services).
    SameProvider { provider: String },
    /// Same canonical domain text (Services, domain records only).
    SameDomain { domain: String },
    /// Same install location (Software).
    SameInstallLocation { location: String },
}

impl DuplicateEvidence {
    /// A short, stable label for CLI and log output.
    pub fn label(&self) -> String {
        match self {
            DuplicateEvidence::SameNormalizedName {
                normalized_name, ..
            } => {
                format!("same normalized name ({normalized_name})")
            }
            DuplicateEvidence::SameProvider { provider } => format!("same provider ({provider})"),
            DuplicateEvidence::SameDomain { domain } => format!("same domain ({domain})"),
            DuplicateEvidence::SameInstallLocation { location } => {
                format!("same install location ({location})")
            }
        }
    }
}

/// One reviewable duplicate pair.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicateCandidate {
    /// The smaller canonical Asset id of the pair.
    pub left: AssetSummary,
    /// The larger canonical Asset id of the pair.
    pub right: AssetSummary,
    /// Every reason this pair was reported, in a stable order.
    pub evidence: Vec<DuplicateEvidence>,
}

/// A duplicate review query.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateQuery {
    /// Restrict to these asset kinds. Empty means every kind.
    pub kinds: Vec<crate::domain::asset::AssetKind>,
    /// Whether archived assets may appear as candidates. Defaults to `true`:
    /// merging an archived duplicate into a live survivor is a legitimate flow
    /// (ADR 0005), so hiding it would hide the review.
    pub include_archived: bool,
    pub page: PageRequest,
}

impl Default for DuplicateQuery {
    fn default() -> Self {
        DuplicateQuery {
            kinds: Vec::new(),
            include_archived: true,
            page: PageRequest::default(),
        }
    }
}

/// Read-only duplicate review (docs/11 Phase 4C).
#[derive(Debug, Clone)]
pub struct DuplicateReviewService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> DuplicateReviewService<F> {
    pub fn new(factory: F) -> Self {
        DuplicateReviewService { factory }
    }

    /// One page of duplicate candidates, deterministically ordered.
    ///
    /// Read-only by construction: this never touches canonical state, and the
    /// merge it may lead to is a separate explicit command.
    pub fn candidates(&mut self, query: &DuplicateQuery) -> AppResult<Page<DuplicateCandidate>> {
        let limit = query.page.effective_limit();
        self.factory.read(&mut |q| {
            let rows = load_library_rows(q, &LibraryModule::ALL)?;

            let eligible: Vec<&crate::application::library_service::LibraryRow> = rows
                .iter()
                .filter(|row| {
                    if row.asset.lifecycle_state == LifecycleState::Merged {
                        // A tombstone is a redirect, not an inventory entry.
                        return false;
                    }
                    if !query.include_archived
                        && row.asset.lifecycle_state == LifecycleState::Archived
                    {
                        return false;
                    }
                    query.kinds.is_empty() || query.kinds.contains(&row.asset.kind)
                })
                .collect();

            let mut candidates = collect_candidates(&eligible);
            candidates.sort_by_key(|candidate| (candidate.left.id, candidate.right.id));

            let total = candidates.len();
            let items = candidates
                .into_iter()
                .skip(query.page.offset)
                .take(limit)
                .collect();
            Ok(Page {
                items,
                offset: query.page.offset,
                limit,
                total: Some(total),
            })
        })
    }
}

/// Groups eligible rows into buckets and emits one candidate per pair that
/// shares at least one deterministic key.
///
/// Two bucket families:
///
/// 1. `(kind, normalized name)` — the general, module-independent rule;
/// 2. module-specific deterministic keys (provider, domain, install location).
///
/// Only rows inside the same bucket are compared, so the cost is bounded by the
/// largest bucket instead of by the square of the library size.
fn collect_candidates(
    rows: &[&crate::application::library_service::LibraryRow],
) -> Vec<DuplicateCandidate> {
    // Bucket key → row indexes. A pair may share several keys; the evidence is
    // merged and de-duplicated per pair rather than reported twice.
    let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let keys = bucket_keys(row);
        for key in keys {
            buckets.entry(key).or_default().push(index);
        }
    }

    let mut found: HashMap<(AssetId, AssetId), Vec<DuplicateEvidence>> = HashMap::new();
    for indexes in buckets.values() {
        if indexes.len() < 2 {
            continue;
        }
        for (position, left) in indexes.iter().enumerate() {
            for right in &indexes[position + 1..] {
                // Compare in canonical pair order (smaller id first) so the
                // evidence value is taken from the same side regardless of the
                // order the module reader happened to return rows in — the
                // in-memory double and SQLite order rows differently.
                let (first, second) = if rows[*left].asset.id < rows[*right].asset.id {
                    (rows[*left], rows[*right])
                } else {
                    (rows[*right], rows[*left])
                };
                let Some(evidence) = evidence_between(first, second) else {
                    continue;
                };
                if evidence.is_empty() {
                    continue;
                }
                let entry = found.entry((first.asset.id, second.asset.id)).or_default();
                for item in evidence {
                    if !entry.contains(&item) {
                        entry.push(item);
                    }
                }
            }
        }
    }

    let by_id: HashMap<AssetId, &crate::application::library_service::LibraryRow> =
        rows.iter().map(|row| (row.asset.id, *row)).collect();

    found
        .into_iter()
        .filter_map(|((left_id, right_id), mut evidence)| {
            let left = by_id.get(&left_id)?;
            let right = by_id.get(&right_id)?;
            // Stable evidence order for a stable API.
            evidence.sort_by_key(|item| format!("{:?}", item));
            Some(DuplicateCandidate {
                left: crate::application::library_service::summarize_row(left),
                right: crate::application::library_service::summarize_row(right),
                evidence,
            })
        })
        .collect()
}

/// The deterministic bucket keys one row contributes to.
fn bucket_keys(row: &crate::application::library_service::LibraryRow) -> Vec<String> {
    let mut keys = vec![format!(
        "name:{}:{}",
        row.asset.kind.as_str(),
        normalize_name(&row.asset.name)
    )];
    match &row.details {
        AssetDetails::Service(record) => {
            if let Some(provider) = record.provider.as_deref().map(str::trim) {
                if !provider.is_empty() {
                    keys.push(format!("provider:{}", provider.to_lowercase()));
                }
            }
            if let Some(domain) = record.domain_name.as_deref().map(str::trim) {
                if !domain.is_empty() {
                    keys.push(format!("domain:{domain}"));
                }
            }
        }
        AssetDetails::Software(record) => {
            if let Some(location) = record.install_location.as_deref().map(str::trim) {
                if !location.is_empty() {
                    keys.push(format!("location:{location}"));
                }
            }
        }
        // Media has no module-specific deterministic duplicate key beyond the
        // normalized name; rating/year are not identity.
        AssetDetails::Media(_) => {}
    }
    keys
}

/// The evidence that two rows in the same bucket actually share. Returns
/// `None` when they must not be compared at all — the only such case is a
/// different asset kind, which the name bucket already separates.
fn evidence_between(
    left: &crate::application::library_service::LibraryRow,
    right: &crate::application::library_service::LibraryRow,
) -> Option<Vec<DuplicateEvidence>> {
    if left.asset.kind != right.asset.kind {
        return None;
    }
    let mut evidence = Vec::new();

    let left_name = normalize_name(&left.asset.name);
    let right_name = normalize_name(&right.asset.name);
    if !left_name.is_empty() && left_name == right_name {
        evidence.push(DuplicateEvidence::SameNormalizedName {
            normalized_name: left_name,
            kind: left.asset.kind,
        });
    }

    // Module-specific keys only ever apply within one module, which the kind
    // check above already guarantees.
    //
    // Every string value below is taken from `left`, the canonically smaller
    // asset, so the evidence does not depend on the order the module reader
    // returned rows in. The comparison rule differs per field on purpose, and
    // matches how each field is stored:
    //
    // - `provider` is free text stored as typed, so it is compared
    //   case-insensitively;
    // - `domain_name` is normalized to lowercase by `ServiceRecord::validate`,
    //   so an exact comparison is already case-insensitive;
    // - `install_location` is a filesystem path stored verbatim, so it is
    //   compared exactly — a path's case is not assumed to be insignificant.
    match (&left.details, &right.details) {
        (AssetDetails::Service(l), AssetDetails::Service(r)) => {
            if let (Some(a), Some(b)) = (
                l.provider
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty()),
                r.provider
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty()),
            ) {
                if a.eq_ignore_ascii_case(b) {
                    evidence.push(DuplicateEvidence::SameProvider {
                        provider: a.to_string(),
                    });
                }
            }
            if let (Some(a), Some(b)) = (
                l.domain_name.as_deref().filter(|v| !v.is_empty()),
                r.domain_name.as_deref().filter(|v| !v.is_empty()),
            ) {
                if a == b {
                    evidence.push(DuplicateEvidence::SameDomain {
                        domain: a.to_string(),
                    });
                }
            }
        }
        (AssetDetails::Software(l), AssetDetails::Software(r)) => {
            if let (Some(a), Some(b)) = (
                l.install_location
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty()),
                r.install_location
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty()),
            ) {
                if a == b {
                    evidence.push(DuplicateEvidence::SameInstallLocation {
                        location: a.to_string(),
                    });
                }
            }
        }
        _ => {}
    }

    Some(evidence)
}
