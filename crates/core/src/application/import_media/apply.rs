//! Bounded transaction batches and canonical create/update policies.
use super::matching::PlannedAction;
use crate::application::import_parse::ImportCandidate;
use crate::application::media_service::update_projection;
use crate::application::shared::ensure_ref_available;
use crate::application::SharedIdGenerator;
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::media::{MediaRecord, MediaStatus, Progress};
use crate::ports::uow::{UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde_json::json;
use std::collections::HashSet;

/// Applies one batch of planned records in a single short transaction.
/// Returns (creates, updates, unchanged).
pub(super) fn commit_chunk<F: UnitOfWorkFactory>(
    factory: &mut F,
    ids: &SharedIdGenerator,
    chunk: &mut [(ImportCandidate, PlannedAction)],
    now: crate::domain::Timestamp,
) -> AppResult<(usize, usize, usize)> {
    let mut creates = 0usize;
    let mut updates = 0usize;
    let mut unchanged = 0usize;

    factory.transact(&mut |uow| {
        for (candidate, action) in chunk.iter_mut() {
            match action {
                PlannedAction::Create => {
                    commit_create(uow, ids, candidate, now)?;
                    creates += 1;
                }
                PlannedAction::Update {
                    asset_id,
                    matched_by,
                } => {
                    let changed = commit_update(uow, candidate, *asset_id, matched_by, now)?;
                    if changed {
                        updates += 1;
                    } else {
                        unchanged += 1;
                    }
                }
                PlannedAction::Conflict { .. } => {
                    // Heuristic/ambiguous matches are never committed.
                }
            }
        }
        Ok(())
    })?;

    Ok((creates, updates, unchanged))
}

fn commit_create(
    uow: &mut dyn UnitOfWork,
    ids: &SharedIdGenerator,
    candidate: &mut ImportCandidate,
    now: crate::domain::Timestamp,
) -> AppResult<()> {
    let asset_id = candidate
        .planned_asset_id
        .unwrap_or_else(|| AssetId::from_uuid(ids.new_id()));
    candidate.planned_asset_id = Some(asset_id);

    let status = candidate.status.unwrap_or(MediaStatus::Planned);
    let mut record = MediaRecord::new(asset_id, candidate.media_type, status);
    record.rating = candidate.rating;
    record.year = candidate.year;
    record.platform = candidate.platform.clone();
    record.progress = candidate.progress.clone();
    record.notes = candidate.notes.clone();
    record.started_at = candidate.started_at;
    record.completed_at = candidate.completed_at;
    record.validate()?;

    let asset = Asset::new(
        asset_id,
        candidate.media_type.asset_kind(),
        candidate.title.clone(),
        candidate.summary.clone(),
        now,
    )?;

    // Ownerless refs become real references owned by the created asset.
    let mut owned_refs = Vec::with_capacity(candidate.refs.len());
    for reference in &candidate.refs {
        let owned = AssetExternalRef::new(
            asset_id,
            reference.namespace.clone(),
            reference.external_id.clone(),
            reference.source_url.clone(),
            now,
        );
        owned.validate()?;
        ensure_ref_available(uow, &owned)?;
        owned_refs.push(owned);
    }

    uow.assets().insert(&asset)?;
    uow.media().upsert(&record)?;

    let mut tags = Vec::new();
    for name in &candidate.tags {
        let tag = uow.tags().ensure(name)?;
        uow.tags().attach(asset_id, tag.id)?;
        tags.push(tag);
    }
    for reference in &owned_refs {
        uow.external_refs().insert(reference)?;
    }

    uow.activity().append(&ActivityEvent::new(
        event_types::ASSET_CREATED,
        Some(asset_id),
        actors::IMPORT,
        json!({ "kind": asset.kind.as_str(), "name": asset.name.clone() }),
        now,
    ))?;
    uow.activity().append(&ActivityEvent::new(
        event_types::MEDIA_CREATED,
        Some(asset_id),
        actors::IMPORT,
        json!({
            "media_type": record.media_type.as_str(),
            "status": record.status.as_str(),
        }),
        now,
    ))?;

    update_projection(uow, &asset, &record)?;
    Ok(())
}

/// Applies an update plan. Field policy: imported `Some` values overwrite,
/// `None` keeps the existing value; `progress.current` is the one monotonic
/// exception (see [`merge_progress`]); tags and refs are unioned. Returns
/// whether anything actually changed (idempotent re-import). A refused
/// progress merge still counts as a change so the refusal reaches the
/// activity event instead of disappearing.
fn commit_update(
    uow: &mut dyn UnitOfWork,
    candidate: &ImportCandidate,
    asset_id: AssetId,
    matched_by: &str,
    now: crate::domain::Timestamp,
) -> AppResult<bool> {
    let mut asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    asset.ensure_mutable()?;
    let mut record = uow
        .media()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("media record", asset_id))?;

    let mut changes: Vec<String> = Vec::new();

    if asset.name != candidate.title {
        asset.name = candidate.title.clone();
        changes.push("title".into());
    }
    if candidate.summary.is_some() && asset.summary != candidate.summary {
        asset.summary = candidate.summary.clone();
        changes.push("summary".into());
    }
    if let Some(year) = candidate.year {
        if record.year != Some(year) {
            record.year = Some(year);
            changes.push("year".into());
        }
    }
    if candidate.platform.is_some() && record.platform != candidate.platform {
        record.platform = candidate.platform.clone();
        changes.push("platform".into());
    }
    if candidate.notes.is_some() && record.notes != candidate.notes {
        record.notes = candidate.notes.clone();
        changes.push("notes".into());
    }
    if let Some(status) = candidate.status {
        if record.status != status {
            record.status = status;
            changes.push(format!("status:{}", status.as_str()));
        }
    }
    if candidate.rating.is_some() && record.rating != candidate.rating {
        record.rating = candidate.rating;
        changes.push("rating".into());
    }
    // Progress is merged by partial order, not last-write-wins.
    merge_progress(&mut record.progress, &candidate.progress, &mut changes);
    if candidate.started_at.is_some() && record.started_at != candidate.started_at {
        record.started_at = candidate.started_at;
        changes.push("started_at".into());
    }
    if candidate.completed_at.is_some() && record.completed_at != candidate.completed_at {
        record.completed_at = candidate.completed_at;
        changes.push("completed_at".into());
    }

    record.validate()?;

    let existing_tags: HashSet<String> = uow
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|t| t.name)
        .collect();
    for name in &candidate.tags {
        if !existing_tags.iter().any(|t| t.eq_ignore_ascii_case(name)) {
            let tag = uow.tags().ensure(name)?;
            uow.tags().attach(asset_id, tag.id)?;
            changes.push(format!("tag:{name}"));
        }
    }

    let existing_refs: HashSet<(String, String)> = uow
        .external_refs()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|r| (r.namespace.clone(), r.external_id.clone()))
        .collect();
    for reference in &candidate.refs {
        let key = (reference.namespace.clone(), reference.external_id.clone());
        if !existing_refs.contains(&key) {
            // Ownerless ref is owned by the matched asset on the update path
            // as well — never a placeholder ID (P1: FK-safe enrichment).
            let owned = AssetExternalRef::new(
                asset_id,
                reference.namespace.clone(),
                reference.external_id.clone(),
                reference.source_url.clone(),
                now,
            );
            owned.validate()?;
            ensure_ref_available(uow, &owned)?;
            uow.external_refs().insert(&owned)?;
            changes.push(format!(
                "ref:{}:{}",
                reference.namespace, reference.external_id
            ));
        }
    }

    if changes.is_empty() {
        return Ok(false);
    }

    asset.touch(now);
    uow.assets().update(&asset)?;
    uow.media().upsert(&record)?;
    uow.activity().append(&ActivityEvent::new(
        event_types::MEDIA_IMPORTED,
        Some(asset_id),
        actors::IMPORT,
        json!({ "matched_by": matched_by, "changes": changes }),
        now,
    ))?;
    update_projection(uow, &asset, &record)?;
    Ok(true)
}

/// Merges imported progress into stored progress.
///
/// Every other imported field is last-write-wins, which is wrong for a
/// counter: a re-exported list can be older than the library, so applying
/// `3/28` over a stored `8/28` would lose viewing history that only the user
/// has. `current` is therefore merged by maximum, and `total` — a fact about
/// the work rather than the user — stays correctable in either direction.
///
/// A merge that would break the `current <= total` invariant, or that would
/// leave a unit without any number, is refused as a whole instead of being
/// clamped: clamping silently rewrites the user's count. The refusal is
/// reported as a change so it lands in the activity event.
fn merge_progress(existing: &mut Progress, incoming: &Progress, changes: &mut Vec<String>) {
    if incoming.is_empty() {
        return;
    }

    let current = match (existing.current, incoming.current) {
        (Some(stored), Some(proposed)) => Some(stored.max(proposed)),
        (stored, proposed) => stored.or(proposed),
    };
    let total = incoming.total.or(existing.total);

    let refused = match (current, total) {
        (Some(current), Some(total)) if current > total => Some("current_exceeds_total"),
        _ if current.is_none() && total.is_none() && incoming.unit.is_some() => {
            Some("unit_without_value")
        }
        _ => None,
    };
    if let Some(reason) = refused {
        changes.push(format!("progress_merge_refused:{reason}"));
        return;
    }

    if current != existing.current {
        changes.push("progress_current".into());
    }
    if total != existing.total {
        changes.push("progress_total".into());
    }
    if incoming.unit.is_some() && incoming.unit != existing.unit {
        changes.push("progress_unit".into());
    }

    existing.current = current;
    existing.total = total;
    if incoming.unit.is_some() {
        existing.unit = incoming.unit.clone();
    }
}
