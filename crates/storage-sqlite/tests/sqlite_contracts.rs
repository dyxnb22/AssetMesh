//! SQLite contract tests: connection configuration, migrations,
//! repository behavior, transaction atomicity, search projection, and the
//! portable round trip against the real storage adapter.

use assetmesh_core::application::media_service::{
    CreateMedia, ExternalRefInput, MediaService, UpdateMediaMetadata,
};
use assetmesh_core::application::portable::{
    read_bundle_from_directory, PortableExportService, PortableImportService, EXPORT_VERSION,
};
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::{
    CreateService, Patch, ServiceService, UpdateService,
};
use assetmesh_core::application::{SharedClock, SharedIdGenerator};
use assetmesh_core::domain::asset::{Asset, AssetKind};
use assetmesh_core::domain::ids::{AssetId, RelationId};
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::domain::relation::{
    Relation, RelationProvenance, RelationType, ALL_TYPES, STORABLE_TYPES,
};
use assetmesh_core::domain::service::{BillingCadence, ServiceRecord, ServiceType};
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::clock::Clock;
use assetmesh_core::ports::repos::{MediaFilter, ServiceFilter};
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
    fn portable_import_service(&self) -> PortableImportService<SharedSqlite> {
        PortableImportService::new(self.factory.clone())
    }
    fn service_service(
        &self,
    ) -> assetmesh_core::application::service_service::ServiceService<SharedSqlite> {
        assetmesh_core::application::service_service::ServiceService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
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
    let err = match assetmesh_storage_sqlite::open(&path) {
        Ok(_) => panic!("tampered migration must fail loudly"),
        Err(err) => err,
    };
    // Carried by the variant, not by message text: adapters classify on this.
    assert!(
        matches!(err, assetmesh_core::AppError::CorruptData { .. }),
        "a diverged migration is corruption, got [{}] {}",
        err.category(),
        err.message()
    );

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
use assetmesh_core::domain::software::InstallSource;
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

fn ts(value: &str) -> assetmesh_core::domain::Timestamp {
    chrono::DateTime::parse_from_rfc3339(value)
        .expect("test timestamps are well-formed")
        .with_timezone(&chrono::Utc)
}

fn service_cmd(name: &str, service_type: ServiceType) -> CreateService {
    CreateService {
        name: name.into(),
        service_type,
        summary: None,
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost_minor: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
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

/// The checksum of migration 0002 as recorded by the Phase 2 binary, frozen
/// for the same reason as [`PHASE1_MIGRATION_0001_CHECKSUM`]: databases
/// written by Phase 2 carry this recorded checksum, so editing 0002 would
/// break their upgrade path.
const PHASE2_MIGRATION_0002_CHECKSUM: &str =
    "dc44ebf1dec095cdd318b9db8b0bae26749fa51790423a9482dfd3d77484fef0";

#[test]
fn migration_0002_is_unchanged_from_the_phase2_history() {
    let sql2 = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations/0002_software_relations_v1.sql"
    ));
    assert_eq!(
        migration_checksum(sql2),
        PHASE2_MIGRATION_0002_CHECKSUM,
        "migration 0002 was modified after being applied by Phase 2 \
         installations; historical databases record the frozen checksum and \
         will refuse to open — this needs an explicit compatibility decision"
    );
}

/// The flagship Phase 3 migration test (docs/10): a REAL database produced by
/// the Phase 2 binary — commit 4ff3119, built in a worktree and driven through
/// the CLI — is opened by the current binary and must migrate to migration
/// 0003 without touching a single historical row.
///
/// Like the Phase 1 fixture this carries the real layout, pragmas, migration
/// ledger, and FTS5 shadow tables of that release. It additionally carries
/// Phase 2 state the Phase 1 fixture cannot: software records, relations with
/// inverse semantics, tags, and one archived asset — so the upgrade proves
/// Modules 2 and 3 coexist on the shared Asset identity.
#[test]
fn migration_0003_upgrades_a_real_phase2_database_preserving_everything() {
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase2_software.db");
    assert!(
        fixture.exists(),
        "missing Phase 2 fixture: {}",
        fixture.display()
    );

    let dir = std::env::temp_dir().join(format!("assetmesh-phase2-upgrade-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("phase2-upgraded.db");
    std::fs::copy(&fixture, &db_path).unwrap();

    // The historical ledger must carry both frozen checksums, proving the
    // fixture is genuinely from the Phase 2 release and was not rebuilt from
    // today's SQL.
    let ledger: Vec<(i64, String)> = rusqlite::Connection::open(&db_path)
        .unwrap()
        .prepare("SELECT version, checksum FROM assetmesh_migrations ORDER BY version")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        ledger,
        vec![
            (1, PHASE1_MIGRATION_0001_CHECKSUM.to_string()),
            (2, PHASE2_MIGRATION_0002_CHECKSUM.to_string())
        ],
        "the Phase 2 fixture must carry the frozen Phase 1 and 2 checksums"
    );

    // Phase 2 shape: no services module and no service_records table yet.
    let raw = rusqlite::Connection::open(&db_path).unwrap();
    let has_service_table: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'service_records'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        has_service_table, 0,
        "the fixture must predate service_records"
    );
    let modules: Vec<String> = raw
        .prepare("SELECT module_id FROM module_metadata ORDER BY module_id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(modules, vec!["media", "software"]);
    drop(raw);

    // Snapshot the historical state that must survive untouched.
    let media_before = table_count(&db_path, "media_records");
    let software_before = table_count(&db_path, "software_records");
    let relations_before = table_count(&db_path, "relations");
    let activity_before = table_count(&db_path, "activity_events");
    let refs_before = table_count(&db_path, "external_refs");
    let tags_before = table_count(&db_path, "tags");
    assert_eq!(media_before, 3);
    assert_eq!(software_before, 4);
    assert_eq!(relations_before, 4);
    assert!(activity_before > 0);
    assert!(refs_before > 0);
    assert!(tags_before > 0);

    // Opening with the current binary applies migrations 0003 and 0004.
    let mut factory = SharedSqlite(Arc::new(
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
    assert!(version >= 3, "migration 0003 must have applied");

    // No historical row was lost or duplicated.
    assert_eq!(table_count(&db_path, "media_records"), media_before);
    assert_eq!(table_count(&db_path, "software_records"), software_before);
    assert_eq!(table_count(&db_path, "relations"), relations_before);
    assert_eq!(table_count(&db_path, "activity_events"), activity_before);
    assert_eq!(table_count(&db_path, "external_refs"), refs_before);
    assert_eq!(table_count(&db_path, "tags"), tags_before);

    // Migration 0004 rebuilds the relations table, so the copied Phase 2 rows
    // must still read back with their original types and endpoints — a rebuild
    // that lost or mangled them would be silent otherwise.
    let mut relations = RelationService::new(factory.clone(), clock_shared(), ids_shared());
    let all: Vec<Relation> = factory.read(&mut |q| q.relations().list_all()).unwrap();
    assert_eq!(all.len(), relations_before as usize);
    for stored in &all {
        assert!(
            STORABLE_TYPES.contains(&stored.relation_type),
            "a migrated row carries a type the registry no longer stores: {}",
            stored.relation_type
        );
        assert!(
            stored.is_canonical(),
            "a migrated row is not in canonical form: {stored:?}"
        );
        let views = relations.list_for_asset(stored.source_asset_id).unwrap();
        assert!(
            views
                .iter()
                .any(|v| v.relation_id == stored.id && v.other_asset_id == stored.target_asset_id),
            "migrated relation {} is unreadable from its source",
            stored.id
        );
    }

    // And the widened CHECK now accepts the Phase 3 service types. The
    // relation itself is not written here: this test's later assertions count
    // the fixture's relations, and a real write is covered by
    // `sql_relations_check_accepts_the_new_types_and_rejects_their_inverses`.
    let create_sql: String = factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'relations'",
                [],
                |row| row.get::<_, String>(0),
            )
        })
        .unwrap()
        .unwrap();
    for service_type in ["hosted_on", "points_to"] {
        assert!(
            create_sql.contains(&format!("'{service_type}'")),
            "the rebuilt relations table must accept {service_type}:\n{create_sql}"
        );
    }

    // The services module is registered with its V1 schema version.
    let services_version: i64 = factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row(
                "SELECT schema_version FROM module_metadata WHERE module_id = 'services'",
                [],
                |row| row.get(0),
            )
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        services_version,
        assetmesh_core::domain::service::SCHEMA_VERSION
    );

    // Historical data is still readable through the current application layer.
    let mut software = SoftwareService::new(factory.clone(), clock_shared(), ids_shared());
    let rows = software.list_software(&SoftwareFilter::default()).unwrap();
    assert_eq!(rows.len(), software_before as usize);
    let ollama = rows
        .iter()
        .find(|r| r.entry.asset.name == "Ollama")
        .expect("the Phase 2 fixture's Ollama record must survive the upgrade");
    assert_eq!(
        ollama.entry.asset.kind,
        SoftwareCategory::Runtime.asset_kind()
    );
    // The fixture's archive must survive too: the shared lifecycle is a
    // cross-module contract, not something migration 0003 resets.
    let vscode = rows
        .iter()
        .find(|r| r.entry.asset.name == "Visual Studio Code")
        .expect("the archived fixture asset must survive");
    assert!(vscode.entry.asset.archived_at.is_some());

    // Relations survived with their inverse semantics intact: the four
    // relations stored by the Phase 2 binary still resolve from either side.
    // Three of them touch Homebrew (the `installed_via` inverses); the fourth
    // is `VSCode uses Ollama`, which only the two endpoints can see.
    let mut relations = RelationService::new(factory.clone(), clock_shared(), ids_shared());
    let brew = rows
        .iter()
        .find(|r| r.entry.asset.name == "Homebrew")
        .expect("the fixture's Homebrew record must survive");
    let brew_relations = relations.list_for_asset(brew.entry.asset.id).unwrap();
    assert_eq!(
        brew_relations.len(),
        3,
        "the three `installed_via` relations must resolve as inverses from Homebrew"
    );
    let ollama_relations = relations.list_for_asset(ollama.entry.asset.id).unwrap();
    assert_eq!(
        ollama_relations.len(),
        2,
        "Ollama must resolve both its own `installed_via` and the `uses` inverse"
    );
    assert!(
        ollama_relations.iter().any(|r| {
            r.relation_type == RelationType::InstalledVia && r.other_asset_id == brew.entry.asset.id
        }),
        "the stored forward relation must survive"
    );
    assert!(
        ollama_relations
            .iter()
            .any(|r| r.relation_type == RelationType::UsedBy
                && r.other_asset_id == vscode.entry.asset.id),
        "the `uses` relation must resolve from its target side as `used_by`"
    );

    let mut media = MediaService::new(factory.clone(), clock_shared(), ids_shared());
    let media_rows = media.list_media(&MediaFilter::default()).unwrap();
    assert_eq!(media_rows.len(), media_before as usize);
    drop(media);
    drop(relations);
    drop(software);

    // Services work on the upgraded database: the new module shares the same
    // Asset table, search projection, and activity stream as Media/Software.
    let mut services = ServiceService::new(factory.clone(), clock_shared(), ids_shared());
    let created = services
        .create_service(service_cmd("Linear", ServiceType::Saas))
        .unwrap();
    assert_eq!(created.entry.asset.name, "Linear");
    assert_eq!(created.entry.record.service_type, ServiceType::Saas);
    assert_eq!(created.entry.asset.kind, AssetKind::ServiceSaas);
    drop(services);

    // Reopen is idempotent and the new service is durable.
    let reopened = assetmesh_storage_sqlite::open(db_path.to_str().unwrap()).unwrap();
    let mut check = assetmesh_core::application::service_service::ServiceService::new(
        SharedSqlite(Arc::new(reopened)),
        clock_shared(),
        ids_shared(),
    );
    let rows = check
        .list_services(&assetmesh_core::ports::repos::ServiceFilter::default())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.name, "Linear");

    // The historical search projection still resolves after the upgrade, and
    // the new service is searchable alongside the upgraded software.
    let mut search = SearchService::new(factory, clock_shared());
    search.rebuild().unwrap();
    let hits = search.search("ollama", 10).unwrap();
    assert_eq!(hits.len(), 1, "Phase 2 software must still be searchable");
    assert_eq!(hits[0].kind, "software.runtime");
    let hits = search.search("linear", 10).unwrap();
    assert_eq!(hits.len(), 1, "the new service must be searchable");
    assert_eq!(hits[0].kind, "service.saas");

    std::fs::remove_dir_all(&dir).ok();
}

fn table_count(db_path: &std::path::Path, table: &str) -> i64 {
    rusqlite::Connection::open(db_path)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
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
fn sqlite_list_for_assets_returns_every_touching_relation_once() {
    use assetmesh_core::domain::ids::RelationId;
    use assetmesh_core::domain::relation::{RelationProvenance, RelationType};

    let t = env();
    let a = assets_media(&t, "A");
    let b = assets_media(&t, "B");
    let c = assets_media(&t, "C");
    let unrelated = assets_media(&t, "Unrelated");

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        t.factory.clone(),
        t.clock.clone(),
        t.ids.clone(),
    );
    relations
        .attach(
            a,
            RelationType::DependsOn,
            b,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
    relations
        .attach(c, RelationType::Uses, b, None, RelationProvenance::Manual)
        .unwrap();
    // Touches neither a nor b.
    relations
        .attach(
            unrelated,
            RelationType::RelatedTo,
            c,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();

    let mut factory = t.factory.clone();
    let both = factory
        .read(&mut |q| q.relations().list_for_assets(&[a, b]))
        .unwrap();
    assert_eq!(both.len(), 2, "a→b and c→b both touch the requested set");

    let one = factory
        .read(&mut |q| q.relations().list_for_assets(&[b]))
        .unwrap();
    assert_eq!(one.len(), 2);

    let none = factory
        .read(&mut |q| q.relations().list_for_assets(&[unrelated]))
        .unwrap();
    // Only the unrelated→c row touches `unrelated`.
    assert_eq!(none.len(), 1);

    let empty = factory
        .read(&mut |q| q.relations().list_for_assets(&[]))
        .unwrap();
    assert!(empty.is_empty(), "an empty slice is not an error");

    // The batch result agrees with the per-asset reader, and the symmetric
    // mirror row is not double-counted.
    let per_asset = {
        let mut seen: Vec<RelationId> = relations
            .list_for_asset(b)
            .unwrap()
            .into_iter()
            .map(|r| r.relation_id)
            .collect();
        seen.extend(
            relations
                .list_for_asset(a)
                .unwrap()
                .into_iter()
                .map(|r| r.relation_id),
        );
        seen.sort();
        seen.dedup();
        seen
    };
    let mut batched: Vec<RelationId> = both.into_iter().map(|r| r.id).collect();
    batched.sort();
    assert_eq!(batched, per_asset);

    // Deterministic ordering across repeated calls.
    let again = factory
        .read(&mut |q| q.relations().list_for_assets(&[a, b]))
        .unwrap();
    let ids: Vec<RelationId> = again.iter().map(|r| r.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

/// Creates one media asset through the real service and returns its id.
fn assets_media(t: &TestSqlite, title: &str) -> assetmesh_core::domain::ids::AssetId {
    t.media_service()
        .create_media(create_cmd(title, MediaType::Anime))
        .unwrap()
        .entry
        .asset
        .id
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

#[test]
fn service_crud_round_trips_every_field() {
    let t = env();
    let mut services = t.service_service();

    let view = services
        .create_service(CreateService {
            summary: Some("Team chat".into()),
            provider: Some("Linear".into()),
            account_label: Some("Work".into()),
            endpoint_url: Some("https://api.linear.app".into()),
            dashboard_url: Some("https://linear.app".into()),
            plan: Some("Business".into()),
            cost_minor: Some(3200),
            currency: Some("USD".into()),
            billing_cadence: Some(BillingCadence::Monthly),
            renews_at: Some(ts("2026-11-01T00:00:00Z")),
            expires_at: Some(ts("2027-11-01T00:00:00Z")),
            auto_renew: Some(true),
            notes: Some("Billed monthly".into()),
            tags: vec!["pm".into(), "work".into()],
            external_refs: vec![ExternalRefInput {
                namespace: "linear".into(),
                external_id: "org-42".into(),
                source_url: None,
            }],
            ..service_cmd("Linear", ServiceType::Saas)
        })
        .unwrap();

    let id = view.entry.asset.id;
    assert_eq!(view.entry.asset.kind, AssetKind::ServiceSaas);
    assert_eq!(view.entry.record.cost_minor, Some(3200));
    assert_eq!(view.entry.record.currency.as_deref(), Some("USD"));
    assert_eq!(view.tags, vec!["pm", "work"]);

    // The whole record survives a fresh read from SQLite.
    let reloaded = services.get_service(id).unwrap();
    assert_eq!(reloaded.entry.record, view.entry.record);
    assert_eq!(reloaded.entry.asset.summary.as_deref(), Some("Team chat"));
    assert_eq!(
        reloaded
            .external_refs
            .iter()
            .map(|r| (r.namespace.as_str(), r.external_id.as_str()))
            .collect::<Vec<_>>(),
        vec![("linear", "org-42")]
    );

    // An explicit patch writes through and is durable.
    let updated = services
        .update_service(UpdateService {
            asset_id: id,
            plan: Patch::Set("Enterprise".into()),
            cost_minor: Patch::Set(9900),
            currency: Patch::Set("usd".into()),
            ..UpdateService::default()
        })
        .unwrap();
    assert_eq!(updated.entry.record.plan.as_deref(), Some("Enterprise"));
    assert_eq!(updated.entry.record.cost_minor, Some(9900));
    // Currency normalizes even on the SQL write path.
    assert_eq!(updated.entry.record.currency.as_deref(), Some("USD"));

    drop(services);
    let mut reopened = t.service_service();
    let rows = reopened.list_services(&ServiceFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.record.plan.as_deref(), Some("Enterprise"));
    assert_eq!(rows[0].entry.record.currency.as_deref(), Some("USD"));
}

#[test]
fn sqlite_service_upsert_rejects_type_kind_mismatch() {
    let t = env();

    // A software asset shares the Asset table but cannot own a service record.
    let software_id = t
        .software_service()
        .create_software(software_cmd("Ollama", SoftwareCategory::Runtime))
        .unwrap()
        .entry
        .asset
        .id;

    let mismatched = ServiceRecord::new(software_id, ServiceType::Saas);
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&mismatched))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
    assert!(err.to_string().contains("requires asset kind"), "{err:?}");

    // Nothing was written.
    let stored = t
        .factory
        .clone()
        .read(&mut |q| q.services().get(software_id))
        .unwrap();
    assert!(stored.is_none());

    // The guard is about the pair, not the direction: an api record on a saas
    // asset fails the same way.
    let service_id = t
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;
    let mismatched = ServiceRecord::new(service_id, ServiceType::Api);
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&mismatched))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");

    // The original record is untouched by the failed overwrite attempt.
    let stored = t
        .factory
        .clone()
        .read(&mut |q| q.services().get(service_id))
        .unwrap()
        .unwrap();
    assert_eq!(stored.service_type, ServiceType::Saas);
}

#[test]
fn sqlite_service_upsert_normalizes_and_rejects_at_the_boundary() {
    let t = env();
    let id = t
        .service_service()
        .create_service(service_cmd("Hetzner", ServiceType::Vps))
        .unwrap()
        .entry
        .asset
        .id;

    // Unnormalized text is canonicalized by the repository itself, so even a
    // direct UnitOfWork write cannot store raw input.
    let mut raw = ServiceRecord::new(id, ServiceType::Vps);
    raw.provider = Some("  Hetzner Online  ".into());
    t.factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&raw))
        .unwrap();
    let stored = t
        .factory
        .clone()
        .read(&mut |q| q.services().get(id))
        .unwrap()
        .unwrap();
    assert_eq!(stored.provider.as_deref(), Some("Hetzner Online"));

    // Money invariants are enforced at the boundary before any SQL runs.
    let mut bad = ServiceRecord::new(id, ServiceType::Vps);
    bad.cost_minor = Some(-5);
    bad.currency = Some("USD".into());
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&bad))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    let mut bad = ServiceRecord::new(id, ServiceType::Vps);
    bad.cost_minor = Some(100);
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&bad))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
}

#[test]
fn service_search_projection_and_rebuild_work_in_sqlite() {
    let t = env();
    let mut services = t.service_service();
    let id = services
        .create_service(CreateService {
            provider: Some("Cloudflare".into()),
            domain_name: Some("assetmesh.dev".into()),
            plan: Some("Pro".into()),
            cost_minor: Some(2000),
            currency: Some("USD".into()),
            billing_cadence: Some(BillingCadence::Monthly),
            notes: Some("DNS and CDN".into()),
            tags: vec!["infra".into()],
            external_refs: vec![ExternalRefInput {
                namespace: "cloudflare".into(),
                external_id: "zone-7".into(),
                source_url: None,
            }],
            ..service_cmd("Cloudflare", ServiceType::Domain)
        })
        .unwrap()
        .entry
        .asset
        .id;

    let mut search = t.search_service();
    // Money appears in its display form, never as a float (ADR 0010).
    let hits = search.search("USD 20.00", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].asset_id, id);
    assert_eq!(hits[0].kind, "service.domain");

    // Tags, aliases, provider, and the canonical domain are all search terms.
    assert_eq!(search.search("infra", 10).unwrap().len(), 1);
    assert_eq!(search.search("cloudflare:zone-7", 10).unwrap().len(), 1);
    assert_eq!(search.search("assetmesh.dev", 10).unwrap().len(), 1);

    // Wipe the projection and rebuild: canonical data restores search.
    t.factory
        .clone()
        .transact(&mut |uow| uow.search_index().replace_all(&[]))
        .unwrap();
    assert!(search.search("Cloudflare", 10).unwrap().is_empty());
    search.rebuild().unwrap();
    assert_eq!(search.search("Cloudflare", 10).unwrap().len(), 1);
}

#[test]
fn sqlite_service_upsert_rejects_credential_url_and_domain_misuse() {
    let t = env();
    let id = t
        .service_service()
        .create_service(service_cmd("Mixpanel", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // A credential-bearing URL is rejected at the storage boundary too — the
    // canonical vocabulary has no field that may hold it (ADR 0010).
    let mut leaky = ServiceRecord::new(id, ServiceType::Saas);
    leaky.endpoint_url = Some("https://user:pass@mixpanel.com".into());
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&leaky))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
    assert!(err.to_string().contains("credentials"), "{err:?}");

    // domain_name on a non-domain service is canonical-metadata misuse.
    let mut misuse = ServiceRecord::new(id, ServiceType::Saas);
    misuse.domain_name = Some("mixpanel.com".into());
    let err = t
        .factory
        .clone()
        .transact(&mut |uow| uow.services().upsert(&misuse))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
}

// ---------------------------------------------------------------------------
// Repository-boundary kind guard (docs/10 "Asset kinds and type compatibility")
// ---------------------------------------------------------------------------

/// Writes an Asset with a changed kind, bypassing the application layer: the
/// guard is a repository-boundary contract the spec names explicitly, so it is
/// exercised there directly.
fn retype_asset(factory: &mut SharedSqlite, id: AssetId, kind: AssetKind) -> AppResult<()> {
    factory.transact(&mut |w| {
        let mut asset = w.assets().get(id)?.expect("asset exists");
        asset.kind = kind;
        w.assets().update(&asset)
    })
}

#[test]
fn asset_repo_rejects_a_kind_change_that_strands_a_service_record() {
    // The typed detail's discriminator (service_type) is only consistent with
    // the kind assigned at creation, and no write path re-types an asset, so
    // the repository boundary refuses ANY kind change while the old module's
    // detail row remains — including a within-module one like
    // service.saas -> service.api, which would leave a `saas` record on an
    // `api` asset. No application layer exposes this, so it is tested at the
    // repository boundary the spec names.
    let mut t = env();
    let created = t
        .service_service()
        .create_service(service_cmd("Linear", ServiceType::Saas))
        .unwrap();
    let id = created.entry.asset.id;

    // Within-module re-type while the record exists: refused.
    let err =
        retype_asset(&mut t.factory, id, AssetKind::parse("service.api").unwrap()).unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
    assert!(err.to_string().contains("record"), "{err}");

    // Cross-module re-type is refused for the same reason.
    let err =
        retype_asset(&mut t.factory, id, AssetKind::parse("media.movie").unwrap()).unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");

    // The stored kind and the record's type are both unchanged.
    let (kind, ty) = t
        .factory
        .read(&mut |q| {
            Ok((
                q.assets().get(id)?.unwrap().kind,
                q.services().get(id)?.unwrap().service_type,
            ))
        })
        .unwrap();
    assert_eq!(kind.as_str(), "service.saas");
    assert_eq!(ty.as_str(), "saas");

    // The documented escape hatch: remove the module record first, then the
    // re-type is allowed and no invariant is left dangling.
    t.factory
        .transact(&mut |w| w.services().delete(id))
        .unwrap();
    retype_asset(&mut t.factory, id, AssetKind::parse("media.movie").unwrap()).unwrap();
    let kind = t
        .factory
        .read(&mut |q| Ok(q.assets().get(id)?.unwrap().kind))
        .unwrap();
    assert_eq!(kind.as_str(), "media.movie");
}

#[test]
fn sqlite_service_round_trips_through_the_portable_bundle() {
    // Phase 3D: the services section is real, so a ServiceRecord written by
    // this binary survives export → import in SQLite (not just in the
    // in-memory doubles).
    let t = env();
    let created = t
        .service_service()
        .create_service(CreateService {
            provider: Some("Hetzner".into()),
            plan: Some("CX22".into()),
            cost_minor: Some(449),
            currency: Some("eur".into()),
            billing_cadence: Some(BillingCadence::Monthly),
            renews_at: Some(ts("2026-11-01T00:00:00Z")),
            auto_renew: Some(true),
            ..service_cmd("Hetzner VPS", ServiceType::Vps)
        })
        .unwrap();
    let id = created.entry.asset.id;

    let bundle = t.export_service().export("test").unwrap();
    assert_eq!(bundle.manifest.modules["services"].schema_version, 1);
    assert_eq!(bundle.manifest.record_counts["services"], 1);

    // Restore into a fresh database through the real SQLite adapter.
    let restored = env();
    let report = restored
        .portable_import_service()
        .import_bundle(&bundle, false)
        .unwrap();
    assert_eq!(report.services_created, 1);

    let restored_record = restored
        .service_service()
        .get_service(id)
        .unwrap()
        .entry
        .record;
    assert_eq!(restored_record.service_type, ServiceType::Vps);
    assert_eq!(restored_record.provider.as_deref(), Some("Hetzner"));
    assert_eq!(restored_record.plan.as_deref(), Some("CX22"));
    // Money round-trips as integer minor units with the normalized currency.
    assert_eq!(restored_record.cost_minor, Some(449));
    assert_eq!(restored_record.currency.as_deref(), Some("EUR"));
    assert_eq!(
        restored_record.billing_cadence,
        Some(BillingCadence::Monthly)
    );
    assert_eq!(restored_record.renews_at, Some(ts("2026-11-01T00:00:00Z")));
    assert_eq!(restored_record.auto_renew, Some(true));
}

// ---------------------------------------------------------------------------
// Service relation types (Phase 3C)
// ---------------------------------------------------------------------------

#[test]
fn service_relation_types_persist_and_resolve_inverses_in_sqlite() {
    let mut t = env();
    let api = t
        .service_service()
        .create_service(service_cmd("AssetMesh API", ServiceType::Api))
        .unwrap()
        .entry
        .asset
        .id;
    let vps = t
        .service_service()
        .create_service(service_cmd("RackNerd VPS", ServiceType::Vps))
        .unwrap()
        .entry
        .asset
        .id;
    let domain = t
        .service_service()
        .create_service(service_cmd("assetmesh.dev", ServiceType::Domain))
        .unwrap()
        .entry
        .asset
        .id;

    let mut relations = RelationService::new(t.factory.clone(), t.clock.clone(), t.ids.clone());
    relations
        .attach(
            api,
            RelationType::HostedOn,
            vps,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
    relations
        .attach(
            domain,
            RelationType::PointsTo,
            api,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();

    // Each endpoint reads the fact from its own side: the source keeps the
    // stored primary type, the target gets the view-time inverse.
    let api_relations = relations.list_for_asset(api).unwrap();
    assert!(api_relations
        .iter()
        .any(|r| r.relation_type == RelationType::HostedOn && r.other_asset_id == vps));
    assert!(api_relations
        .iter()
        .any(|r| r.relation_type == RelationType::PointedToBy && r.other_asset_id == domain));

    let vps_relations = relations.list_for_asset(vps).unwrap();
    assert!(vps_relations
        .iter()
        .any(|r| r.relation_type == RelationType::Hosts && r.other_asset_id == api));

    let domain_relations = relations.list_for_asset(domain).unwrap();
    assert!(domain_relations
        .iter()
        .any(|r| r.relation_type == RelationType::PointsTo && r.other_asset_id == api));

    // Stating the same fact via the inverse type is a conflict, not a second
    // row: one fact still has exactly one canonical row.
    let err = relations
        .attach(
            vps,
            RelationType::Hosts,
            api,
            None,
            RelationProvenance::Manual,
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
    let err = relations
        .attach(
            api,
            RelationType::PointedToBy,
            domain,
            None,
            RelationProvenance::Manual,
        )
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");

    // Exactly two rows are stored, and they survive a reopen of the database.
    let stored = t
        .factory
        .read(&mut |q| Ok(q.relations().list_all()?.len()))
        .unwrap();
    assert_eq!(stored, 2);
}

#[test]
fn service_relation_types_remove_cleanly_from_both_endpoints() {
    // docs/10 requires attach/remove coverage for both new inverse pairs.
    // Removing the single canonical row must make the fact disappear from
    // every endpoint's view, including the view-time inverse.
    let mut t = env();
    let api = t
        .service_service()
        .create_service(service_cmd("AssetMesh API", ServiceType::Api))
        .unwrap()
        .entry
        .asset
        .id;
    let vps = t
        .service_service()
        .create_service(service_cmd("RackNerd VPS", ServiceType::Vps))
        .unwrap()
        .entry
        .asset
        .id;
    let domain = t
        .service_service()
        .create_service(service_cmd("assetmesh.dev", ServiceType::Domain))
        .unwrap()
        .entry
        .asset
        .id;

    let mut relations = RelationService::new(t.factory.clone(), t.clock.clone(), t.ids.clone());
    let hosted_on = relations
        .attach(
            api,
            RelationType::HostedOn,
            vps,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
    let points_to = relations
        .attach(
            domain,
            RelationType::PointsTo,
            api,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();

    // Removing by the canonical row's own id is enough: the inverse views are
    // derived from that row, so they vanish with it.
    relations.remove(hosted_on.id).unwrap();
    relations.remove(points_to.id).unwrap();

    for endpoint in [api, vps, domain] {
        let views = relations.list_for_asset(endpoint).unwrap();
        assert!(
            views.is_empty(),
            "{endpoint} still sees a removed relation: {views:?}"
        );
    }

    // Nothing is left in storage, and the ids are gone rather than reused.
    let stored = t
        .factory
        .read(&mut |q| {
            Ok((
                q.relations().list_all()?.len(),
                q.relations().get(hosted_on.id)?.is_some(),
                q.relations().get(points_to.id)?.is_some(),
            ))
        })
        .unwrap();
    assert_eq!(stored, (0, false, false));

    // Re-stating either fact afterwards is a fresh attach, not a conflict —
    // the canonical row really is gone.
    relations
        .attach(
            api,
            RelationType::HostedOn,
            vps,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
    relations
        .attach(
            domain,
            RelationType::PointsTo,
            api,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();
    let stored = t
        .factory
        .read(&mut |q| Ok(q.relations().list_all()?.len()))
        .unwrap();
    assert_eq!(stored, 2);
}

#[test]
fn sql_relations_check_accepts_the_new_types_and_rejects_their_inverses() {
    // The stored-type CHECK must stay in lockstep with the Rust registry
    // (docs/10): the canonical primaries are storable, the view-time inverses
    // are not — even for a direct write that skips the application layer.
    let mut t = env();
    let a = t
        .service_service()
        .create_service(service_cmd("API", ServiceType::Api))
        .unwrap()
        .entry
        .asset
        .id;
    let b = t
        .service_service()
        .create_service(service_cmd("VPS", ServiceType::Vps))
        .unwrap()
        .entry
        .asset
        .id;

    for stored_type in ["hosted_on", "points_to"] {
        t.factory
            .transact(&mut |uow| {
                uow.relations().insert(
                    &Relation::new(
                        RelationId::generate(),
                        a,
                        b,
                        RelationType::parse(stored_type).unwrap(),
                        RelationProvenance::Imported,
                        chrono::Utc::now(),
                    )
                    .unwrap(),
                )
            })
            .unwrap();
    }
    assert_eq!(
        t.factory
            .read(&mut |q| Ok(q.relations().list_all()?.len()))
            .unwrap(),
        2
    );

    // The inverse types are refused by the repository boundary (they are
    // view-time derivations) before SQL ever sees them.
    for inverse in ["hosts", "pointed_to_by"] {
        let err = t
            .factory
            .transact(&mut |uow| {
                uow.relations().insert(
                    &Relation::new(
                        RelationId::generate(),
                        b,
                        a,
                        RelationType::parse(inverse).unwrap(),
                        RelationProvenance::Imported,
                        chrono::Utc::now(),
                    )
                    .unwrap(),
                )
            })
            .unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
    }

    // The SQL CHECK itself rejects an inverse, so the table cannot drift from
    // the registry even if a future write path forgot to validate. Both asset
    // ids are real rows, so the foreign keys are satisfied and the CHECK is the
    // only constraint that can reject this write.
    let err = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.execute(
                "INSERT INTO relations (id, source_asset_id, target_asset_id, relation_type, \
                 provenance, created_at) VALUES (?1, ?2, ?3, 'hosts', 'manual', ?4)",
                [
                    RelationId::generate().to_string(),
                    b.to_string(),
                    a.to_string(),
                    chrono::Utc::now().to_rfc3339(),
                ],
            )
        })
        .unwrap()
        .unwrap_err();
    assert!(err.to_string().contains("CHECK"), "{err}");
}

#[test]
fn the_stored_relation_type_check_matches_the_rust_registry() {
    // The SQL CHECK is a hand-maintained mirror of the Rust registry, and a
    // mirror nobody reads is a mirror that drifts. Reading the constraint
    // back out of the schema and comparing it against
    // `STORABLE_TYPES` makes drift a test failure instead of a
    // runtime rejection on some future write path (docs/10 "Relations").
    let mut t = env();
    let create_sql: String = t
        .factory
        .0
        .with_raw_connection(|conn| {
            conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'relations'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
        })
        .unwrap();

    // Pull the quoted type list out of `relation_type TEXT ... IN ('a', 'b')`.
    let list = create_sql
        .split("relation_type TEXT")
        .nth(1)
        .and_then(|rest| rest.split("IN (").nth(1))
        .and_then(|rest| rest.split(')').next())
        .unwrap_or_else(|| panic!("no relation_type CHECK found in:\n{create_sql}"));
    let sql_types: Vec<String> = list
        .split(',')
        .map(|raw| raw.trim().trim_matches('\'').to_string())
        .collect();

    let registry_types: Vec<String> = STORABLE_TYPES
        .iter()
        .map(|t| t.as_str().to_string())
        .collect();
    assert_eq!(
        sql_types, registry_types,
        "the stored-type CHECK and the Rust registry disagree:\n{create_sql}"
    );

    // Every storable type really is accepted by the CHECK, and every inverse
    // really is refused, so the two lists above are not merely similar text.
    let a = t
        .service_service()
        .create_service(service_cmd("API", ServiceType::Api))
        .unwrap()
        .entry
        .asset
        .id;
    let b = t
        .service_service()
        .create_service(service_cmd("VPS", ServiceType::Vps))
        .unwrap()
        .entry
        .asset
        .id;
    for relation_type in ALL_TYPES {
        let outcome = t.factory.transact(&mut |uow| {
            uow.relations().insert(
                &Relation::new(
                    RelationId::generate(),
                    a,
                    b,
                    *relation_type,
                    RelationProvenance::Imported,
                    chrono::Utc::now(),
                )
                .unwrap(),
            )
        });
        if STORABLE_TYPES.contains(relation_type) {
            assert!(
                outcome.is_ok(),
                "{} is registered as storable but the write failed: {outcome:?}",
                relation_type.as_str()
            );
        } else {
            assert!(
                matches!(outcome, Err(AppError::Validation { .. })),
                "{} is a view-time inverse and must not reach SQL: {outcome:?}",
                relation_type.as_str()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Batched tag loading
// ---------------------------------------------------------------------------

/// A module list must not cost one query per row for tags.
///
/// Every module's `list` renders a page of rows with their tag names. Loading
/// them per row is an N+1: the query count grows with the page, so a listing
/// gets slower exactly as the library gets bigger — and nothing in the API
/// would ever show it, because the rows come back identical either way.
///
/// The assertion is deliberately against a second page size rather than an
/// absolute number: the query count must not depend on the row count. That
/// fails only if the tags for the whole page are fetched together, and it does
/// not need to be re-tuned when an unrelated query is added.
#[test]
fn module_lists_load_tags_for_the_whole_page_in_one_query() {
    use assetmesh_core::application::software_service::CreateSoftware;
    use assetmesh_core::ports::repos::SoftwareFilter;

    fn software_cmd(name: &str, tag: &str) -> CreateSoftware {
        CreateSoftware {
            name: name.into(),
            category: SoftwareCategory::Application,
            summary: None,
            install_source: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: None,
            notes: None,
            architecture: None,
            installed_at: None,
            tags: vec![tag.to_string()],
            external_refs: Vec::new(),
        }
    }

    let small = env();
    for i in 0..6 {
        small
            .software_service()
            .create_software(software_cmd(&format!("Small {i}"), &format!("tag-{i}")))
            .unwrap();
    }

    // Arming after the seeding means only the list's own statements count. It
    // must be installed on the connection the read scope will use, which for an
    // in-memory database is the write connection.
    assetmesh_storage_sqlite::statement_accounting::arm(&small.factory.0);
    let small_rows = small
        .software_service()
        .list_software(&SoftwareFilter::default())
        .unwrap();
    let small_count = assetmesh_storage_sqlite::statement_accounting::take();
    assert_eq!(small_rows.len(), 6);

    let large = env();
    for i in 0..24 {
        large
            .software_service()
            .create_software(software_cmd(&format!("Large {i}"), &format!("tag-{i}")))
            .unwrap();
    }

    assetmesh_storage_sqlite::statement_accounting::arm(&large.factory.0);
    let large_rows = large
        .software_service()
        .list_software(&SoftwareFilter::default())
        .unwrap();
    let large_count = assetmesh_storage_sqlite::statement_accounting::take();
    assert_eq!(large_rows.len(), 24);

    assert_eq!(
        small_count, large_count,
        "listing 6 rows and 24 rows must cost the same number of statements; \
         loading tags once per row is the usual cause of a difference \
         ({small_count} vs {large_count})"
    );

    // And the rows really did come back with their tags, so the batching was
    // not achieved by dropping them. Compared as sets: the list's sort order is
    // the filter's business, not this test's.
    let mut expected: Vec<String> = (0..24).map(|i| format!("tag-{i}")).collect();
    expected.sort();
    let mut actual: Vec<String> = large_rows
        .iter()
        .flat_map(|row| row.tags.iter().cloned())
        .collect();
    actual.sort();
    assert_eq!(actual, expected);
    assert!(
        large_rows.iter().all(|row| !row.tags.is_empty()),
        "every listed row must still carry its own tag"
    );
}
