//! SQLite contract tests: connection configuration, migrations,
//! repository behavior, transaction atomicity, search projection, and the
//! portable round trip against the real storage adapter.

use assetmesh_core::application::media_service::{
    CreateMedia, ExternalRefInput, MediaService, UpdateMediaMetadata,
};
use assetmesh_core::application::portable::{
    read_bundle_from_directory, PortableExportService, PortableImportService, EXPORT_VERSION,
};
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::{SharedClock, SharedIdGenerator};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::ports::clock::Clock;
use assetmesh_core::ports::repos::MediaFilter;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, AppResult};
use assetmesh_storage_sqlite::SharedSqlite;
use std::sync::Arc;

fn env() -> TestSqlite {
    let factory = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    let clock: SharedClock = Arc::new(assetmesh_core::ports::clock::SystemClock);
    let ids: SharedIdGenerator = Arc::new(assetmesh_core::ports::ids::UuidV7Generator);
    TestSqlite {
        factory,
        clock,
        ids,
    }
}

struct TestSqlite {
    factory: SharedSqlite,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl TestSqlite {
    fn media_service(&self) -> MediaService<SharedSqlite> {
        MediaService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn asset_service(
        &self,
    ) -> assetmesh_core::application::asset_service::AssetService<SharedSqlite> {
        assetmesh_core::application::asset_service::AssetService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
    }
    fn search_service(&self) -> SearchService<SharedSqlite> {
        SearchService::new(self.factory.clone(), self.clock.clone())
    }
    fn export_service(&self) -> PortableExportService<SharedSqlite> {
        PortableExportService::new(self.factory.clone(), self.clock.clone())
    }
}

fn create_cmd(title: &str, media_type: MediaType) -> CreateMedia {
    CreateMedia {
        title: title.into(),
        media_type,
        summary: None,
        status: None,
        rating: None,
        year: Some(2020),
        platform: None,
        progress: Progress::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

// ---------------------------------------------------------------------------
// Connection configuration (ADR 0007)
// ---------------------------------------------------------------------------

#[test]
fn connection_configures_foreign_keys_wal_and_busy_timeout() {
    // WAL applies to file-backed databases; :memory: keeps its own mode.
    let dir = std::env::temp_dir().join(format!("assetmesh-pragma-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let factory = assetmesh_storage_sqlite::open(&dir.join("p.db").to_string_lossy()).unwrap();
    factory
        .with_raw_connection(|conn| {
            let foreign_keys: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .unwrap();
            assert_eq!(foreign_keys, 1, "foreign_keys must be ON");

            let busy: i64 = conn
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
                .unwrap();
            assert!(busy > 0, "busy_timeout must be bounded and positive");

            let journal: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert_eq!(journal.to_lowercase(), "wal", "file DBs must use WAL");
        })
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn foreign_keys_are_enforced() {
    let mut factory = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    let err = factory
        .transact(&mut |uow| {
            // Insert a media record with no backing asset: FK must reject it.
            uow.media()
                .upsert(&assetmesh_core::domain::media::MediaRecord {
                    asset_id: AssetId::generate(),
                    media_type: MediaType::Movie,
                    status: MediaStatus::Planned,
                    rating: None,
                    year: None,
                    platform: None,
                    progress: Progress::default(),
                    notes: None,
                    started_at: None,
                    completed_at: None,
                })
        })
        .unwrap_err();
    assert!(matches!(
        err,
        AppError::Storage { .. } | AppError::Conflict { .. }
    ));
}

// ---------------------------------------------------------------------------
// Migrations
// ---------------------------------------------------------------------------

#[test]
fn fresh_database_reaches_latest_and_reopens_cleanly() {
    let dir = std::env::temp_dir().join(format!("assetmesh-mig-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let path = db_path.to_string_lossy().to_string();

    {
        let factory = assetmesh_storage_sqlite::open(&path).unwrap();
        factory
            .with_raw_connection(|conn| {
                let version: i64 = conn
                    .query_row("SELECT MAX(version) FROM assetmesh_migrations", [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(version, assetmesh_storage_sqlite::latest_db_version());
            })
            .unwrap();
    }

    // Reopening applies nothing new and verifies checksums.
    {
        let factory = assetmesh_storage_sqlite::open(&path).unwrap();
        factory
            .with_raw_connection(|conn| {
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM assetmesh_migrations", [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(count, assetmesh_storage_sqlite::latest_db_version());
            })
            .unwrap();
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn modified_applied_migration_fails_loudly() {
    // Build a database at the latest version, tamper with the recorded
    // checksum, and verify the runner refuses to start.
    let dir = std::env::temp_dir().join(format!("assetmesh-tamper-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.db").to_string_lossy().to_string();
    {
        let _ = assetmesh_storage_sqlite::open(&path).unwrap();
    }
    // Tamper with the recorded checksum.
    {
        use rusqlite::Connection;
        let conn = Connection::open(&path).unwrap();
        conn.execute("UPDATE assetmesh_migrations SET checksum = 'tampered'", [])
            .unwrap();
    }
    let result = assetmesh_storage_sqlite::open(&path);
    assert!(result.is_err(), "tampered migration must fail loudly");

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Repository contracts
// ---------------------------------------------------------------------------

#[test]
fn media_crud_round_trips_all_fields() {
    let env = env();
    let mut media = env.media_service();

    let view = media
        .create_media(CreateMedia {
            status: Some(MediaStatus::Completed),
            rating: Some(8.5),
            year: Some(1999),
            platform: Some("Steam".into()),
            progress: Progress {
                current: Some(45.0),
                total: Some(100.0),
                unit: Some("percent".into()),
            },
            notes: Some("great".into()),
            tags: vec!["fps".into(), "classic".into()],
            external_refs: vec![ExternalRefInput {
                namespace: "steam".into(),
                external_id: "1091500".into(),
                source_url: None,
            }],
            ..create_cmd("Cyberpunk", MediaType::Game)
        })
        .unwrap();
    let id = view.entry.asset.id;

    let fetched = media.get_media(id).unwrap();
    let record = &fetched.entry.record;
    assert_eq!(record.status, MediaStatus::Completed);
    assert_eq!(record.rating, Some(8.5));
    assert_eq!(record.year, Some(1999));
    assert_eq!(record.platform.as_deref(), Some("Steam"));
    assert_eq!(record.progress.current, Some(45.0));
    assert_eq!(record.progress.total, Some(100.0));
    assert_eq!(record.progress.unit.as_deref(), Some("percent"));
    assert_eq!(record.notes.as_deref(), Some("great"));
    assert_eq!(fetched.tags, vec!["classic".to_string(), "fps".to_string()]);
    assert!(record.started_at.is_some() && record.completed_at.is_some());

    // Update flows through the same application layer.
    let updated = media
        .update_metadata(UpdateMediaMetadata {
            asset_id: id,
            notes: Some("revisited".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(updated.entry.record.notes.as_deref(), Some("revisited"));
}

#[test]
fn external_ref_uniqueness_is_enforced_across_services() {
    let env = env();
    let mut media = env.media_service();

    media
        .create_media(CreateMedia {
            external_refs: vec![ExternalRefInput {
                namespace: "igdb".into(),
                external_id: "1877".into(),
                source_url: None,
            }],
            ..create_cmd("Hades", MediaType::Game)
        })
        .unwrap();

    let err = media
        .create_media(CreateMedia {
            external_refs: vec![ExternalRefInput {
                namespace: "igdb".into(),
                external_id: "1877".into(),
                source_url: None,
            }],
            ..create_cmd("Hades II", MediaType::Game)
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));

    // The application-level pre-check gives the same result via the port.
    let mut factory = env.factory.clone();
    factory
        .read(&mut |uow| {
            let found = uow
                .external_refs()
                .find_asset_by_ref("igdb", "1877")?
                .expect("ref must resolve");
            assert!(!found.to_string().is_empty());
            Ok(())
        })
        .unwrap();
}

#[test]
fn transaction_rolls_back_state_and_activity_atomically() {
    let env = env();
    let mut media = env.media_service();
    let created = media
        .create_media(create_cmd("Atomic", MediaType::Movie))
        .unwrap();
    let id = created.entry.asset.id;

    // A use case that fails mid-transaction must leave no partial state.
    let mut factory = env.factory.clone();
    let result: AppResult<()> = factory.transact(&mut |uow| {
        let mut asset = uow.assets().get(id)?.unwrap();
        asset.name = "Renamed".into();
        asset.touch(assetmesh_core::ports::clock::SystemClock.now());
        uow.assets().update(&asset)?;
        // Then fail.
        Err(AppError::conflict("boom"))
    });
    assert!(result.is_err());

    let view = media.get_media(id).unwrap();
    assert_eq!(
        view.entry.asset.name, "Atomic",
        "rename must be rolled back"
    );
}

#[test]
fn use_case_failure_rolls_back_everything() {
    let env = env();
    let mut media = env.media_service();
    media
        .create_media(create_cmd("First", MediaType::Movie))
        .unwrap();

    // create_media with a duplicate external ref fails during the insert;
    // the asset/media/activity writes before it must roll back.
    let mut media2 = env.media_service();
    // first attach the ref
    media2
        .create_media(CreateMedia {
            external_refs: vec![ExternalRefInput {
                namespace: "tmdb".into(),
                external_id: "42".into(),
                source_url: None,
            }],
            ..create_cmd("Ref owner", MediaType::Movie)
        })
        .unwrap();

    let before = media.list_media(&MediaFilter::default()).unwrap().len();

    let err = media2
        .create_media(CreateMedia {
            external_refs: vec![ExternalRefInput {
                namespace: "tmdb".into(),
                external_id: "42".into(),
                source_url: None,
            }],
            ..create_cmd("Should not exist", MediaType::Movie)
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }));

    let after = media.list_media(&MediaFilter::default()).unwrap().len();
    assert_eq!(
        before, after,
        "failed use case must not persist partial state"
    );
}

// ---------------------------------------------------------------------------
// Search projection
// ---------------------------------------------------------------------------

#[test]
fn search_finds_titles_notes_platform_and_refs_and_rebuilds() {
    let env = env();
    let mut media = env.media_service();
    let mut search = env.search_service();

    media
        .create_media(CreateMedia {
            notes: Some("cozy farming simulator".into()),
            platform: Some("Nintendo Switch".into()),
            tags: vec!["cozy".into()],
            external_refs: vec![ExternalRefInput {
                namespace: "steam".into(),
                external_id: "1091500".into(),
                source_url: None,
            }],
            ..create_cmd("Stardew Valley", MediaType::Game)
        })
        .unwrap();

    assert!(search.search("stardew", 10).unwrap().len() == 1);
    assert!(search.search("farming", 10).unwrap().len() == 1);
    assert!(search.search("switch", 10).unwrap().len() == 1);
    assert!(search.search("steam:1091500", 10).unwrap().len() == 1);

    // CJK substring falls back to LIKE.
    let mut media2 = env.media_service();
    media2
        .create_media(create_cmd("葬送的芙莉莲", MediaType::Anime))
        .unwrap();
    let hits = search.search("芙莉莲", 10).unwrap();
    assert!(hits.iter().any(|h| h.title.contains("芙莉莲")));

    // Drop the projection entirely and rebuild from canonical state.
    let mut factory = env.factory.clone();
    factory
        .transact(&mut |uow| uow.search_index().clear())
        .unwrap();
    assert!(search.search("stardew", 10).unwrap().is_empty());

    let report = search.rebuild().unwrap();
    assert_eq!(report.indexed, 2);
    assert!(search.search("stardew", 10).unwrap().len() == 1);
}

// ---------------------------------------------------------------------------
// Portable round trip on SQLite
// ---------------------------------------------------------------------------

#[test]
fn sqlite_portable_round_trip() {
    let env_a = env();
    {
        let mut media = env_a.media_service();
        let mut assets = env_a.asset_service();

        let dup = media
            .create_media(create_cmd("Duplicate Movie", MediaType::Movie))
            .unwrap();
        let keep = media
            .create_media(CreateMedia {
                rating: Some(7.0),
                external_refs: vec![ExternalRefInput {
                    namespace: "tmdb".into(),
                    external_id: "123".into(),
                    source_url: None,
                }],
                ..create_cmd("Kept Movie", MediaType::Movie)
            })
            .unwrap();
        assets.archive_asset(dup.entry.asset.id).unwrap();
        assets
            .merge_assets(dup.entry.asset.id, keep.entry.asset.id)
            .unwrap();
    }

    let mut export = env_a.export_service();
    let bundle = export.export("0.1.0-test").unwrap();
    assert_eq!(bundle.manifest.version, EXPORT_VERSION);

    let dir = std::env::temp_dir().join(format!("assetmesh-rt-{}", uuid::Uuid::now_v7()));
    assetmesh_core::application::portable::write_bundle_to_directory(&bundle, &dir).unwrap();

    let env_b = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    {
        let reloaded = read_bundle_from_directory(&dir).unwrap();
        let mut import = PortableImportService::new(env_b.clone());
        import.import_bundle(&reloaded, false).unwrap();
    }

    // Verify canonical equivalence by exporting B and diffing the files.
    let mut export_b = PortableExportService::new(env_b.clone(), env_a.clock.clone());
    let bundle_b = export_b.export("compare").unwrap();

    for path in [
        "assets.jsonl",
        "external_refs.jsonl",
        "activity.jsonl",
        "modules/media.jsonl",
        "asset_tags.jsonl",
    ] {
        assert_eq!(
            bundle.file(path).unwrap(),
            bundle_b.file(path).unwrap(),
            "canonical data must be identical for {path}"
        );
    }

    let mut search_b = SearchService::new(env_b.clone(), env_a.clock.clone());
    assert!(search_b.search("kept", 10).unwrap().len() == 1);

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Concurrency (ADR 0007): migration races, snapshot reads, WAL concurrency
// ---------------------------------------------------------------------------

#[test]
fn concurrent_first_open_serializes_migrations() {
    let dir = std::env::temp_dir().join(format!("assetmesh-race-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("race.db").to_string_lossy().to_string();

    // Several processes (threads with independent connections) opening the
    // same fresh database: every opener must succeed — the migration runs
    // under one immediate transaction and later openers see it applied.
    let mut handles = Vec::new();
    for i in 0..8 {
        let p = path.clone();
        handles.push(std::thread::spawn(move || {
            let factory = assetmesh_storage_sqlite::open(&p)?;
            factory.with_raw_connection(|conn| {
                let n: i64 = conn
                    .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(n, 0, "thread {i} migrated database");
            })
        }));
    }
    for handle in handles {
        handle
            .join()
            .unwrap()
            .expect("concurrent open must succeed");
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_scopes_are_snapshot_consistent_and_do_not_block_writes() {
    let dir = std::env::temp_dir().join(format!("assetmesh-snap-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("snap.db").to_string_lossy().to_string();

    let factory = SharedSqlite(Arc::new(assetmesh_storage_sqlite::open(&path).unwrap()));
    {
        let mut media = MediaService::new(factory.clone(), env().clock, env().ids);
        media
            .create_media(create_cmd("First", MediaType::Movie))
            .unwrap();
    }

    // A long read scope must observe one consistent snapshot even though a
    // writer commits mid-read, and the writer must not block on the reader.
    let mut reader = factory.clone();
    let clock = env().clock;
    let ids = env().ids;
    let reader_handle = std::thread::spawn(move || {
        reader.read(&mut |q| {
            let before = q
                .assets()
                .list(&assetmesh_core::ports::repos::AssetFilter {
                    kind: None,
                    lifecycle: Some(assetmesh_core::ports::repos::LifecycleFilter::All),
                })?;
            assert_eq!(before.len(), 1);
            std::thread::sleep(std::time::Duration::from_millis(300));
            let after = q
                .assets()
                .list(&assetmesh_core::ports::repos::AssetFilter {
                    kind: None,
                    lifecycle: Some(assetmesh_core::ports::repos::LifecycleFilter::All),
                })?;
            // Snapshot consistency: the concurrent commit is invisible here.
            assert_eq!(after.len(), 1, "read scope must see one snapshot");
            Ok(())
        })
    });

    // Concurrent write while the reader is inside its snapshot.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut media = MediaService::new(factory.clone(), clock, ids);
    media
        .create_media(create_cmd("Second", MediaType::Movie))
        .unwrap();

    reader_handle.join().unwrap().unwrap();

    // The new asset is visible to a fresh read scope.
    let mut factory = factory;
    factory
        .read(&mut |q| {
            let all = q
                .assets()
                .list(&assetmesh_core::ports::repos::AssetFilter {
                    kind: None,
                    lifecycle: Some(assetmesh_core::ports::repos::LifecycleFilter::All),
                })?;
            assert_eq!(all.len(), 2);
            Ok(())
        })
        .unwrap();

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn non_finite_progress_is_rejected_by_services() {
    let env = env();
    let mut media = env.media_service();

    let mut cmd = create_cmd("Bad progress", MediaType::Anime);
    cmd.progress = Progress {
        current: Some(f64::INFINITY),
        total: Some(28.0),
        unit: Some("episode".into()),
    };
    assert!(
        media.create_media(cmd).is_err(),
        "infinity must be rejected"
    );

    let created = media
        .create_media(create_cmd("Fine", MediaType::Anime))
        .unwrap();
    let err = media
        .update_progress(
            created.entry.asset.id,
            Progress {
                current: Some(f64::NAN),
                total: None,
                unit: Some("episode".into()),
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("finite"), "{err}");
}

// ---------------------------------------------------------------------------
// Atomic export replacement
// ---------------------------------------------------------------------------

#[test]
fn export_overwrite_preserves_bundle_on_swap() {
    use assetmesh_core::application::portable::{
        read_bundle_from_directory, write_bundle_to_directory,
    };

    let dir = std::env::temp_dir().join(format!("assetmesh-swap-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("bundle");

    let env_a = env();
    {
        let mut media = env_a.media_service();
        media
            .create_media(create_cmd("V1 asset", MediaType::Movie))
            .unwrap();
    }
    let mut export = env_a.export_service();
    let bundle1 = export.export("swap-test-1").unwrap();
    write_bundle_to_directory(&bundle1, &target).unwrap();
    let manifest1 = std::fs::read_to_string(target.join("manifest.json")).unwrap();

    // Overwrite with a new bundle; the swap must fully replace the old one.
    {
        let mut media = env_a.media_service();
        media
            .create_media(create_cmd("V2 asset", MediaType::Game))
            .unwrap();
    }
    let bundle2 = export.export("swap-test-2").unwrap();
    write_bundle_to_directory(&bundle2, &target).unwrap();

    let reloaded = read_bundle_from_directory(&target).unwrap();
    assert_eq!(reloaded.manifest.app_version, "swap-test-2");
    assert_ne!(
        std::fs::read_to_string(target.join("manifest.json")).unwrap(),
        manifest1,
        "old bundle must be replaced"
    );

    // No staging/backup leftovers.
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.contains("staging") || name.contains("previous")
        })
        .collect();
    assert!(leftovers.is_empty(), "staging leftovers: {leftovers:?}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn rebuild_and_concurrent_writes_keep_the_projection_current() {
    let dir = std::env::temp_dir().join(format!("assetmesh-rebuild-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rebuild.db").to_string_lossy().to_string();

    let factory = SharedSqlite(Arc::new(assetmesh_storage_sqlite::open(&path).unwrap()));
    let clock: SharedClock = Arc::new(assetmesh_core::ports::clock::SystemClock);
    let ids: SharedIdGenerator = Arc::new(assetmesh_core::ports::ids::UuidV7Generator);

    {
        let mut media = MediaService::new(factory.clone(), clock.clone(), ids.clone());
        media
            .create_media(create_cmd("Before rebuild", MediaType::Movie))
            .unwrap();
    }

    // Rebuild runs concurrently with a writer committing a new asset. The
    // fenced rebuild either includes the new asset (writer committed first)
    // or the writer's own synchronous projection update lands after the
    // rebuild — either way the final index must contain it.
    let writer_factory = factory.clone();
    let writer_clock = clock.clone();
    let writer_ids = ids.clone();
    let writer = std::thread::spawn(move || {
        let mut media = MediaService::new(writer_factory, writer_clock, writer_ids);
        for i in 0..10 {
            let title = format!("Concurrent {i}");
            media
                .create_media(CreateMedia {
                    title,
                    media_type: MediaType::Movie,
                    ..create_cmd("ignored", MediaType::Movie)
                })
                .unwrap();
        }
    });

    let mut search = SearchService::new(factory.clone(), clock);
    let _report = search.rebuild().unwrap();
    writer.join().unwrap();

    // Drop the projection and rebuild again after everything settled: the
    // rebuild must observe all committed canonical state.
    let report = search.rebuild().unwrap();
    assert!(
        report.indexed >= 11,
        "rebuild sees every live asset: {}",
        report.indexed
    );
    let hits = search.search("concurrent", 50).unwrap();
    assert_eq!(
        hits.len(),
        10,
        "writer assets fully indexed: {}",
        hits.len()
    );
    assert!(search.search("before rebuild", 10).unwrap().len() == 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn invalid_or_older_module_versions_are_rejected() {
    let dir = std::env::temp_dir().join(format!("assetmesh-modver-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.db").to_string_lossy().to_string();

    for bad_version in [0i64, -1, 2, 99] {
        {
            let _ = assetmesh_storage_sqlite::open(&path).unwrap();
            // Tamper the module metadata.
            factory_tamper_module_version(&path, bad_version);
        }
        let result = assetmesh_storage_sqlite::open(&path);
        let err = result.err().expect("invalid module version must fail");
        assert!(
            err.to_string().contains("media module"),
            "version {bad_version}: {err}"
        );
        // Restore a valid database for the next iteration.
        std::fs::remove_file(&path).ok();
        for suffix in ["-wal", "-shm"] {
            std::fs::remove_file(format!("{path}{suffix}")).ok();
        }
    }

    std::fs::remove_dir_all(&dir).ok();
}

fn factory_tamper_module_version(path: &str, version: i64) {
    use rusqlite::Connection;
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "UPDATE module_metadata SET schema_version = ?1 WHERE module_id = 'media'",
        [version],
    )
    .unwrap();
}
