//! Relation Tauri command handlers — Phase 5 (P5-07).
//!
//! Exposes read-only queries and write operations from `RelationService` and
//! `RelationQueryService` in `assetmesh-core`.
//! Follows the architectural invariant: Zero direct SQL / raw repository access.

use assetmesh_core::application::relation_query_service::{
    TraversalDirection, TraversalOptions, DEFAULT_MAX_DEPTH,
};
use assetmesh_core::domain::ids::{AssetId, RelationId};
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use tauri::State;

use crate::dto::{
    MutationReceiptDto, NeighborViewDto, RelationAttachDto, RelationNeighborsQueryDto,
    RelationRemoveDto, RelationTraverseQueryDto, RelationViewDto, TraversalViewDto,
};
use crate::error::DesktopError;
use crate::state::DesktopState;

pub fn relation_list_impl(
    asset_id: String,
    state: &DesktopState,
) -> Result<Vec<RelationViewDto>, DesktopError> {
    let id = uuid::Uuid::parse_str(&asset_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset_id: {e}")))?;

    state.with_modules(|modules| {
        let mut svc = modules.relation();
        let views = svc.list_for_asset(id)?;
        Ok(views.into_iter().map(RelationViewDto::from).collect())
    })
}

pub fn relation_neighbors_impl(
    query: RelationNeighborsQueryDto,
    state: &DesktopState,
) -> Result<Vec<NeighborViewDto>, DesktopError> {
    let asset_id = uuid::Uuid::parse_str(&query.asset_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset_id: {e}")))?;

    let direction = match query.direction.as_deref() {
        Some(d) => TraversalDirection::parse(d).ok_or_else(|| {
            DesktopError::invalid_input(format!("invalid traversal direction: {d}"))
        })?,
        None => TraversalDirection::Both,
    };

    let relation_types = match query.relation_types {
        Some(types) => types
            .into_iter()
            .map(|t| {
                RelationType::parse(&t).ok_or_else(|| {
                    DesktopError::invalid_input(format!("invalid relation type: {t}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let options = TraversalOptions {
        direction,
        relation_types,
        max_depth: DEFAULT_MAX_DEPTH,
        include_archived: query.include_archived.unwrap_or(false),
    };

    state.with_modules(|modules| {
        let mut svc = modules.relation_query();
        let neighbors = match direction {
            TraversalDirection::Outgoing => svc.outgoing(asset_id, &options)?,
            TraversalDirection::Incoming => svc.incoming(asset_id, &options)?,
            TraversalDirection::Both => svc.neighbors(asset_id, &options)?,
        };
        Ok(neighbors.into_iter().map(NeighborViewDto::from).collect())
    })
}

pub fn relation_traverse_impl(
    query: RelationTraverseQueryDto,
    state: &DesktopState,
) -> Result<TraversalViewDto, DesktopError> {
    let asset_id = uuid::Uuid::parse_str(&query.asset_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset_id: {e}")))?;

    let direction = match query.direction.as_deref() {
        Some(d) => TraversalDirection::parse(d).ok_or_else(|| {
            DesktopError::invalid_input(format!("invalid traversal direction: {d}"))
        })?,
        None => TraversalDirection::Outgoing,
    };

    let relation_types = match query.relation_types {
        Some(types) => types
            .into_iter()
            .map(|t| {
                RelationType::parse(&t).ok_or_else(|| {
                    DesktopError::invalid_input(format!("invalid relation type: {t}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let options = TraversalOptions {
        direction,
        relation_types,
        max_depth: query.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        include_archived: query.include_archived.unwrap_or(false),
    };

    state.with_modules(|modules| {
        let mut svc = modules.relation_query();
        let view = match query.mode.as_deref() {
            Some("dependencies") => svc.dependencies(asset_id, &options)?,
            Some("dependents") => svc.dependents(asset_id, &options)?,
            Some("impact") => svc.impact(asset_id, &options)?,
            Some("traverse") | None => svc.traverse(asset_id, &options)?,
            Some(other) => {
                return Err(DesktopError::invalid_input(format!(
                    "unknown traversal mode: {other}. Expected dependencies, dependents, impact, or traverse"
                )))
            }
        };
        Ok(TraversalViewDto::from(view))
    })
}

pub fn relation_attach_impl(
    payload: RelationAttachDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    let source = uuid::Uuid::parse_str(&payload.source_asset_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid source_asset_id: {e}")))?;
    let target = uuid::Uuid::parse_str(&payload.target_asset_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid target_asset_id: {e}")))?;
    let relation_type = RelationType::parse(&payload.relation_type).ok_or_else(|| {
        DesktopError::invalid_input(format!("invalid relation type: {}", payload.relation_type))
    })?;

    state.with_modules(|modules| {
        let mut svc = modules.relation();
        let relation = svc.attach_with_revisions(
            source,
            relation_type,
            target,
            payload.note,
            RelationProvenance::Manual,
            payload.expected_source_revision,
            payload.expected_target_revision,
        )?;

        Ok(MutationReceiptDto {
            operation: "relation.attach".to_string(),
            asset_ids: vec![
                relation.source_asset_id.to_string(),
                relation.target_asset_id.to_string(),
            ],
            revision: None,
            changed: true,
            warnings: Vec::new(),
        })
    })
}

pub fn relation_remove_impl(
    payload: RelationRemoveDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    let relation_id = uuid::Uuid::parse_str(&payload.relation_id)
        .map(RelationId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid relation_id: {e}")))?;

    let context_asset_id = match payload.context_asset_id.as_deref() {
        Some(s) => Some(
            uuid::Uuid::parse_str(s)
                .map(AssetId::from_uuid)
                .map_err(|e| {
                    DesktopError::invalid_input(format!("invalid context_asset_id: {e}"))
                })?,
        ),
        None => None,
    };

    state.with_modules(|modules| {
        let mut svc = modules.relation();
        svc.remove_with_revision(
            relation_id,
            context_asset_id,
            payload.expected_context_revision,
        )?;

        Ok(MutationReceiptDto {
            operation: "relation.remove".to_string(),
            asset_ids: Vec::new(),
            revision: None,
            changed: true,
            warnings: Vec::new(),
        })
    })
}

#[tauri::command]
pub async fn relation_list(
    asset_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<RelationViewDto>, DesktopError> {
    relation_list_impl(asset_id, &state)
}

#[tauri::command]
pub async fn relation_neighbors(
    query: RelationNeighborsQueryDto,
    state: State<'_, DesktopState>,
) -> Result<Vec<NeighborViewDto>, DesktopError> {
    relation_neighbors_impl(query, &state)
}

#[tauri::command]
pub async fn relation_traverse(
    query: RelationTraverseQueryDto,
    state: State<'_, DesktopState>,
) -> Result<TraversalViewDto, DesktopError> {
    relation_traverse_impl(query, &state)
}

#[tauri::command]
pub async fn relation_attach(
    payload: RelationAttachDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    relation_attach_impl(payload, &state)
}

#[tauri::command]
pub async fn relation_remove(
    payload: RelationRemoveDto,
    state: State<'_, DesktopState>,
) -> Result<MutationReceiptDto, DesktopError> {
    relation_remove_impl(payload, &state)
}
