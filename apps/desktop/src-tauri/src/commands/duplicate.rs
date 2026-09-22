//! Duplicate Review and Merge Tauri command handlers — Phase 5 (P5-08).
//!
//! Exposes read-only candidate review from `DuplicateReviewService` and explicit
//! canonical merge from `AssetService` in `assetmesh-core`.
//! Follows the architectural invariant: Zero direct SQL / raw repository access.

use assetmesh_core::application::duplicate_review_service::DuplicateQuery;
use assetmesh_core::application::library_service::PageRequest;
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::domain::ids::AssetId;
use tauri::State;

use crate::dto::{
    DuplicateCandidateDto, DuplicateQueryDto, MergeApplyDto, MergePreviewDto, MergePreviewQueryDto,
    MutationReceiptDto, PageDto,
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

    state.with_modules(|modules| {
        let mut svc = modules.duplicate_review();
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

    // No adapter-side rule: the self-merge and kind checks live in
    // `MergePreviewService` / `AssetService`, so preview and apply classify the
    // same request the same way.
    state.with_modules(|modules| {
        let mut svc = modules.merge_preview();
        let preview = svc.preview(winner_id, loser_id)?;
        Ok(preview.into())
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

    state.with_modules(|modules| {
        let mut svc = modules.asset();
        let outcome = svc.merge_assets_with_revisions(
            loser_id,
            winner_id,
            input.expected_loser_revision,
            input.expected_winner_revision,
        )?;
        Ok(MutationReceiptDto {
            operation: "asset.merge".into(),
            asset_ids: vec![winner_id.to_string(), loser_id.to_string()],
            revision: Some(outcome.revision),
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
