//! SQLite FTS5 implementation of the `SearchIndex` port (ADR 0006).
//!
//! The `search_documents` FTS5 table holds the derived projection. It is
//! disposable: `clear` + `replace_all` rebuild it from canonical data.
//! FTS5 (unicode61) handles word-based queries; a LIKE substring fallback
//! covers CJK and short substrings that tokenization cannot serve.

use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::search::{SearchDocument, SearchHit};
use assetmesh_core::ports::search::{SearchIndex, SearchReader};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;
use std::collections::HashSet;

pub struct SqliteSearchIndex<'conn> {
    pub(crate) conn: &'conn Connection,
}

impl SearchReader for SqliteSearchIndex<'_> {
    /// Search results are the union of FTS full-text matches (relevance
    /// ordered) and substring matches, deduplicated by asset. Running both
    /// deterministically guarantees that a tokenized hit never suppresses a
    /// substring-only match (CJK or partial words), and FTS errors are
    /// propagated rather than silently downgraded.
    fn search(&mut self, query: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut hits: Vec<SearchHit> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut push = |hit: SearchHit, seen: &mut HashSet<String>| -> bool {
            if hits.len() >= limit {
                return false;
            }
            if seen.insert(hit.asset_id.to_string()) {
                hits.push(hit);
            }
            true
        };

        if let Some(match_query) = build_match_query(query) {
            for hit in self.query_fts(&match_query, limit)? {
                if !push(hit, &mut seen) {
                    return Ok(hits);
                }
            }
        }
        for hit in self.query_like(query, limit)? {
            if !push(hit, &mut seen) {
                break;
            }
        }
        Ok(hits)
    }
}

impl SearchIndex for SqliteSearchIndex<'_> {
    fn upsert(&mut self, document: &SearchDocument) -> AppResult<()> {
        let asset_id = document.asset_id.to_string();
        // Remove any existing row for this asset, then insert fresh.
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT rowid FROM search_documents WHERE asset_id = ?1",
                [&asset_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(crate::map_error)?;
        if let Some(rowid) = existing {
            self.conn
                .execute("DELETE FROM search_documents WHERE rowid = ?1", [rowid])
                .map_err(crate::map_error)?;
        }
        self.conn
            .execute(
                "INSERT INTO search_documents (title, subtitle, body, keywords, kind, asset_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    document.title,
                    document.subtitle,
                    document.body,
                    document.keywords.join(" "),
                    document.kind,
                    asset_id,
                ],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
    fn remove(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM search_documents WHERE asset_id = ?1",
                [asset_id.to_string()],
            )
            .map_err(crate::map_error)?;
        Ok(())
    }
    fn clear(&mut self) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM search_documents", [])
            .map_err(crate::map_error)?;
        Ok(())
    }
    fn replace_all(&mut self, documents: &[SearchDocument]) -> AppResult<()> {
        self.clear()?;
        for document in documents {
            self.upsert(document)?;
        }
        Ok(())
    }
}

impl SqliteSearchIndex<'_> {
    fn query_fts(&self, match_query: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT asset_id, kind, title, subtitle FROM search_documents \
                 WHERE search_documents MATCH ?1 \
                 ORDER BY bm25(search_documents), title LIMIT ?2",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map(rusqlite::params![match_query, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(crate::map_error)?;
        let mut hits = Vec::new();
        for row in rows {
            let (asset_id, kind, title, subtitle) = row.map_err(crate::map_error)?;
            hits.push(SearchHit {
                asset_id: parse_hit_id(&asset_id)?,
                kind,
                title,
                subtitle,
            });
        }
        Ok(hits)
    }

    fn query_like(&self, query: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self
            .conn
            .prepare(
                "SELECT asset_id, kind, title, subtitle FROM search_documents \
                 WHERE title LIKE ?1 ESCAPE '\\' OR subtitle LIKE ?1 ESCAPE '\\' \
                    OR body LIKE ?1 ESCAPE '\\' OR keywords LIKE ?1 ESCAPE '\\' \
                 ORDER BY title LIMIT ?2",
            )
            .map_err(crate::map_error)?;
        let rows = stmt
            .query_map(rusqlite::params![pattern, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(crate::map_error)?;
        let mut hits = Vec::new();
        for row in rows {
            let (asset_id, kind, title, subtitle) = row.map_err(crate::map_error)?;
            hits.push(SearchHit {
                asset_id: parse_hit_id(&asset_id)?,
                kind,
                title,
                subtitle,
            });
        }
        Ok(hits)
    }
}

fn parse_hit_id(value: &str) -> AppResult<AssetId> {
    uuid::Uuid::parse_str(value)
        .map(AssetId::from_uuid)
        .map_err(|e| AppError::storage(format!("invalid asset id in search index: {e}")))
}

/// Builds an FTS5 MATCH expression: quoted tokens ANDed, last token as a
/// prefix. Returns None when the query has no usable tokens.
fn build_match_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.replace('"', "\"\""))
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let last = tokens.len() - 1;
    let parts: Vec<String> = tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            if i == last {
                format!("\"{token}\"*")
            } else {
                format!("\"{token}\"")
            }
        })
        .collect();
    Some(parts.join(" AND "))
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
