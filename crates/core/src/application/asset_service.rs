//! Asset-level use cases: archive, explicit merge, and external references.
//!
//! Merge semantics follow ADR 0005: merges are explicit application
//! operations, the losing identity stays explainable via a `merged_into`
//! redirect, and uncertain matches are never merged silently.

use crate::application::media_service::{load_active_asset, update_projection};
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::ports::uow::UnitOfWorkFactory;
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
            if let Some(record) = uow.media().get(asset_id)? {
                update_projection(uow, &asset, &record)?;
            }
            Ok(())
        })
    }

    /// Explicitly merges `loser` into `winner`.
    ///
    /// Phase 1 behavior (full conflict UI is deferred, see DEVELOPMENT.md):
    /// - both assets must exist, be active, and share the same kind;
    /// - external refs move to the winner; pairs the winner already owns are
    ///   dropped from the loser as redundant duplicates;
    /// - tags are unioned;
    /// - if only the loser has media details they move to the winner; if both
    ///   exist the survivor's details win and the loser's record is preserved
    ///   inside the `asset.merged` activity payload for traceability;
    /// - the loser becomes a `merged` tombstone pointing at the winner;
    /// - relations/collections/attachments do not exist yet in Phase 1, so
    ///   their merge handling is a documented deferral (ADR 0005).
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
            let mut loser_details_json: Option<serde_json::Value> = None;
            match (loser_record, winner_record) {
                (Some(loser_rec), Some(_winner_rec)) => {
                    // Preserve the loser's details in the activity payload;
                    // serialization failure propagates instead of storing a
                    // lossy placeholder.
                    loser_details_json = Some(serde_json::to_value(&loser_rec).map_err(|e| {
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
            if let Some(details) = loser_details_json {
                payload["loser_media_details"] = details;
            }
            uow.activity().append(&ActivityEvent::new(
                event_types::ASSET_MERGED,
                Some(winner_id),
                actors::USER,
                payload,
                now,
            ))?;

            // Projection: drop the loser, refresh the winner.
            uow.search_index().remove(loser_id)?;
            if let Some(record) = uow.media().get(winner_id)? {
                update_projection(uow, &winner_mut, &record)?;
            }
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
            if let Some(record) = uow.media().get(asset_id)? {
                update_projection(uow, &asset, &record)?;
            }
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

            if let Some(record) = uow.media().get(asset_id)? {
                update_projection(uow, &asset, &record)?;
            }
            Ok(())
        })
    }
}
