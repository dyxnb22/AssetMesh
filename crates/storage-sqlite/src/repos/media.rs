//! SQLite implementation of `MediaRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::{MediaEntry, MediaRecord, MediaStatus, MediaType, Progress};
use assetmesh_core::ports::repos::{
    MediaFilter, MediaListRow, MediaReader, MediaRepository, MediaSort,
};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;

pub struct SqliteMediaRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

const ASSET_COLS: &str =
    "a.id, a.kind, a.name, a.summary, a.lifecycle_state, a.revision, a.created_at, \
     a.updated_at, a.archived_at, a.merged_into_asset_id";
pub(crate) const MEDIA_COLS: &str = "m.media_type, m.status, m.rating, m.year, m.platform, \
                          m.progress_current, m.progress_total, m.progress_unit, m.notes, \
                          m.started_at, m.completed_at";

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

pub(crate) fn parse_record(asset_id: AssetId, row: &rusqlite::Row, offset: usize) -> AppResult<MediaRecord> {
    let media_type: String = col(row, offset)?;
    let status: String = col(row, offset + 1)?;
    let rating: Option<f64> = col(row, offset + 2)?;
    let year: Option<i32> = col(row, offset + 3)?;
    let platform: Option<String> = col(row, offset + 4)?;
    let progress_current: Option<f64> = col(row, offset + 5)?;
    let progress_total: Option<f64> = col(row, offset + 6)?;
    let progress_unit: Option<String> = col(row, offset + 7)?;
    let notes: Option<String> = col(row, offset + 8)?;
    let started_at: Option<String> = col(row, offset + 9)?;
    let completed_at: Option<String> = col(row, offset + 10)?;

    Ok(MediaRecord {
        asset_id,
        media_type: MediaType::parse(&media_type)
            .ok_or_else(|| AppError::storage(format!("unknown stored media_type: {media_type}")))?,
        status: MediaStatus::parse(&status)
            .ok_or_else(|| AppError::storage(format!("unknown stored status: {status}")))?,
        rating,
        year,
        platform,
        progress: Progress {
            current: progress_current,
            total: progress_total,
            unit: progress_unit,
        },
        notes,
        started_at: match &started_at {
            Some(t) => Some(ts_from_string(t)?),
            None => None,
        },
        completed_at: match &completed_at {
            Some(t) => Some(ts_from_string(t)?),
            None => None,
        },
    })
}

fn record_params(record: &MediaRecord) -> Vec<Box<dyn rusqlite::ToSql>> {
    vec![
        Box::new(uuid_to_string(record.asset_id.as_uuid())),
        Box::new(record.media_type.as_str().to_string()),
        Box::new(record.status.as_str().to_string()),
        Box::new(record.rating),
        Box::new(record.year),
        Box::new(record.platform.clone()),
        Box::new(record.progress.current),
        Box::new(record.progress.total),
        Box::new(record.progress.unit.clone()),
        Box::new(record.notes.clone()),
        Box::new(record.started_at.map(ts_to_string)),
        Box::new(record.completed_at.map(ts_to_string)),
    ]
}

impl MediaReader for SqliteMediaRepo<'_> {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<MediaRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT m.media_type, m.status, m.rating, m.year, m.platform, \
                      m.progress_current, m.progress_total, m.progress_unit, m.notes, \
                      m.started_at, m.completed_at FROM media_records m WHERE m.asset_id = ?1",
            )
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(asset_id.as_uuid())], |row| {
                crate::repos::app_row(parse_record(asset_id, row, 0))
            })
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(row_result(row)?)),
            None => Ok(None),
        }
    }
    fn list(&mut self, filter: &MediaFilter) -> AppResult<Vec<MediaListRow>> {
        let mut sql = format!(
            "SELECT {ASSET_COLS}, {MEDIA_COLS} FROM media_records m \
             JOIN assets a ON a.id = m.asset_id WHERE 1=1"
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(media_type) = filter.media_type {
            params.push(media_type.as_str().to_string());
            sql.push_str(&format!(" AND m.media_type = ?{}", params.len()));
        }
        if let Some(status) = filter.status {
            params.push(status.as_str().to_string());
            sql.push_str(&format!(" AND m.status = ?{}", params.len()));
        }
        if let Some(platform) = &filter.platform {
            params.push(platform.clone());
            sql.push_str(&format!(" AND m.platform = ?{}", params.len()));
        }
        if let Some(tag) = &filter.tag {
            params.push(tag.to_lowercase());
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM asset_tags ft JOIN tags t ON t.id = ft.tag_id \
                 WHERE ft.asset_id = a.id AND lower(t.name) = ?{})",
                params.len()
            ));
        }

        match filter.sort {
            MediaSort::UpdatedDesc => sql.push_str(" ORDER BY a.updated_at DESC, a.id DESC"),
            MediaSort::TitleAsc => sql.push_str(" ORDER BY a.name COLLATE NOCASE ASC, a.id ASC"),
            MediaSort::RatingDesc => {
                sql.push_str(" ORDER BY m.rating IS NULL, m.rating DESC, a.id ASC")
            }
            MediaSort::CompletedDesc => {
                sql.push_str(" ORDER BY m.completed_at IS NULL, m.completed_at DESC, a.id ASC")
            }
        }

        let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(refs.as_slice(), |row| {
                let asset = crate::repos::asset::row_to_asset(row)?;
                // asset columns consumed 0..10; media columns start at 10
                let record = crate::repos::app_row(parse_record(asset.id, row, 10))?;
                Ok((asset, record))
            })
            .map_err(crate::map_error)?;

        let mut rows_with_assets = Vec::new();
        for row in rows {
            let (asset, record) = row.map_err(crate::map_error)?;
            rows_with_assets.push((asset, record));
        }

        // Tags for the whole page in one query. A per-row query here made the
        // list O(rows) round-trips, which shows up as a page that gets slower
        // as the library grows.
        let asset_ids: Vec<AssetId> = rows_with_assets.iter().map(|(a, _)| a.id).collect();
        let tags_by_asset = crate::repos::batch_tags(self.conn, &asset_ids)?;

        let mut out = Vec::with_capacity(rows_with_assets.len());
        for (asset, record) in rows_with_assets {
            let tags = tags_by_asset.get(&asset.id).cloned().unwrap_or_default();
            out.push(MediaListRow {
                entry: MediaEntry { asset, record },
                tags,
            });
        }
        Ok(out)
    }
    fn list_all(&mut self) -> AppResult<Vec<MediaRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT m.asset_id, m.media_type, m.status, m.rating, m.year, m.platform, \
                      m.progress_current, m.progress_total, m.progress_unit, m.notes, \
                      m.started_at, m.completed_at FROM media_records m ORDER BY m.asset_id",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([], |row| {
                let asset_id_str: String = row.get(0)?;
                let asset_id = AssetId::from_uuid(
                    uuid_from_string(&asset_id_str).map_err(crate::repos::box_app_error)?,
                );
                crate::repos::app_row(parse_record(asset_id, row, 1))
            })
            .map_err(crate::map_error)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row_result(row)?);
        }
        Ok(records)
    }
}

impl MediaRepository for SqliteMediaRepo<'_> {
    fn upsert(&mut self, record: &MediaRecord) -> AppResult<()> {
        record.validate()?;
        let params = record_params(record);
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        self.conn
            .execute(
                "INSERT INTO media_records (asset_id, media_type, status, rating, year, platform, \
                 progress_current, progress_total, progress_unit, notes, started_at, completed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
                 ON CONFLICT(asset_id) DO UPDATE SET media_type=?2, status=?3, rating=?4, \
                 year=?5, platform=?6, progress_current=?7, progress_total=?8, progress_unit=?9, \
                 notes=?10, started_at=?11, completed_at=?12",
                refs.as_slice(),
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM media_records WHERE asset_id = ?1",
                [uuid_to_string(asset_id.as_uuid())],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
