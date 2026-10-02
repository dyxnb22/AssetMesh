//! Destination conflict checks and dispositions, shared by dry-run and commit.
use super::import::PortableImportReport;
use super::validation::DecodedBundle;
use crate::domain::asset::AssetKind;
use crate::domain::ids::{ActivityId, AssetId, ExternalRefId, RelationId, TagId};
use crate::ports::repos::{
    ActivityReader, AssetReader, ExternalRefReader, MediaReader, RelationReader, ServiceReader,
    SoftwareReader, TagReader,
};
use crate::{AppError, AppResult};
use std::collections::{HashMap, HashSet};

/// One consistent snapshot of the destination, taken before commit or
/// dry-run. Both compute their dispositions from the same snapshot shape, so
/// dry-run is field-for-field identical to what commit would do.
pub(super) struct DestinationSnapshot {
    pub(super) asset_ids: HashSet<AssetId>,
    /// Kinds of destination assets, so an import can detect a cross-module
    /// re-typing that would strand a module record (e.g. a bundle declaring a
    /// media.* kind over an asset that carries a software record).
    asset_kinds: HashMap<AssetId, AssetKind>,
    media_asset_ids: HashSet<AssetId>,
    software_asset_ids: HashSet<AssetId>,
    /// Services detail ids, so a re-type away from a service.* kind is caught
    /// here rather than only at the repository boundary during commit.
    service_asset_ids: HashSet<AssetId>,
    info_asset_ids: HashSet<AssetId>,
    pub(super) ref_pairs: HashMap<(String, String), AssetId>,
    ref_ids: HashMap<ExternalRefId, (String, String)>,
    pub(super) activity_ids: HashSet<ActivityId>,
    tag_ids: HashSet<TagId>,
    tag_names: HashSet<String>,
    pub(super) relation_ids: HashSet<RelationId>,
    /// Canonical relation triples keyed for duplicate/conflict checks,
    /// including the mirror-image key of symmetric types.
    relation_triples: HashMap<(String, String, String), RelationId>,
}

impl DestinationSnapshot {
    pub(super) fn load<R: DestinationRead>(
        mut readers: R,
        incoming: &DecodedBundle,
    ) -> AppResult<Self> {
        let ids: Vec<_> = incoming.assets.iter().map(|asset| asset.id).collect();
        let assets = readers.assets().get_many(&ids)?;
        let asset_ids = assets.iter().map(|asset| asset.id).collect();
        let asset_kinds = assets
            .into_iter()
            .map(|asset| (asset.id, asset.kind))
            .collect();
        let media_asset_ids = readers.media().existing_ids(&ids)?.into_iter().collect();
        let software_asset_ids = readers.software().existing_ids(&ids)?.into_iter().collect();
        let service_asset_ids = readers.services().existing_ids(&ids)?.into_iter().collect();
        let info_asset_ids = readers.info().existing_ids(&ids)?.into_iter().collect();
        let mut ref_pairs = HashMap::new();
        let mut ref_ids = HashMap::new();
        for reference in &incoming.refs {
            if let Some(owner) = readers
                .external_refs()
                .find_asset_by_ref(&reference.namespace, &reference.external_id)?
            {
                ref_pairs.insert(
                    (reference.namespace.clone(), reference.external_id.clone()),
                    owner,
                );
            }
            if let Some(existing) = readers.external_refs().get(reference.id)? {
                ref_ids.insert(existing.id, (existing.namespace, existing.external_id));
            }
        }
        let mut activity_ids = HashSet::new();
        for event in &incoming.activity {
            if readers.activity().get(event.id)?.is_some() {
                activity_ids.insert(event.id);
            }
        }
        let mut tag_ids = HashSet::new();
        let mut tag_names = HashSet::new();
        for tag in &incoming.tags {
            if readers.tags().get(tag.id)?.is_some() {
                tag_ids.insert(tag.id);
            }
            if readers.tags().find_by_name(&tag.name)?.is_some() {
                tag_names.insert(tag.name.clone());
            }
        }
        let mut relation_ids = HashSet::new();
        let mut relation_triples = HashMap::new();
        for relation in &incoming.relations {
            if readers.relations().get(relation.id)?.is_some() {
                relation_ids.insert(relation.id);
            }
            if let Some(existing) = readers.relations().find(
                relation.source_asset_id,
                relation.target_asset_id,
                relation.relation_type,
            )? {
                relation_triples.insert(
                    (
                        existing.source_asset_id.to_string(),
                        existing.target_asset_id.to_string(),
                        existing.relation_type.as_str().to_string(),
                    ),
                    existing.id,
                );
            }
        }
        Ok(DestinationSnapshot {
            asset_ids,
            asset_kinds,
            media_asset_ids,
            software_asset_ids,
            service_asset_ids,
            info_asset_ids,
            ref_pairs,
            ref_ids,
            activity_ids,
            tag_ids,
            tag_names,
            relation_ids,
            relation_triples,
        })
    }
}

/// Read access to the destination tables.
///
/// Implemented for both the read scope (`QueryUnitOfWork`) and the write
/// scope (`UnitOfWork`): the snapshot must be takeable *inside* the commit
/// transaction as well as from a read-only scope, and duplicating the loader
/// body for the two cases would let them drift. A write scope upcasts to its
/// reader supertraits — every repository trait extends its reader trait.
pub(super) trait DestinationRead {
    fn assets(&mut self) -> &mut dyn AssetReader;
    fn media(&mut self) -> &mut dyn MediaReader;
    fn software(&mut self) -> &mut dyn SoftwareReader;
    fn services(&mut self) -> &mut dyn ServiceReader;
    fn info(&mut self) -> &mut dyn crate::ports::repos::InfoReader;
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader;
    fn activity(&mut self) -> &mut dyn ActivityReader;
    fn tags(&mut self) -> &mut dyn TagReader;
    fn relations(&mut self) -> &mut dyn RelationReader;
}

/// Thin newtypes over the two scopes. Passing `&mut dyn UnitOfWork` directly
/// would freeze the borrow for the whole transaction (the trait object's
/// lifetime bound is the outer borrow); a wrapper gives the loader a lifetime
/// of its own so the transaction keeps mutating after the snapshot is built.
pub(super) struct ReadScope<'a>(pub(super) &'a mut dyn crate::ports::uow::QueryUnitOfWork);
pub(super) struct WriteScope<'a>(pub(super) &'a mut dyn crate::ports::uow::UnitOfWork);

impl DestinationRead for ReadScope<'_> {
    fn assets(&mut self) -> &mut dyn AssetReader {
        self.0.assets()
    }
    fn media(&mut self) -> &mut dyn MediaReader {
        self.0.media()
    }
    fn software(&mut self) -> &mut dyn SoftwareReader {
        self.0.software()
    }
    fn services(&mut self) -> &mut dyn ServiceReader {
        self.0.services()
    }
    fn info(&mut self) -> &mut dyn crate::ports::repos::InfoReader {
        self.0.info()
    }
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        self.0.external_refs()
    }
    fn activity(&mut self) -> &mut dyn ActivityReader {
        self.0.activity()
    }
    fn tags(&mut self) -> &mut dyn TagReader {
        self.0.tags()
    }
    fn relations(&mut self) -> &mut dyn RelationReader {
        self.0.relations()
    }
}

impl DestinationRead for WriteScope<'_> {
    fn assets(&mut self) -> &mut dyn AssetReader {
        self.0.assets()
    }
    fn media(&mut self) -> &mut dyn MediaReader {
        self.0.media()
    }
    fn software(&mut self) -> &mut dyn SoftwareReader {
        self.0.software()
    }
    fn services(&mut self) -> &mut dyn ServiceReader {
        self.0.services()
    }
    fn info(&mut self) -> &mut dyn crate::ports::repos::InfoReader {
        self.0.info()
    }
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        self.0.external_refs()
    }
    fn activity(&mut self) -> &mut dyn ActivityReader {
        self.0.activity()
    }
    fn tags(&mut self) -> &mut dyn TagReader {
        self.0.tags()
    }
    fn relations(&mut self) -> &mut dyn RelationReader {
        self.0.relations()
    }
}

/// Identity conflicts that make commit impossible — checked identically for
/// dry-run and commit, before any mutation.
pub(super) fn check_destination(
    decoded: &DecodedBundle,
    snapshot: &DestinationSnapshot,
) -> AppResult<()> {
    // An external ref must not be re-pointed at a different asset than it
    // already belongs to: that would silently rewrite identity (ADR 0005).
    for reference in &decoded.refs {
        if let Some(existing) = snapshot
            .ref_pairs
            .get(&(reference.namespace.clone(), reference.external_id.clone()))
        {
            if *existing != reference.asset_id {
                return Err(AppError::import_conflict(format!(
                    "external ref {}:{} is attached to asset {}, but the bundle attaches it to {}",
                    reference.namespace, reference.external_id, existing, reference.asset_id
                )));
            }
        }
        // A ref row id already in the destination but carrying a different
        // pair would violate the primary key at commit.
        if let Some((namespace, external_id)) = snapshot.ref_ids.get(&reference.id) {
            if *namespace != reference.namespace || *external_id != reference.external_id {
                return Err(AppError::import_conflict(format!(
                    "external ref id {} already exists in the destination as {}:{}",
                    reference.id, namespace, external_id
                )));
            }
        }
    }
    // A bundled relation must not overwrite a destination relation between
    // the same endpoints with a different identity.
    for relation in &decoded.relations {
        let triple = (
            relation.source_asset_id.to_string(),
            relation.target_asset_id.to_string(),
            relation.relation_type.as_str().to_string(),
        );
        if let Some(existing) = snapshot.relation_triples.get(&triple) {
            if *existing != relation.id {
                return Err(AppError::import_conflict(format!(
                    "relation {} between {} and {} already exists in the destination as {}",
                    relation.relation_type,
                    relation.source_asset_id,
                    relation.target_asset_id,
                    existing
                )));
            }
        }
    }
    // A bundle must not re-type an asset while the destination still carries
    // the old module's record — that would strand a software record on a
    // media.* asset (or the reverse), and the repository boundary rejects ANY
    // kind change while the row remains, within a module included
    // (media.movie -> media.game strands a movie record the same way). Caught
    // here so dry-run and commit fail identically, before any canonical
    // mutation; the stranded record belongs to the OLD module.
    for asset in &decoded.assets {
        let Some(&stored_kind) = snapshot.asset_kinds.get(&asset.id) else {
            continue;
        };
        if stored_kind == asset.kind {
            continue;
        }
        let has_old_record = match stored_kind.module() {
            "media" => snapshot.media_asset_ids.contains(&asset.id),
            "software" => snapshot.software_asset_ids.contains(&asset.id),
            "services" => snapshot.service_asset_ids.contains(&asset.id),
            "info" => snapshot.info_asset_ids.contains(&asset.id),
            _ => false,
        };
        if has_old_record {
            return Err(AppError::import_conflict(format!(
                "asset {} is a {} in the destination and the bundle re-types it as {}; remove the \
                 {} record on the destination before importing",
                asset.id,
                stored_kind,
                asset.kind,
                stored_kind.module()
            )));
        }
    }
    Ok(())
}

/// Computes the exact dispositions of an import from one destination
/// snapshot. Dry-run returns this directly; commit performs these writes.
pub(super) fn dispositions(
    decoded: &DecodedBundle,
    snapshot: &DestinationSnapshot,
) -> PortableImportReport {
    let mut report = PortableImportReport::default();
    for asset in &decoded.assets {
        if snapshot.asset_ids.contains(&asset.id) {
            report.assets_updated += 1;
        } else {
            report.assets_created += 1;
        }
    }
    // Judged by module-record existence, not asset existence: an asset can
    // be present in the destination while its module details are not.
    for record in &decoded.media {
        if snapshot.media_asset_ids.contains(&record.asset_id) {
            report.media_updated += 1;
        } else {
            report.media_created += 1;
        }
    }
    for record in &decoded.software {
        if snapshot.software_asset_ids.contains(&record.asset_id) {
            report.software_updated += 1;
        } else {
            report.software_created += 1;
        }
    }
    for record in &decoded.services {
        if snapshot.service_asset_ids.contains(&record.asset_id) {
            report.services_updated += 1;
        } else {
            report.services_created += 1;
        }
    }
    for record in &decoded.info {
        if snapshot.info_asset_ids.contains(&record.asset_id) {
            report.info_updated += 1;
        } else {
            report.info_created += 1;
        }
    }
    for reference in &decoded.refs {
        match snapshot
            .ref_pairs
            .get(&(reference.namespace.clone(), reference.external_id.clone()))
        {
            Some(_) => report.external_refs_deduplicated += 1,
            None => report.external_refs_created += 1,
        }
    }
    report.activity_created = decoded
        .activity
        .iter()
        .filter(|e| !snapshot.activity_ids.contains(&e.id))
        .count();
    report.tags_created = decoded
        .tags
        .iter()
        .filter(|t| !snapshot.tag_names.contains(&t.name) && !snapshot.tag_ids.contains(&t.id))
        .count();
    for relation in &decoded.relations {
        if snapshot.relation_ids.contains(&relation.id) {
            report.relations_updated += 1;
        } else {
            report.relations_created += 1;
        }
    }
    report
}
