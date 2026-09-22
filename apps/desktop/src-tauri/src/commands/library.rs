use assetmesh_core::application::library_service::LibraryService;
use assetmesh_core::domain::asset::LifecycleState;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::AppError;
use tauri::State;

use crate::dto::{
    AssetDetailDto, AssetSummaryDto, ExternalRefDto, LibraryQueryDto, LibrarySearchQueryDto,
    PageDto,
};
use crate::error::DesktopError;
use crate::state::DesktopState;

#[tauri::command]
pub fn library_list(
    query: Option<LibraryQueryDto>,
    state: State<'_, DesktopState>,
) -> Result<PageDto<AssetSummaryDto>, DesktopError> {
    library_list_impl(query, &state)
}

pub fn library_list_impl(
    query: Option<LibraryQueryDto>,
    state: &DesktopState,
) -> Result<PageDto<AssetSummaryDto>, DesktopError> {
    let app_query = query.unwrap_or_default().try_into()?;
    state.with_factory(|factory| {
        let mut svc = LibraryService::new(factory.clone());
        let page = svc.list_assets(&app_query)?;
        Ok(page.into())
    })
}

#[tauri::command]
pub fn library_get(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<AssetDetailDto, DesktopError> {
    library_get_impl(&id, &state)
}

pub fn library_get_impl(
    id_str: &str,
    state: &DesktopState,
) -> Result<AssetDetailDto, DesktopError> {
    let asset_id = uuid::Uuid::parse_str(id_str)
        .map(AssetId::from_uuid)
        .map_err(|e| DesktopError::invalid_input(format!("invalid asset ID: {e}")))?;

    state.with_factory(|factory| {
        let mut svc = LibraryService::new(factory.clone());
        match svc.get_asset(asset_id) {
            Ok(view) => Ok(view.into()),
            Err(AppError::Conflict { .. }) => {
                // Merged tombstone: load redirect target
                factory
                    .read(&mut |q| {
                        let asset = q
                            .assets()
                            .get(asset_id)?
                            .ok_or_else(|| AppError::not_found("asset", asset_id))?;
                        if asset.lifecycle_state == LifecycleState::Merged {
                            let mut tags: Vec<String> = q
                                .tags()
                                .list_for_asset(asset_id)?
                                .into_iter()
                                .map(|t| t.name)
                                .collect();
                            tags.sort();
                            let external_refs = q
                                .external_refs()
                                .list_for_asset(asset_id)?
                                .into_iter()
                                .map(|r| ExternalRefDto {
                                    namespace: r.namespace,
                                    external_id: r.external_id,
                                    source_url: r.source_url,
                                })
                                .collect();
                            Ok(AssetDetailDto::for_merged(&asset, tags, external_refs))
                        } else {
                            Err(AppError::conflict(format!(
                                "asset {asset_id} conflict while reading detail"
                            )))
                        }
                    })
                    .map_err(Into::into)
            }
            Err(err) => Err(err.into()),
        }
    })
}

#[tauri::command]
pub fn library_search(
    query: LibrarySearchQueryDto,
    state: State<'_, DesktopState>,
) -> Result<PageDto<AssetSummaryDto>, DesktopError> {
    library_search_impl(query, &state)
}

pub fn library_search_impl(
    query: LibrarySearchQueryDto,
    state: &DesktopState,
) -> Result<PageDto<AssetSummaryDto>, DesktopError> {
    let app_query = query.try_into()?;
    state.with_factory(|factory| {
        let mut svc = LibraryService::new(factory.clone());
        let page = svc.search_assets(&app_query)?;
        Ok(page.into())
    })
}
