use assetmesh_core::{application::info_service::CreateInfo, domain::info::InfoType};
use assetmesh_desktop_lib::{state::AppStatus, DesktopState};

#[test]
fn recovery_remains_available_when_source_database_cannot_be_opened() {
    let root = std::env::temp_dir().join(format!(
        "assetmesh-desktop-recovery-{}",
        uuid::Uuid::now_v7()
    ));
    std::fs::create_dir(&root).unwrap();
    let db = root.join("assetmesh.db");
    let state = DesktopState::new();
    state.initialize(&db).unwrap();
    state
        .with_modules(|modules| {
            modules.info().create(CreateInfo {
                name: "Keep this record".into(),
                info_type: InfoType::Text,
                value: "durable".into(),
                notes: None,
                tags: Vec::new(),
            })?;
            Ok(())
        })
        .unwrap();
    let entry = state.backup().unwrap().create("manual").unwrap();
    // A diverged migration checksum makes the normal application refuse to open.
    state
        .with_factory(|factory| {
            factory.0.with_raw_connection(|conn| {
                conn.execute(
                    "UPDATE assetmesh_migrations SET checksum='damaged' WHERE version=1",
                    [],
                )
                .unwrap()
            })?;
            Ok(())
        })
        .unwrap();
    drop(state);
    let failed = DesktopState::new();
    assert!(matches!(
        failed.initialize(&db).unwrap(),
        AppStatus::CorruptFailure { .. }
    ));
    let backup = failed.backup().unwrap();
    assert!(backup.create("manual").is_err());
    backup.preview(&entry.source_dir).unwrap();
    backup
        .restore(&entry.source_dir, &entry.fingerprint)
        .unwrap();
    drop(failed);
    let reopened = DesktopState::new();
    assert!(matches!(
        reopened.initialize(&db).unwrap(),
        AppStatus::Ready { .. }
    ));
    assert_eq!(reopened.default_db_path().unwrap(), db);
    reopened
        .with_factory(|factory| {
            let count: i64 = factory.0.with_raw_connection(|conn| {
                conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
                    .unwrap()
            })?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
    // The original remains damaged and unmodified, available for inspection.
    assert!(assetmesh_storage_sqlite::open(db.to_str().unwrap()).is_err());
    drop(reopened);
    let _ = std::fs::remove_dir_all(root);
}
