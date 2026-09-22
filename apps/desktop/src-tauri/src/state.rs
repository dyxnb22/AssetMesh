//! Composition root and state management for the desktop adapter.

use std::path::Path;
use std::sync::{Arc, RwLock};

use assetmesh_core::application::activity_service::ActivityService;
use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::duplicate_review_service::DuplicateReviewService;
use assetmesh_core::application::library_service::LibraryService;
use assetmesh_core::application::media_service::MediaService;
use assetmesh_core::application::merge_preview_service::MergePreviewService;
use assetmesh_core::application::portable::{PortableExportService, PortableImportService};
use assetmesh_core::application::relation_query_service::RelationQueryService;
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::service_service::ServiceService;
use assetmesh_core::application::software_service::SoftwareService;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_core::{SharedClock, SharedIdGenerator};
use assetmesh_storage_sqlite::SharedSqlite;
use serde::{Deserialize, Serialize};

use crate::error::DesktopError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AppStatus {
    Loading,
    Ready { db_path: String },
    SetupFailure { message: String },
    CorruptFailure { message: String },
}

/// Centralized module registry providing zero-boilerplate access to domain services.
/// Cloned cheaply from the composition root via an `Arc<SqliteFactory>`.
#[derive(Clone)]
pub struct DesktopModules {
    factory: SharedSqlite,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl DesktopModules {
    pub fn new(factory: SharedSqlite, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        Self {
            factory,
            clock,
            ids,
        }
    }

    pub fn factory(&self) -> &SharedSqlite {
        &self.factory
    }

    pub fn clock(&self) -> &SharedClock {
        &self.clock
    }

    pub fn ids(&self) -> &SharedIdGenerator {
        &self.ids
    }

    pub fn media(&self) -> MediaService<SharedSqlite> {
        MediaService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }

    pub fn software(&self) -> SoftwareService<SharedSqlite> {
        SoftwareService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }

    pub fn service(&self) -> ServiceService<SharedSqlite> {
        ServiceService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }

    pub fn relation(&self) -> RelationService<SharedSqlite> {
        RelationService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }

    pub fn relation_query(&self) -> RelationQueryService<SharedSqlite> {
        RelationQueryService::new(self.factory.clone())
    }

    pub fn activity(&self) -> ActivityService<SharedSqlite> {
        ActivityService::new(self.factory.clone())
    }

    pub fn asset(&self) -> AssetService<SharedSqlite> {
        AssetService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }

    pub fn library(&self) -> LibraryService<SharedSqlite> {
        LibraryService::new(self.factory.clone())
    }

    pub fn duplicate_review(&self) -> DuplicateReviewService<SharedSqlite> {
        DuplicateReviewService::new(self.factory.clone())
    }

    pub fn merge_preview(&self) -> MergePreviewService<SharedSqlite> {
        MergePreviewService::new(self.factory.clone())
    }

    pub fn portable_export(&self) -> PortableExportService<SharedSqlite> {
        PortableExportService::new(self.factory.clone(), self.clock.clone())
    }

    pub fn portable_import(&self) -> PortableImportService<SharedSqlite> {
        PortableImportService::new(self.factory.clone())
    }
}

pub struct DesktopState {
    pub status: RwLock<AppStatus>,
    pub factory: RwLock<Option<SharedSqlite>>,
    pub clock: SharedClock,
    pub ids: SharedIdGenerator,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopState {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(AppStatus::Loading),
            factory: RwLock::new(None),
            clock: Arc::new(SystemClock),
            ids: Arc::new(UuidV7Generator),
        }
    }

    pub fn with_clock_and_ids(clock: SharedClock, ids: SharedIdGenerator) -> Self {
        Self {
            status: RwLock::new(AppStatus::Loading),
            factory: RwLock::new(None),
            clock,
            ids,
        }
    }

    /// Initializes the database connection and runs pending migrations.
    pub fn initialize(&self, db_path: &Path) -> Result<AppStatus, DesktopError> {
        let path_str = db_path.to_string_lossy().to_string();
        match assetmesh_storage_sqlite::open(&path_str) {
            Ok(sqlite_factory) => {
                let shared = SharedSqlite(Arc::new(sqlite_factory));
                *self
                    .factory
                    .write()
                    .map_err(|_| DesktopError::internal("Lock poisoned"))? = Some(shared);
                let status = AppStatus::Ready { db_path: path_str };
                *self
                    .status
                    .write()
                    .map_err(|_| DesktopError::internal("Lock poisoned"))? = status.clone();
                Ok(status)
            }
            Err(err) => {
                let status = match &err {
                    assetmesh_core::AppError::CorruptData { .. }
                    | assetmesh_core::AppError::UnsupportedSchemaVersion { .. } => {
                        AppStatus::CorruptFailure {
                            message: err.message(),
                        }
                    }
                    _ => AppStatus::SetupFailure {
                        message: err.message(),
                    },
                };
                *self
                    .status
                    .write()
                    .map_err(|_| DesktopError::internal("Lock poisoned"))? = status.clone();
                Ok(status)
            }
        }
    }

    pub fn get_status(&self) -> AppStatus {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or(AppStatus::SetupFailure {
                message: "Internal lock failure".into(),
            })
    }

    /// Builds a [`DesktopModules`] handle under a momentary read lock.
    ///
    /// Because `SharedSqlite` wraps an `Arc<SqliteFactory>`, cloning it is an
    /// atomic counter increment. The lock on `DesktopState` is released
    /// immediately, allowing read and write operations across commands to run
    /// without mutex lock contention.
    pub fn modules(&self) -> Result<DesktopModules, DesktopError> {
        let guard = self
            .factory
            .read()
            .map_err(|_| DesktopError::internal("Lock poisoned"))?;
        let factory = guard
            .as_ref()
            .ok_or_else(|| DesktopError::setup_required("Database is not initialized"))?;
        Ok(DesktopModules::new(
            factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        ))
    }

    /// Executes a closure against centralized [`DesktopModules`].
    pub fn with_modules<R>(
        &self,
        f: impl FnOnce(&DesktopModules) -> Result<R, DesktopError>,
    ) -> Result<R, DesktopError> {
        let modules = self.modules()?;
        f(&modules)
    }

    /// Borrows the factory to run an application service action.
    ///
    /// Non-blocking: clones the inner `SharedSqlite` (`Arc`) under a momentary
    /// read lock so callers do NOT hold exclusive write locks across operations.
    pub fn with_factory<R>(
        &self,
        f: impl FnOnce(&mut SharedSqlite) -> Result<R, DesktopError>,
    ) -> Result<R, DesktopError> {
        let mut factory = {
            let guard = self
                .factory
                .read()
                .map_err(|_| DesktopError::internal("Lock poisoned"))?;
            guard
                .as_ref()
                .ok_or_else(|| DesktopError::setup_required("Database is not initialized"))?
                .clone()
        };
        f(&mut factory)
    }
}
