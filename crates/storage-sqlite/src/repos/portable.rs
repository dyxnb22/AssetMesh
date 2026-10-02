//! A row cursor over the frozen export format; no full-table collections.
use crate::repos;
use assetmesh_core::{application::portable::*, domain::ids::AssetId, AppError, AppResult};
use rusqlite::Connection;
use std::collections::HashSet;

pub(crate) fn private_asset_ids(conn: &Connection) -> AppResult<HashSet<String>> {
    let mut statement = conn
        .prepare(
            "WITH RECURSIVE private(id) AS (
        SELECT asset_id FROM info_records WHERE info_type='api_key'
        UNION SELECT a.id FROM assets a JOIN private p ON a.merged_into_asset_id=p.id
    ) SELECT id FROM private LIMIT 100001",
        )
        .map_err(crate::map_error)?;
    let ids = statement
        .query_map([], |row| row.get(0))
        .map_err(crate::map_error)?
        .collect::<Result<HashSet<String>, _>>()
        .map_err(crate::map_error)?;
    if ids.len() > MAX_BUNDLE_RECORDS {
        return Err(AppError::validation(
            "portable export exceeds its record limit",
        ));
    }
    Ok(ids)
}

pub(crate) fn visit(
    conn: &Connection,
    visit: &mut dyn FnMut(PortableRow) -> AppResult<()>,
) -> AppResult<()> {
    let mut count = 0usize;
    macro_rules! scan {
        ($sql:expr, $decode:expr) => {{
            let mut statement = conn.prepare($sql).map_err(crate::map_error)?;
            let mut rows = statement.query([]).map_err(crate::map_error)?;
            while let Some(row) = rows.next().map_err(crate::map_error)? {
                count += 1;
                if count > MAX_BUNDLE_RECORDS {
                    return Err(AppError::validation(
                        "portable export exceeds its record limit",
                    ));
                }
                // Reject oversized stored fields before copying or JSON decoding.
                for index in 0..row.as_ref().column_count() {
                    match row.get_ref(index).map_err(crate::map_error)? {
                        rusqlite::types::ValueRef::Text(bytes)
                        | rusqlite::types::ValueRef::Blob(bytes)
                            if bytes.len() > 1024 * 1024 =>
                        {
                            return Err(AppError::validation(
                                "stored export field exceeds the 1 MiB limit",
                            ))
                        }
                        _ => {}
                    }
                }
                visit(($decode)(row)?)?;
            }
        }};
    }
    let id = |row: &rusqlite::Row| -> AppResult<AssetId> {
        let text: String = row.get(0).map_err(crate::map_error)?;
        Ok(AssetId::from_uuid(repos::uuid_from_string(&text)?))
    };
    scan!("SELECT id,kind,name,summary,lifecycle_state,revision,created_at,updated_at,archived_at,merged_into_asset_id FROM assets ORDER BY id", |row| -> AppResult<_> {
        Ok(PortableRow::Asset(PortableAssetV1::from_domain(&repos::asset::parse_asset(row)?)))
    });
    scan!(
        &format!(
            "SELECT m.asset_id, {} FROM media_records m ORDER BY m.asset_id",
            repos::media::MEDIA_COLS
        ),
        |row| -> AppResult<_> {
            Ok(PortableRow::Media(PortableMediaRecordV1::from_domain(
                &repos::media::parse_record(id(row)?, row, 1)?,
            )))
        }
    );
    scan!(
        &format!(
            "SELECT s.asset_id, {} FROM software_records s ORDER BY s.asset_id",
            repos::software::SOFTWARE_COLS
        ),
        |row| -> AppResult<_> {
            Ok(PortableRow::Software(
                PortableSoftwareRecordV1::from_domain(&repos::software::parse_record(
                    id(row)?,
                    row,
                    1,
                )?),
            ))
        }
    );
    scan!(
        &format!(
            "SELECT s.asset_id, {} FROM service_records s ORDER BY s.asset_id",
            repos::service::SERVICE_COLS
        ),
        |row| -> AppResult<_> {
            Ok(PortableRow::Service(PortableServiceRecordV1::from_domain(
                &repos::service::parse_record(id(row)?, row, 1)?,
            )))
        }
    );
    scan!("SELECT i.asset_id,i.info_type,i.value,i.notes FROM info_records i JOIN assets a ON a.id=i.asset_id WHERE a.lifecycle_state!='merged' ORDER BY i.asset_id", |row: &rusqlite::Row| -> AppResult<_> {
        Ok(PortableRow::Info(PortableInfoRecordV1::from_domain(&repos::info::parse_record(id(row)?,row.get(1).map_err(crate::map_error)?,row.get(2).map_err(crate::map_error)?,row.get(3).map_err(crate::map_error)?)?)))
    });
    scan!("SELECT id,asset_id,namespace,external_id,source_url,metadata,created_at,updated_at FROM external_refs ORDER BY namespace,external_id,id", |row| -> AppResult<_> {
        Ok(PortableRow::ExternalRef(PortableExternalRefV1::from_domain(&repos::external_ref::parse_ref_inner(row)?)))
    });
    scan!("SELECT id,occurred_at,event_type,asset_id,actor,payload FROM activity_events ORDER BY occurred_at,id", |row| -> AppResult<_> {
        Ok(PortableRow::Activity(PortableActivityEventV1::from_domain(&repos::activity::parse_event_inner(row)?)))
    });
    scan!(
        "SELECT id,name,created_at FROM tags ORDER BY id",
        |row| -> AppResult<_> {
            Ok(PortableRow::Tag(PortableTagV1::from_domain(
                &repos::tag::parse_tag_inner(row)?,
            )))
        }
    );
    scan!(
        "SELECT asset_id,tag_id FROM asset_tags ORDER BY asset_id,tag_id",
        |row: &rusqlite::Row| -> AppResult<_> {
            Ok(PortableRow::Membership(AssetTagRow {
                asset_id: row.get(0).map_err(crate::map_error)?,
                tag_id: row.get(1).map_err(crate::map_error)?,
            }))
        }
    );
    scan!("SELECT id,source_asset_id,target_asset_id,relation_type,note,provenance,created_at FROM relations ORDER BY id", |row| -> AppResult<_> {
        Ok(PortableRow::Relation(PortableRelationV1::from_domain(&repos::relation::parse_relation(row)?)))
    });
    Ok(())
}
