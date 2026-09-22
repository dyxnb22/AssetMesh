//! Storage-backed SQLite implementation of `LibraryReadPort` (P5-04, R4).
//!
//! Provides SQL-level `LIMIT :limit OFFSET :offset` pagination, two-stage counting,
//! and batch hydration (tags & subtitles) restricted strictly to the current page.

use assetmesh_core::application::projection::{media_subtitle, service_subtitle, software_subtitle};
use assetmesh_core::domain::asset::{AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::repos::{
    AssetSummary, LifecycleFilter, LibraryModule, LibraryQuery, LibraryReadPort, LibrarySort, Page,
};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;
use std::collections::HashMap;

use crate::repos::media::MEDIA_COLS;
use crate::repos::service::SERVICE_COLS;
use crate::repos::software::SOFTWARE_COLS;
use crate::repos::{row_result, ts_from_string, uuid_from_string, uuid_to_string};

fn resolve_allowed_kinds(modules: &[LibraryModule], kinds: &[AssetKind]) -> Vec<AssetKind> {
    let mut allowed = Vec::new();
    let mod_set = if modules.is_empty() {
        LibraryModule::ALL.to_vec()
    } else {
        modules.to_vec()
    };

    let target_kinds = if kinds.is_empty() {
        AssetKind::ALL.to_vec()
    } else {
        kinds.to_vec()
    };

    for k in target_kinds {
        if mod_set.contains(&LibraryModule::of_kind(k)) && !allowed.contains(&k) {
            allowed.push(k);
        }
    }
    allowed
}

pub struct SqliteLibraryRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

impl LibraryReadPort for SqliteLibraryRepo<'_> {
    fn query_library(&mut self, query: &LibraryQuery) -> AppResult<Page<AssetSummary>> {
        let limit = query.page.effective_limit();
        let offset = query.page.offset;

        // Resolve allowed modules and kinds
        let allowed_kinds = resolve_allowed_kinds(&query.modules, &query.kinds);
        if allowed_kinds.is_empty() {
            return Ok(Page::empty(&query.page));
        }

        let mut where_clauses = Vec::new();
        let mut count_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // 1. Merged tombstones are never in the library under any filter
        match query.lifecycle {
            LifecycleFilter::Active => {
                where_clauses.push("a.lifecycle_state = 'active'".to_string());
            }
            LifecycleFilter::ActiveOrArchived => {
                where_clauses.push("a.lifecycle_state IN ('active', 'archived')".to_string());
            }
            LifecycleFilter::All => {
                where_clauses.push("a.lifecycle_state != 'merged'".to_string());
            }
        }

        // 2. Kinds filter (if restricted)
        if allowed_kinds.len() < AssetKind::ALL.len() {
            let placeholders = vec!["?"; allowed_kinds.len()].join(", ");
            where_clauses.push(format!("a.kind IN ({placeholders})"));
            for k in &allowed_kinds {
                count_params.push(Box::new(k.as_str().to_string()));
            }
        }

        // 3. Module record existence (only assets that have module typed details)
        where_clauses.push(
            "(
                (a.kind LIKE 'media.%' AND EXISTS (SELECT 1 FROM media_records m WHERE m.asset_id = a.id)) OR
                (a.kind LIKE 'software.%' AND EXISTS (SELECT 1 FROM software_records sw WHERE sw.asset_id = a.id)) OR
                (a.kind LIKE 'service.%' AND EXISTS (SELECT 1 FROM service_records sv WHERE sv.asset_id = a.id))
            )".to_string(),
        );

        // 4. Tags filter: each tag must match case-insensitively
        for tag in &query.tags {
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                where_clauses.push(
                    "EXISTS (
                        SELECT 1 FROM asset_tags at
                        JOIN tags t ON t.id = at.tag_id
                        WHERE at.asset_id = a.id AND LOWER(t.name) = LOWER(?)
                    )".to_string(),
                );
                count_params.push(Box::new(trimmed.to_string()));
            }
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // First stage: count total matching rows
        let count_sql = format!("SELECT COUNT(*) FROM assets a {where_sql}");
        let count_refs: Vec<&dyn rusqlite::ToSql> =
            count_params.iter().map(|p| p.as_ref()).collect();
        let total_count: usize = self
            .conn
            .query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))
            .map_err(crate::map_error)?;

        if total_count == 0 || offset >= total_count {
            return Ok(Page {
                items: Vec::new(),
                offset,
                limit,
                total: Some(total_count),
            });
        }

        // Second stage: paged rows with SQL LIMIT and OFFSET
        let order_by = match query.sort {
            LibrarySort::UpdatedDesc => "ORDER BY a.updated_at DESC, a.id ASC",
            LibrarySort::UpdatedAsc => "ORDER BY a.updated_at ASC, a.id ASC",
            LibrarySort::NameAsc => "ORDER BY LOWER(a.name) ASC, a.id ASC",
            LibrarySort::NameDesc => "ORDER BY LOWER(a.name) DESC, a.id ASC",
            LibrarySort::KindAsc => "ORDER BY a.kind ASC, LOWER(a.name) ASC, a.id ASC",
        };

        let page_sql = format!(
            "SELECT a.id, a.kind, a.name, a.lifecycle_state, a.revision, a.updated_at \
             FROM assets a {where_sql} {order_by} LIMIT ? OFFSET ?"
        );

        let mut page_params = count_params;
        page_params.push(Box::new(limit as i64));
        page_params.push(Box::new(offset as i64));

        let page_refs: Vec<&dyn rusqlite::ToSql> =
            page_params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&page_sql).map_err(crate::map_error)?;
        let rows = stmt
            .query_map(page_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let name: String = row.get(2)?;
                let lifecycle: String = row.get(3)?;
                let revision: i64 = row.get(4)?;
                let updated_at: String = row.get(5)?;
                Ok((id, kind, name, lifecycle, revision, updated_at))
            })
            .map_err(crate::map_error)?;

        struct PartialAsset {
            id: AssetId,
            kind: AssetKind,
            name: String,
            lifecycle: LifecycleState,
            revision: i64,
            updated_at: Timestamp,
        }

        let mut page_assets = Vec::with_capacity(limit);
        for r in rows {
            let (id_str, kind_str, name, lifecycle_str, revision, updated_at_str) = row_result(r)?;
            let id = AssetId::from_uuid(uuid_from_string(&id_str)?);
            let kind = AssetKind::parse(&kind_str)
                .ok_or_else(|| AppError::storage(format!("unknown stored kind: {kind_str}")))?;
            let lifecycle = match lifecycle_str.as_str() {
                "active" => LifecycleState::Active,
                "archived" => LifecycleState::Archived,
                "merged" => LifecycleState::Merged,
                other => {
                    return Err(AppError::storage(format!(
                        "unknown stored lifecycle: {other}"
                    )))
                }
            };
            let updated_at = ts_from_string(&updated_at_str)?;
            page_assets.push(PartialAsset {
                id,
                kind,
                name,
                lifecycle,
                revision,
                updated_at,
            });
        }

        if page_assets.is_empty() {
            return Ok(Page {
                items: Vec::new(),
                offset,
                limit,
                total: Some(total_count),
            });
        }

        // Hydrate batch tags (restricted to this page's assets)
        let asset_ids: Vec<AssetId> = page_assets.iter().map(|a| a.id).collect();
        let tags_by_asset = crate::repos::batch_tags(self.conn, &asset_ids)?;

        // Hydrate subtitles in batch per module
        let mut subtitles: HashMap<AssetId, Option<String>> = HashMap::new();

        let mut media_ids = Vec::new();
        let mut software_ids = Vec::new();
        let mut service_ids = Vec::new();

        for a in &page_assets {
            match a.kind.module() {
                "media" => media_ids.push(a.id),
                "software" => software_ids.push(a.id),
                "services" => service_ids.push(a.id),
                _ => {}
            }
        }

        if !media_ids.is_empty() {
            let placeholders = vec!["?"; media_ids.len()].join(", ");
            let sql = format!(
                "SELECT m.asset_id, {MEDIA_COLS} FROM media_records m WHERE m.asset_id IN ({placeholders})"
            );
            let params: Vec<String> = media_ids.iter().map(|id| uuid_to_string(id.as_uuid())).collect();
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
            let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
            let rows = stmt.query_map(refs.as_slice(), |row| {
                let id_str: String = row.get(0)?;
                let asset_id = AssetId::from_uuid(crate::repos::app_row(uuid_from_string(&id_str))?);
                let record = crate::repos::app_row(crate::repos::media::parse_record(asset_id, row, 1))?;
                Ok((asset_id, media_subtitle(&record)))
            }).map_err(crate::map_error)?;
            for r in rows {
                let (id, sub) = row_result(r)?;
                subtitles.insert(id, sub);
            }
        }

        if !software_ids.is_empty() {
            let placeholders = vec!["?"; software_ids.len()].join(", ");
            let sql = format!(
                "SELECT s.asset_id, {SOFTWARE_COLS} FROM software_records s WHERE s.asset_id IN ({placeholders})"
            );
            let params: Vec<String> = software_ids.iter().map(|id| uuid_to_string(id.as_uuid())).collect();
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
            let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
            let rows = stmt.query_map(refs.as_slice(), |row| {
                let id_str: String = row.get(0)?;
                let asset_id = AssetId::from_uuid(crate::repos::app_row(uuid_from_string(&id_str))?);
                let record = crate::repos::app_row(crate::repos::software::parse_record(asset_id, row, 1))?;
                Ok((asset_id, software_subtitle(&record)))
            }).map_err(crate::map_error)?;
            for r in rows {
                let (id, sub) = row_result(r)?;
                subtitles.insert(id, sub);
            }
        }

        if !service_ids.is_empty() {
            let placeholders = vec!["?"; service_ids.len()].join(", ");
            let sql = format!(
                "SELECT s.asset_id, {SERVICE_COLS} FROM service_records s WHERE s.asset_id IN ({placeholders})"
            );
            let params: Vec<String> = service_ids.iter().map(|id| uuid_to_string(id.as_uuid())).collect();
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
            let mut stmt = self.conn.prepare(&sql).map_err(crate::map_error)?;
            let rows = stmt.query_map(refs.as_slice(), |row| {
                let id_str: String = row.get(0)?;
                let asset_id = AssetId::from_uuid(crate::repos::app_row(uuid_from_string(&id_str))?);
                let record = crate::repos::app_row(crate::repos::service::parse_record(asset_id, row, 1))?;
                Ok((asset_id, service_subtitle(&record)))
            }).map_err(crate::map_error)?;
            for r in rows {
                let (id, sub) = row_result(r)?;
                subtitles.insert(id, sub);
            }
        }

        // Assemble AssetSummary in query order
        let items: Vec<AssetSummary> = page_assets
            .into_iter()
            .map(|a| {
                let tags = tags_by_asset.get(&a.id).cloned().unwrap_or_default();
                let subtitle = subtitles.get(&a.id).cloned().flatten();
                AssetSummary {
                    id: a.id,
                    kind: a.kind,
                    name: a.name,
                    lifecycle: a.lifecycle,
                    revision: a.revision,
                    subtitle,
                    tags,
                    updated_at: a.updated_at,
                }
            })
            .collect();

        Ok(Page {
            items,
            offset,
            limit,
            total: Some(total_count),
        })
    }
}
