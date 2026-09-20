//! SQLite repository implementations of the core ports.

mod activity;
mod asset;
mod external_ref;
mod media;
mod relation;
mod search;
mod service;
mod software;
mod tag;

pub use activity::SqliteActivityRepo;
pub use asset::SqliteAssetRepo;
pub use external_ref::SqliteExternalRefRepo;
pub use media::SqliteMediaRepo;
pub use relation::SqliteRelationRepo;
pub use search::SqliteSearchIndex;
pub use service::SqliteServiceRepo;
pub use software::SqliteSoftwareRepo;
pub use tag::SqliteTagRepo;

use assetmesh_core::domain::Timestamp;
use assetmesh_core::AppResult;
use uuid::Uuid;

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
