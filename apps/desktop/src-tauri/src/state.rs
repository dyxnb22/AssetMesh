//! Composition root and state management for the desktop adapter.

use assetmesh_core::application::backup_service::BackupService;
use assetmesh_storage_sqlite::backup::{selected_database, SqliteBackupStore};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use assetmesh_core::application::activity_service::ActivityService;
use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::duplicate_review_service::DuplicateReviewService;
use assetmesh_core::application::info_service::InfoService;
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

    pub fn info(&self) -> InfoService<SharedSqlite> {
        InfoService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
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
        AssetService::new(self.factory.clone(), self.clock.clone())
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
    pub startup_timings: crate::startup::StartupTimings,
    runtime: RwLock<RuntimeState>,
    default_db_path: RwLock<Option<PathBuf>>,
    pub clock: SharedClock,
    pub ids: SharedIdGenerator,
    backup: RwLock<Option<Arc<BackupService<SharedSqlite>>>>,
    history_maintenance_due: Mutex<Option<assetmesh_core::domain::Timestamp>>,
    /// In-memory registry of the local-service processes this process
    /// started. Never persisted: running state is temporary runtime fact.
    service_runtime: crate::runtime::LocalServiceRuntime,
    service_operations: Mutex<()>,
    service_status_cache: Mutex<Option<(std::time::Instant, Vec<crate::runtime::RuntimeStatus>)>>,
    service_runtime_shutdown: AtomicBool,
}

enum RuntimeState {
    Loading,
    Ready {
        db_path: String,
        modules: DesktopModules,
    },
    SetupFailure {
        message: String,
    },
    CorruptFailure {
        message: String,
    },
}

impl RuntimeState {
    fn status(&self) -> AppStatus {
        match self {
            Self::Loading => AppStatus::Loading,
            Self::Ready { db_path, .. } => AppStatus::Ready {
                db_path: db_path.clone(),
            },
            Self::SetupFailure { message } => AppStatus::SetupFailure {
                message: message.clone(),
            },
            Self::CorruptFailure { message } => AppStatus::CorruptFailure {
                message: message.clone(),
            },
        }
    }
}

impl Default for DesktopState {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopState {
    pub fn new() -> Self {
        Self {
            startup_timings: crate::startup::StartupTimings::default(),
            runtime: RwLock::new(RuntimeState::Loading),
            default_db_path: RwLock::new(None),
            backup: RwLock::new(None),
            history_maintenance_due: Mutex::new(None),
            service_runtime: crate::runtime::LocalServiceRuntime::new(),
            service_operations: Mutex::new(()),
            service_status_cache: Mutex::new(None),
            service_runtime_shutdown: AtomicBool::new(false),
            clock: Arc::new(SystemClock),
            ids: Arc::new(UuidV7Generator),
        }
    }

    pub fn with_clock_and_ids(clock: SharedClock, ids: SharedIdGenerator) -> Self {
        Self {
            startup_timings: crate::startup::StartupTimings::default(),
            runtime: RwLock::new(RuntimeState::Loading),
            default_db_path: RwLock::new(None),
            backup: RwLock::new(None),
            history_maintenance_due: Mutex::new(None),
            service_runtime: crate::runtime::LocalServiceRuntime::new(),
            service_operations: Mutex::new(()),
            service_status_cache: Mutex::new(None),
            service_runtime_shutdown: AtomicBool::new(false),
            clock,
            ids,
        }
    }

    /// The location the most recent initialization attempt used.
    ///
    /// A retry after a startup failure has to re-open the same file, and only
    /// this state object knows which one the launch asked for.
    pub fn default_db_path(&self) -> Option<PathBuf> {
        self.default_db_path.read().ok().and_then(|p| p.clone())
    }

    /// Initializes the database connection and runs pending migrations.
    pub fn initialize(&self, db_path: &Path) -> Result<AppStatus, DesktopError> {
        let selected = selected_database(db_path);
        let path_str = selected
            .as_ref()
            .unwrap_or(&db_path.to_path_buf())
            .to_string_lossy()
            .to_string();
        if let Some(parent) = db_path.parent() {
            // A failure here still surfaces from `open` below; aborting the launch
            // instead would leave the user with no window and no explanation.
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut guard) = self.default_db_path.write() {
            *guard = Some(db_path.to_path_buf());
        }
        let opened = selected.and_then(|_| assetmesh_storage_sqlite::open(&path_str));
        let next = match opened {
            Ok(sqlite_factory) => {
                let shared = SharedSqlite(Arc::new(sqlite_factory));
                let store = Arc::new(SqliteBackupStore::new(shared.clone(), db_path));
                let backup = Arc::new(BackupService::new(
                    store,
                    shared.clone(),
                    self.clock.clone(),
                ));
                *self
                    .backup
                    .write()
                    .map_err(|_| DesktopError::internal("Backup lock failed"))? = Some(backup);
                RuntimeState::Ready {
                    db_path: path_str,
                    modules: DesktopModules::new(shared, self.clock.clone(), self.ids.clone()),
                }
            }
            Err(err) => {
                let recovery = SharedSqlite(Arc::new(assetmesh_storage_sqlite::open_in_memory()?));
                let store = Arc::new(SqliteBackupStore::new(recovery.clone(), db_path));
                *self
                    .backup
                    .write()
                    .map_err(|_| DesktopError::internal("Backup lock failed"))? = Some(Arc::new(
                    BackupService::new(store, recovery, self.clock.clone()).recovery_only(),
                ));
                let corrupt = matches!(
                    err,
                    assetmesh_core::AppError::CorruptData { .. }
                        | assetmesh_core::AppError::UnsupportedSchemaVersion { .. }
                );
                // The underlying reason and path stay on stderr: `AppStatus` crosses
                // into the webview, and its message is tested to carry neither.
                eprintln!("[assetmesh] database initialization failed: {err} (path: {path_str})");
                let message = DesktopError::from(err).message;
                if corrupt {
                    RuntimeState::CorruptFailure { message }
                } else {
                    RuntimeState::SetupFailure { message }
                }
            }
        };
        let status = next.status();
        *self
            .runtime
            .write()
            .map_err(|_| DesktopError::internal("Lock poisoned"))? = next;
        Ok(status)
    }

    pub fn backup(&self) -> Result<Arc<BackupService<SharedSqlite>>, DesktopError> {
        self.backup
            .read()
            .map_err(|_| DesktopError::internal("Backup lock failed"))?
            .clone()
            .ok_or_else(|| DesktopError::setup_required("Database is not initialized"))
    }

    pub fn backup_tick(&self, flush: bool) {
        if let Ok(backup) = self.backup() {
            if !flush && matches!(self.get_status(), AppStatus::Ready { .. }) {
                let now = self.clock.now();
                if let Ok(mut due) = self.history_maintenance_due.lock() {
                    if due.is_none_or(|next| now >= next) {
                        let result = self.with_modules(|modules| {
                            Ok(modules.factory().0.maintain_history(now, || {
                                backup.create("before_history_cleanup").map(|_| ())
                            })?)
                        });
                        *due = Some(
                            now + if result.is_ok() {
                                chrono::Duration::days(1)
                            } else {
                                chrono::Duration::hours(1)
                            },
                        );
                        if let Err(error) = result {
                            eprintln!("[assetmesh] history maintenance postponed: {error}");
                        }
                    }
                }
            }
            if let Err(error) = backup.tick(env!("CARGO_PKG_VERSION"), flush) {
                eprintln!("[assetmesh] automatic backup failed: {error}");
            }
        }
    }

    pub fn recovery_point(&self, reason: &str) -> Result<(), DesktopError> {
        self.backup()?.create(reason)?;
        Ok(())
    }

    /// Serialize canonical service changes with launch/stop decisions. The
    /// runtime registry alone cannot protect the read-config → spawn interval.
    pub(crate) fn with_service_operation<R>(
        &self,
        action: impl FnOnce() -> Result<R, DesktopError>,
    ) -> Result<R, DesktopError> {
        let _guard = self
            .service_operations
            .lock()
            .map_err(|_| DesktopError::internal("Service operation lock failed"))?;
        // Check after acquiring the lock: an operation queued before Quit
        // must not launch a new process after shutdown has cleaned up.
        if self.service_runtime_shutdown.load(Ordering::Acquire) {
            return Err(DesktopError::unavailable("AssetMesh is quitting"));
        }
        let result = action();
        self.invalidate_service_statuses();
        result
    }

    pub fn invalidate_service_statuses(&self) {
        if let Ok(mut cached) = self.service_status_cache.lock() {
            *cached = None;
        }
    }

    pub(crate) fn cached_service_statuses(
        &self,
        read: impl FnOnce() -> Result<Vec<crate::runtime::RuntimeStatus>, DesktopError>,
    ) -> Result<Vec<crate::runtime::RuntimeStatus>, DesktopError> {
        let mut cache = self
            .service_status_cache
            .lock()
            .map_err(|_| DesktopError::internal("Service status cache lock failed"))?;
        let mut statuses = match cache.as_ref() {
            Some((at, rows)) if at.elapsed() < std::time::Duration::from_secs(5) => rows.clone(),
            _ => {
                let rows = read()?;
                *cache = Some((std::time::Instant::now(), rows.clone()));
                rows
            }
        };
        // Managed process transitions remain immediate even inside the probe cache.
        for managed in self.service_runtime.statuses() {
            if let Some(row) = statuses
                .iter_mut()
                .find(|row| row.asset_id == managed.asset_id)
            {
                *row = managed;
            } else {
                statuses.push(managed);
            }
        }
        Ok(statuses)
    }

    /// The local-service process runtime: one per application process.
    pub fn service_runtime(&self) -> &crate::runtime::LocalServiceRuntime {
        &self.service_runtime
    }

    /// Stops every service process this application started. Boundedly
    /// blocks, so it can run from the exit hook. Returns true for the first
    /// shutdown so successive native exit events flush the final backup once.
    pub fn shutdown_service_runtime(&self) -> bool {
        let first_shutdown = !self.service_runtime_shutdown.swap(true, Ordering::AcqRel);
        if let Ok(_guard) = self.service_operations.lock() {
            self.service_runtime.shutdown_all();
        }
        first_shutdown
    }

    pub fn get_status(&self) -> AppStatus {
        self.runtime
            .read()
            .map(|s| s.status())
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
            .runtime
            .read()
            .map_err(|_| DesktopError::internal("Lock poisoned"))?;
        match &*guard {
            RuntimeState::Ready { modules, .. } => Ok(modules.clone()),
            _ => Err(DesktopError::setup_required("Database is not initialized")),
        }
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
        let mut factory = self.modules()?.factory().clone();
        f(&mut factory)
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    #[test]
    fn status_consumers_share_collection_and_service_operations_invalidate_it() {
        let state = DesktopState::new();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        for _ in 0..100 {
            state
                .cached_service_statuses(|| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Vec::new())
                })
                .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        state.with_service_operation(|| Ok(())).unwrap();
        state
            .cached_service_statuses(|| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            })
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn a_service_operation_queued_before_quit_cannot_run_after_shutdown_begins() {
        let state = Arc::new(DesktopState::new());
        let guard = state.service_operations.lock().unwrap();
        let (entering, queued) = mpsc::channel();
        let action_state = state.clone();
        let action = std::thread::spawn(move || {
            entering.send(()).unwrap();
            action_state.with_service_operation(|| Ok("would start a service"))
        });
        queued.recv_timeout(Duration::from_secs(3)).unwrap();

        let shutdown_state = state.clone();
        let shutdown = std::thread::spawn(move || shutdown_state.shutdown_service_runtime());
        let deadline = Instant::now() + Duration::from_secs(3);
        while !state.service_runtime_shutdown.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "shutdown did not begin");
            std::thread::yield_now();
        }
        drop(guard);

        assert_eq!(action.join().unwrap().unwrap_err().category, "unavailable");
        shutdown.join().unwrap();
    }
}
