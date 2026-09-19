//! SQLite implementation of `ActivityRepository`.

use crate::repos::{row_result, ts_from_string, ts_to_string, uuid_from_string, uuid_to_string};
use assetmesh_core::domain::activity::ActivityEvent;
use assetmesh_core::domain::ids::{ActivityId, AssetId};
use assetmesh_core::ports::repos::{ActivityReader, ActivityRepository};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;

pub struct SqliteActivityRepo<'conn> {
    pub(crate) conn: &'conn Connection,
}

fn col<T: rusqlite::types::FromSql>(row: &rusqlite::Row, idx: usize) -> AppResult<T> {
    row.get(idx).map_err(crate::map_error)
}

fn parse_event(row: &rusqlite::Row) -> rusqlite::Result<ActivityEvent> {
    crate::repos::app_row(parse_event_inner(row))
}

fn parse_event_inner(row: &rusqlite::Row) -> AppResult<ActivityEvent> {
    let id: String = col(row, 0)?;
    let occurred_at: String = col(row, 1)?;
    let event_type: String = col(row, 2)?;
    let asset_id: Option<String> = col(row, 3)?;
    let actor: String = col(row, 4)?;
    let payload: String = col(row, 5)?;

    Ok(ActivityEvent {
        id: ActivityId::from_uuid(uuid_from_string(&id)?),
        occurred_at: ts_from_string(&occurred_at)?,
        event_type,
        asset_id: match &asset_id {
            Some(text) => Some(AssetId::from_uuid(uuid_from_string(text)?)),
            None => None,
        },
        actor,
        payload: serde_json::from_str(&payload)
            .map_err(|e| AppError::storage(format!("invalid stored activity payload: {e}")))?,
    })
}

fn append_impl(conn: &Connection, event: &ActivityEvent) -> AppResult<()> {
    event.validate()?;
    conn.execute(
        "INSERT INTO activity_events (id, occurred_at, event_type, asset_id, actor, payload) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            uuid_to_string(event.id.as_uuid()),
            ts_to_string(event.occurred_at),
            event.event_type,
            event.asset_id.map(|id| uuid_to_string(id.as_uuid())),
            event.actor,
            serde_json::to_string(&event.payload).map_err(|e| AppError::storage(format!(
                "activity payload serialization failed: {e}"
            )))?,
        ],
    )
    .map_err(crate::map_error)?;
    Ok(())
}

impl ActivityReader for SqliteActivityRepo<'_> {
    fn list_for_asset(&mut self, asset_id: AssetId, limit: usize) -> AppResult<Vec<ActivityEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, occurred_at, event_type, asset_id, actor, payload \
                 FROM activity_events WHERE asset_id = ?1 \
                 ORDER BY occurred_at DESC, id DESC LIMIT ?2",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![uuid_to_string(asset_id.as_uuid()), limit as i64],
                parse_event,
            )
            .map_err(crate::map_error)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row_result(row)?);
        }
        Ok(events)
    }
    fn list_recent(&mut self, limit: usize) -> AppResult<Vec<ActivityEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, occurred_at, event_type, asset_id, actor, payload \
                 FROM activity_events ORDER BY occurred_at DESC, id DESC LIMIT ?1",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map([limit as i64], parse_event)
            .map_err(crate::map_error)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row_result(row)?);
        }
        Ok(events)
    }
    fn list_all(&mut self) -> AppResult<Vec<ActivityEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, occurred_at, event_type, asset_id, actor, payload \
                 FROM activity_events ORDER BY occurred_at ASC, id ASC",
            )
            .map_err(crate::map_error)?;
        let rows = stmt.query_map([], parse_event).map_err(crate::map_error)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row_result(row)?);
        }
        Ok(events)
    }
    fn get(&mut self, id: ActivityId) -> AppResult<Option<ActivityEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, occurred_at, event_type, asset_id, actor, payload \
                 FROM activity_events WHERE id = ?1",
            )
            .map_err(crate::map_error)?;
        let mut rows = stmt
            .query_map([uuid_to_string(id.as_uuid())], parse_event)
            .map_err(crate::map_error)?;
        match rows.next() {
            Some(row) => Ok(Some(row_result(row)?)),
            None => Ok(None),
        }
    }
}

impl ActivityRepository for SqliteActivityRepo<'_> {
    fn append(&mut self, event: &ActivityEvent) -> AppResult<()> {
        append_impl(self.conn, event)
    }
    fn upsert(&mut self, event: &ActivityEvent) -> AppResult<()> {
        let changed = self
            .conn
            .execute(
                "UPDATE activity_events SET occurred_at = ?2, event_type = ?3, asset_id = ?4, \
                 actor = ?5, payload = ?6 WHERE id = ?1",
                rusqlite::params![
                    uuid_to_string(event.id.as_uuid()),
                    ts_to_string(event.occurred_at),
                    event.event_type,
                    event.asset_id.map(|id| uuid_to_string(id.as_uuid())),
                    event.actor,
                    serde_json::to_string(&event.payload).map_err(|e| AppError::storage(
                        format!("activity payload serialization failed: {e}")
                    ))?,
                ],
            )
            .map_err(crate::map_error)?;
        if changed == 0 {
            append_impl(self.conn, event)?;
        }
        Ok(())
    }
}
