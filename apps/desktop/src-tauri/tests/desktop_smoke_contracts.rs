//! Smoke and contract tests for the desktop adapter commands and composition root.

use std::sync::Arc;

use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::media::{MediaType, Progress};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands;
use assetmesh_desktop_lib::state::{AppStatus, DesktopState};

fn temp_db_path(test_name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-desktop-{}-{}.db",
        test_name,
        uuid::Uuid::now_v7()
    ))
}

#[test]
fn desktop_initializes_database_and_reports_ready() {
    let db_path = temp_db_path("init-ready");
    let state = DesktopState::new();

    assert_eq!(state.get_status(), AppStatus::Loading);

    let status = state.initialize(&db_path).unwrap();
    match status {
        AppStatus::Ready { db_path: p } => {
            assert_eq!(p, db_path.to_string_lossy());
        }
        other => panic!("expected Ready, got {:?}", other),
    }

    assert_eq!(
        state.get_status(),
        AppStatus::Ready {
            db_path: db_path.to_string_lossy().to_string()
        }
    );

    // Reopen is idempotent
    let reopen = state.initialize(&db_path).unwrap();
    assert!(matches!(reopen, AppStatus::Ready { .. }));

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn desktop_reports_corrupt_failure_on_corrupt_database() {
    let db_path = temp_db_path("init-corrupt");
    std::fs::write(&db_path, b"not a valid sqlite file").unwrap();

    let state = DesktopState::new();
    let status = state.initialize(&db_path).unwrap();

    match status {
        AppStatus::CorruptFailure { message } | AppStatus::SetupFailure { message } => {
            assert!(!message.is_empty());
        }
        AppStatus::Ready { .. } => panic!("corrupt db must not report ready"),
        AppStatus::Loading => panic!("loading is not a terminal init state"),
    }

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn diverged_migration_checksum_classifies_as_corrupt_failure() {
    // A database that was migrated and then edited on disk is corruption,
    // not a setup problem. This used to be detected by sniffing the error
    // message for the word "checksum"; it is now carried by the
    // `CorruptData` variant, so this test pins the classification to the
    // variant rather than to any wording.
    let db_path = temp_db_path("diverged-checksum");
    {
        let state = DesktopState::new();
        assert!(matches!(
            state.initialize(&db_path),
            Ok(AppStatus::Ready { .. })
        ));
    }

    // Rewrite a recorded checksum so the canonical migration no longer
    // matches what the database says was applied.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE assetmesh_migrations SET checksum = 'tampered' WHERE version = \
             (SELECT MIN(version) FROM assetmesh_migrations)",
            [],
        )
        .unwrap();
    }

    let state = DesktopState::new();
    let status = state.initialize(&db_path).unwrap();
    match status {
        AppStatus::CorruptFailure { message } => {
            assert!(
                message.contains("checksum"),
                "message should name the checksum mismatch: {message}"
            );
        }
        AppStatus::SetupFailure { message } => {
            panic!("a diverged migration must be corrupt_failure, not setup_failure: {message}")
        }
        other => panic!("expected CorruptFailure, got {:?}", other),
    }

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn unopenable_database_classifies_as_setup_failure() {
    // A file that is not a database at all cannot be opened — that is a
    // setup problem, and it must NOT be reported as corruption.
    let db_path = temp_db_path("unopenable");
    std::fs::write(&db_path, b"not a valid sqlite file").unwrap();

    let state = DesktopState::new();
    let status = state.initialize(&db_path).unwrap();
    match status {
        AppStatus::SetupFailure { .. } => {}
        AppStatus::CorruptFailure { message } => {
            panic!("an unopenable file must be setup_failure, not corrupt_failure: {message}")
        }
        other => panic!("expected SetupFailure, got {:?}", other),
    }

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn app_capabilities_returns_without_requiring_database() {
    let caps = commands::app_capabilities().unwrap();
    assert_eq!(caps.modules, vec!["media", "software", "services"]);
    assert!(!caps.features.runtime_enrichment);
    assert!(!caps.features.projects);
}

#[test]
fn library_list_smoke_renders_real_asset_summaries_from_temp_db() {
    let db_path = temp_db_path("smoke-list");
    let state = DesktopState::new();
    state.initialize(&db_path).unwrap();

    // Populate using core application services
    state
        .with_factory(|factory| {
            let clock = Arc::new(SystemClock);
            let ids = Arc::new(UuidV7Generator);

            let mut media_svc = MediaService::new(factory.clone(), clock.clone(), ids.clone());
            media_svc
                .create_media(CreateMedia {
                    title: "Dungeon Meshi".into(),
                    media_type: MediaType::Anime,
                    summary: Some("Cooking in dungeons".into()),
                    status: None,
                    rating: Some(9.0),
                    year: Some(2024),
                    platform: None,
                    progress: Progress::default(),
                    notes: None,
                    tags: vec!["fantasy".into(), "cooking".into()],
                    external_refs: Vec::new(),
                    started_at: None,
                    completed_at: None,
                })
                .unwrap();

            let mut sw_svc = SoftwareService::new(factory.clone(), clock.clone(), ids.clone());
            sw_svc
                .create_software(CreateSoftware {
                    name: "VS Code".into(),
                    category: SoftwareCategory::Application,
                    summary: None,
                    install_source: None,
                    version: Some("1.96.0".into()),
                    install_location: None,
                    executable_path: None,
                    purpose: None,
                    notes: None,
                    architecture: None,
                    installed_at: None,
                    tags: vec!["editor".into()],
                    external_refs: Vec::new(),
                })
                .unwrap();

            let mut svc_svc = ServiceService::new(factory.clone(), clock.clone(), ids.clone());
            svc_svc
                .create_service(CreateService {
                    name: "GitHub Copilot".into(),
                    service_type: ServiceType::Saas,
                    summary: None,
                    provider: Some("GitHub".into()),
                    account_label: None,
                    endpoint_url: None,
                    dashboard_url: None,
                    domain_name: None,
                    plan: Some("Individual".into()),
                    cost_minor: Some(1000),
                    currency: Some("USD".into()),
                    billing_cadence: None,
                    renews_at: None,
                    expires_at: None,
                    auto_renew: None,
                    notes: None,
                    tags: vec!["ai".into()],
                    external_refs: Vec::new(),
                })
                .unwrap();

            Ok(())
        })
        .unwrap();

    // Now call the desktop adapter command implementation directly
    let page = commands::library_list_impl(None, &state).unwrap();

    assert_eq!(page.total, Some(3));
    assert_eq!(page.items.len(), 3);

    // Verify fields in real AssetSummaryDto
    let names: Vec<&str> = page.items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Dungeon Meshi"));
    assert!(names.contains(&"VS Code"));
    assert!(names.contains(&"GitHub Copilot"));

    let anime = page
        .items
        .iter()
        .find(|i| i.name == "Dungeon Meshi")
        .unwrap();
    assert_eq!(anime.kind, "media.anime");
    assert_eq!(anime.lifecycle, "active");
    assert!(anime.tags.contains(&"fantasy".to_string()));

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn library_list_fails_when_uninitialized() {
    let state = DesktopState::new();
    let err = commands::library_list_impl(None, &state).unwrap_err();
    assert_eq!(err.category, "setup_required");
}

#[test]
fn desktop_modules_centralized_assembly_and_concurrent_nonblocking_reads() {
    let db_path = temp_db_path("concurrency-reads");
    let state = Arc::new(DesktopState::new());
    state.initialize(&db_path).expect("failed to init state");

    // Centralized DesktopModules assembly test
    let modules = state.modules().expect("modules available");
    let mut media_svc = modules.media();
    let media = media_svc
        .create_media(CreateMedia {
            title: "Concurrency Item".into(),
            media_type: MediaType::Anime,
            summary: None,
            status: None,
            rating: None,
            year: None,
            platform: None,
            progress: Progress::default(),
            notes: None,
            tags: vec!["concurrent".into()],
            external_refs: Vec::new(),
            started_at: None,
            completed_at: None,
        })
        .unwrap();

    let asset_id_str = media.entry.asset.id.to_string();

    // Spawn long write transaction in background
    let state_write = Arc::clone(&state);
    let (tx_start, rx_start) = std::sync::mpsc::channel();
    let (tx_finish, rx_finish) = std::sync::mpsc::channel();

    let write_handle = std::thread::spawn(move || {
        state_write.with_modules(|m| {
            let mut svc = m.software();
            tx_start.send(()).unwrap();
            rx_finish.recv().unwrap();
            svc.create_software(CreateSoftware {
                name: "Concurrent App".into(),
                category: SoftwareCategory::Tool,
                summary: None,
                install_source: None,
                version: None,
                install_location: None,
                executable_path: None,
                purpose: None,
                notes: None,
                architecture: None,
                installed_at: None,
                tags: vec![],
                external_refs: Vec::new(),
            })
            .unwrap();
            Ok(())
        })
    });

    // Wait for background worker to begin its execution
    rx_start.recv().unwrap();

    // Concurrent read queries execute immediately without waiting for writer to finish
    let mut handles = Vec::new();
    for _ in 0..5 {
        let state_read = Arc::clone(&state);
        let id_str = asset_id_str.clone();
        handles.push(std::thread::spawn(move || {
            let detail = commands::library_get_impl(&id_str, &state_read).unwrap();
            assert_eq!(detail.name, "Concurrency Item");
            let list = commands::library_list_impl(None, &state_read).unwrap();
            assert!(list.items.iter().any(|i| i.name == "Concurrency Item"));
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Now let writer finish
    tx_finish.send(()).unwrap();
    write_handle.join().unwrap().unwrap();

    let _ = std::fs::remove_file(&db_path);
}
