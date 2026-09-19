//! SQLite implementation of `TagRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::ids::{AssetId, TagId};
use assetmesh_core::domain::tag::Tag;
use assetmesh_core::ports::repos::{TagReader, TagRepository};
use assetmesh_core::AppResult;
use rusqlite::Connection;

pub struct SqliteTagRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

fn parse_tag(row: &rusqlite::Row) -> rusqlite::Result<Tag> {
    crate::repos::app_row(parse_tag_inner(row))
}

fn parse_tag_inner(row: &rusqlite::Row) -> AppResult<Tag> {
    let id: String = col(row, 0)?;
    let name: String = col(row, 1)?;
    let created_at: String = col(row, 2)?;
    Ok(Tag {
        id: TagId::from_uuid(uuid_from_string(&id)?),
        name,
        created_at: ts_from_string(&created_at)?,
    })
}

impl TagReader for SqliteTagRepo<'_> {
    fn find_by_name(&mut self, name: &str) -> AppResult<Option<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM tags WHERE name = ?1")
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([name], parse_tag)
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(row_result(row)?)),
            None => Ok(None),
        }
    }
    fn get(&mut self, id: TagId) -> AppResult<Option<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM tags WHERE id = ?1")
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(id.as_uuid())], parse_tag)
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(row_result(row)?)),
            None => Ok(None),
        }
    }
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.id, t.name, t.created_at FROM asset_tags at \
                 JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = ?1 ORDER BY t.name",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([uuid_to_string(asset_id.as_uuid())], parse_tag)
            .map_err(crate::map_error)?;
        let mut tags = Vec::new();
        for row in rows {
            tags.push(row_result(row)?);
        }
        Ok(tags)
    }
    fn list_all(&mut self) -> AppResult<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM tags ORDER BY name")
            .map_err(crate::map_error)?;
        let rows = stmt.query_map([], parse_tag).map_err(crate::map_error)?;
        let mut tags = Vec::new();
        for row in rows {
            tags.push(row_result(row)?);
        }
        Ok(tags)
    }
    fn list_memberships(&mut self) -> AppResult<Vec<(AssetId, TagId)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT asset_id, tag_id FROM asset_tags ORDER BY asset_id, tag_id")
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(crate::map_error)?;
        let mut pairs = Vec::new();
        for row in rows {
            let (asset, tag) = row_result(row)?;
            pairs.push((
                AssetId::from_uuid(uuid_from_string(&asset)?),
                TagId::from_uuid(uuid_from_string(&tag)?),
            ));
        }
        Ok(pairs)
    }
}

impl TagRepository for SqliteTagRepo<'_> {
    fn ensure(&mut self, name: &str) -> AppResult<Tag> {
        let name = name.trim();
        if name.is_empty() {
            return Err(assetmesh_core::AppError::validation(
                "tag name must not be empty",
            ));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM tags WHERE name = ?1")
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([name], parse_tag)
            .map_err(crate::map_error)?;
        if let Some(row) = rows.next() {
            return row_result(row);
        }

        let tag = Tag::new(name, chrono::Utc::now());
        self.insert(&tag)?;
        Ok(tag)
    }
    fn insert(&mut self, tag: &Tag) -> AppResult<()> {
        tag.validate()?;
        self.conn
            .execute(
                "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    uuid_to_string(tag.id.as_uuid()),
                    tag.name,
                    ts_to_string(tag.created_at),
                ],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
    fn update(&mut self, tag: &Tag) -> AppResult<()> {
        tag.validate()?;
        let changed = self
            .conn
            .execute(
                "UPDATE tags SET name = ?2, created_at = ?3 WHERE id = ?1",
                rusqlite::params![
                    uuid_to_string(tag.id.as_uuid()),
                    tag.name,
                    ts_to_string(tag.created_at),
                ],
            )
            .map_err(crate::map_error)?;
        if changed == 0 {
            return Err(assetmesh_core::AppError::not_found("tag", tag.id));
        }
        Ok(())
    }
    fn attach(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()> {
        self.insert_membership(asset_id, tag_id)
    }
    fn detach(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
                rusqlite::params![
                    uuid_to_string(asset_id.as_uuid()),
                    uuid_to_string(tag_id.as_uuid()),
                ],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
    fn insert_membership(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
                rusqlite::params![
                    uuid_to_string(asset_id.as_uuid()),
                    uuid_to_string(tag_id.as_uuid()),
                ],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
