//! Duplicate Review and Merge Tauri command handlers — Phase 5 (P5-08).
//!
//! Exposes read-only candidate review from `DuplicateReviewService` and explicit
//! canonical merge from `AssetService` in `assetmesh-core`.
//! Follows the architectural invariant: Zero direct SQL / raw repository access.

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::duplicate_review_service::{
    DuplicateQuery, DuplicateReviewService,
};
use assetmesh_core::application::library_service::PageRequest;
use assetmesh_core::domain::asset::{AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use tauri::State;

use crate::dto::{
    AssetSummaryDto, DuplicateCandidateDto, DuplicateQueryDto, ExternalRefDto, MergeApplyDto,
    MergePreviewDto, MergePreviewQueryDto, MutationReceiptDto, PageDto,
};
use crate::error::DesktopError;
use crate::state::DesktopState;

pub fn duplicate_candidates_impl(
    query: DuplicateQueryDto,
    state: &DesktopState,
) -> Result<PageDto<DuplicateCandidateDto>, DesktopError> {
    let kinds = match query.kinds {
        Some(ks) => ks
            .into_iter()
            .map(|k| {
                AssetKind::parse(&k).ok_or_else(|| {
                    DesktopError::invalid_input(format!("unknown asset kind: '{k}'"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let include_archived = query.include_archived.unwrap_or(true);
    let page_request = PageRequest::new(query.limit.unwrap_or(20), query.offset.unwrap_or(0));

    let core_query = DuplicateQuery {
        kinds,
        include_archived,
        page: page_request,
    };

    state.with_factory(|factory| {
        let mut svc = DuplicateReviewService::new(factory.clone());
        let page = svc.candidates(&core_query)?;
        Ok(PageDto::from(page))
    })
}

pub fn merge_preview_impl(
    query: MergePreviewQueryDto,
    state: &DesktopState,
) -> Result<MergePreviewDto, DesktopError> {
    let winner_id = uuid::Uuid::parse_str(&query.winner_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid winner_id: {e}")))?;
    let loser_id = uuid::Uuid::parse_str(&query.loser_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid loser_id: {e}")))?;

    if winner_id == loser_id {
        return Err(DesktopError::conflict("cannot merge an asset into itself"));
    }

    state.with_factory(|factory| {
        let preview = factory.read(&mut |q| {
            let loser = q
                .assets()
                .get(loser_id)?
                .ok_or_else(|| assetmesh_core::AppError::not_found("asset", loser_id))?;
            let winner = q
                .assets()
                .get(winner_id)?
                .ok_or_else(|| assetmesh_core::AppError::not_found("asset", winner_id))?;

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

            // Tags
            let loser_tags = q.tags().list_for_asset(loser_id)?;
            let winner_tags = q.tags().list_for_asset(winner_id)?;
            let transferred_tags: Vec<String> = loser_tags
                .into_iter()
                .filter(|lt| !winner_tags.iter().any(|wt| wt.id == lt.id))
                .map(|t| t.name)
                .collect();

            // External refs
            let loser_refs = q.external_refs().list_for_asset(loser_id)?;
            let mut transferred_external_refs = Vec::new();
            let mut redundant_external_refs = Vec::new();

            for r in loser_refs {
                let dto = ExternalRefDto {
                    namespace: r.namespace.clone(),
                    external_id: r.external_id.clone(),
                    source_url: r.source_url.clone(),
                };
                let target = q.external_refs().find_asset_by_ref(&r.namespace, &r.external_id)?;
                if let Some(existing) = target {
                    if existing == winner_id {
                        redundant_external_refs.push(dto);
                        continue;
                    }
                }
                transferred_external_refs.push(dto);
            }

            // Relations
            let loser_relations = q.relations().list_for_asset(loser_id)?;
            let winner_relations = q.relations().list_for_asset(winner_id)?;
            let mut transferred_relations_count = 0;
            let mut redundant_relations_count = 0;

            for rel in loser_relations {
                let other = if rel.source_asset_id == loser_id {
                    rel.target_asset_id
                } else {
                    rel.source_asset_id
                };
                if other == winner_id {
                    redundant_relations_count += 1;
                    continue;
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
                    redundant_relations_count += 1;
                } else {
                    transferred_relations_count += 1;
                }
            }

            // Service details field-level conflict detection
            let loser_service = q.services().get(loser_id)?;
            let winner_service = q.services().get(winner_id)?;
            if let (Some(l_rec), Some(w_rec)) = (loser_service, winner_service) {
                let mut service_conflicts = Vec::new();
                let mut check_field = |field_name: &str, v1: &Option<String>, v2: &Option<String>| {
                    if let (Some(a), Some(b)) = (v1, v2) {
                        if a != b {
                            service_conflicts.push(format!("{field_name}: '{a}' vs '{b}'"));
                        }
                    }
                };
                check_field("provider", &w_rec.provider, &l_rec.provider);
                check_field("account_label", &w_rec.account_label, &l_rec.account_label);
                check_field("endpoint_url", &w_rec.endpoint_url, &l_rec.endpoint_url);
                check_field("dashboard_url", &w_rec.dashboard_url, &l_rec.dashboard_url);
                check_field("domain_name", &w_rec.domain_name, &l_rec.domain_name);
                check_field("plan", &w_rec.plan, &l_rec.plan);
                check_field("notes", &w_rec.notes, &l_rec.notes);
                if let (Some(a), Some(b)) = (w_rec.billing_cadence, l_rec.billing_cadence) {
                    if a != b {
                        service_conflicts.push(format!("billing_cadence: '{a:?}' vs '{b:?}'"));
                    }
                }
                if let (Some(a), Some(b)) = (w_rec.renews_at, l_rec.renews_at) {
                    if a != b {
                        service_conflicts.push(format!("renews_at: '{a}' vs '{b}'"));
                    }
                }
                if let (Some(a), Some(b)) = (w_rec.expires_at, l_rec.expires_at) {
                    if a != b {
                        service_conflicts.push(format!("expires_at: '{a}' vs '{b}'"));
                    }
                }
                if let (Some(a), Some(b)) = (w_rec.auto_renew, l_rec.auto_renew) {
                    if a != b {
                        service_conflicts.push(format!("auto_renew: '{a}' vs '{b}'"));
                    }
                }
                match (
                    (w_rec.cost_minor, w_rec.currency.as_deref()),
                    (l_rec.cost_minor, l_rec.currency.as_deref()),
                ) {
                    ((Some(w_cost), Some(w_curr)), (Some(l_cost), Some(l_curr)))
                        if w_cost != l_cost || w_curr != l_curr =>
                    {
                        service_conflicts.push(format!(
                            "cost: {w_cost} {w_curr} vs {l_cost} {l_curr}"
                        ));
                    }
                    _ => {}
                }

                if !service_conflicts.is_empty() {
                    conflicts.push(format!(
                        "Service details conflict on: {}",
                        service_conflicts.join(", ")
                    ));
                }
            }

            // Media / Software notices
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

            let winner_summary = AssetSummaryDto {
                id: winner.id.to_string(),
                kind: winner.kind.as_str().to_string(),
                name: winner.name,
                lifecycle: winner.lifecycle_state.as_str().to_string(),
                subtitle: winner.summary,
                tags: winner_tags.into_iter().map(|t| t.name).collect(),
                updated_at: winner.updated_at.to_rfc3339(),
            };

            let loser_summary = AssetSummaryDto {
                id: loser.id.to_string(),
                kind: loser.kind.as_str().to_string(),
                name: loser.name,
                lifecycle: loser.lifecycle_state.as_str().to_string(),
                subtitle: loser.summary,
                tags: transferred_tags.clone(),
                updated_at: loser.updated_at.to_rfc3339(),
            };

            Ok(MergePreviewDto {
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
        })?;
        Ok(preview)
    })
}

pub fn merge_apply_impl(
    input: MergeApplyDto,
    state: &DesktopState,
) -> Result<MutationReceiptDto, DesktopError> {
    let winner_id = uuid::Uuid::parse_str(&input.winner_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid winner_id: {e}")))?;
    let loser_id = uuid::Uuid::parse_str(&input.loser_id)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid loser_id: {e}")))?;

    state.with_factory(|factory| {
        let mut svc = AssetService::new(factory.clone(), state.clock.clone(), state.ids.clone());
        svc.merge_assets(loser_id, winner_id)?;
        Ok(MutationReceiptDto {
            operation: "asset.merge".into(),
            asset_ids: vec![winner_id.to_string(), loser_id.to_string()],
            revision: None,
            changed: true,
            warnings: vec![],
        })
    })
}

#[tauri::command]
pub async fn duplicate_candidates(
    state: State<'_, DesktopState>,
    query: DuplicateQueryDto,
) -> Result<PageDto<DuplicateCandidateDto>, DesktopError> {
    duplicate_candidates_impl(query, &state)
}

#[tauri::command]
pub async fn merge_preview(
    state: State<'_, DesktopState>,
    query: MergePreviewQueryDto,
) -> Result<MergePreviewDto, DesktopError> {
    merge_preview_impl(query, &state)
}

#[tauri::command]
pub async fn merge_apply(
    state: State<'_, DesktopState>,
    input: MergeApplyDto,
) -> Result<MutationReceiptDto, DesktopError> {
    merge_apply_impl(input, &state)
}
