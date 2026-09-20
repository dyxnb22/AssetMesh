//! Unit of work over SQLite.
//!
//! `transact` opens a short IMMEDIATE transaction on the single writer
//! connection and commits/rolls back around the closure; all repositories in
//! one scope share that connection, which is what makes "canonical state +
//! activity + projection" commits atomic.
//!
//! `read` opens a deferred, query-only transaction on a pooled reader
//! connection (file databases), giving every read scope a consistent
//! snapshot: concurrent writers cannot tear multi-query reads such as
//! portable export, and reads do not block writers (WAL). Mutation methods
//! are unreachable through the read scope's type.
//!
//! The factory is safe to share as [`std::sync::Arc`] — each scope takes
//! only the locks it needs for its own connection.

use assetmesh_core::ports::repos::{
    ActivityReader, ActivityRepository, AssetReader, AssetRepository, ExternalRefReader,
    ExternalRefRepository, MediaReader, MediaRepository, RelationReader, RelationRepository,
    ServiceReader, ServiceRepository, SoftwareReader, SoftwareRepository, TagReader, TagRepository,
};
use assetmesh_core::ports::search::{SearchIndex, SearchReader};
use assetmesh_core::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use assetmesh_core::{AppError, AppResult};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::repos::{
    SqliteActivityRepo, SqliteAssetRepo, SqliteExternalRefRepo, SqliteMediaRepo,
    SqliteRelationRepo, SqliteSearchIndex, SqliteServiceRepo, SqliteSoftwareRepo, SqliteTagRepo,
};

pub struct SqliteFactory {
    path: String,
    is_memory: bool,
    /// Single writer connection: IMMEDIATE write transactions and, for
    /// in-memory databases, read scopes too (there is nothing else to read).
    writer: Mutex<Connection>,
    /// Pooled read connections for file databases.
    readers: Mutex<Vec<Connection>>,
}

impl SqliteFactory {
    pub(crate) fn new(path: String, conn: Connection) -> Self {
        let is_memory = path == ":memory:";
        SqliteFactory {
            path,
            is_memory,
            writer: Mutex::new(conn),
            readers: Mutex::new(Vec::new()),
        }
    }

    /// Raw connection access for diagnostics and tests.
    pub fn with_raw_connection<T>(&self, f: impl FnOnce(&Connection) -> T) -> Result<T, AppError> {
        let conn = self
            .writer
            .lock()
            .map_err(|_| AppError::storage("sqlite lock poisoned"))?;
        Ok(f(&conn))
    }

    fn transact_impl<T>(
        &self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut conn = self
            .writer
            .lock()
            .map_err(|_| AppError::storage("sqlite lock poisoned"))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(crate::map_error)?;

        let result = {
            let mut uow = SqliteUow::new(&tx);
            work(&mut uow)
        };

        match result {
            Ok(value) => {
                tx.commit().map_err(crate::map_error)?;
                Ok(value)
            }
            Err(error) => {
                // Rollback also happens on drop; explicit for clarity.
                let _ = tx.rollback();
                Err(error)
            }
        }
    }

    fn read_impl<T>(
        &self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        if self.is_memory {
            let mut conn = self
                .writer
                .lock()
                .map_err(|_| AppError::storage("sqlite lock poisoned"))?;
            return run_snapshot_read(&mut conn, work);
        }

        let mut conn = self.acquire_reader()?;
        let result = run_snapshot_read(&mut conn, work);
        // Return the connection to the pool only if it is still usable;
        // run_snapshot_read resets query-only mode, and a connection that
        // cannot be reset must not be reused.
        if !conn.query_only_enabled() {
            let mut pool = self
                .readers
                .lock()
                .map_err(|_| AppError::storage("sqlite lock poisoned"))?;
            pool.push(conn);
        }
        result
    }

    fn acquire_reader(&self) -> Result<Connection, AppError> {
        if let Some(conn) = self
            .readers
            .lock()
            .map_err(|_| AppError::storage("sqlite lock poisoned"))?
            .pop()
        {
            return Ok(conn);
        }
        // New connections share the same initialization policy and are
        // opened after `open()` has finished migrating.
        crate::connection::connect(&self.path)
    }
}

/// Runs one read scope inside a deferred, query-only transaction so it sees
/// a single consistent snapshot (WAL readers never block the writer).
fn run_snapshot_read<T>(
    conn: &mut Connection,
    work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
) -> AppResult<T> {
    set_query_only(conn, true)?;
    let result = (|| {
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
            .map_err(crate::map_error)?;
        let mut uow = SqliteQueryUow::new(&tx);
        match work(&mut uow) {
            Ok(value) => {
                // Commit merely releases the snapshot; a read-only
                // transaction never wrote anything.
                tx.commit().map_err(crate::map_error)?;
                Ok(value)
            }
            Err(error) => {
                let _ = tx.rollback();
                Err(error)
            }
        }
    })();
    // Reset so pooled connections can never be handed back still in
    // query-only mode.
    set_query_only(conn, false)?;
    result
}

fn set_query_only(conn: &Connection, enabled: bool) -> AppResult<()> {
    conn.pragma_update(None, "query_only", if enabled { "ON" } else { "OFF" })
        .map_err(|e| {
            AppError::storage(format!(
                "failed to switch query-only mode on reader connection: {e}"
            ))
        })
}

trait QueryOnlyCheck {
    fn query_only_enabled(&self) -> bool;
}

impl QueryOnlyCheck for Connection {
    fn query_only_enabled(&self) -> bool {
        self.query_row("PRAGMA query_only", [], |row| {
            row.get::<_, i64>(0).map(|v| v != 0)
        })
        .unwrap_or(true) // if unreadable, do not reuse the connection
    }
}

impl UnitOfWorkFactory for SqliteFactory {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        self.transact_impl(work)
    }

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        self.read_impl(work)
    }
}

/// Cheaply shareable handle around one SQLite factory. Each scope locks
/// only its own connection — there is no outer mutex serializing the whole
/// factory.
#[derive(Clone)]
pub struct SharedSqlite(pub Arc<SqliteFactory>);

impl UnitOfWorkFactory for SharedSqlite {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        self.0.transact_impl(work)
    }

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        self.0.read_impl(work)
    }
}

/// Repository bundles shared by the write and read scopes.
struct RepoBundle<'conn> {
    assets: SqliteAssetRepo<'conn>,
    media: SqliteMediaRepo<'conn>,
    software: SqliteSoftwareRepo<'conn>,
    services: SqliteServiceRepo<'conn>,
    refs: SqliteExternalRefRepo<'conn>,
    activity: SqliteActivityRepo<'conn>,
    tags: SqliteTagRepo<'conn>,
    relations: SqliteRelationRepo<'conn>,
    search: SqliteSearchIndex<'conn>,
}

impl<'conn> RepoBundle<'conn> {
    fn new(conn: &'conn Connection) -> Self {
        RepoBundle {
            assets: SqliteAssetRepo { conn },
            media: SqliteMediaRepo { conn },
            software: SqliteSoftwareRepo { conn },
            services: SqliteServiceRepo { conn },
            refs: SqliteExternalRefRepo { conn },
            activity: SqliteActivityRepo { conn },
            tags: SqliteTagRepo { conn },
            relations: SqliteRelationRepo { conn },
            search: SqliteSearchIndex { conn },
        }
    }
}

/// Write scope: every repository, one transaction.
pub(crate) struct SqliteUow<'conn> {
    repos: RepoBundle<'conn>,
}

impl<'conn> SqliteUow<'conn> {
    pub(crate) fn new(conn: &'conn Connection) -> Self {
        SqliteUow {
            repos: RepoBundle::new(conn),
        }
    }
}

impl<'conn> UnitOfWork for SqliteUow<'conn> {
    fn assets(&mut self) -> &mut dyn AssetRepository {
        &mut self.repos.assets
    }

    fn media(&mut self) -> &mut dyn MediaRepository {
        &mut self.repos.media
    }

    fn software(&mut self) -> &mut dyn SoftwareRepository {
        &mut self.repos.software
    }

    fn services(&mut self) -> &mut dyn ServiceRepository {
        &mut self.repos.services
    }

    fn external_refs(&mut self) -> &mut dyn ExternalRefRepository {
        &mut self.repos.refs
    }

    fn activity(&mut self) -> &mut dyn ActivityRepository {
        &mut self.repos.activity
    }

    fn tags(&mut self) -> &mut dyn TagRepository {
        &mut self.repos.tags
    }

    fn relations(&mut self) -> &mut dyn RelationRepository {
        &mut self.repos.relations
    }

    fn search_index(&mut self) -> &mut dyn SearchIndex {
        &mut self.repos.search
    }
}

/// Read scope: reader capabilities only — mutation methods are not
/// reachable through this type.
pub(crate) struct SqliteQueryUow<'conn> {
    repos: RepoBundle<'conn>,
}

impl<'conn> SqliteQueryUow<'conn> {
    pub(crate) fn new(conn: &'conn Connection) -> Self {
        SqliteQueryUow {
            repos: RepoBundle::new(conn),
        }
    }
}

impl<'conn> QueryUnitOfWork for SqliteQueryUow<'conn> {
    fn assets(&mut self) -> &mut dyn AssetReader {
        &mut self.repos.assets
    }

    fn media(&mut self) -> &mut dyn MediaReader {
        &mut self.repos.media
    }

    fn software(&mut self) -> &mut dyn SoftwareReader {
        &mut self.repos.software
    }

    fn services(&mut self) -> &mut dyn ServiceReader {
        &mut self.repos.services
    }

    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        &mut self.repos.refs
    }

    fn activity(&mut self) -> &mut dyn ActivityReader {
        &mut self.repos.activity
    }

    fn tags(&mut self) -> &mut dyn TagReader {
        &mut self.repos.tags
    }

    fn relations(&mut self) -> &mut dyn RelationReader {
        &mut self.repos.relations
    }

    fn search_index(&mut self) -> &mut dyn SearchReader {
        &mut self.repos.search
    }
}
