//! Search index port (ADR 0006). Implemented by the SQLite adapter over an
//! FTS5 table; the index is derived state and must be droppable/rebuildable.
//! Reader and writer capabilities are split so read scopes cannot mutate the
//! projection.

use crate::domain::ids::AssetId;
use crate::domain::search::{SearchDocument, SearchHit};
use crate::AppResult;

pub trait SearchReader {
    /// Full-text query with relevance ordering; must also serve substring
    /// queries (CJK, partial words) and never silently drop storage errors.
    fn search(&mut self, query: &str, limit: usize) -> AppResult<Vec<SearchHit>>;
}

pub trait SearchIndex: SearchReader {
    /// Inserts or refreshes the document for one asset.
    fn upsert(&mut self, document: &SearchDocument) -> AppResult<()>;
    fn remove(&mut self, asset_id: AssetId) -> AppResult<()>;
    /// Removes every document. The index must be rebuildable afterwards.
    fn clear(&mut self) -> AppResult<()>;
    /// Atomically replaces the whole projection (rebuild path).
    fn replace_all(&mut self, documents: &[SearchDocument]) -> AppResult<()>;
}
