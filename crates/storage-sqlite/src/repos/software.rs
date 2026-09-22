//! SQLite implementation of `SoftwareRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::software::{
    InstallSource, SoftwareCategory, SoftwareEntry, SoftwareRecord,
};
use assetmesh_core::ports::repos::{
    SoftwareFilter, SoftwareListRow, SoftwareReader, SoftwareRepository, SoftwareSort,
};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;

pub struct SqliteSoftwareRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

const ASSET_COLS: &str =
    "a.id, a.kind, a.name, a.summary, a.lifecycle_state, a.revision, a.created_at, \
     a.updated_at, a.archived_at, a.merged_into_asset_id";
pub(crate) const SOFTWARE_COLS: &str =
    "s.category, s.install_source, s.version, s.install_location, s.executable_path, \
     s.purpose, s.notes, s.discovered_at, s.installed_at, s.architecture";

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

pub(crate) fn parse_record(
    asset_id: AssetId,
    row: &rusqlite::Row,
    offset: usize,
) -> AppResult<SoftwareRecord> {
    let category: String = col(row, offset)?;
    let install_source: String = col(row, offset + 1)?;
    let version: Option<String> = col(row, offset + 2)?;
    let install_location: Option<String> = col(row, offset + 3)?;
    let executable_path: Option<String> = col(row, offset + 4)?;
    let purpose: Option<String> = col(row, offset + 5)?;
    let notes: Option<String> = col(row, offset + 6)?;
    let discovered_at: Option<String> = col(row, offset + 7)?;
    let installed_at: Option<String> = col(row, offset + 8)?;
    let architecture: Option<String> = col(row, offset + 9)?;

    Ok(SoftwareRecord {
        asset_id,
        category: SoftwareCategory::parse(&category).ok_or_else(|| {
            AppError::storage(format!("unknown stored software category: {category}"))
        })?,
        install_source: InstallSource::parse(&install_source).ok_or_else(|| {
            AppError::storage(format!("unknown stored install_source: {install_source}"))
        })?,
        version,
        install_location,
        executable_path,
        purpose,
        notes,
        discovered_at: match &discovered_at {
            Some(t) => Some(ts_from_string(t)?),
            None => None,
        },
        installed_at: match &installed_at {
            Some(t) => Some(ts_from_string(t)?),
            None => None,
        },
        architecture,
    })
}

fn record_params(record: &SoftwareRecord) -> Vec<Box<dyn rusqlite::ToSql>> {
    vec![
        Box::new(uuid_to_string(record.asset_id.as_uuid())),
        Box::new(record.category.as_str().to_string()),
        Box::new(record.install_source.as_str().to_string()),
        Box::new(record.version.clone()),
        Box::new(record.install_location.clone()),
        Box::new(record.executable_path.clone()),
        Box::new(record.purpose.clone()),
        Box::new(record.notes.clone()),
        Box::new(record.discovered_at.map(ts_to_string)),
        Box::new(record.installed_at.map(ts_to_string)),
        Box::new(record.architecture.clone()),
    ]
}

impl SoftwareReader for SqliteSoftwareRepo<'_> {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<SoftwareRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.category, s.install_source, s.version, s.install_location, \
                      s.executable_path, s.purpose, s.notes, s.discovered_at, s.installed_at, \
                      s.architecture FROM software_records s WHERE s.asset_id = ?1",
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

    fn list(&mut self, filter: &SoftwareFilter) -> AppResult<Vec<SoftwareListRow>> {
        let mut sql = format!(
            "SELECT {ASSET_COLS}, {SOFTWARE_COLS} FROM software_records s \
             JOIN assets a ON a.id = s.asset_id WHERE 1=1"
        );
        let mut params: Vec<String> = Vec::new();

        if let Some(category) = filter.category {
            params.push(category.as_str().to_string());
            sql.push_str(&format!(" AND s.category = ?{}", params.len()));
        }
        if let Some(install_source) = filter.install_source {
            params.push(install_source.as_str().to_string());
            sql.push_str(&format!(" AND s.install_source = ?{}", params.len()));
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
            SoftwareSort::UpdatedDesc => sql.push_str(" ORDER BY a.updated_at DESC, a.id DESC"),
            SoftwareSort::TitleAsc => sql.push_str(" ORDER BY a.name COLLATE NOCASE ASC, a.id ASC"),
        }

        let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(refs.as_slice(), |row| {
                let asset = crate::repos::asset::row_to_asset(row)?;
                // asset columns consumed 0..10; software columns start at 10
                let record = crate::repos::app_row(parse_record(asset.id, row, 10))?;
                Ok((asset, record))
            })
            .map_err(crate::map_error)?;

        let mut rows_with_assets = Vec::new();
        for row in rows {
            let (asset, record) = row.map_err(crate::map_error)?;
            rows_with_assets.push((asset, record));
        }

        // Tags for the whole page in one query (see `repos::batch_tags`): a
        // per-row query here made the list O(rows) round-trips.
        let asset_ids: Vec<AssetId> = rows_with_assets.iter().map(|(a, _)| a.id).collect();
        let tags_by_asset = crate::repos::batch_tags(self.conn, &asset_ids)?;

        let mut out = Vec::with_capacity(rows_with_assets.len());
        for (asset, record) in rows_with_assets {
            let tags = tags_by_asset.get(&asset.id).cloned().unwrap_or_default();
            out.push(SoftwareListRow {
                entry: SoftwareEntry { asset, record },
                tags,
            });
        }
        Ok(out)
    }

    fn list_all(&mut self) -> AppResult<Vec<SoftwareRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.asset_id, s.category, s.install_source, s.version, s.install_location, \
                      s.executable_path, s.purpose, s.notes, s.discovered_at, s.installed_at, \
                      s.architecture FROM software_records s ORDER BY s.asset_id",
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

impl SqliteSoftwareRepo<'_> {
    /// Enforces category ↔ asset-kind compatibility at the storage boundary
    /// so even direct UnitOfWork writes cannot attach a software record to
    /// an asset of another module (e.g. a CLI record on a `media.movie`
    /// asset). An asset that does not exist yet is left to the foreign key.
    fn ensure_category_matches_asset(&self, record: &SoftwareRecord) -> AppResult<()> {
        let kind: Option<String> = self
            .conn
            .query_row(
                "SELECT kind FROM assets WHERE id = ?1",
                [uuid_to_string(record.asset_id.as_uuid())],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(crate::map_error)?;
        if let Some(kind) = kind {
            let expected = record.category.asset_kind().as_str();
            if kind != expected {
                return Err(assetmesh_core::AppError::conflict(format!(
                    "software record category {} requires asset kind {expected}, but asset {} has kind {kind}",
                    record.category, record.asset_id
                )));
            }
        }
        Ok(())
    }
}

impl SoftwareRepository for SqliteSoftwareRepo<'_> {
    fn upsert(&mut self, record: &SoftwareRecord) -> AppResult<()> {
        // Enforce the invariants at the repository boundary too: a direct
        // UnitOfWork write must not bypass normalization. The port takes the
        // record by shared reference, so the normalized copy is what is
        // stored while callers keep their own (already canonical) value.
        let mut normalized = record.clone();
        normalized.validate()?;
        self.ensure_category_matches_asset(&normalized)?;
        let params = record_params(&normalized);
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        self.conn
            .execute(
                "INSERT INTO software_records (asset_id, category, install_source, version, \
                 install_location, executable_path, purpose, notes, discovered_at, \
                 installed_at, architecture) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
                 ON CONFLICT(asset_id) DO UPDATE SET category=?2, install_source=?3, \
                 version=?4, install_location=?5, executable_path=?6, purpose=?7, notes=?8, \
                 discovered_at=?9, installed_at=?10, architecture=?11",
                refs.as_slice(),
            )
            .map_err(crate::map_error)?;
        Ok(())
    }

    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM software_records WHERE asset_id = ?1",
                [uuid_to_string(asset_id.as_uuid())],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
}
