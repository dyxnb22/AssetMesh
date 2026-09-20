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
    ExternalRefRepository, MediaReader, MediaRepository, RelationReader, RelationRepository,
    ServiceReader, ServiceRepository, SoftwareReader, SoftwareRepository, TagReader, TagRepository,
};
use crate::ports::search::{SearchIndex, SearchReader};
use crate::AppResult;

/// Write scope: read + mutate every repository, committed atomically.
pub trait UnitOfWork {
    fn assets(&mut self) -> &mut dyn AssetRepository;
    fn media(&mut self) -> &mut dyn MediaRepository;
    fn software(&mut self) -> &mut dyn SoftwareRepository;
    fn services(&mut self) -> &mut dyn ServiceRepository;
    fn external_refs(&mut self) -> &mut dyn ExternalRefRepository;
    fn activity(&mut self) -> &mut dyn ActivityRepository;
    fn tags(&mut self) -> &mut dyn TagRepository;
    fn relations(&mut self) -> &mut dyn RelationRepository;
    fn search_index(&mut self) -> &mut dyn SearchIndex;
}

/// Read scope: query-only access. Mutation methods are not reachable through
/// this trait, and implementations must execute the scope against a single
/// consistent snapshot.
pub trait QueryUnitOfWork {
    fn assets(&mut self) -> &mut dyn AssetReader;
    fn media(&mut self) -> &mut dyn MediaReader;
    fn software(&mut self) -> &mut dyn SoftwareReader;
    fn services(&mut self) -> &mut dyn ServiceReader;
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader;
    fn activity(&mut self) -> &mut dyn ActivityReader;
    fn tags(&mut self) -> &mut dyn TagReader;
    fn relations(&mut self) -> &mut dyn RelationReader;
    fn search_index(&mut self) -> &mut dyn SearchReader;
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
