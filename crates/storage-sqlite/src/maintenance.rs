//! Routine history retention; unknown and significant events stay intact.
use assetmesh_core::{domain::Timestamp, AppError, AppResult};
use rusqlite::Connection;

const RETENTION_SQL: &str = "WITH routine AS (
    SELECT id,occurred_at FROM activity_events
    WHERE actor != 'import' AND event_type IN (
        'media.started','media.progress_changed','media.completed',
        'media.paused','media.dropped','media.rating_changed'
    )
), retained AS (
    SELECT id FROM routine WHERE occurred_at >= ?1
    ORDER BY occurred_at DESC,id DESC LIMIT 10000
)";

pub(crate) fn candidates(conn: &Connection, before: &str) -> AppResult<usize> {
    conn.query_row(&format!("{RETENTION_SQL} SELECT COUNT(*) FROM routine WHERE id NOT IN (SELECT id FROM retained)"), [before], |row| row.get(0)).map_err(crate::map_error)
}

pub(crate) fn prune(conn: &mut Connection, before: &str) -> AppResult<usize> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(crate::map_error)?;
    let count = tx.execute(&format!("{RETENTION_SQL} DELETE FROM activity_events WHERE id IN (SELECT id FROM routine WHERE id NOT IN (SELECT id FROM retained))"), [before]).map_err(crate::map_error)?;
    tx.commit().map_err(crate::map_error)?;
    compact_if_useful(conn)?;
    Ok(count)
}

fn compact_if_useful(conn: &Connection) -> AppResult<()> {
    let pages: u64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(crate::map_error)?;
    let free: u64 = conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .map_err(crate::map_error)?;
    let size: u64 = conn
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .map_err(crate::map_error)?;
    if free.saturating_mul(size) >= 16 * 1024 * 1024 && free.saturating_mul(4) >= pages {
        // Reclaim only substantial free space. Lock contention postpones
        // compaction; successful retention is not reported as a failure.
        let _ = conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(PASSIVE);");
    }
    Ok(())
}

impl crate::SqliteFactory {
    /// A complete recovery snapshot is required before deleting routine rows.
    pub fn maintain_history(
        &self,
        now: Timestamp,
        before_prune: impl FnOnce() -> AppResult<()>,
    ) -> AppResult<usize> {
        let before = (now - chrono::Duration::days(180)).to_rfc3339();
        if self.with_raw_connection(|conn| candidates(conn, &before))?? == 0 {
            // Retry postponed compaction even when no more rows need pruning.
            self.with_raw_connection(|conn| {
                if self.writes_are_disabled() {
                    Ok(())
                } else {
                    compact_if_useful(conn)
                }
            })??;
            return Ok(0);
        }
        before_prune()?;
        self.with_raw_connection_mut(|conn| {
            if self.writes_are_disabled() {
                return Err(AppError::conflict(
                    "restart AssetMesh to use the restored library",
                ));
            }
            prune(conn, &before)
        })?
    }
}
