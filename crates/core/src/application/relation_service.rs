//! Relation use cases: minimal shared infrastructure Phase 2 actually needs
//! (docs/03 relation registry, ADR 0005). Relations connect shared assets;
//! no module-private table coupling. The Phase 4 relation query/explorer
//! layer stays out of scope here.

use crate::application::shared::load_active_asset;
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::ids::{AssetId, RelationId};
use crate::domain::relation::{Relation, RelationProvenance, RelationType};
use crate::domain::Timestamp;
use crate::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde_json::json;

/// A relation as seen from one asset: the effective relation type already
/// resolves inverse/symmetric semantics, so adapters never re-derive them.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RelationView {
    pub relation_id: RelationId,
    pub other_asset_id: AssetId,
    pub other_asset_name: String,
    /// Relation type as read from this asset's perspective.
    pub relation_type: RelationType,
    /// True when this asset is the stored source of the row.
    pub outgoing: bool,
    pub note: Option<String>,
    pub provenance: RelationProvenance,
    pub created_at: Timestamp,
}

pub struct RelationService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> RelationService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        RelationService {
            factory,
            clock,
            ids,
        }
    }

    /// Attaches a relation between two active assets. The relation is stored
    /// in canonical form (inverse-pair types collapse onto their primary
    /// direction, symmetric types in canonical endpoint order), so stating
    /// the same fact from either side — `A depends_on B` or
    /// `B dependency_of A` — resolves to the same row and is reported as a
    /// conflict instead of duplicated.
    pub fn attach(
        &mut self,
        source: AssetId,
        relation_type: RelationType,
        target: AssetId,
        note: Option<String>,
        provenance: RelationProvenance,
    ) -> AppResult<Relation> {
        let now = self.clock.now();
        let note = crate::domain::validation::optional_text(&note, "note")?;
        let relation_id = RelationId::from_uuid(self.ids.new_id());

        self.factory.transact(&mut |uow| {
            load_active_asset(uow, source)?;
            load_active_asset(uow, target)?;
            let mut relation = Relation::new(
                relation_id,
                source,
                target,
                relation_type,
                provenance,
                now,
            )?
            .canonical_form();
            relation.note = note.clone();

            if find_duplicate(uow, &relation)?.is_some() {
                return Err(AppError::conflict(format!(
                    "relation {} between {} and {} already exists: relations store one canonical row per fact, so stating it via the inverse type is the same fact",
                    relation.relation_type, relation.source_asset_id, relation.target_asset_id
                )));
            }

            uow.relations().insert(&relation)?;
            uow.activity().append(&ActivityEvent::new(
                event_types::RELATION_CREATED,
                Some(relation.source_asset_id),
                actors::USER,
                json!({
                    "relation_id": relation.id.to_string(),
                    "relation_type": relation.relation_type.as_str(),
                    "source_asset_id": relation.source_asset_id.to_string(),
                    "target_asset_id": relation.target_asset_id.to_string(),
                }),
                now,
            ))?;
            Ok(relation)
        })
    }

    /// Every relation touching `asset_id`, with inverse semantics resolved
    /// from that asset's perspective.
    pub fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<RelationView>> {
        self.factory.read(&mut |q| list_views(q, asset_id))
    }

    pub fn remove(&mut self, relation_id: RelationId) -> AppResult<()> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let relation = uow
                .relations()
                .get(relation_id)?
                .ok_or_else(|| AppError::not_found("relation", relation_id))?;
            uow.relations().delete(relation_id)?;
            uow.activity().append(&ActivityEvent::new(
                event_types::RELATION_REMOVED,
                Some(relation.source_asset_id),
                actors::USER,
                json!({
                    "relation_type": relation.relation_type.as_str(),
                    "source_asset_id": relation.source_asset_id.to_string(),
                    "target_asset_id": relation.target_asset_id.to_string(),
                }),
                now,
            ))?;
            Ok(())
        })
    }
}

/// Finds the duplicate of a canonical-form relation. All stored rows are in
/// canonical form (enforced by every write path and by the SQLite CHECK
/// restricting stored types to the canonical set), so duplicates are exactly
/// equal triples.
fn find_duplicate(uow: &mut dyn UnitOfWork, relation: &Relation) -> AppResult<Option<Relation>> {
    for existing in uow.relations().list_for_asset(relation.source_asset_id)? {
        let same = existing.source_asset_id == relation.source_asset_id
            && existing.target_asset_id == relation.target_asset_id
            && existing.relation_type == relation.relation_type;
        if same {
            return Ok(Some(existing));
        }
    }
    Ok(None)
}

pub(crate) fn list_views(
    q: &mut dyn QueryUnitOfWork,
    asset_id: AssetId,
) -> AppResult<Vec<RelationView>> {
    let mut views = Vec::new();
    for relation in q.relations().list_for_asset(asset_id)? {
        let other = if relation.source_asset_id == asset_id {
            relation.target_asset_id
        } else {
            relation.source_asset_id
        };
        let other_asset = q
            .assets()
            .get(other)?
            .ok_or_else(|| AppError::not_found("asset", other))?;
        views.push(RelationView {
            relation_id: relation.id,
            other_asset_id: other,
            other_asset_name: other_asset.name,
            relation_type: relation
                .relation_type
                .effective_from(relation.source_asset_id, asset_id),
            outgoing: relation.source_asset_id == asset_id,
            note: relation.note.clone(),
            provenance: relation.provenance,
            created_at: relation.created_at,
        });
    }
    views.sort_by(|a, b| {
        (&a.relation_type.as_str(), &a.other_asset_name)
            .cmp(&(&b.relation_type.as_str(), &b.other_asset_name))
    });
    Ok(views)
}
