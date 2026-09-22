//! Composition root and state management for the desktop adapter.

use std::path::Path;
use std::sync::{Arc, RwLock};

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
                let msg = err.to_string();
                let status = if msg.contains("checksum")
                    || msg.contains("tamper")
                    || msg.contains("modified")
                    || matches!(
                        err,
                        assetmesh_core::AppError::UnsupportedSchemaVersion { .. }
                    ) {
                    AppStatus::CorruptFailure { message: msg }
                } else {
                    AppStatus::SetupFailure { message: msg }
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

    /// Borrows the factory to run an application service action.
    pub fn with_factory<R>(
        &self,
        f: impl FnOnce(&mut SharedSqlite) -> Result<R, DesktopError>,
    ) -> Result<R, DesktopError> {
        let mut guard = self
            .factory
            .write()
            .map_err(|_| DesktopError::internal("Lock poisoned"))?;
        let factory = guard
            .as_mut()
            .ok_or_else(|| DesktopError::setup_required("Database is not initialized"))?;
        f(factory)
    }
}
