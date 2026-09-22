//! SQLite repository implementations of the core ports.

mod activity;
mod asset;
mod external_ref;
mod library;
mod media;
mod relation;
mod search;
mod service;
mod software;
mod tag;

pub use activity::SqliteActivityRepo;
pub use asset::SqliteAssetRepo;
pub use external_ref::SqliteExternalRefRepo;
pub use library::SqliteLibraryRepo;
pub use media::SqliteMediaRepo;
pub use relation::SqliteRelationRepo;
pub use search::SqliteSearchIndex;
pub use service::SqliteServiceRepo;
pub use software::SqliteSoftwareRepo;
pub use tag::SqliteTagRepo;

use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::repos::TagReader;
use assetmesh_core::AppResult;
use uuid::Uuid;

/// Tag names for a page of assets, keyed by asset id.
///
/// Shared by the module list queries, which all render a page of rows with
/// their tags. Loading them per row is an N+1: the number of queries grows with
/// the page, so a long library makes every listing slower. This does it in one
/// query via [`TagReader::list_for_assets`](assetmesh_core::ports::repos::TagReader::list_for_assets).
///
/// The asset ids come from rows the caller already has, so the query only needs
/// the ids and returns no duplicate work.
pub(crate) fn batch_tags(
    conn: &rusqlite::Connection,
    asset_ids: &[assetmesh_core::domain::ids::AssetId],
) -> AppResult<std::collections::HashMap<assetmesh_core::domain::ids::AssetId, Vec<String>>> {
    let mut tags = crate::repos::tag::SqliteTagRepo { conn };
    tags.list_for_assets(asset_ids).map(|pairs| {
        let mut out: std::collections::HashMap<_, Vec<String>> =
            std::collections::HashMap::with_capacity(asset_ids.len());
        for (asset_id, tag) in pairs {
            out.entry(asset_id).or_default().push(tag.name);
        }
        out
    })
}

/// Converts a `query_map` row result into an application result.
pub(crate) fn row_result<T>(row: rusqlite::Result<T>) -> AppResult<T> {
    row.map_err(crate::map_error)
}

/// Boxes an application error inside a driver error so row parsers can use
/// `?` while remaining usable with `query_map`.
pub(crate) fn box_app_error(e: assetmesh_core::AppError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

/// Adapter: AppResult-valued row parser -> rusqlite::Result-valued.
pub(crate) fn app_row<T>(result: AppResult<T>) -> rusqlite::Result<T> {
    result.map_err(box_app_error)
}

pub(crate) fn ts_to_string(ts: Timestamp) -> String {
    ts.to_rfc3339()
}

pub(crate) fn ts_from_string(value: &str) -> AppResult<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|ts| ts.with_timezone(&chrono::Utc))
        .map_err(|e| assetmesh_core::AppError::storage(format!("invalid stored timestamp: {e}")))
}

pub(crate) fn uuid_to_string(id: impl Into<Uuid>) -> String {
    let id: Uuid = id.into();
    id.to_string()
}

pub(crate) fn uuid_from_string(value: &str) -> AppResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|e| assetmesh_core::AppError::storage(format!("invalid stored id: {e}")))
}
