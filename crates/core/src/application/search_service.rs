//! Search use cases: query the projection and rebuild it from canonical
//! state (ADR 0006). The projection is disposable; a rebuild always
//! reproduces it from assets, media details, tags, and external refs.

use crate::application::projection::project_media;
use crate::application::SharedClock;
use crate::domain::asset::LifecycleState;
use crate::domain::search::SearchHit;
use crate::ports::repos::{AssetFilter, LifecycleFilter};
use crate::ports::uow::UnitOfWorkFactory;
use crate::AppResult;

#[derive(Debug, Clone)]
pub struct SearchService<F: UnitOfWorkFactory> {
    factory: F,
    #[allow(dead_code)]
    clock: SharedClock,
}

#[derive(Debug, Clone)]
pub struct RebuildReport {
    pub indexed: usize,
}

impl<F: UnitOfWorkFactory> SearchService<F> {
    pub fn new(factory: F, clock: SharedClock) -> Self {
        SearchService { factory, clock }
    }

    pub fn search(&mut self, query: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        let query = query.trim().to_string();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.factory
            .read(&mut |uow| uow.search_index().search(&query, limit))
    }

    /// Drops the projection and rebuilds it from canonical data. Projecting
    /// and replacing happen inside ONE immediate write transaction, so a
    /// concurrent canonical commit either lands before the rebuild (and is
    /// included) or waits for it (and keeps its own synchronous projection
    /// update) — a rebuild can never publish stale documents.
    pub fn rebuild(&mut self) -> AppResult<RebuildReport> {
        self.factory.transact(&mut |uow| {
            let documents = project_all(uow)?;
            let indexed = documents.len();
            uow.search_index().replace_all(&documents)?;
            Ok(RebuildReport { indexed })
        })
    }
}

/// Projects every live asset from canonical state within the caller's scope.
pub(crate) fn project_all(
    uow: &mut dyn crate::ports::uow::UnitOfWork,
) -> AppResult<Vec<crate::domain::search::SearchDocument>> {
    let mut documents = Vec::new();
    let assets = uow.assets().list(&AssetFilter {
        kind: None,
        lifecycle: Some(LifecycleFilter::All),
    })?;
    for asset in assets {
        // Merged tombstones are redirects, not searchable entries.
        if asset.lifecycle_state == LifecycleState::Merged {
            continue;
        }
        let record = match uow.media().get(asset.id)? {
            Some(record) => record,
            None => continue, // assets without module details are not searchable yet
        };
        let tags = uow.tags().list_for_asset(asset.id)?;
        let refs = uow.external_refs().list_for_asset(asset.id)?;
        documents.push(project_media(&asset, &record, &tags, &refs));
    }
    Ok(documents)
}
