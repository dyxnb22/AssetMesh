//! Recovery behavior against real SQLite/WAL and portable bundles.

#[test]
fn durable_change_markers_survive_reopen_and_track_other_writers_and_rollbacks() {
    let env = Env::new();
    env.add("note", InfoType::Text);
    let store = SqliteBackupStore::new(env.factory.clone(), &env.database);
    let first_token = store.change_token().unwrap();
    let first = store.snapshot(env.clock.now(), "automatic").unwrap();
    let json: serde_json::Value = serde_json::from_slice(
        &fs::read(PathBuf::from(&first.source_dir).join("backup.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(json["change_token"], first_token);
    let reopened = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(env.database.to_str().unwrap()).unwrap(),
    ));
    let other = SqliteBackupStore::new(reopened.clone(), &env.database);
    assert_eq!(other.change_token().unwrap(), first_token);
    assert_eq!(
        other
            .snapshot(env.clock.now() + Duration::hours(1), "automatic")
            .unwrap()
            .id,
        first.id
    );
    reopened
        .0
        .with_raw_connection_mut(|conn| {
            let tx = conn.transaction().unwrap();
            tx.execute("UPDATE assets SET name='rolled back'", [])
                .unwrap();
            tx.rollback().unwrap();
        })
        .unwrap();
    assert_eq!(store.change_token().unwrap(), first_token);
    reopened
        .0
        .with_raw_connection(|conn| {
            conn.execute("UPDATE assets SET name='external write'", [])
                .unwrap()
        })
        .unwrap();
    assert_ne!(store.change_token().unwrap(), first_token);
    let second = store
        .snapshot(env.clock.now() + Duration::hours(2), "automatic")
        .unwrap();
    assert_ne!(second.id, first.id);
    fs::write(
        PathBuf::from(&second.source_dir).join("library.db"),
        b"damaged",
    )
    .unwrap();
    assert!(store.preview(&second.source_dir).is_err());
    let repaired = store
        .snapshot(env.clock.now() + Duration::hours(3), "automatic")
        .unwrap();
    assert_ne!(repaired.id, second.id);
    store.preview(&repaired.source_dir).unwrap();
}

#[test]
fn backups_without_change_markers_keep_their_v1_fingerprint_contract() {
    use assetmesh_core::ports::backup::BackupEntry;
    use sha2::{Digest, Sha256};
    #[derive(serde::Serialize)]
    struct LegacyManifest {
        format: String,
        entry: BackupEntry,
        checksums: BTreeMap<String, String>,
        preferences: BackupPreferences,
    }
    let env = Env::new();
    let entry = env.service.create("manual").unwrap();
    let path = PathBuf::from(&entry.source_dir).join("backup.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("change_token");
    let mut legacy = LegacyManifest {
        format: value["format"].as_str().unwrap().into(),
        entry: serde_json::from_value(value["entry"].clone()).unwrap(),
        checksums: serde_json::from_value(value["checksums"].clone()).unwrap(),
        preferences: serde_json::from_value(value["preferences"].clone()).unwrap(),
    };
    legacy.entry.fingerprint.clear();
    legacy.entry.source_dir.clear();
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&legacy).unwrap()));
    value["entry"]["fingerprint"] = fingerprint.into();
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    env.service.preview(&entry.source_dir).unwrap();
}

use assetmesh_core::{
    application::{
        backup_service::BackupService,
        info_service::{CreateInfo, InfoService},
        library_service::LibraryService,
    },
    domain::{info::InfoType, Timestamp},
    ports::{
        backup::{BackupPreferences, BackupRetention, BackupStore},
        Clock, UuidV7Generator,
    },
};
use assetmesh_storage_sqlite::{
    backup::{backup_directory, selected_database, SqliteBackupStore},
    SharedSqlite,
};
use chrono::{Duration, TimeZone, Utc};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
struct TestClock(Mutex<Timestamp>);
impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }
}
impl TestClock {
    fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}
struct Env {
    root: PathBuf,
    database: PathBuf,
    factory: SharedSqlite,
    clock: Arc<TestClock>,
    service: BackupService<SharedSqlite>,
}
impl Env {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "assetmesh-backup-contract-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&root).unwrap();
        let database = root.join("assetmesh.db");
        let factory = SharedSqlite(Arc::new(
            assetmesh_storage_sqlite::open(database.to_str().unwrap()).unwrap(),
        ));
        let clock = Arc::new(TestClock(Mutex::new(
            Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap(),
        )));
        let store = Arc::new(SqliteBackupStore::new(factory.clone(), &database));
        let service = BackupService::new(store, factory.clone(), clock.clone());
        Self {
            root,
            database,
            factory,
            clock,
            service,
        }
    }
    fn add(&self, name: &str, kind: InfoType) -> assetmesh_core::domain::asset::Asset {
        InfoService::new(
            self.factory.clone(),
            self.clock.clone(),
            Arc::new(UuidV7Generator),
        )
        .create(CreateInfo {
            name: name.into(),
            info_type: kind,
            value: format!("value of {name}"),
            notes: None,
            tags: vec!["personal".into()],
        })
        .unwrap()
    }
    fn count(&self) -> u64 {
        self.factory
            .0
            .with_raw_connection(|c| {
                c.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
                    .unwrap()
            })
            .unwrap()
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn damaged_manifests_are_reported_without_blocking_backup_and_recovery() {
    let env = Env::new();
    env.add("original", InfoType::Text);
    let good = env.service.create("manual").unwrap();
    let directory = backup_directory(&env.database);
    let bad = directory.join(uuid::Uuid::now_v7().to_string());
    fs::create_dir(&bad).unwrap();
    fs::write(bad.join("backup.json"), b"{broken").unwrap();
    let missing = directory.join(uuid::Uuid::now_v7().to_string());
    fs::create_dir(&missing).unwrap();
    let mismatch = directory.join(uuid::Uuid::now_v7().to_string());
    fs::create_dir(&mismatch).unwrap();
    fs::copy(
        PathBuf::from(&good.source_dir).join("backup.json"),
        mismatch.join("backup.json"),
    )
    .unwrap();
    let oversized = directory.join(uuid::Uuid::now_v7().to_string());
    fs::create_dir(&oversized).unwrap();
    fs::write(oversized.join("backup.json"), vec![b'x'; 262_145]).unwrap();

    let status = env.service.status().unwrap();
    assert_eq!(status.entries.len(), 1);
    assert_eq!(status.issues.len(), 4);
    assert_eq!(status.entries[0].id, good.id);
    env.add("later", InfoType::Text);
    env.clock.advance(Duration::minutes(31));
    env.service.tick("test", false).unwrap();
    let status = env.service.status().unwrap();
    assert!(status.last_error.is_none());
    assert_eq!(status.issues.len(), 4);
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.reason == "automatic" && entry.asset_count == 2));
    assert!(status.entries.iter().any(|entry| entry.kind == "portable"));
    assert_eq!(fs::read(bad.join("backup.json")).unwrap(), b"{broken");
    assert!(missing.exists() && mismatch.exists() && oversized.exists());
    assert!(env.service.preview(bad.to_str().unwrap()).is_err());
    let preview = env.service.preview(&good.source_dir).unwrap();
    assert_eq!(preview.asset_count, 1);
    let restored = env
        .service
        .restore(&good.source_dir, &preview.fingerprint)
        .unwrap();
    let conn = rusqlite::Connection::open(restored.db_path).unwrap();
    let count: u64 = conn
        .query_row("SELECT count(*) FROM assets", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(bad.exists(), "recovery must preserve the affected package");
}

#[test]
fn snapshot_captures_wal_and_restores_to_new_library_with_preferences() {
    let env = Env::new();
    let asset = env.add("API", InfoType::ApiKey);
    let preferences = BTreeMap::from([
        ("assetmesh-theme".into(), "dark".into()),
        ("assetmesh-saved-filters-v1".into(), "[]".into()),
    ]);
    env.service.save_preferences(preferences.clone()).unwrap();
    let entry = env.service.create("manual").unwrap();
    assert!(entry.contains_api_keys);
    assert_eq!(entry.asset_count, 1);
    env.add("later", InfoType::Text);
    let receipt = env
        .service
        .restore(&entry.source_dir, &entry.fingerprint)
        .unwrap();
    assert_ne!(PathBuf::from(&receipt.db_path), env.database);
    assert_eq!(receipt.preferences, preferences);
    assert!(receipt.restart_required);
    assert!(env.service.status().unwrap().restore_pending);
    assert_eq!(env.count(), 2, "the running source is preserved");
    assert_eq!(
        selected_database(&env.database).unwrap(),
        PathBuf::from(&receipt.db_path)
    );
    let restored = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(&receipt.db_path).unwrap(),
    ));
    let detail = LibraryService::new(restored.clone())
        .get_asset(asset.id)
        .unwrap();
    assert_eq!(detail.asset.name, "API");
    let count: i64 = restored
        .0
        .with_raw_connection(|c| {
            c.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
                .unwrap()
        })
        .unwrap();
    assert_eq!(count, 1);
    let store = SqliteBackupStore::new(restored, &env.database);
    assert_eq!(store.preferences().unwrap(), preferences);
    assert!(
        env.service.create("manual").is_err(),
        "pending recovery cannot be overwritten"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&entry.source_dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(PathBuf::from(&entry.source_dir).join("library.db"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn changed_or_damaged_backup_is_rejected_without_switching_or_mutating_source() {
    let env = Env::new();
    env.add("original", InfoType::Text);
    let entry = env.service.create("manual").unwrap();
    assert!(env
        .service
        .restore(&entry.source_dir, "wrong-fingerprint")
        .is_err());
    assert_eq!(selected_database(&env.database).unwrap(), env.database);
    let path = PathBuf::from(&entry.source_dir).join("library.db");
    let mut contents = fs::read(&path).unwrap();
    contents[100] ^= 1;
    fs::write(path, contents).unwrap();
    assert!(env.service.preview(&entry.source_dir).is_err());
    assert!(env
        .service
        .restore(&entry.source_dir, &entry.fingerprint)
        .is_err());
    assert_eq!(env.count(), 1);
    assert!(!env.service.status().unwrap().restore_pending);
    assert_eq!(selected_database(&env.database).unwrap(), env.database);
}

#[test]
fn portable_external_copy_recovers_on_another_installation() {
    let source = Env::new();
    let asset = source.add("portable", InfoType::Text);
    source.add("private key", InfoType::ApiKey);
    source
        .service
        .save_preferences(BTreeMap::from([("assetmesh-lang".into(), "zh".into())]))
        .unwrap();
    let destination = Env::new();
    let exported = source
        .service
        .export_copy(destination.root.to_str().unwrap(), "test-version")
        .unwrap();
    let entry = destination.service.preview(&exported).unwrap();
    assert_eq!(entry.kind, "portable");
    assert!(entry.contains_api_keys);
    assert_eq!(entry.asset_count, 2);
    let receipt = destination
        .service
        .restore(&exported, &entry.fingerprint)
        .unwrap();
    assert_eq!(receipt.preferences["assetmesh-lang"], "zh");
    let recovered = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(&receipt.db_path).unwrap(),
    ));
    assert_eq!(
        LibraryService::new(recovered)
            .get_asset(asset.id)
            .unwrap()
            .asset
            .name,
        "portable"
    );
    assert_eq!(destination.count(), 0);
    assert_eq!(source.count(), 2);
}

#[test]
fn automatic_backups_coalesce_changes_flush_on_exit_and_notice_other_writers() {
    let env = Env::new();
    env.service.tick("test", false).unwrap();
    let initial = env.service.status().unwrap().entries.len();
    env.service.tick("test", false).unwrap();
    assert_eq!(env.service.status().unwrap().entries.len(), initial);
    env.add("changed", InfoType::Text);
    env.clock.advance(Duration::minutes(29));
    env.service.tick("test", false).unwrap();
    assert_eq!(env.service.status().unwrap().entries.len(), initial);
    env.clock.advance(Duration::minutes(1));
    env.service.tick("test", false).unwrap();
    assert_eq!(env.service.status().unwrap().entries.len(), initial + 1);
    let external = assetmesh_storage_sqlite::open(env.database.to_str().unwrap()).unwrap();
    external
        .with_raw_connection(|c| {
            c.execute("UPDATE assets SET name='external update'", [])
                .unwrap()
        })
        .unwrap();
    env.service.tick("test", true).unwrap();
    assert_eq!(env.service.status().unwrap().entries.len(), initial + 2);
}

#[test]
fn retention_preserves_recent_snapshots_daily_history_and_operation_points() {
    let env = Env::new();
    for index in 0..9 {
        env.add(&format!("Daily {index}"), InfoType::Text);
        env.service.create("automatic").unwrap();
        env.clock.advance(Duration::days(1));
    }
    for index in 0..15 {
        env.add(&format!("Recent {index}"), InfoType::Text);
        env.service.create("automatic").unwrap();
        env.clock.advance(Duration::minutes(1));
    }
    for _ in 0..8 {
        env.service.create("before_import").unwrap();
        env.clock.advance(Duration::minutes(1));
    }
    let entries = env.service.status().unwrap().entries;
    let automatic: Vec<_> = entries.iter().filter(|e| e.reason == "automatic").collect();
    assert!(automatic.len() >= 5 && automatic.len() <= 11);
    let days: std::collections::BTreeSet<_> = automatic
        .iter()
        .map(|e| e.created_at.date_naive())
        .collect();
    assert_eq!(days.len(), 7);
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.reason == "before_import")
            .count(),
        3
    );
}

#[test]
fn unchanged_relaunches_reuse_the_latest_snapshot_without_triggering_weekly_exports() {
    let env = Env::new();
    env.add("Keep", InfoType::Text);
    env.service.tick("test", false).unwrap();
    let original: Vec<_> = env
        .service
        .status()
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.id)
        .collect();
    env.clock.advance(Duration::days(8));
    let reopened = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(env.database.to_str().unwrap()).unwrap(),
    ));
    let store = Arc::new(SqliteBackupStore::new(reopened.clone(), &env.database));
    let service = BackupService::new(store, reopened, env.clock.clone());
    service.tick("test", false).unwrap();
    service.tick("test", true).unwrap();
    assert_eq!(
        service
            .status()
            .unwrap()
            .entries
            .into_iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        original
    );

    // An offline/other-connection write must still be backed up on relaunch.
    env.add("Offline change", InfoType::Text);
    let reopened = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(env.database.to_str().unwrap()).unwrap(),
    ));
    let store = Arc::new(SqliteBackupStore::new(reopened.clone(), &env.database));
    let service = BackupService::new(store, reopened, env.clock.clone());
    service.tick("test", false).unwrap();
    let status = service.status().unwrap();
    assert!(status
        .entries
        .iter()
        .any(|e| e.kind == "snapshot" && e.asset_count == 2));
    assert!(status.entries.len() > original.len());
    assert!(status.storage_bytes > 0);
}

#[test]
fn preference_only_changes_create_a_new_usable_recovery_point() {
    let env = Env::new();
    env.service.tick("test", false).unwrap();
    let original = env.service.status().unwrap().entries.len();
    env.service
        .save_preferences(BTreeMap::from([("assetmesh-theme".into(), "dark".into())]))
        .unwrap();
    env.service.tick("test", true).unwrap();
    let entries = env.service.status().unwrap().entries;
    assert_eq!(entries.len(), original + 1);
    let receipt = env
        .service
        .restore(&entries[0].source_dir, &entries[0].fingerprint)
        .unwrap();
    assert_eq!(receipt.preferences["assetmesh-theme"], "dark");
}

#[test]
fn space_budget_evicts_oldest_copies_but_always_preserves_a_usable_latest_point() {
    let env = Env::new();
    for index in 0..5 {
        env.add(&format!("Record {index}"), InfoType::Text);
        env.service.create("automatic").unwrap();
        env.clock.advance(Duration::minutes(1));
    }
    let store = SqliteBackupStore::new(env.factory.clone(), &env.database);
    let entries = store.list().unwrap();
    let budget = store.storage_bytes().unwrap() / 2;
    let mut retention = BackupRetention {
        keep_ids: entries.iter().map(|e| e.id.clone()).collect(),
        protected_ids: vec![entries[0].id.clone()],
        max_bytes: budget,
        now: env.clock.now(),
    };
    store.prune(&retention).unwrap();
    assert!(store.storage_bytes().unwrap() <= budget);
    assert_eq!(store.list().unwrap()[0].id, entries[0].id);
    assert!(!PathBuf::from(&entries.last().unwrap().source_dir).exists());
    store.preview(&entries[0].source_dir).unwrap();

    retention.max_bytes = 1; // An essential point can exceed a small budget.
    store.prune(&retention).unwrap();
    assert_eq!(store.list().unwrap().len(), 1);
    store.preview(&entries[0].source_dir).unwrap();
    assert_eq!(env.count(), 5);
}

fn age_file(path: &std::path::Path, now: Timestamp, days: i64) {
    let modified: std::time::SystemTime = (now - Duration::days(days)).into();
    fs::File::open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
}

#[test]
fn maintenance_cleans_only_aged_owned_staging_and_runs_without_new_writes() {
    let env = Env::new();
    env.service.tick("test", false).unwrap();
    let root = backup_directory(&env.database);
    let aged = root.join(format!(".partial-{}", uuid::Uuid::now_v7()));
    let fresh = root.join(format!(".partial-{}", uuid::Uuid::now_v7()));
    let unrelated = root.join(".partial-my-notes");
    for path in [&aged, &fresh, &unrelated] {
        fs::create_dir(path).unwrap();
        fs::write(path.join("notes.txt"), "preserve unknown folders").unwrap();
    }
    let temporary = root.join(format!("preferences.partial-{}", uuid::Uuid::now_v7()));
    fs::write(&temporary, "interrupted write").unwrap();
    age_file(&aged, env.clock.now(), 2);
    age_file(&unrelated, env.clock.now(), 2);
    age_file(&temporary, env.clock.now(), 2);
    let before = env.service.status().unwrap().entries.len();
    env.service.tick("test", false).unwrap();
    assert!(
        aged.exists(),
        "idle maintenance is deferred until its hourly check"
    );
    env.clock.advance(Duration::hours(1));
    env.service.tick("test", false).unwrap();
    assert!(!aged.exists());
    assert!(!temporary.exists());
    assert!(fresh.exists());
    assert!(unrelated.exists());
    assert_eq!(env.service.status().unwrap().entries.len(), before);
}

#[test]
fn restored_library_cleanup_protects_selected_running_previous_and_open_libraries() {
    let env = Env::new();
    env.add("Original", InfoType::Text);
    let entry = env.service.create("manual").unwrap();
    let selected = PathBuf::from(
        env.service
            .restore(&entry.source_dir, &entry.fingerprint)
            .unwrap()
            .db_path,
    );
    let restored = selected.parent().unwrap();
    let mut unused = Vec::new();
    for _ in 0..4 {
        let path = restored.join(format!("{}.db", uuid::Uuid::now_v7()));
        fs::copy(&selected, &path).unwrap();
        unused.push(path);
    }
    let open = assetmesh_storage_sqlite::open(unused[0].to_str().unwrap()).unwrap();
    let previous = &unused[1];
    for path in [&selected, &unused[0], &unused[2], &unused[3]] {
        age_file(path, env.clock.now(), 40);
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", unused[0].display()));
        if sidecar.exists() {
            age_file(&sidecar, env.clock.now(), 40);
        }
    }
    age_file(previous, env.clock.now(), 35);

    let running = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(unused[2].to_str().unwrap()).unwrap(),
    ));
    let store = SqliteBackupStore::new(running, &env.database);
    store
        .prune(&BackupRetention {
            keep_ids: vec![entry.id.clone()],
            protected_ids: vec![entry.id.clone()],
            max_bytes: 1,
            now: env.clock.now(),
        })
        .unwrap();
    assert!(selected.exists());
    assert!(previous.exists());
    assert!(unused[0].exists(), "an open library must not be unlinked");
    assert!(
        unused[2].exists(),
        "the running library must not be unlinked"
    );
    assert!(
        !unused[3].exists(),
        "an expired inactive managed library is cleaned"
    );
    assert!(env.database.exists());
    drop(open);
}

#[test]
fn failed_backup_keeps_work_dirty_and_recovers_when_destination_is_available() {
    let env = Env::new();
    env.service.tick("test", false).unwrap();
    env.add("not yet backed up", InfoType::Text);
    env.clock.advance(Duration::minutes(31));
    let root = backup_directory(&env.database);
    let moved = env.root.join("unavailable-backups");
    fs::rename(&root, &moved).unwrap();
    fs::write(&root, "blocked").unwrap();
    assert!(env.service.tick("test", false).is_err());
    assert_eq!(env.count(), 1);
    fs::remove_file(&root).unwrap();
    fs::rename(&moved, &root).unwrap();
    assert!(env.service.status().unwrap().last_error.is_some());
    env.service.tick("test", false).unwrap();
    assert!(env.service.status().unwrap().last_error.is_none());
    assert_eq!(env.service.status().unwrap().entries[0].asset_count, 1);
}

#[test]
fn migration_creates_recovery_point_with_original_schema_before_upgrading() {
    let env = Env::new();
    let legacy = env.root.join("legacy.db");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase2_software.db"),
        &legacy,
    )
    .unwrap();
    let factory = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(legacy.to_str().unwrap()).unwrap(),
    ));
    let store = SqliteBackupStore::new(factory, &legacy);
    let entry = store.list().unwrap().pop().unwrap();
    assert_eq!(entry.reason, "before_migration");
    store.preview(&entry.source_dir).unwrap();
    let old =
        rusqlite::Connection::open(PathBuf::from(entry.source_dir).join("library.db")).unwrap();
    let version: i64 = old
        .query_row("SELECT max(version) FROM assetmesh_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(version, 2);
    assert!(entry.asset_count > 0);
}

#[test]
fn only_supported_preferences_can_be_backed_up() {
    let env = Env::new();
    for prefs in [
        BTreeMap::from([("password".into(), "secret".into())]),
        BTreeMap::from([("assetmesh-theme".into(), "invalid".into())]),
        BTreeMap::from([("assetmesh-saved-filters-v1".into(), "{}".into())]),
    ] {
        assert!(env.service.save_preferences(prefs).is_err());
    }
    assert_eq!(env.service.preferences().unwrap(), BackupPreferences::new());
}
