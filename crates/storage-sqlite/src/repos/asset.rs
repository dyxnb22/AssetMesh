//! SQLite implementation of `AssetRepository`.

use crate::repos::{ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::asset::{Asset, AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::ports::repos::{AssetFilter, AssetReader, AssetRepository, LifecycleFilter};
use assetmesh_core::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension};

pub struct SqliteAssetRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

fn parse_asset(row: &rusqlite::Row) -> AppResult<Asset> {
    let id: String = col(row, 0)?;
    let kind: String = col(row, 1)?;
    let name: String = col(row, 2)?;
    let summary: Option<String> = col(row, 3)?;
    let lifecycle: String = col(row, 4)?;
    let revision: i64 = col(row, 5)?;
    let created_at: String = col(row, 6)?;
    let updated_at: String = col(row, 7)?;
    let archived_at: Option<String> = col(row, 8)?;
    let merged_into: Option<String> = col(row, 9)?;

    Ok(Asset {
        id: AssetId::from_uuid(uuid_from_string(&id)?),
        kind: AssetKind::parse(&kind)
            .ok_or_else(|| AppError::storage(format!("unknown stored asset kind: {kind}")))?,
        name,
        summary,
        lifecycle_state: LifecycleState::parse(&lifecycle)
            .ok_or_else(|| AppError::storage(format!("unknown lifecycle state: {lifecycle}")))?,
        revision,
        created_at: ts_from_string(&created_at)?,
        updated_at: ts_from_string(&updated_at)?,
        archived_at: match &archived_at {
            Some(text) => Some(ts_from_string(text)?),
            None => None,
        },
        merged_into: match &merged_into {
            Some(text) => Some(AssetId::from_uuid(uuid_from_string(text)?)),
            None => None,
        },
    })
}

/// Adapter so `query_map` can use the domain parser; the AppError is boxed
/// inside the driver error and re-mapped at the call site.
pub(crate) fn row_to_asset(row: &rusqlite::Row) -> rusqlite::Result<Asset> {
    parse_asset(row).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

impl AssetReader for SqliteAssetRepo<'_> {
    fn get(&mut self, id: AssetId) -> AppResult<Option<Asset>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, name, summary, lifecycle_state, revision, created_at, \
                      updated_at, archived_at, merged_into_asset_id FROM assets WHERE id = ?1",
            )
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(id.as_uuid())], row_to_asset)
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(row.map_err(crate::map_error)?)),
            None => Ok(None),
        }
    }

    fn list(&mut self, filter: &AssetFilter) -> AppResult<Vec<Asset>> {
        let mut sql = "SELECT id, kind, name, summary, lifecycle_state, revision, created_at, \
             updated_at, archived_at, merged_into_asset_id FROM assets WHERE 1=1"
            .to_string();
        let mut params: Vec<String> = Vec::new();
        if let Some(kind) = filter.kind {
            params.push(kind.as_str().to_string());
            sql.push_str(&format!(" AND kind = ?{}", params.len()));
        }
        match filter.lifecycle.unwrap_or(LifecycleFilter::Active) {
            LifecycleFilter::All => {}
            LifecycleFilter::Active => sql.push_str(" AND lifecycle_state = 'active'"),
            LifecycleFilter::ActiveOrArchived => sql.push_str(" AND lifecycle_state != 'merged'"),
        }
        sql.push_str(" ORDER BY id");

        let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(refs.as_slice(), row_to_asset)
            .map_err(crate::map_error)?;
        let mut assets = Vec::new();
        for row in rows {
            assets.push(row.map_err(crate::map_error)?);
        }
        Ok(assets)
    }
}

impl AssetRepository for SqliteAssetRepo<'_> {
    fn insert(&mut self, asset: &Asset) -> AppResult<()> {
        asset.validate()?;
        self.conn
            .execute(
                "INSERT INTO assets (id, kind, name, summary, lifecycle_state, revision, \
                 created_at, updated_at, archived_at, merged_into_asset_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    uuid_to_string(asset.id.as_uuid()),
                    asset.kind.as_str(),
                    asset.name,
                    asset.summary,
                    asset.lifecycle_state.as_str(),
                    asset.revision,
                    ts_to_string(asset.created_at),
                    ts_to_string(asset.updated_at),
                    asset.archived_at.map(ts_to_string),
                    asset.merged_into.map(|id| uuid_to_string(id.as_uuid())),
                ],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }

    fn update(&mut self, asset: &Asset) -> AppResult<()> {
        asset.validate()?;
        self.ensure_kind_change_keeps_module_records_compatible(asset)?;
        let changed = self
            .conn
            .execute(
                "UPDATE assets SET kind = ?2, name = ?3, summary = ?4, lifecycle_state = ?5, \
                 revision = ?6, created_at = ?7, updated_at = ?8, archived_at = ?9, \
                 merged_into_asset_id = ?10 WHERE id = ?1",
                rusqlite::params![
                    uuid_to_string(asset.id.as_uuid()),
                    asset.kind.as_str(),
                    asset.name,
                    asset.summary,
                    asset.lifecycle_state.as_str(),
                    asset.revision,
                    ts_to_string(asset.created_at),
                    ts_to_string(asset.updated_at),
                    asset.archived_at.map(ts_to_string),
                    asset.merged_into.map(|id| uuid_to_string(id.as_uuid())),
                ],
            )
            .map_err(crate::map_error)?;
        if changed == 0 {
            return Err(AppError::not_found("asset", asset.id));
        }
        Ok(())
    }
}

impl SqliteAssetRepo<'_> {
    /// Rejects a kind change that would strand the asset's existing module
    /// record: a software record on a media.* asset, or a `saas` record on a
    /// `service.api` asset, is an invalid state the repository boundary must
    /// not write. The typed detail's own discriminator (media_type / category
    /// / service_type) is only consistent with the kind assigned at creation,
    /// and no application write path ever re-types an asset, so ANY kind
    /// change is refused while the old module's detail row remains — remove
    /// the record first if a re-type is genuinely required.
    fn ensure_kind_change_keeps_module_records_compatible(&self, asset: &Asset) -> AppResult<()> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT kind FROM assets WHERE id = ?1",
                [uuid_to_string(asset.id.as_uuid())],
                |row| row.get(0),
            )
            .optional()
            .map_err(crate::map_error)?;
        let Some(stored) = stored else {
            return Ok(()); // an insert, or an asset that does not exist yet
        };
        if stored == asset.kind.as_str() {
            return Ok(());
        }
        let stored_kind = AssetKind::parse(&stored)
            .ok_or_else(|| AppError::storage(format!("unknown stored asset kind: {stored}")))?;
        // The record that would be stranded belongs to the OLD module:
        // changing away from a module leaves its detail row on an asset the
        // module no longer owns; changing within a module leaves a row whose
        // discriminator no longer matches the kind (docs/10 kind/type 1:1).
        let table = module_detail_table(stored_kind.module())?;
        let present: bool = self
            .conn
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE asset_id = ?1)"),
                [uuid_to_string(asset.id.as_uuid())],
                |row| row.get(0),
            )
            .map_err(crate::map_error)?;
        if present {
            return Err(AppError::conflict(format!(
                "cannot change asset {} from {} to {} while it has a {} record; remove the \
                 {} record first",
                asset.id,
                stored_kind,
                asset.kind,
                stored_kind.module(),
                stored_kind.module()
            )));
        }
        Ok(())
    }
}

/// The typed-details table a module owns. Shared by the cross-module kind
/// guard, which must know every module's detail table to detect a stranded
/// record; a new module adds its table here.
fn module_detail_table(module: &str) -> AppResult<&'static str> {
    match module {
        "media" => Ok("media_records"),
        "software" => Ok("software_records"),
        "services" => Ok("service_records"),
        other => Err(AppError::storage(format!(
            "unknown module {other:?} in stored asset kind"
        ))),
    }
}
