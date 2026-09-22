use assetmesh_core::domain::ids::AssetId;
use tauri::State;

use crate::dto::{
    AssetDetailDto, AssetSummaryDto, LibraryQueryDto, LibrarySearchQueryDto, PageDto,
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
    state.with_modules(|modules| {
        let mut svc = modules.library();
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

    state.with_modules(|modules| {
        let mut svc = modules.library();
        let outcome = svc.get_detail(asset_id)?;
        Ok(AssetDetailDto::from(outcome))
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
    state.with_modules(|modules| {
        let mut svc = modules.library();
        let page = svc.search_assets(&app_query)?;
        Ok(page.into())
    })
}
