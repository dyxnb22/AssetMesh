//! Personal backup policy shared by every interface adapter.
use super::portable::PortableExportService;
use crate::{
    domain::Timestamp,
    ports::{
        backup::{
            BackupEntry, BackupIssue, BackupPreferences, BackupRetention, BackupStore,
            RestoreReceipt,
        },
        UnitOfWorkFactory,
    },
    AppError, AppResult, SharedClock,
};
use chrono::Duration;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Serialize)]
pub struct BackupStatus {
    pub directory: String,
    pub entries: Vec<BackupEntry>,
    pub issues: Vec<BackupIssue>,
    pub storage_bytes: u64,
    pub budget_bytes: u64,
    pub last_error: Option<String>,
    pub restore_pending: bool,
}

const BACKUP_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

struct Schedule {
    token: Option<String>,
    last_snapshot: Option<Timestamp>,
    last_maintenance: Option<Timestamp>,
    last_error: Option<String>,
    restore_pending: bool,
}

pub struct BackupService<F> {
    store: Arc<dyn BackupStore>,
    factory: F,
    clock: SharedClock,
    schedule: Mutex<Schedule>,
    recovery_only: bool,
}

impl<F: UnitOfWorkFactory + Clone> BackupService<F> {
    pub fn new(store: Arc<dyn BackupStore>, factory: F, clock: SharedClock) -> Self {
        Self {
            store,
            factory,
            clock,
            recovery_only: false,
            schedule: Mutex::new(Schedule {
                token: None,
                last_snapshot: None,
                last_maintenance: None,
                last_error: None,
                restore_pending: false,
            }),
        }
    }

    pub fn recovery_only(mut self) -> Self {
        self.recovery_only = true;
        self
    }

    pub fn status(&self) -> AppResult<BackupStatus> {
        let schedule = self
            .schedule
            .lock()
            .map_err(|_| AppError::storage("backup lock failed"))?;
        let inventory = self.store.inventory()?;
        Ok(BackupStatus {
            directory: self.store.directory(),
            entries: inventory.entries,
            issues: inventory.issues,
            storage_bytes: self.store.storage_bytes()?,
            budget_bytes: BACKUP_BUDGET_BYTES,
            last_error: schedule.last_error.clone(),
            restore_pending: schedule.restore_pending,
        })
    }

    pub fn preferences(&self) -> AppResult<BackupPreferences> {
        self.store.preferences()
    }

    pub fn save_preferences(&self, preferences: BackupPreferences) -> AppResult<()> {
        for (key, value) in &preferences {
            if ![
                "assetmesh-saved-filters-v1",
                "assetmesh-theme",
                "assetmesh-lang",
            ]
            .contains(&key.as_str())
                || value.len() > 65_536
            {
                return Err(AppError::validation(
                    "unsupported or oversized backup preference",
                ));
            }
            match key.as_str() {
                "assetmesh-theme" if !["system", "light", "dark"].contains(&value.as_str()) => {
                    return Err(AppError::validation("invalid theme preference"))
                }
                "assetmesh-lang" if !["zh", "en"].contains(&value.as_str()) => {
                    return Err(AppError::validation("invalid language preference"))
                }
                "assetmesh-saved-filters-v1" => {
                    let parsed: serde_json::Value = serde_json::from_str(value)
                        .map_err(|_| AppError::validation("invalid saved filters"))?;
                    if !parsed.is_array() || parsed.as_array().is_some_and(|a| a.len() > 50) {
                        return Err(AppError::validation("invalid saved filters"));
                    }
                }
                _ => {}
            }
        }
        let schedule = self
            .schedule
            .lock()
            .map_err(|_| AppError::storage("backup lock failed"))?;
        if schedule.restore_pending {
            return Err(AppError::conflict(
                "restart to finish restoring the library",
            ));
        }
        self.store.save_preferences(&preferences)
    }

    pub fn create(&self, reason: &str) -> AppResult<BackupEntry> {
        if self.recovery_only {
            return Err(AppError::setup_required(
                "restore a backup before creating new backups",
            ));
        }
        let mut schedule = self
            .schedule
            .lock()
            .map_err(|_| AppError::storage("backup lock failed"))?;
        if schedule.restore_pending {
            return Err(AppError::conflict(
                "restart to finish restoring the library",
            ));
        }
        let now = self.clock.now();
        // Read the token before copying: a concurrent commit during the backup
        // remains dirty and will be captured by a later snapshot.
        let token = self.store.change_token()?;
        match self.store.snapshot(now, reason) {
            Ok(entry) => {
                schedule.token = Some(token);
                schedule.last_snapshot = Some(now);
                schedule.last_error = None;
                self.prune()?;
                Ok(entry)
            }
            Err(error) => {
                schedule.last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Called once after launch and every minute while the desktop is open.
    /// Failures are visible in status and leave the work dirty for a retry.
    pub fn tick(&self, app_version: &str, flush: bool) -> AppResult<()> {
        let result = self.tick_inner(app_version, flush);
        if let Err(error) = &result {
            if let Ok(mut schedule) = self.schedule.lock() {
                schedule.last_error = Some(error.to_string());
            }
        }
        result
    }

    fn tick_inner(&self, app_version: &str, flush: bool) -> AppResult<()> {
        if self.recovery_only {
            return Ok(());
        }
        let now = self.clock.now();
        let token = self.store.change_token()?;
        let (due, maintenance_due) = {
            let schedule = self
                .schedule
                .lock()
                .map_err(|_| AppError::storage("backup lock failed"))?;
            if schedule.restore_pending {
                return Ok(());
            }
            let due = schedule.token.as_ref() != Some(&token)
                && (flush
                    || schedule
                        .last_snapshot
                        .is_none_or(|last| now - last >= Duration::minutes(30)));
            (
                due,
                schedule
                    .last_maintenance
                    .is_none_or(|last| now - last >= Duration::hours(1)),
            )
        };
        if !due && !maintenance_due {
            return Ok(());
        }
        if due {
            self.create("automatic")?;
        }
        let entries = self.store.list()?;
        let last_portable = entries
            .iter()
            .filter(|entry| entry.kind == "portable")
            .map(|e| e.created_at)
            .max();
        let changed = last_portable.is_none_or(|last| {
            entries
                .iter()
                .any(|entry| entry.kind == "snapshot" && entry.created_at > last)
        });
        if changed && last_portable.is_none_or(|last| now - last >= Duration::days(7)) {
            let mut export = PortableExportService::new(self.factory.clone(), self.clock.clone());
            self.store.portable_stream(now, &mut |directory| {
                export.export_to_directory(app_version, directory, true)
            })?;
            self.prune()?;
            self.schedule
                .lock()
                .map_err(|_| AppError::storage("backup lock failed"))?
                .last_error = None;
        }
        // Apply a changed retention policy and clean abandoned staging files
        // even when there have been no new writes in this session.
        self.prune()?;
        self.schedule
            .lock()
            .map_err(|_| AppError::storage("backup lock failed"))?
            .last_maintenance = Some(now);
        Ok(())
    }

    pub fn export_copy(&self, target: &str, app_version: &str) -> AppResult<String> {
        let schedule = self
            .schedule
            .lock()
            .map_err(|_| AppError::storage("backup lock failed"))?;
        if self.recovery_only || schedule.restore_pending {
            return Err(AppError::conflict("open a usable library before exporting"));
        }
        let mut export = PortableExportService::new(self.factory.clone(), self.clock.clone());
        let entry = self
            .store
            .portable_stream(self.clock.now(), &mut |directory| {
                export.export_to_directory(app_version, directory, true)
            })?;
        let path = self.store.copy_to(&entry.source_dir, target)?;
        self.prune()?;
        Ok(path)
    }

    pub fn preview(&self, source: &str) -> AppResult<BackupEntry> {
        self.store.preview(source)
    }

    pub fn restore(&self, source: &str, fingerprint: &str) -> AppResult<RestoreReceipt> {
        let mut schedule = self
            .schedule
            .lock()
            .map_err(|_| AppError::storage("backup lock failed"))?;
        if schedule.restore_pending {
            return Err(AppError::conflict("a restore is already pending"));
        }
        self.preview(source)?;
        // Do not prune here: the selected recovery point may be the oldest one.
        if !self.recovery_only {
            self.store.snapshot(self.clock.now(), "before_restore")?;
        }
        let receipt = self.store.restore(source, fingerprint)?;
        schedule.restore_pending = true;
        Ok(receipt)
    }

    fn prune(&self) -> AppResult<()> {
        let entries = self.store.list()?;
        let mut keep = BTreeSet::new();
        let mut protected = BTreeSet::new();
        let mut days = BTreeSet::new();
        let today = self.clock.now().date_naive();
        for entry in entries
            .iter()
            .filter(|e| e.kind == "snapshot" && e.reason == "automatic")
            .take(5)
        {
            keep.insert(entry.id.clone());
        }
        for entry in entries
            .iter()
            .filter(|e| e.kind == "snapshot" && e.reason == "automatic")
        {
            let day = entry.created_at.date_naive();
            if day >= today - Duration::days(6) && day <= today && days.insert(day) {
                keep.insert(entry.id.clone());
            }
        }
        for entry in entries
            .iter()
            .filter(|e| e.kind == "snapshot" && e.reason != "automatic")
            .take(3)
        {
            keep.insert(entry.id.clone());
        }
        for entry in entries.iter().filter(|e| e.kind == "portable").take(2) {
            keep.insert(entry.id.clone());
        }
        for entry in [
            entries
                .iter()
                .find(|e| e.kind == "snapshot" && e.reason == "automatic"),
            entries
                .iter()
                .find(|e| e.kind == "snapshot" && e.reason != "automatic"),
            entries.iter().find(|e| e.kind == "portable"),
        ]
        .into_iter()
        .flatten()
        {
            protected.insert(entry.id.clone());
        }
        self.store.prune(&BackupRetention {
            keep_ids: keep.into_iter().collect(),
            protected_ids: protected.into_iter().collect(),
            max_bytes: BACKUP_BUDGET_BYTES,
            now: self.clock.now(),
        })
    }
}
