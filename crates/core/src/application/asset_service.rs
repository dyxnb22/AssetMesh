//! Asset-level use cases: archive, explicit merge, and external references.
//!
//! Merge semantics follow ADR 0005: merges are explicit application
//! operations, the losing identity stays explainable via a `merged_into`
//! redirect, and uncertain matches are never merged silently.

use crate::application::media_service::update_projection;
use crate::application::service_service::update_service_projection;
use crate::application::shared::load_active_asset;
use crate::application::software_service::update_software_projection;
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::{Asset, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::ports::uow::{UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct AssetView {
    pub asset: Asset,
    pub external_refs: Vec<AssetExternalRef>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AssetService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    #[allow(dead_code)]
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> AssetService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        AssetService {
            factory,
            clock,
            ids,
        }
    }

    pub fn get_asset(&mut self, asset_id: AssetId) -> AppResult<AssetView> {
        self.factory.read(&mut |q| {
            let asset = q
                .assets()
                .get(asset_id)?
                .ok_or_else(|| AppError::not_found("asset", asset_id))?;
            let external_refs = q.external_refs().list_for_asset(asset_id)?;
            let tags = q
                .tags()
                .list_for_asset(asset_id)?
                .into_iter()
                .map(|t| t.name)
                .collect();
            Ok(AssetView {
                asset,
                external_refs,
                tags,
            })
        })
    }

    /// Archives an active asset. Archived assets reject normal mutations but
    /// remain searchable and exported.
    pub fn archive_asset(&mut self, asset_id: AssetId) -> AppResult<()> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = load_active_asset(uow, asset_id)?;
            asset.archive(now);
            uow.assets().update(&asset)?;
            uow.activity().append(&ActivityEvent::new(
                event_types::ASSET_ARCHIVED,
                Some(asset_id),
                actors::USER,
                json!({}),
                now,
            ))?;
            // Keep the projection fresh; archived assets stay searchable.
            // Storage failures propagate — they must not be silently
            // swallowed while the canonical change commits.
            refresh_projection(uow, &asset)?;
            Ok(())
        })
    }

    /// Explicitly merges `loser` into `winner`.
    ///
    /// Phase 2 behavior (full conflict UI is deferred, see DEVELOPMENT.md):
    /// - both assets must exist; the loser may be active or archived (never
    ///   already merged); the winner must be active; kinds must match;
    /// - external refs move to the winner; pairs the winner already owns are
    ///   dropped from the loser as redundant duplicates;
    /// - tags are unioned;
    /// - module details (media, software) move if the winner has none; if
    ///   both exist the survivor wins and the loser's record is preserved
    ///   inside the `asset.merged` activity payload for traceability;
    /// - service details move if the winner has none, but a merge of two
    ///   assets that BOTH carry a ServiceRecord is refused: two same-type
    ///   records can still conflict on provider/plan/cost/renewal, and
    ///   docs/10 forbids silently choosing a survivor. Field-level conflict
    ///   review is Phase 3C, so the merge fails loudly and leaves both
    ///   records intact rather than discarding one;
    /// - relations touching the loser are re-pointed at the winner, or
    ///   dropped when that would duplicate the winner's own relations or
    ///   connect the winner to itself;
    /// - the loser becomes a `merged` tombstone pointing at the winner;
    /// - collections/attachments do not exist yet, so their merge handling
    ///   remains a documented deferral (ADR 0005).
    pub fn merge_assets(&mut self, loser_id: AssetId, winner_id: AssetId) -> AppResult<()> {
        let now = self.clock.now();

        if loser_id == winner_id {
            return Err(AppError::validation("cannot merge an asset into itself"));
        }

        self.factory.transact(&mut |uow| {
            // The loser may be active or archived (archiving a duplicate and
            // merging it later is a legitimate flow); it must not already be
            // merged. The survivor must be active.
            let mut loser = uow
                .assets()
                .get(loser_id)?
                .ok_or_else(|| AppError::not_found("asset", loser_id))?;
            if loser.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
                return Err(AppError::conflict(
                    "asset is already merged and cannot be merged again",
                ));
            }
            let winner = load_active_asset(uow, winner_id)?;

            if loser.kind != winner.kind {
                return Err(AppError::conflict(format!(
                    "cannot merge assets of different kinds: {} vs {}",
                    loser.kind, winner.kind
                )));
            }

            // External references: move or drop as redundant duplicates.
            for reference in uow.external_refs().list_for_asset(loser_id)? {
                let target = uow
                    .external_refs()
                    .find_asset_by_ref(&reference.namespace, &reference.external_id)?;
                match target {
                    Some(existing) if existing == winner_id => {
                        // Winner already owns this alias; loser's copy is redundant.
                        uow.external_refs().delete(reference.id)?;
                    }
                    _ => {
                        let mut moved = reference;
                        moved.asset_id = winner_id;
                        moved.updated_at = now;
                        uow.external_refs().update(&moved)?;
                    }
                }
            }

            // Tags: union into winner, detach from loser.
            for tag in uow.tags().list_for_asset(loser_id)? {
                uow.tags().attach(winner_id, tag.id)?;
                uow.tags().detach(loser_id, tag.id)?;
            }

            // Media details: move, or survivor-wins with provenance.
            let loser_record = uow.media().get(loser_id)?;
            let winner_record = uow.media().get(winner_id)?;
            let mut loser_media_json: Option<serde_json::Value> = None;
            let mut loser_software_json: Option<serde_json::Value> = None;
            match (loser_record, winner_record) {
                (Some(loser_rec), Some(_winner_rec)) => {
                    // Preserve the loser's details in the activity payload;
                    // serialization failure propagates instead of storing a
                    // lossy placeholder.
                    loser_media_json = Some(serde_json::to_value(&loser_rec).map_err(|e| {
                        AppError::storage(format!("failed to serialize merged media details: {e}"))
                    })?);
                    uow.media().delete(loser_id)?;
                }
                (Some(loser_rec), None) => {
                    let mut moved = loser_rec;
                    moved.asset_id = winner_id;
                    uow.media().delete(loser_id)?;
                    uow.media().upsert(&moved)?;
                }
                (None, _) => {}
            }

            // Software details: same rules as media details.
            let loser_software = uow.software().get(loser_id)?;
            let winner_software = uow.software().get(winner_id)?;
            match (loser_software, winner_software) {
                (Some(loser_rec), Some(_winner_rec)) => {
                    loser_software_json = Some(serde_json::to_value(&loser_rec).map_err(|e| {
                        AppError::storage(format!(
                            "failed to serialize merged software details: {e}"
                        ))
                    })?);
                    uow.software().delete(loser_id)?;
                }
                (Some(loser_rec), None) => {
                    let mut moved = loser_rec;
                    moved.asset_id = winner_id;
                    uow.software().delete(loser_id)?;
                    uow.software().upsert(&moved)?;
                }
                (None, _) => {}
            }

            // Service details: the kind-equality check above guarantees both
            // records are the same ServiceType (docs/10 merge rule 1), but two
            // records of one type still disagree on provider, plan, cost, or
            // renewal, and rules 3/6 forbid silently choosing a survivor. The
            // field-level conflict review that would resolve them is Phase 3C,
            // so a merge of two service assets is refused outright rather than
            // winner-takes-all — the whole transaction rolls back and both
            // records survive untouched. A record still moves when the winner
            // has none (rule 2).
            let loser_service = uow.services().get(loser_id)?;
            let winner_service = uow.services().get(winner_id)?;
            match (loser_service, winner_service) {
                (Some(_loser_rec), Some(winner_rec)) => {
                    return Err(AppError::conflict(format!(
                        "cannot merge service {loser} into {winner}: both carry a {ty} service \
                         record, and merging two service records needs explicit conflict review \
                         (provider, plan, cost, renewal) which is not implemented yet; reconcile \
                         the records and delete one asset first",
                        loser = loser_id,
                        winner = winner_id,
                        ty = winner_rec.service_type
                    )));
                }
                (Some(loser_rec), None) => {
                    let mut moved = loser_rec;
                    moved.asset_id = winner_id;
                    uow.services().delete(loser_id)?;
                    uow.services().upsert(&moved)?;
                }
                (None, _) => {}
            }

            // Relations: re-point the loser endpoint at the winner, or drop
            // the relation when the winner is already connected (or the
            // relation connected the merge pair itself).
            for relation in uow.relations().list_for_asset(loser_id)? {
                let other = if relation.source_asset_id == loser_id {
                    relation.target_asset_id
                } else {
                    relation.source_asset_id
                };
                uow.relations().delete(relation.id)?;
                if other == winner_id {
                    continue; // would connect the winner to itself
                }
                let mut moved = relation;
                if moved.source_asset_id == loser_id {
                    moved.source_asset_id = winner_id;
                } else {
                    moved.target_asset_id = winner_id;
                }
                moved = moved.canonical_form();
                let duplicate =
                    uow.relations()
                        .list_for_asset(winner_id)?
                        .into_iter()
                        .any(|existing| {
                            existing.relation_type == moved.relation_type
                                && ((existing.source_asset_id == moved.source_asset_id
                                    && existing.target_asset_id == moved.target_asset_id)
                                    || (moved.relation_type.is_symmetric()
                                        && existing.source_asset_id == moved.target_asset_id
                                        && existing.target_asset_id == moved.source_asset_id))
                        });
                if !duplicate {
                    uow.relations().insert(&moved)?;
                }
            }

            // Tombstone the loser; identity stays explainable (ADR 0005).
            loser.mark_merged(winner_id, now);
            loser.validate()?;
            uow.assets().update(&loser)?;

            let mut winner_mut = winner;
            winner_mut.touch(now);
            uow.assets().update(&winner_mut)?;

            let mut payload = json!({
                "loser_id": loser_id.to_string(),
                "loser_name": loser.name.clone(),
            });
            if let Some(details) = loser_media_json {
                payload["loser_media_details"] = details;
            }
            if let Some(details) = loser_software_json {
                payload["loser_software_details"] = details;
            }
            // No `loser_service_details` payload: the only arm that would
            // produce one (both sides carrying a record) is refused above, so
            // there is never a discarded service record to preserve. Phase 3C
            // conflict resolution will reinstate this when it can resolve
            // field-level conflicts instead of refusing.
            uow.activity().append(&ActivityEvent::new(
                event_types::ASSET_MERGED,
                Some(winner_id),
                actors::USER,
                payload,
                now,
            ))?;

            // Projection: drop the loser, refresh the winner.
            uow.search_index().remove(loser_id)?;
            refresh_projection(uow, &winner_mut)?;
            Ok(())
        })
    }

    /// Attaches a namespaced external reference to an active asset.
    pub fn attach_external_ref(
        &mut self,
        asset_id: AssetId,
        namespace: &str,
        external_id: &str,
        source_url: Option<String>,
    ) -> AppResult<()> {
        let now = self.clock.now();
        let reference = AssetExternalRef::new(asset_id, namespace, external_id, source_url, now);
        reference.validate()?;

        self.factory.transact(&mut |uow| {
            let asset = load_active_asset(uow, asset_id)?;
            if let Some(existing) = uow
                .external_refs()
                .find_asset_by_ref(&reference.namespace, &reference.external_id)?
            {
                if existing == asset_id {
                    return Err(AppError::conflict(
                        "this external ref is already attached to the asset",
                    ));
                }
                return Err(AppError::conflict(format!(
                    "external ref {}:{} is already attached to asset {}",
                    reference.namespace, reference.external_id, existing
                )));
            }
            uow.external_refs().insert(&reference)?;

            // Refs feed search keywords, so refresh the projection.
            refresh_projection(uow, &asset)?;
            Ok(())
        })
    }

    pub fn remove_external_ref(
        &mut self,
        asset_id: AssetId,
        namespace: &str,
        external_id: &str,
    ) -> AppResult<()> {
        self.factory.transact(&mut |uow| {
            let asset = load_active_asset(uow, asset_id)?;
            let references = uow.external_refs().list_for_asset(asset_id)?;
            let reference = references
                .into_iter()
                .find(|r| r.namespace == namespace && r.external_id == external_id)
                .ok_or_else(|| {
                    AppError::not_found("external ref", format!("{namespace}:{external_id}"))
                })?;
            uow.external_refs().delete(reference.id)?;

            refresh_projection(uow, &asset)?;
            Ok(())
        })
    }
}

/// Refreshes the search projection of one asset from whichever module owns
/// its details, inside the caller's transaction. Assets without module
/// details lose any stale document.
pub(crate) fn refresh_projection(uow: &mut dyn UnitOfWork, asset: &Asset) -> AppResult<()> {
    if asset.lifecycle_state == LifecycleState::Merged {
        uow.search_index().remove(asset.id)?;
        return Ok(());
    }
    if let Some(record) = uow.media().get(asset.id)? {
        update_projection(uow, asset, &record)?;
    } else if let Some(record) = uow.software().get(asset.id)? {
        update_software_projection(uow, asset, &record)?;
    } else if let Some(record) = uow.services().get(asset.id)? {
        update_service_projection(uow, asset, &record)?;
    } else {
        uow.search_index().remove(asset.id)?;
    }
    Ok(())
}
