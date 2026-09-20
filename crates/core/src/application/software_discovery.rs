//! Software discovery candidates and matching (docs/06 discovery lifecycle,
//! ADR 0005).
//!
//! Candidates are advisory DTOs produced by providers — never canonical
//! state. Classification follows the ADR 0005 precedence: exact namespaced
//! external references first, then deterministic module keys (Software has
//! none beyond its external refs), then heuristics. Heuristic matches are
//! reported as review suggestions only; nothing here ever mutates canonical
//! data.

use crate::domain::ids::AssetId;
use crate::ports::uow::QueryUnitOfWork;
use crate::{AppError, AppResult};
use serde::Serialize;
use std::collections::BTreeSet;

/// The candidate DTO lives at the port boundary ([`crate::ports::providers`]);
/// the application layer consumes it for classification and adoption.
pub use crate::ports::providers::{CandidateRef, SoftwareCandidate};

/// Why a candidate was classified this way (docs/06: the caller must be able
/// to understand WHY). Heuristic results are review-only.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateDisposition {
    /// No exact ref and no heuristic similarity: adoption would create a new
    /// canonical asset.
    New,
    /// A namespaced external ref exactly identifies one existing software
    /// asset (ADR 0005 precedence level 2).
    ExactMatch { asset_id: AssetId },
    /// Normalized-name similarity only. Review required; adoption never
    /// silently merges or updates these.
    PotentialDuplicate { asset_ids: Vec<AssetId> },
    /// Ambiguous state the caller must resolve: refs pointing at different
    /// assets, a ref owned by a non-software asset, or similar.
    Conflict { message: String },
}

impl CandidateDisposition {
    pub fn kind(&self) -> &'static str {
        match self {
            CandidateDisposition::New => "new",
            CandidateDisposition::ExactMatch { .. } => "exact_match",
            CandidateDisposition::PotentialDuplicate { .. } => "potential_duplicate",
            CandidateDisposition::Conflict { .. } => "conflict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassifiedCandidate {
    pub candidate: SoftwareCandidate,
    pub disposition: CandidateDisposition,
}

/// Classifies one candidate against canonical state. Read-only; runs inside
/// the caller's scope (read scope for review, write transaction for
/// adoption re-check).
///
/// Precedence (ADR 0005):
/// 1. exact external refs — all candidate refs must agree on one owner,
///    which must be a software asset, else the classification is a conflict;
/// 2. deterministic module key — Software's deterministic identity IS its
///    namespaced external refs, so this level adds nothing;
/// 3. heuristic: normalized display-name similarity → review suggestion.
pub fn classify_candidate(
    q: &mut dyn QueryUnitOfWork,
    candidate: &SoftwareCandidate,
) -> AppResult<CandidateDisposition> {
    candidate.validate()?;

    // 1. Exact namespaced external references.
    let mut owners: BTreeSet<AssetId> = BTreeSet::new();
    for reference in &candidate.external_refs {
        if let Some(owner) = q
            .external_refs()
            .find_asset_by_ref(&reference.namespace, &reference.external_id)?
        {
            owners.insert(owner);
        }
    }
    match owners.len() {
        1 => {
            let owner = *owners.iter().next().unwrap();
            let asset = q
                .assets()
                .get(owner)?
                .ok_or_else(|| AppError::not_found("asset", owner))?;
            if asset.kind.module() == "software" {
                return Ok(CandidateDisposition::ExactMatch { asset_id: owner });
            }
            return Ok(CandidateDisposition::Conflict {
                message: format!(
                    "external refs identify asset {owner} of kind {} — not a software asset",
                    asset.kind
                ),
            });
        }
        n if n > 1 => {
            let ids: Vec<String> = owners.iter().map(|id| id.to_string()).collect();
            return Ok(CandidateDisposition::Conflict {
                message: format!(
                    "candidate external refs point at different assets: {}",
                    ids.join(", ")
                ),
            });
        }
        _ => {}
    }

    // 3. Heuristic: normalized display-name similarity against active
    // software assets. Reported for review, never auto-applied.
    let normalized = normalize_name(&candidate.display_name);
    let mut similar: Vec<AssetId> = Vec::new();
    if !normalized.is_empty() {
        for record in q.software().list_all()? {
            let Some(asset) = q.assets().get(record.asset_id)? else {
                continue;
            };
            if asset.lifecycle_state != crate::domain::asset::LifecycleState::Active {
                continue;
            }
            if normalize_name(&asset.name) == normalized {
                similar.push(asset.id);
            }
        }
    }
    if similar.is_empty() {
        Ok(CandidateDisposition::New)
    } else {
        Ok(CandidateDisposition::PotentialDuplicate { asset_ids: similar })
    }
}

/// Normalized-name key: casefolded, whitespace-collapsed. Heuristic only
/// (ADR 0005): distinct software may legitimately share a name.
pub fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
