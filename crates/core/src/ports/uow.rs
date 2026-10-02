//! Unit-of-work ports: the transaction boundary used by application services.
//!
//! A write `UnitOfWork` bundles every repository needed by a use case so
//! canonical state changes and their activity events commit atomically in one
//! SQLite transaction (ADR 0007). Transactions stay short: external I/O
//! happens before the factory opens one.
//!
//! A read scope (`QueryUnitOfWork`) exposes only reader capabilities and runs
//! in a snapshot-consistent, query-only transaction: concurrent commits by
//! other connections cannot tear multi-query reads (e.g. portable export).

use crate::ports::repos::{
    ActivityReader, ActivityRepository, AssetReader, AssetRepository, ExternalRefReader,
    ExternalRefRepository, InfoReader, InfoRepository, LibraryReadPort, MediaReader,
    MediaRepository, RelationReader, RelationRepository, ServiceReader, ServiceRepository,
    SoftwareReader, SoftwareRepository, TagReader, TagRepository,
};
use crate::ports::search::{SearchIndex, SearchReader};
use crate::AppResult;

/// Write scope: read + mutate every repository, committed atomically.
pub trait UnitOfWork {
    fn assets(&mut self) -> &mut dyn AssetRepository;
    fn media(&mut self) -> &mut dyn MediaRepository;
    fn software(&mut self) -> &mut dyn SoftwareRepository;
    fn services(&mut self) -> &mut dyn ServiceRepository;
    fn info(&mut self) -> &mut dyn InfoRepository;
    fn external_refs(&mut self) -> &mut dyn ExternalRefRepository;
    fn activity(&mut self) -> &mut dyn ActivityRepository;
    fn tags(&mut self) -> &mut dyn TagRepository;
    fn relations(&mut self) -> &mut dyn RelationRepository;
    fn search_index(&mut self) -> &mut dyn SearchIndex;
    fn library(&mut self) -> &mut dyn LibraryReadPort;
}

/// Read scope: query-only access. Mutation methods are not reachable through
/// this trait, and implementations must execute the scope against a single
/// consistent snapshot.
pub trait QueryUnitOfWork {
    /// Visits portable rows in one consistent snapshot. Storage adapters stream
    /// rows; in-memory adapters can use the default implementation.
    fn visit_portable(
        &mut self,
        visit: &mut dyn FnMut(crate::application::portable::PortableRow) -> AppResult<()>,
    ) -> AppResult<()> {
        use crate::application::portable::*;
        let mut assets = self.assets().list(&crate::ports::repos::AssetFilter {
            lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
            ..Default::default()
        })?;
        assets.sort_by_key(|row| row.id);
        for row in assets {
            visit(PortableRow::Asset(PortableAssetV1::from_domain(&row)))?;
        }
        let mut media = self.media().list_all()?;
        media.sort_by_key(|row| row.asset_id);
        for row in media {
            visit(PortableRow::Media(PortableMediaRecordV1::from_domain(&row)))?;
        }
        let mut software = self.software().list_all()?;
        software.sort_by_key(|row| row.asset_id);
        for row in software {
            visit(PortableRow::Software(
                PortableSoftwareRecordV1::from_domain(&row),
            ))?;
        }
        let mut services = self.services().list_all()?;
        services.sort_by_key(|row| row.asset_id);
        for row in services {
            visit(PortableRow::Service(PortableServiceRecordV1::from_domain(
                &row,
            )))?;
        }
        let mut info = self.info().list()?;
        info.sort_by_key(|row| row.record.asset_id);
        for row in info {
            visit(PortableRow::Info(PortableInfoRecordV1::from_domain(
                &row.record,
            )))?;
        }
        let mut refs = self.external_refs().list_all()?;
        refs.sort_by(|a, b| (&a.namespace, &a.external_id).cmp(&(&b.namespace, &b.external_id)));
        for row in refs {
            visit(PortableRow::ExternalRef(
                PortableExternalRefV1::from_domain(&row),
            ))?;
        }
        let mut activity = self.activity().list_all()?;
        activity.sort_by_key(|row| (row.occurred_at, row.id));
        for row in activity {
            visit(PortableRow::Activity(PortableActivityEventV1::from_domain(
                &row,
            )))?;
        }
        let mut tags = self.tags().list_all()?;
        tags.sort_by_key(|row| row.id);
        for row in tags {
            visit(PortableRow::Tag(PortableTagV1::from_domain(&row)))?;
        }
        let mut memberships = self.tags().list_memberships()?;
        memberships.sort();
        for (asset_id, tag_id) in memberships {
            visit(PortableRow::Membership(AssetTagRow {
                asset_id: asset_id.to_string(),
                tag_id: tag_id.to_string(),
            }))?;
        }
        let mut relations = self.relations().list_all()?;
        relations.sort_by_key(|row| row.id);
        for row in relations {
            visit(PortableRow::Relation(PortableRelationV1::from_domain(&row)))?;
        }
        Ok(())
    }

    /// API key assets and tombstones redirecting to them, without values.
    fn private_asset_ids(&mut self) -> AppResult<std::collections::HashSet<String>> {
        let mut ids: std::collections::HashSet<String> = self
            .info()
            .list()?
            .into_iter()
            .filter(|entry| entry.record.info_type == crate::domain::info::InfoType::ApiKey)
            .map(|entry| entry.asset.id.to_string())
            .collect();
        let assets = self.assets().list(&crate::ports::repos::AssetFilter {
            lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
            ..Default::default()
        })?;
        loop {
            let before = ids.len();
            for asset in &assets {
                if asset
                    .merged_into
                    .is_some_and(|id| ids.contains(&id.to_string()))
                {
                    ids.insert(asset.id.to_string());
                }
            }
            if before == ids.len() {
                break;
            }
        }
        Ok(ids)
    }

    fn assets(&mut self) -> &mut dyn AssetReader;
    fn media(&mut self) -> &mut dyn MediaReader;
    fn software(&mut self) -> &mut dyn SoftwareReader;
    fn services(&mut self) -> &mut dyn ServiceReader;
    fn info(&mut self) -> &mut dyn InfoReader;
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader;
    fn activity(&mut self) -> &mut dyn ActivityReader;
    fn tags(&mut self) -> &mut dyn TagReader;
    fn relations(&mut self) -> &mut dyn RelationReader;
    fn search_index(&mut self) -> &mut dyn SearchReader;
    fn library(&mut self) -> &mut dyn LibraryReadPort;
}

/// Opens work scopes over a [`UnitOfWork`] / [`QueryUnitOfWork`].
///
/// `transact` commits on `Ok` and rolls back on `Err` — this is what makes
/// "update canonical state + append activity + update projection" atomic.
/// `read` runs a read-only, snapshot-consistent scope; implementations must
/// not take a SQLite write lock for it.
pub trait UnitOfWorkFactory {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T>;

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T>;
}
