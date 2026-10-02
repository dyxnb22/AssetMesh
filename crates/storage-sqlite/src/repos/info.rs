use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::info::{InfoEntry, InfoRecord, InfoType};
use assetmesh_core::ports::repos::{InfoReader, InfoRepository};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;

use crate::repos::{app_row, row_result, uuid_to_string};

pub struct SqliteInfoRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

pub(crate) fn parse_record(
    asset_id: AssetId,
    kind: String,
    value: String,
    notes: Option<String>,
) -> AppResult<InfoRecord> {
    Ok(InfoRecord {
        asset_id,
        info_type: InfoType::parse(&kind)
            .ok_or_else(|| AppError::storage(format!("unknown stored information type: {kind}")))?,
        value,
        notes,
    })
}

impl InfoReader for SqliteInfoRepo<'_> {
    fn existing_ids(&mut self, ids: &[AssetId]) -> AppResult<Vec<AssetId>> {
        crate::repos::existing_module_ids(self.conn, "info_records", ids)
    }
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<InfoRecord>> {
        let result = self.conn.query_row(
            "SELECT info_type, value, notes FROM info_records WHERE asset_id = ?1",
            [uuid_to_string(asset_id.as_uuid())],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        );
        match result {
            Ok((kind, value, notes)) => Ok(Some(parse_record(asset_id, kind, value, notes)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(crate::map_error(error)),
        }
    }

    fn list(&mut self) -> AppResult<Vec<InfoEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.kind, a.name, a.summary, a.lifecycle_state, a.revision, a.created_at, \
                    a.updated_at, a.archived_at, a.merged_into_asset_id, \
                    i.info_type, i.value, i.notes \
             FROM info_records i JOIN assets a ON a.id = i.asset_id \
             WHERE a.lifecycle_state != 'merged' ORDER BY a.updated_at DESC, a.id ASC"
        ).map_err(crate::map_error)?;
        let rows = stmt
            .query_map([], |row| {
                let asset = app_row(crate::repos::asset::parse_asset(row))?;
                let record = app_row(parse_record(
                    asset.id,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))?;
                Ok(InfoEntry {
                    asset,
                    record,
                    tags: Vec::new(),
                })
            })
            .map_err(crate::map_error)?;
        let mut entries = rows.map(row_result).collect::<AppResult<Vec<_>>>()?;
        let ids: Vec<_> = entries.iter().map(|entry| entry.asset.id).collect();
        let tags = crate::repos::batch_tags(self.conn, &ids)?;
        for entry in &mut entries {
            entry.tags = tags.get(&entry.asset.id).cloned().unwrap_or_default();
        }
        Ok(entries)
    }
}

impl InfoRepository for SqliteInfoRepo<'_> {
    fn upsert(&mut self, record: &InfoRecord) -> AppResult<()> {
        let mut record = record.clone();
        record.validate()?;
        let kind: String = self
            .conn
            .query_row(
                "SELECT kind FROM assets WHERE id = ?1",
                [uuid_to_string(record.asset_id.as_uuid())],
                |row| row.get(0),
            )
            .map_err(crate::map_error)?;
        if kind != AssetKind::InfoItem.as_str() {
            return Err(AppError::conflict(
                "information details require an info.item asset",
            ));
        }
        self.conn.execute(
            "INSERT INTO info_records (asset_id, info_type, value, notes) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(asset_id) DO UPDATE SET info_type = excluded.info_type, value = excluded.value, notes = excluded.notes",
            rusqlite::params![uuid_to_string(record.asset_id.as_uuid()), record.info_type.as_str(), record.value, record.notes],
        ).map_err(crate::map_error)?;
        Ok(())
    }

    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM info_records WHERE asset_id = ?1",
                [uuid_to_string(asset_id.as_uuid())],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
