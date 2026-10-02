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
//! - **Bounded work.** Identity buckets avoid unrelated comparisons; an
//!   explicit pair budget also bounds large buckets. A partial scan reports
//!   `total: None`, never an exact count for an incomplete result.
//! - **Deterministic pairs.** Every pair is ordered by canonical Asset id, so
//!   `A/B` and `B/A` are one candidate.

use std::collections::BTreeMap;

use crate::application::library_service::{
    load_library_rows, AssetDetails, AssetSummary, LibraryModule, Page, PageRequest,
};
use crate::application::software_discovery::normalize_name;
use crate::domain::asset::LifecycleState;
use crate::domain::ids::AssetId;
use crate::ports::uow::UnitOfWorkFactory;
use crate::AppResult;
use serde::{Deserialize, Serialize};

pub const MAX_DUPLICATE_PAIR_CHECKS: usize = 10_000;
type CandidatePairs = BTreeMap<(AssetId, AssetId), Vec<DuplicateEvidence>>;

/// Why two assets look like the same thing.
///
/// Every variant is derived from canonical, non-secret fields. Nothing here is
/// a probability, a ranking, or a decision.
/// Serialized as `{"evidence": "same_normalized_name", …}` so a transport
/// consumer sees a self-describing union. The tag is `evidence` rather than
/// `kind` because one variant already carries an asset `kind` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "snake_case")]
pub enum DuplicateEvidence {
    /// Same normalized (casefolded, whitespace-collapsed) name **and** the
    /// same asset kind. Different kinds are never duplicates: a film and a CLI
    /// tool may legitimately share a name.
    SameNormalizedName {
        normalized_name: String,
        kind: crate::domain::asset::AssetKind,
    },
    /// Same provider label, supplementary to matching name or domain.
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
            let modules: Vec<_> = LibraryModule::ALL
                .into_iter()
                .filter(|module| {
                    query.kinds.is_empty() || query.kinds.iter().any(|kind| module.matches(*kind))
                })
                .collect();
            let rows = load_library_rows(q, &modules)?;

            let mut eligible: Vec<&crate::application::library_service::LibraryRow> = rows
                .iter()
                .filter(|row| {
                    if row.asset.lifecycle_state == LifecycleState::Merged
                        || row.asset.kind == crate::domain::asset::AssetKind::InfoItem
                    {
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

            eligible.sort_by_key(|row| row.asset.id);
            let (pairs, truncated) = collect_candidates(&eligible);
            let total = (!truncated).then_some(pairs.len());
            let by_id: BTreeMap<_, _> = eligible.iter().map(|row| (row.asset.id, *row)).collect();
            // Hydrate summaries only for the returned page, not every pair.
            let items = pairs
                .into_iter()
                .skip(query.page.offset)
                .take(limit)
                .map(|((left, right), evidence)| DuplicateCandidate {
                    left: crate::application::library_service::summarize_row(by_id[&left]),
                    right: crate::application::library_service::summarize_row(by_id[&right]),
                    evidence,
                })
                .collect();
            Ok(Page {
                items,
                offset: query.page.offset,
                limit,
                total,
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
/// 2. module-specific identity keys (domain, install location).
///
/// Buckets and pairs are visited deterministically. Large groups stop at the
/// work budget; the partial set remains stable across adapters and retries.
fn collect_candidates(
    rows: &[&crate::application::library_service::LibraryRow],
) -> (CandidatePairs, bool) {
    // Bucket key → row indexes. A pair may share several keys; the evidence is
    // merged and de-duplicated per pair rather than reported twice.
    let mut buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let keys = bucket_keys(row);
        for key in keys {
            buckets.entry(key).or_default().push(index);
        }
    }

    let mut found = CandidatePairs::new();
    let mut checks = 0;
    let mut truncated = false;
    'buckets: for indexes in buckets.values() {
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
                let pair = (first.asset.id, second.asset.id);
                if found.contains_key(&pair) {
                    continue;
                }
                if checks == MAX_DUPLICATE_PAIR_CHECKS {
                    truncated = true;
                    break 'buckets;
                }
                checks += 1;
                let Some(evidence) = evidence_between(first, second) else {
                    continue;
                };
                if evidence.is_empty() {
                    continue;
                }
                let mut evidence = evidence;
                evidence.sort_by_key(|item| format!("{item:?}"));
                found.insert(pair, evidence);
            }
        }
    }

    (found, truncated)
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
            if let Some(domain) = record.domain_name.as_deref().map(str::trim) {
                if !domain.is_empty() {
                    keys.push(format!("domain:{}:{domain}", row.asset.kind.as_str()));
                }
            }
        }
        AssetDetails::Software(record) => {
            if let Some(location) = record.install_location.as_deref().map(str::trim) {
                if !location.is_empty() {
                    keys.push(format!("location:{}:{location}", row.asset.kind.as_str()));
                }
            }
        }
        // Media has no module-specific deterministic duplicate key beyond the
        // normalized name; rating/year are not identity.
        AssetDetails::Media(_) | AssetDetails::Info(_) => {}
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
    if left.asset.kind == crate::domain::asset::AssetKind::InfoItem {
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
