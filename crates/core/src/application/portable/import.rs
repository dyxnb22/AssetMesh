//! Transactional import orchestration after bundle preflight.
use super::budget::MAX_BUNDLE_RECORDS;
use super::destination::{
    check_destination, dispositions, DestinationSnapshot, ReadScope, WriteScope,
};
use super::format::{id_from_wire, PortableBundle};
use super::validation::preflight;
use crate::domain::ids::{AssetId, RelationId, TagId};
use crate::ports::uow::UnitOfWorkFactory;
use crate::{AppError, AppResult};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PortableImportReport {
    pub assets_created: usize,
    pub assets_updated: usize,
    pub media_created: usize,
    pub media_updated: usize,
    pub software_created: usize,
    pub software_updated: usize,
    pub services_created: usize,
    pub services_updated: usize,
    pub info_created: usize,
    pub info_updated: usize,
    pub external_refs_created: usize,
    pub external_refs_deduplicated: usize,
    pub activity_created: usize,
    pub tags_created: usize,
    pub relations_created: usize,
    pub relations_updated: usize,
}

/// Fully decoded and preflighted bundle contents.
pub struct PortableImportService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> PortableImportService<F> {
    pub fn new(factory: F) -> Self {
        PortableImportService { factory }
    }

    /// Imports a portable bundle deterministically by canonical ID. The
    /// bundle is the source of truth for every asset it contains: existing
    /// rows with the same IDs are updated, and destination module state that
    /// the bundle no longer carries (e.g. a merged tombstone's old media
    /// details) is removed, keeping the destination equivalent to the bundle.
    /// Dry-run runs the identical preflight, destination checks, and
    /// disposition computation, so it fails exactly when commit would and
    /// reports field-for-field the same counts.
    pub fn import_bundle(
        &mut self,
        bundle: &PortableBundle,
        dry_run: bool,
    ) -> AppResult<PortableImportReport> {
        // Shared preflight — identical for dry-run and commit.
        let decoded = preflight(bundle)?;

        if dry_run {
            // Dry-run reads the destination without writing, so a read scope
            // is enough: it reports exactly what commit would do.
            let snapshot = self
                .factory
                .read(&mut |q| DestinationSnapshot::load(ReadScope(q), &decoded))?;
            check_destination(&decoded, &snapshot)?;
            return Ok(dispositions(&decoded, &snapshot));
        }

        // Commit. The destination snapshot and the conflict check run INSIDE
        // the write transaction, not before it: an IMMEDIATE transaction
        // holds the write lock from the snapshot through commit, so no other
        // connection can insert a conflicting row between "checked the
        // destination" and "wrote the rows". Taking the snapshot outside the
        // transaction leaves a check-then-act window the size of the whole
        // import — wide enough for a concurrent import or a running desktop
        // session to invalidate every identity check below.
        //
        // Commit. Assets first (self-referencing merged_into is resolved in
        // a second pass), then module details, refs, tags, activity,
        // relations, and finally module-state reconciliation + projections.
        self.factory.transact(&mut |uow| {
            let snapshot = DestinationSnapshot::load(WriteScope(uow), &decoded)?;
            check_destination(&decoded, &snapshot)?;
            let report = dispositions(&decoded, &snapshot);

            for asset in &decoded.assets {
                let mut staged = asset.clone();
                // Intermediate state for the two-pass insert: no redirect
                // target yet (FK) and no tombstone marker (validation).
                staged.merged_into = None;
                if staged.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
                    staged.lifecycle_state = crate::domain::asset::LifecycleState::Active;
                }
                if snapshot.asset_ids.contains(&asset.id) {
                    uow.assets().update(&staged)?;
                } else {
                    uow.assets().insert(&staged)?;
                }
            }
            for asset in &decoded.assets {
                if asset.merged_into.is_some() {
                    uow.assets().update(asset)?;
                }
            }

            for record in &decoded.media {
                uow.media().upsert(record)?;
            }
            for record in &decoded.software {
                uow.software().upsert(record)?;
            }
            for record in &decoded.services {
                uow.services().upsert(record)?;
            }
            for record in &decoded.info {
                uow.info().upsert(record)?;
            }

            for reference in &decoded.refs {
                let known = snapshot
                    .ref_pairs
                    .get(&(reference.namespace.clone(), reference.external_id.clone()));
                if known.is_some() {
                    continue; // same owner, same pair: deduplicated
                }
                uow.external_refs().insert(reference)?;
            }

            // Tags preserve their exported identity; a name collision with a
            // different id remaps membership onto the existing tag. The map is
            // keyed by canonical UUID, and the membership lookup goes through
            // the same wire parser: bundle writers are free to emit uppercase
            // hex, and the preflight a few lines below parses `pair.tag_id`
            // with `id_from_wire` too, so string-comparing raw wire text
            // against a lowercase key would miss and report a spurious
            // "unknown tag id" for a bundle that round-trips through
            // preflight cleanly.
            let mut tag_id_map: BTreeMap<Uuid, TagId> = BTreeMap::new();
            for tag in &decoded.tags {
                let resolved = if let Some(existing) = uow.tags().find_by_name(&tag.name)? {
                    existing.id
                } else if uow.tags().get(tag.id)?.is_some() {
                    uow.tags().update(tag)?;
                    tag.id
                } else {
                    uow.tags().insert(tag)?;
                    tag.id
                };
                tag_id_map.insert(tag.id.as_uuid(), resolved);
            }

            for pair in &decoded.asset_tags {
                let asset_id = AssetId::from_uuid(id_from_wire(&pair.asset_id, "asset_tags")?);
                let tag_wire = id_from_wire(&pair.tag_id, "asset_tags")?;
                let tag_id = tag_id_map.get(&tag_wire).copied().ok_or_else(|| {
                    AppError::validation(format!(
                        "asset_tags references unknown tag id {}",
                        pair.tag_id
                    ))
                })?;
                uow.tags().insert_membership(asset_id, tag_id)?;
            }

            for event in &decoded.activity {
                if snapshot.activity_ids.contains(&event.id) {
                    uow.activity().upsert(event)?;
                } else {
                    uow.activity().append(event)?;
                }
            }

            for relation in &decoded.relations {
                if snapshot.relation_ids.contains(&relation.id) {
                    uow.relations().update(relation)?;
                } else {
                    uow.relations().insert(relation)?;
                }
            }

            // Module-state reconciliation: the bundle is authoritative for
            // every asset it contains. An asset without module details in
            // the bundle (typically a merged tombstone) must not keep stale
            // destination details or a search document; a details-bearing
            // asset is projected from its ACTUAL committed state (post-remap
            // tags, destination refs), not from the bundle subset.
            // Undeclared (legacy) sections leave destination state of that
            // kind untouched.
            let bundled_relation_ids: HashSet<RelationId> =
                decoded.relations.iter().map(|r| r.id).collect();
            let imported_ids: Vec<_> = decoded.assets.iter().map(|asset| asset.id).collect();
            let imported_set: HashSet<_> = imported_ids.iter().copied().collect();
            let existing_relations = if decoded.relations_declared {
                uow.relations()
                    .list_for_assets_bounded(&imported_ids, MAX_BUNDLE_RECORDS + 1)?
            } else {
                Vec::new()
            };
            if existing_relations.len() > MAX_BUNDLE_RECORDS {
                return Err(AppError::validation(
                    "too many related destination rows for one import",
                ));
            }
            for existing in existing_relations {
                if !decoded.relations_declared || bundled_relation_ids.contains(&existing.id) {
                    continue;
                }
                // Relations among bundled assets that the bundle no longer
                // carries are removed; relations reaching outside the bundle
                // belong to destination-local assets and stay.
                if imported_set.contains(&existing.source_asset_id)
                    && imported_set.contains(&existing.target_asset_id)
                {
                    uow.relations().delete(existing.id)?;
                }
            }

            let media_ids: HashSet<_> = decoded.media.iter().map(|row| row.asset_id).collect();
            let software_ids: HashSet<_> =
                decoded.software.iter().map(|row| row.asset_id).collect();
            let service_ids: HashSet<_> = decoded.services.iter().map(|row| row.asset_id).collect();
            let info_ids: HashSet<_> = decoded.info.iter().map(|row| row.asset_id).collect();
            for asset in &decoded.assets {
                let bundled_media = media_ids.contains(&asset.id);
                let bundled_software =
                    decoded.software_declared && software_ids.contains(&asset.id);
                let bundled_service = decoded.services_declared && service_ids.contains(&asset.id);
                let bundled_info = decoded.info_declared && info_ids.contains(&asset.id);

                if !bundled_media {
                    uow.media().delete(asset.id)?;
                }
                if decoded.software_declared && !bundled_software {
                    uow.software().delete(asset.id)?;
                }
                if decoded.services_declared && !bundled_service {
                    uow.services().delete(asset.id)?;
                }
                if decoded.info_declared && !bundled_info {
                    uow.info().delete(asset.id)?;
                }

                if asset.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
                    uow.search_index().remove(asset.id)?;
                } else if bundled_media {
                    let record = uow
                        .media()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("media record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    let document =
                        crate::application::projection::project_media(asset, &record, &tags, &refs);
                    uow.search_index().upsert(&document)?;
                } else if bundled_software {
                    let record = uow
                        .software()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("software record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    let document = crate::application::projection::project_software(
                        asset, &record, &tags, &refs,
                    );
                    uow.search_index().upsert(&document)?;
                } else if bundled_service {
                    let record = uow
                        .services()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("service record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    let document = crate::application::projection::project_service(
                        asset, &record, &tags, &refs,
                    );
                    uow.search_index().upsert(&document)?;
                } else if bundled_info {
                    let record = uow
                        .info()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("information record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    uow.search_index()
                        .upsert(&crate::application::projection::project_info(
                            asset, &record, &tags, &refs,
                        ))?;
                } else {
                    uow.search_index().remove(asset.id)?;
                }
            }

            Ok(report)
        })
    }
}
