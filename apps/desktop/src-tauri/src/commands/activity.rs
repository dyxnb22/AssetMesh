//! Activity Tauri command handlers — Phase 5 (P5-08).
//!
//! Exposes read-only queries from `ActivityService` in `assetmesh-core`.
//! Follows the architectural invariant: Zero direct SQL / raw repository access.

use assetmesh_core::application::activity_service::ActivityQuery;
use assetmesh_core::application::library_service::PageRequest;
use assetmesh_core::domain::activity::ActivityModule;
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::domain::ids::AssetId;
use tauri::State;

use crate::dto::{ActivityQueryDto, ActivityViewDto, PageDto};
use crate::error::DesktopError;
use crate::state::DesktopState;

pub fn activity_query_impl(
    query: ActivityQueryDto,
    state: &DesktopState,
) -> Result<PageDto<ActivityViewDto>, DesktopError> {
    let asset_id = match query.asset_id {
        Some(s) if !s.trim().is_empty() => {
            let id = uuid::Uuid::parse_str(&s)
                .map(AssetId::from_uuid)
                .map_err(|e| DesktopError::invalid_input(format!("invalid asset_id: {e}")))?;
            Some(id)
        }
        _ => None,
    };

    let modules = match query.modules {
        Some(mods) => mods
            .into_iter()
            .map(|m| {
                ActivityModule::parse(&m).ok_or_else(|| {
                    DesktopError::invalid_input(format!(
                        "unknown activity module '{m}'; expected asset, media, software, services, relation, or import"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let kinds = match query.kinds {
        Some(ks) => ks
            .into_iter()
            .map(|k| {
                AssetKind::parse(&k)
                    .ok_or_else(|| DesktopError::invalid_input(format!("unknown asset kind '{k}'")))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    let since = match query.since {
        Some(s) if !s.trim().is_empty() => {
            let dt = chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|e| {
                    DesktopError::invalid_input(format!("invalid 'since' timestamp: {e}"))
                })?
                .with_timezone(&chrono::Utc);
            Some(dt)
        }
        _ => None,
    };

    let until = match query.until {
        Some(s) if !s.trim().is_empty() => {
            let dt = chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|e| {
                    DesktopError::invalid_input(format!("invalid 'until' timestamp: {e}"))
                })?
                .with_timezone(&chrono::Utc);
            Some(dt)
        }
        _ => None,
    };

    let page_request = PageRequest::new(query.limit.unwrap_or(20), query.offset.unwrap_or(0));

    let core_query = ActivityQuery {
        asset_id,
        event_types: query.event_types.unwrap_or_default(),
        modules,
        kinds,
        actors: query.actors.unwrap_or_default(),
        since,
        until,
        page: page_request,
    };

    state.with_modules(|modules| {
        let mut svc = modules.activity();
        let page = svc.query(&core_query)?;
        Ok(PageDto::from(page))
    })
}

#[tauri::command]
pub async fn activity_query(
    state: State<'_, DesktopState>,
    query: ActivityQueryDto,
) -> Result<PageDto<ActivityViewDto>, DesktopError> {
    activity_query_impl(query, &state)
}
