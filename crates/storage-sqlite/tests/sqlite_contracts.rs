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
use assetmesh_core::domain::asset::{Asset, AssetKind};
use assetmesh_core::domain::ids::{AssetId, RelationId};
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::domain::relation::{Relation, RelationProvenance, RelationType};
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

// ---------------------------------------------------------------------------
// Software module (Phase 2)
// ---------------------------------------------------------------------------

use assetmesh_core::application::software_service::{
    CreateSoftware, SoftwareService, UpdateSoftwareMetadata,
};
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::repos::SoftwareFilter;

impl TestSqlite {
    fn software_service(&self) -> SoftwareService<SharedSqlite> {
        SoftwareService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
}

fn software_cmd(name: &str, category: SoftwareCategory) -> CreateSoftware {
    CreateSoftware {
        name: name.into(),
        category,
        summary: None,
        install_source: None,
        version: Some("1.0.0".into()),
        install_location: Some("/opt/app".into()),
        executable_path: None,
        purpose: Some("testing".into()),
        notes: None,
        architecture: Some("arm64".into()),
        installed_at: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
    }
}

#[test]
fn software_crud_round_trips_all_fields() {
    let t = env();
    let mut software = t.software_service();

    let created = software
        .create_software(CreateSoftware {
            external_refs: vec![ExternalRefInput {
                namespace: "homebrew_formula".into(),
                external_id: "ripgrep".into(),
                source_url: None,
            }],
            ..software_cmd("ripgrep", SoftwareCategory::Cli)
        })
        .unwrap();
    let id = created.entry.asset.id;
    assert_eq!(created.entry.asset.kind.as_str(), "software.cli");
    assert_eq!(created.entry.record.install_source, InstallSource::Unknown);
    assert!(created.external_refs[0].id.as_uuid() != uuid::Uuid::nil());

    // Reopen: persisted state matches.
    let mut software = t.software_service();
    let fetched = software.get_software(id).unwrap();
    assert_eq!(fetched.entry.record, created.entry.record);

    // Update.
    let updated = software
        .update_metadata(UpdateSoftwareMetadata {
            asset_id: id,
            version: Some("2.0".into()),
            purpose: Some("new purpose".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(updated.entry.record.version.as_deref(), Some("2.0"));
    assert_eq!(updated.entry.record.architecture.as_deref(), Some("arm64"));

    // Filter by category via a fresh list.
    let rows = software
        .list_software(&SoftwareFilter {
            category: Some(SoftwareCategory::Cli),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn software_ref_uniqueness_enforced_in_sqlite() {
    let t = env();
    let mut software = t.software_service();
    software
        .create_software(CreateSoftware {
            external_refs: vec![ExternalRefInput {
                namespace: "bundle_id".into(),
                external_id: "com.example.X".into(),
                source_url: None,
            }],
            ..software_cmd("X", SoftwareCategory::Application)
        })
        .unwrap();
    let err = software
        .create_software(CreateSoftware {
            external_refs: vec![ExternalRefInput {
                namespace: "bundle_id".into(),
                external_id: "com.example.X".into(),
                source_url: None,
            }],
            ..software_cmd("X twin", SoftwareCategory::Application)
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
}

#[test]
fn software_search_and_rebuild_work_in_sqlite() {
    let t = env();
    let mut software = t.software_service();
    let created = software
        .create_software(CreateSoftware {
            purpose: Some("local LLM testing".into()),
            external_refs: vec![ExternalRefInput {
                namespace: "homebrew_cask".into(),
                external_id: "ollama".into(),
                source_url: None,
            }],
            ..software_cmd("Ollama", SoftwareCategory::Runtime)
        })
        .unwrap();
    let id = created.entry.asset.id;

    let mut search = t.search_service();
    let hits = search.search("ollama", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "software.runtime");
    assert_eq!(hits[0].asset_id, id);

    // Wipe the projection and rebuild: canonical data restores search.
    search.rebuild().unwrap();
    let hits = search.search("homebrew_cask:ollama", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

/// The checksum of migration 0001 as recorded in the migration ledgers of
/// every database written by Phase 1. Frozen like the checked-in portable
/// bundle fixture: the equality assertion below fires if migration 0001 is
/// ever edited, because existing installations carry this recorded checksum
/// and their upgrade path would fail loudly. Changing 0001 therefore
/// requires an explicit compatibility decision, never a silent edit.
const PHASE1_MIGRATION_0001_CHECKSUM: &str =
    "d84bc66dca1be6ac42bcb2bd6b0df0888104a126b53cad6b827af7a48beed9b7";

fn migration_checksum(sql: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(sql.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn migration_0001_is_unchanged_from_the_phase1_history() {
    let sql1 = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations/0001_core_media_v1.sql"
    ));
    assert_eq!(
        migration_checksum(sql1),
        PHASE1_MIGRATION_0001_CHECKSUM,
        "migration 0001 was modified after being applied by Phase 1 \
         installations; historical databases record the frozen checksum and \
         will refuse to open — this needs an explicit compatibility decision"
    );
}

#[test]
fn migration_from_phase1_database_preserves_media_and_adds_software() {
    // Upgrades against a REAL historical fixture: this database was produced
    // by the Phase 1 binary (commit cdb0710) through the CLI, so it carries
    // the actual layout, pragmas, migration ledger, and FTS5 shadow tables of
    // that release — not an approximation rebuilt from today's 0001 SQL.
    // Rebuilding from the current SQL could not catch a divergence between
    // the historical layout and what the upgrade path assumes (docs/05:
    // migration code must be testable on real legacy fixtures).
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/phase1_media_only.db");
    assert!(
        fixture.exists(),
        "missing Phase 1 fixture: {}",
        fixture.display()
    );

    let dir = std::env::temp_dir().join(format!("assetmesh-phase1-upgrade-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("phase1-upgraded.db");
    std::fs::copy(&fixture, &db_path).unwrap();

    // The historical ledger must record the frozen Phase 1 checksum, proving
    // the fixture is genuinely from that release.
    let stored_checksum: String = rusqlite::Connection::open(&db_path)
        .unwrap()
        .query_row(
            "SELECT checksum FROM assetmesh_migrations WHERE version = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_checksum, PHASE1_MIGRATION_0001_CHECKSUM);

    // Phase 1 shape: media-only, no software/relations tables yet.
    let phase1_tables: Vec<String> = rusqlite::Connection::open(&db_path)
        .unwrap()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|name| !name.starts_with("search_documents"))
        .collect();
    assert!(
        !phase1_tables
            .iter()
            .any(|t| t == "software_records" || t == "relations"),
        "the Phase 1 fixture must predate the software/relations tables: {phase1_tables:?}"
    );
    let media_before: i64 = rusqlite::Connection::open(&db_path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM media_records", [], |row| row.get(0))
        .unwrap();

    // Opening with the current binary migrates to the latest version.
    let factory = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open(db_path.to_str().unwrap()).unwrap(),
    ));
    let version: i64 = factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row("SELECT MAX(version) FROM assetmesh_migrations", [], |row| {
                row.get(0)
            })
        })
        .unwrap()
        .unwrap();
    assert_eq!(version, assetmesh_storage_sqlite::latest_db_version());

    // Every Phase 1 media row survived the upgrade untouched.
    let media_after: i64 = factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row("SELECT COUNT(*) FROM media_records", [], |row| row.get(0))
        })
        .unwrap()
        .unwrap();
    assert_eq!(media_after, media_before);

    // Historical data is still readable through the current application layer
    // — the FTS5 index and its projection survived too.
    let mut media = MediaService::new(factory.clone(), clock_shared(), ids_shared());
    let rows = media.list_media(&MediaFilter::default()).unwrap();
    assert_eq!(rows.len(), media_before as usize);
    let frieren = rows
        .iter()
        .find(|r| r.entry.asset.name.starts_with("Frieren"))
        .expect("the Phase 1 fixture's anime record must survive the upgrade");
    assert_eq!(frieren.entry.asset.kind, MediaType::Anime.asset_kind());
    let view = media.get_media(frieren.entry.asset.id).unwrap();
    assert!(view.external_refs.iter().any(|r| r.namespace == "tmdb"));

    // The software module is registered and functional on the upgraded DB.
    let mut software = SoftwareService::new(factory.clone(), clock_shared(), ids_shared());
    let created = software
        .create_software(software_cmd("PostMigration", SoftwareCategory::Tool))
        .unwrap();
    assert_eq!(created.entry.asset.name, "PostMigration");

    // Reopen is idempotent.
    drop(software);
    drop(media);
    let reopened = assetmesh_storage_sqlite::open(db_path.to_str().unwrap()).unwrap();
    let mut check = SoftwareService::new(
        SharedSqlite(Arc::new(reopened)),
        clock_shared(),
        ids_shared(),
    );
    let rows = check.list_software(&SoftwareFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.name, "PostMigration");

    std::fs::remove_dir_all(&dir).ok();
}

fn clock_shared() -> SharedClock {
    Arc::new(assetmesh_core::ports::clock::SystemClock)
}

fn ids_shared() -> SharedIdGenerator {
    Arc::new(assetmesh_core::ports::ids::UuidV7Generator)
}

#[test]
fn relations_enforce_unique_and_self_constraints() {
    let t = env();
    let mut software = t.software_service();
    let a = software
        .create_software(software_cmd("AppA", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;
    let b = software
        .create_software(software_cmd("AppB", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        t.factory.clone(),
        t.clock.clone(),
        t.ids.clone(),
    );
    relations
        .attach(
            a,
            assetmesh_core::domain::relation::RelationType::DependsOn,
            b,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap();

    // Same relation again → conflict.
    let err = relations
        .attach(
            a,
            assetmesh_core::domain::relation::RelationType::DependsOn,
            b,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");

    // Self relation → validation failure.
    let err = relations
        .attach(
            a,
            assetmesh_core::domain::relation::RelationType::Uses,
            a,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err}");

    // Views resolve inverse semantics from each side.
    let from_a = relations.list_for_asset(a).unwrap();
    assert_eq!(from_a.len(), 1);
    assert_eq!(
        from_a[0].relation_type,
        assetmesh_core::domain::relation::RelationType::DependsOn
    );
    let from_b = relations.list_for_asset(b).unwrap();
    assert_eq!(
        from_b[0].relation_type,
        assetmesh_core::domain::relation::RelationType::DependencyOf
    );
}

#[test]
fn sqlite_portable_round_trip_includes_software() {
    let t = env();
    let mut software = t.software_service();
    software
        .create_software(CreateSoftware {
            external_refs: vec![ExternalRefInput {
                namespace: "homebrew_cask".into(),
                external_id: "iterm2".into(),
                source_url: None,
            }],
            ..software_cmd("iTerm2", SoftwareCategory::Application)
        })
        .unwrap();

    let mut export = t.export_service();
    let bundle = export.export("test").unwrap();

    let other = env();
    let mut import = PortableImportService::new(other.factory.clone());
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.software_created, 1);

    let mut software2 = SoftwareService::new(
        other.factory.clone(),
        other.clock.clone(),
        other.ids.clone(),
    );
    let rows = software2.list_software(&SoftwareFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.name, "iTerm2");
    assert_eq!(
        rows[0].entry.record.install_location.as_deref(),
        Some("/opt/app")
    );
}

#[test]
fn relations_reject_inverse_type_storage_at_the_sql_level() {
    let t = env();
    let mut software = t.software_service();
    let a = software
        .create_software(software_cmd("AppA", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;
    let b = software
        .create_software(software_cmd("AppB", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        t.factory.clone(),
        t.clock.clone(),
        t.ids.clone(),
    );
    relations
        .attach(
            a,
            assetmesh_core::domain::relation::RelationType::DependsOn,
            b,
            None,
            assetmesh_core::domain::relation::RelationProvenance::Manual,
        )
        .unwrap();

    // The migration restricts stored types to the canonical set, so an
    // inverse-typed row is unrepresentable in SQLite itself — even for a
    // direct SQL write that bypassed the application layer.
    let err = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.execute(
                "INSERT INTO relations (id, source_asset_id, target_asset_id, relation_type, \
                 note, provenance, created_at) VALUES ('11111111-1111-7111-8111-111111111111', \
                 ?1, ?2, 'dependency_of', NULL, 'manual', '2026-01-01T00:00:00Z')",
                rusqlite::params![a.to_string(), b.to_string()],
            )
        })
        .unwrap()
        .unwrap_err();
    assert!(
        err.to_string().contains("CHECK constraint"),
        "CHECK(relation_type) must reject inverse storage: {err}"
    );

    // Exactly one row remains: the canonical one.
    let count: i64 = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row("SELECT COUNT(*) FROM relations", [], |row| row.get(0))
        })
        .unwrap()
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn sqlite_software_upsert_rejects_category_kind_mismatch() {
    let t = env();
    let mut media = t.media_service();
    let media_asset = media
        .create_media(CreateMedia {
            title: "A Movie".into(),
            media_type: MediaType::Movie,
            ..create_cmd("placeholder", MediaType::Movie)
        })
        .unwrap()
        .entry
        .asset
        .id;

    // Direct UnitOfWork write: a CLI software record cannot attach to a
    // media asset — the repository boundary enforces category ↔ kind.
    let mut wrong =
        assetmesh_core::domain::software::SoftwareRecord::new(media_asset, SoftwareCategory::Cli);
    wrong.version = Some("1.0".into());
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.software().upsert(&wrong))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("kind"), "{err}");
}

#[test]
fn sqlite_rejects_cross_module_kind_change_in_both_directions() {
    // The stranded-record guard must hold in both directions: a software
    // record cannot survive under a media.* kind, and a media record cannot
    // survive under a software.* kind.
    let t = env();

    // media → software: the media record would strand.
    let mut media = t.media_service();
    let media_asset = media
        .create_media(create_cmd("A Movie", MediaType::Movie))
        .unwrap()
        .entry
        .asset
        .id;
    let now = t.clock.now();
    let mut as_cli = Asset::new(media_asset, AssetKind::SoftwareCli, "A Movie", None, now).unwrap();
    as_cli.touch(now);
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.assets().update(&as_cli))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("media record"), "{err}");
    // The original media asset is untouched.
    let kept = t
        .factory
        .clone()
        .transact(&mut |uow| uow.assets().get(media_asset))
        .unwrap()
        .unwrap();
    assert_eq!(kept.kind, AssetKind::MediaMovie);

    // software → media: the software record would strand (this direction is
    // the one the portable-import review finding exercised).
    let mut software = t.software_service();
    let sw_asset = software
        .create_software(software_cmd("A Tool", SoftwareCategory::Cli))
        .unwrap()
        .entry
        .asset
        .id;
    let mut as_movie = Asset::new(sw_asset, AssetKind::MediaMovie, "A Tool", None, now).unwrap();
    as_movie.touch(now);
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.assets().update(&as_movie))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("software record"), "{err}");

    // Removing the record first unlocks the change.
    t.factory
        .clone()
        .transact(&mut |uow| uow.software().delete(sw_asset))
        .unwrap();
    t.factory
        .clone()
        .transact(&mut |uow| uow.assets().update(&as_movie))
        .unwrap();
}

#[test]
fn relations_reject_reversed_related_to_at_the_sql_level() {
    let t = env();
    let mut software = t.software_service();
    let a = software
        .create_software(software_cmd("AppA", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;
    let b = software
        .create_software(software_cmd("AppB", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;
    let (smaller, larger) = if a.to_string() < b.to_string() {
        (a, b)
    } else {
        (b, a)
    };
    let (source, target) = (larger, smaller);

    // Direct SQL in the non-canonical direction: the CHECK constraint on
    // symmetric endpoint order must reject it, so both halves of one
    // related_to fact cannot coexist even by bypassing the application layer.
    let err = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.execute(
                "INSERT INTO relations (id, source_asset_id, target_asset_id, relation_type, \
                 note, provenance, created_at) VALUES ('22222222-2222-7222-8222-222222222222', \
                 ?1, ?2, 'related_to', NULL, 'manual', '2026-01-01T00:00:00Z')",
                rusqlite::params![source.to_string(), target.to_string()],
            )
        })
        .unwrap()
        .unwrap_err();
    assert!(
        err.to_string().contains("CHECK constraint"),
        "CHECK(related_to endpoint order) must reject the reversed row: {err}"
    );

    let count: i64 = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row("SELECT COUNT(*) FROM relations", [], |row| row.get(0))
        })
        .unwrap()
        .unwrap();
    assert_eq!(count, 0, "no row may survive the rejected insert");

    // And the repository seam rejects the same shape before it reaches SQL.
    let reversed = Relation::new(
        RelationId::generate(),
        source,
        target,
        RelationType::RelatedTo,
        RelationProvenance::Manual,
        t.clock.now(),
    )
    .unwrap();
    assert!(reversed.is_canonical() == (source.to_string() < target.to_string()));
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.relations().insert(&reversed))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err}");
}

#[test]
fn relations_reject_inverse_type_at_the_repository_seam() {
    let t = env();
    let mut software = t.software_service();
    let a = software
        .create_software(software_cmd("AppA", SoftwareCategory::Application))
        .unwrap()
        .entry
        .asset
        .id;
    let b = software
        .create_software(software_cmd("AppB", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;

    // A UnitOfWork write stating a fact via the inverse type must be rejected
    // by the repository rather than stored as a duplicate representation.
    let inverse = Relation::new(
        RelationId::generate(),
        b,
        a,
        RelationType::DependencyOf,
        RelationProvenance::Manual,
        t.clock.now(),
    )
    .unwrap();
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.relations().insert(&inverse))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err}");
    assert!(err.to_string().contains("depends_on"), "{err}");

    let count: i64 = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row("SELECT COUNT(*) FROM relations", [], |row| row.get(0))
        })
        .unwrap()
        .unwrap();
    assert_eq!(count, 0);
}
