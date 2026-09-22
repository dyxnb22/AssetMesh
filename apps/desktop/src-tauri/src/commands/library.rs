//! Library read commands for desktop.

use assetmesh_core::application::library_service::LibraryService;
use tauri::State;

use crate::dto::{AssetSummaryDto, LibraryQueryDto, PageDto};
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
