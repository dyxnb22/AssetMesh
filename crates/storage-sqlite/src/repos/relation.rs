//! SQLite implementation of `RelationRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::ids::{AssetId, RelationId};
use assetmesh_core::domain::relation::{Relation, RelationProvenance, RelationType};
use assetmesh_core::ports::repos::{RelationReader, RelationRepository};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;

pub struct SqliteRelationRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

fn parse_relation(row: &rusqlite::Row) -> AppResult<Relation> {
    let id: String = col(row, 0)?;
    let source: String = col(row, 1)?;
    let target: String = col(row, 2)?;
    let relation_type: String = col(row, 3)?;
    let note: Option<String> = col(row, 4)?;
    let provenance: String = col(row, 5)?;
    let created_at: String = col(row, 6)?;

    Ok(Relation {
        id: RelationId::from_uuid(uuid_from_string(&id)?),
        source_asset_id: AssetId::from_uuid(uuid_from_string(&source)?),
        target_asset_id: AssetId::from_uuid(uuid_from_string(&target)?),
        relation_type: RelationType::parse(&relation_type).ok_or_else(|| {
            AppError::storage(format!("unknown stored relation_type: {relation_type}"))
        })?,
        note,
        provenance: RelationProvenance::parse(&provenance).ok_or_else(|| {
            AppError::storage(format!("unknown stored relation provenance: {provenance}"))
        })?,
        created_at: ts_from_string(&created_at)?,
    })
}

fn parse_row(row: rusqlite::Result<Relation>) -> AppResult<Relation> {
    row_result(row)
}

impl RelationReader for SqliteRelationRepo<'_> {
    fn get(&mut self, id: RelationId) -> AppResult<Option<Relation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_asset_id, target_asset_id, relation_type, note, provenance, \
                 created_at FROM relations WHERE id = ?1",
            )
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(id.as_uuid())], |row| {
                crate::repos::app_row(parse_relation(row))
            })
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(parse_row(row)?)),
            None => Ok(None),
        }
    }

    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<Relation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_asset_id, target_asset_id, relation_type, note, provenance, \
                 created_at FROM relations WHERE source_asset_id = ?1 OR target_asset_id = ?1 \
                 ORDER BY created_at, id",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([uuid_to_string(asset_id.as_uuid())], |row| {
                crate::repos::app_row(parse_relation(row))
            })
            .map_err(crate::map_error)?;
        let mut relations = Vec::new();
        for row in rows {
            relations.push(parse_row(row)?);
        }
        Ok(relations)
    }

    fn list_all(&mut self) -> AppResult<Vec<Relation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_asset_id, target_asset_id, relation_type, note, provenance, \
                 created_at FROM relations ORDER BY id",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([], |row| crate::repos::app_row(parse_relation(row)))
            .map_err(crate::map_error)?;
        let mut relations = Vec::new();
        for row in rows {
            relations.push(parse_row(row)?);
        }
        Ok(relations)
    }
}

impl RelationRepository for SqliteRelationRepo<'_> {
    fn insert(&mut self, relation: &Relation) -> AppResult<()> {
        relation.validate()?;
        relation.ensure_canonical()?;
        self.conn
            .execute(
                "INSERT INTO relations (id, source_asset_id, target_asset_id, relation_type, \
                 note, provenance, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    uuid_to_string(relation.id.as_uuid()),
                    uuid_to_string(relation.source_asset_id.as_uuid()),
                    uuid_to_string(relation.target_asset_id.as_uuid()),
                    relation.relation_type.as_str(),
                    relation.note,
                    relation.provenance.as_str(),
                    ts_to_string(relation.created_at),
                ],
            )
            .map_err(|e| {
                // Surface the UNIQUE(source, target, type) contract clearly,
                // covering the mirror-image row of symmetric types.
                let mapped = crate::map_error(e);
                match &mapped {
                    assetmesh_core::AppError::Conflict { .. } => {
                        assetmesh_core::AppError::conflict(format!(
                            "relation {} between {} and {} already exists",
                            relation.relation_type,
                            relation.source_asset_id,
                            relation.target_asset_id
                        ))
                    }
                    other => other.clone(),
                }
            })?;
        Ok(())
    }

    fn update(&mut self, relation: &Relation) -> AppResult<()> {
        relation.validate()?;
        relation.ensure_canonical()?;
        let changed = self
            .conn
            .execute(
                "UPDATE relations SET source_asset_id = ?2, target_asset_id = ?3, \
                 relation_type = ?4, note = ?5, provenance = ?6, created_at = ?7 WHERE id = ?1",
                rusqlite::params![
                    uuid_to_string(relation.id.as_uuid()),
                    uuid_to_string(relation.source_asset_id.as_uuid()),
                    uuid_to_string(relation.target_asset_id.as_uuid()),
                    relation.relation_type.as_str(),
                    relation.note,
                    relation.provenance.as_str(),
                    ts_to_string(relation.created_at),
                ],
            )
            .map_err(crate::map_error)?;
        if changed == 0 {
            return Err(AppError::not_found("relation", relation.id));
        }
        Ok(())
    }

    fn delete(&mut self, id: RelationId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM relations WHERE id = ?1",
                [uuid_to_string(id.as_uuid())],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
