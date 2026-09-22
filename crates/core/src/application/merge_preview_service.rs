//! Merge preview use case (docs/10 merge rules, ADR 0005).
//!
//! [`AssetService::merge_assets`](crate::application::asset_service::AssetService::merge_assets)
//! is the write side; this is the read side that tells the user, before they
//! commit, exactly what the merge will move and what it will drop as
//! redundant. Computing a preview is a read over six repositories, so it is an
//! application use case: an adapter that computed it itself would have to reach
//! into repositories, which the architectural invariant forbids
//! (CONTRIBUTING.md).

use crate::application::library_service::AssetSummary;
use crate::domain::asset::{Asset, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::relation::Relation;
use crate::domain::service::ServiceRecord;
use crate::error::{AppError, AppResult};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWorkFactory};

/// Everything the merge-review UI needs to describe one merge before it
/// happens.
///
/// `transferred_*` describes what the survivor gains; `redundant_*` describes
/// what is dropped because the survivor already has an equivalent. `conflicts`
/// lists the reasons the merge button must stay disabled — it is the only field
/// the UI needs to decide, but the transfers are what makes the choice
/// understandable.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MergePreviewView {
    pub winner: AssetSummary,
    pub loser: AssetSummary,
    pub can_merge: bool,
    pub conflicts: Vec<String>,
    /// Tag names the winner does not already have, sorted by name.
    pub transferred_tags: Vec<String>,
    pub transferred_external_refs: Vec<AssetExternalRef>,
    pub redundant_external_refs: Vec<AssetExternalRef>,
    pub transferred_relations_count: usize,
    pub redundant_relations_count: usize,
    pub notes: Vec<String>,
}

/// Read-only merge preview. Holds only a factory, because a preview writes
/// nothing and needs no clock or id generator.
pub struct MergePreviewService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> MergePreviewService<F> {
    pub fn new(factory: F) -> Self {
        Self { factory }
    }

    /// Previews merging `loser_id` into `winner_id`.
    ///
    /// Errors:
    /// - `winner_id == loser_id` → `validation`, it is not a meaningful merge;
    /// - either id unknown → `not_found`;
    /// - anything else is storage.
    ///
    /// A preview never returns `conflict` itself: a conflict is *content of the
    /// preview*, reported through [`MergePreviewView::conflicts`], not a failed
    /// call. Only an unpreviewable state (missing asset, storage fault) is an
    /// `Err`.
    pub fn preview(
        &mut self,
        winner_id: AssetId,
        loser_id: AssetId,
    ) -> AppResult<MergePreviewView> {
        if winner_id == loser_id {
            return Err(AppError::validation("cannot merge an asset into itself"));
        }
        self.factory
            .read(&mut |q| build_preview(q, winner_id, loser_id))
    }
}

fn build_preview(
    q: &mut dyn QueryUnitOfWork,
    winner_id: AssetId,
    loser_id: AssetId,
) -> AppResult<MergePreviewView> {
    let loser = q
        .assets()
        .get(loser_id)?
        .ok_or_else(|| AppError::not_found("asset", loser_id))?;
    let winner = q
        .assets()
        .get(winner_id)?
        .ok_or_else(|| AppError::not_found("asset", winner_id))?;

    let mut conflicts = Vec::new();
    let mut notes = Vec::new();

    if loser.lifecycle_state == LifecycleState::Merged {
        conflicts.push("Loser is already merged into another asset".to_string());
    }
    if winner.lifecycle_state != LifecycleState::Active {
        conflicts.push("Winner must be active to receive a merge".to_string());
    }
    if loser.kind != winner.kind {
        conflicts.push(format!(
            "Cannot merge assets of different kinds: '{}' vs '{}'",
            loser.kind, winner.kind
        ));
    }

    let loser_tags = q.tags().list_for_asset(loser_id)?;
    let winner_tags = q.tags().list_for_asset(winner_id)?;
    let transferred_tags: Vec<String> = loser_tags
        .into_iter()
        .filter(|lt| !winner_tags.iter().any(|wt| wt.id == lt.id))
        .map(|t| t.name)
        .collect();

    let loser_refs = q.external_refs().list_for_asset(loser_id)?;
    let mut transferred_external_refs = Vec::new();
    let mut redundant_external_refs = Vec::new();
    for r in loser_refs {
        // A ref that already resolves to the winner is a duplicate alias, not a
        // transferable one — the merge would delete the loser's copy.
        let already_won = q
            .external_refs()
            .find_asset_by_ref(&r.namespace, &r.external_id)?
            == Some(winner_id);
        if already_won {
            redundant_external_refs.push(r);
        } else {
            transferred_external_refs.push(r);
        }
    }

    let loser_relations = q.relations().list_for_asset(loser_id)?;
    let winner_relations = q.relations().list_for_asset(winner_id)?;
    let mut transferred_relations_count = 0usize;
    let mut redundant_relations_count = 0usize;
    for rel in loser_relations {
        classify_relation(
            rel,
            loser_id,
            winner_id,
            &winner_relations,
            &mut transferred_relations_count,
            &mut redundant_relations_count,
        );
    }

    // Services carry the money and renewal fields, so they get field-level
    // conflict detection instead of a whole-record comparison: an unset field
    // on either side is filled in, only a real disagreement blocks the merge
    // (docs/10 rules 4 and 5).
    let loser_service = q.services().get(loser_id)?;
    let winner_service = q.services().get(winner_id)?;
    if let (Some(l_rec), Some(w_rec)) = (loser_service, winner_service) {
        let service_conflicts = conflicting_service_fields(&w_rec, &l_rec);
        if !service_conflicts.is_empty() {
            conflicts.push(format!(
                "Service details conflict on: {}",
                service_conflicts.join(", ")
            ));
        }
    }

    // Media and software records on both sides are not conflicts: the winner
    // keeps its record and the loser's is preserved verbatim in the activity
    // payload. The UI needs to say that, so it is a note rather than silence.
    if q.media().get(loser_id)?.is_some() && q.media().get(winner_id)?.is_some() {
        notes.push(
            "Winner's media metadata is kept. Loser's media record is preserved in the audit log."
                .into(),
        );
    }
    if q.software().get(loser_id)?.is_some() && q.software().get(winner_id)?.is_some() {
        notes.push(
            "Winner's software metadata is kept. Loser's software record is preserved in the audit log."
                .into(),
        );
    }

    let can_merge = conflicts.is_empty();

    let winner_summary = summarize(&winner, winner_tags.into_iter().map(|t| t.name).collect());
    // The loser's summary shows only the tags that would actually move, so the
    // two columns are not symmetric: the winner column lists what it has, the
    // loser column lists what it contributes.
    let loser_summary = summarize(&loser, transferred_tags.clone());

    Ok(MergePreviewView {
        winner: winner_summary,
        loser: loser_summary,
        can_merge,
        conflicts,
        transferred_tags,
        transferred_external_refs,
        redundant_external_refs,
        transferred_relations_count,
        redundant_relations_count,
        notes,
    })
}

/// A relation from the loser is redundant when its other endpoint is the winner
/// (the merge would collapse the edge onto itself) or when the winner already
/// has a canonically-equal edge. Otherwise it transfers, rewritten to point at
/// the winner and compared in canonical form so an undirected duplicate is
/// still caught.
fn classify_relation(
    rel: Relation,
    loser_id: AssetId,
    winner_id: AssetId,
    winner_relations: &[Relation],
    transferred: &mut usize,
    redundant: &mut usize,
) {
    let other = if rel.source_asset_id == loser_id {
        rel.target_asset_id
    } else {
        rel.source_asset_id
    };
    if other == winner_id {
        *redundant += 1;
        return;
    }
    let mut moved = rel;
    if moved.source_asset_id == loser_id {
        moved.source_asset_id = winner_id;
    } else {
        moved.target_asset_id = winner_id;
    }
    moved = moved.canonical_form();

    let is_dup = winner_relations.iter().any(|wr| {
        let wr_canon = wr.clone().canonical_form();
        wr_canon.source_asset_id == moved.source_asset_id
            && wr_canon.target_asset_id == moved.target_asset_id
            && wr_canon.relation_type == moved.relation_type
    });

    if is_dup {
        *redundant += 1;
    } else {
        *transferred += 1;
    }
}

/// Names the service fields where both sides are set and disagree. Fields left
/// empty on one side are deliberately absent: the merge fills them from the
/// other record rather than blocking.
fn conflicting_service_fields(winner: &ServiceRecord, loser: &ServiceRecord) -> Vec<String> {
    let mut conflicts = Vec::new();
    let mut check_text = |field_name: &str, v1: &Option<String>, v2: &Option<String>| {
        if let (Some(a), Some(b)) = (v1, v2) {
            if a != b {
                conflicts.push(format!("{field_name}: '{a}' vs '{b}'"));
            }
        }
    };
    check_text("provider", &winner.provider, &loser.provider);
    check_text("account_label", &winner.account_label, &loser.account_label);
    check_text("endpoint_url", &winner.endpoint_url, &loser.endpoint_url);
    check_text("dashboard_url", &winner.dashboard_url, &loser.dashboard_url);
    check_text("domain_name", &winner.domain_name, &loser.domain_name);
    check_text("plan", &winner.plan, &loser.plan);
    check_text("notes", &winner.notes, &loser.notes);
    if let (Some(a), Some(b)) = (winner.billing_cadence, loser.billing_cadence) {
        if a != b {
            conflicts.push(format!("billing_cadence: '{a:?}' vs '{b:?}'"));
        }
    }
    if let (Some(a), Some(b)) = (winner.renews_at, loser.renews_at) {
        if a != b {
            conflicts.push(format!("renews_at: '{a}' vs '{b}'"));
        }
    }
    if let (Some(a), Some(b)) = (winner.expires_at, loser.expires_at) {
        if a != b {
            conflicts.push(format!("expires_at: '{a}' vs '{b}'"));
        }
    }
    if let (Some(a), Some(b)) = (winner.auto_renew, loser.auto_renew) {
        if a != b {
            conflicts.push(format!("auto_renew: '{a}' vs '{b}'"));
        }
    }
    // Money is a paired value (ADR 0010): a cost disagreement is only meaningful
    // when both amount and currency are present on both sides.
    match (
        (winner.cost_minor, winner.currency.as_deref()),
        (loser.cost_minor, loser.currency.as_deref()),
    ) {
        ((Some(w_cost), Some(w_curr)), (Some(l_cost), Some(l_curr)))
            if w_cost != l_cost || w_curr != l_curr =>
        {
            conflicts.push(format!("cost: {w_cost} {w_curr} vs {l_cost} {l_curr}"));
        }
        _ => {}
    }
    conflicts
}

/// Builds an [`AssetSummary`] from a stored asset and its already-loaded tag
/// names. The merge preview reads tags for its transfer analysis anyway, so it
/// does not pay for the library's module-aware subtitle.
fn summarize(asset: &Asset, tags: Vec<String>) -> AssetSummary {
    AssetSummary {
        id: asset.id,
        kind: asset.kind,
        name: asset.name.clone(),
        lifecycle: asset.lifecycle_state,
        revision: asset.revision,
        subtitle: asset.summary.clone(),
        tags,
        updated_at: asset.updated_at,
    }
}
