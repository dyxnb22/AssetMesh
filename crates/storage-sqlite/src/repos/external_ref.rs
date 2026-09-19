//! SQLite implementation of `ExternalRefRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::external_ref::AssetExternalRef;
use assetmesh_core::domain::ids::{AssetId, ExternalRefId};
use assetmesh_core::ports::repos::{ref_conflict, ExternalRefReader, ExternalRefRepository};
use assetmesh_core::AppResult;
use rusqlite::Connection;

pub struct SqliteExternalRefRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

fn parse_ref(row: &rusqlite::Row) -> rusqlite::Result<AssetExternalRef> {
    crate::repos::app_row(parse_ref_inner(row))
}

fn parse_ref_inner(row: &rusqlite::Row) -> AppResult<AssetExternalRef> {
    let id: String = col(row, 0)?;
    let asset_id: String = col(row, 1)?;
    let namespace: String = col(row, 2)?;
    let external_id: String = col(row, 3)?;
    let source_url: Option<String> = col(row, 4)?;
    let metadata: Option<String> = col(row, 5)?;
    let created_at: String = col(row, 6)?;
    let updated_at: String = col(row, 7)?;

    Ok(AssetExternalRef {
        id: ExternalRefId::from_uuid(uuid_from_string(&id)?),
        asset_id: AssetId::from_uuid(uuid_from_string(&asset_id)?),
        namespace,
        external_id,
        source_url,
        metadata,
        created_at: ts_from_string(&created_at)?,
        updated_at: ts_from_string(&updated_at)?,
    })
}

fn ref_to_row(row: rusqlite::Result<AssetExternalRef>) -> AppResult<AssetExternalRef> {
    row_result(row)
}

impl ExternalRefReader for SqliteExternalRefRepo<'_> {
    fn get(&mut self, id: ExternalRefId) -> AppResult<Option<AssetExternalRef>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, asset_id, namespace, external_id, source_url, metadata, \
                 created_at, updated_at FROM external_refs WHERE id = ?1",
            )
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(id.as_uuid())], parse_ref)
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(ref_to_row(row)?)),
            None => Ok(None),
        }
    }
    fn find_asset_by_ref(
        &mut self,
        namespace: &str,
        external_id: &str,
    ) -> AppResult<Option<AssetId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT asset_id FROM external_refs WHERE namespace = ?1 AND external_id = ?2")
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([namespace, external_id], |row| row.get::<_, String>(0))
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => {
                let asset_id: String = row.map_err(crate::map_error)?;
                Ok(Some(AssetId::from_uuid(uuid_from_string(&asset_id)?)))
            }
            None => Ok(None),
        }
    }
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<AssetExternalRef>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, asset_id, namespace, external_id, source_url, metadata, \
                 created_at, updated_at FROM external_refs WHERE asset_id = ?1 \
                 ORDER BY namespace, external_id",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([uuid_to_string(asset_id.as_uuid())], parse_ref)
            .map_err(crate::map_error)?;
        let mut refs = Vec::new();
        for row in rows {
            refs.push(ref_to_row(row)?);
        }
        Ok(refs)
    }
    fn list_all(&mut self) -> AppResult<Vec<AssetExternalRef>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, asset_id, namespace, external_id, source_url, metadata, \
                 created_at, updated_at FROM external_refs ORDER BY namespace, external_id, id",
            )
            .map_err(crate::map_error)?;
        let rows = stmt.query_map([], parse_ref).map_err(crate::map_error)?;
        let mut refs = Vec::new();
        for row in rows {
            refs.push(ref_to_row(row)?);
        }
        Ok(refs)
    }
}

impl ExternalRefRepository for SqliteExternalRefRepo<'_> {
    fn insert(&mut self, reference: &AssetExternalRef) -> AppResult<()> {
        reference.validate()?;
        self.conn
            .execute(
                "INSERT INTO external_refs (id, asset_id, namespace, external_id, source_url, \
                 metadata, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    uuid_to_string(reference.id.as_uuid()),
                    uuid_to_string(reference.asset_id.as_uuid()),
                    reference.namespace,
                    reference.external_id,
                    reference.source_url,
                    reference.metadata,
                    ts_to_string(reference.created_at),
                    ts_to_string(reference.updated_at),
                ],
            )
            .map_err(|e| {
                // Surface the UNIQUE(namespace, external_id) contract clearly.
                let mapped = crate::map_error(e);
                match &mapped {
                    assetmesh_core::AppError::Conflict { .. } => {
                        ref_conflict(&reference.namespace, &reference.external_id)
                    }
                    other => other.clone(),
                }
            })?;
        Ok(())
    }
    fn update(&mut self, reference: &AssetExternalRef) -> AppResult<()> {
        reference.validate()?;
        let changed = self
            .conn
            .execute(
                "UPDATE external_refs SET asset_id = ?2, namespace = ?3, external_id = ?4, \
                 source_url = ?5, metadata = ?6, created_at = ?7, updated_at = ?8 WHERE id = ?1",
                rusqlite::params![
                    uuid_to_string(reference.id.as_uuid()),
                    uuid_to_string(reference.asset_id.as_uuid()),
                    reference.namespace,
                    reference.external_id,
                    reference.source_url,
                    reference.metadata,
                    ts_to_string(reference.created_at),
                    ts_to_string(reference.updated_at),
                ],
            )
            .map_err(crate::map_error)?;
        if changed == 0 {
            return Err(assetmesh_core::AppError::not_found(
                "external ref",
                reference.id,
            ));
        }
        Ok(())
    }
    fn delete(&mut self, id: ExternalRefId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM external_refs WHERE id = ?1",
                [uuid_to_string(id.as_uuid())],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
