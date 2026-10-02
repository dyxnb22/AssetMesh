//! The menu bar is another entry point into the canonical desktop services.
//! Exercise its model against real SQLite and controlled local processes,
//! without depending on a native menu, the main window, or global settings.

#![cfg(unix)]

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use assetmesh_desktop_lib::commands::{
    library_get_impl, service_command_impl, service_runtime_logs_impl, service_runtime_status_impl,
};
use assetmesh_desktop_lib::dto::ServiceCommandDto;
use assetmesh_desktop_lib::menu_bar::{
    MenuBarService, MenuBarSnapshot, MenuLanguage, ServiceMenuAction,
};
use assetmesh_desktop_lib::runtime::{RuntimeState, RuntimeStatus};
use assetmesh_desktop_lib::state::{AppStatus, DesktopState};

struct Fixture {
    state: DesktopState,
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("assetmesh-menu-bar-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&directory).expect("unique fixture directory");
        let state = DesktopState::new();
        assert!(matches!(
            state
                .initialize(&directory.join("assetmesh.db"))
                .expect("real SQLite initializes"),
            AppStatus::Ready { .. }
        ));
        Self { state, directory }
    }

    fn snapshot(&self) -> MenuBarSnapshot {
        MenuBarSnapshot::read(&self.state).expect("menu snapshot reads")
    }

    fn archive(&self, asset_id: &str) {
        let revision = library_get_impl(asset_id, &self.state)
            .expect("canonical revision reads")
            .revision;
        service_command_impl(
            ServiceCommandDto::Archive {
                asset_id: asset_id.into(),
                expected_revision: Some(revision),
            },
            &self.state,
        )
        .expect("service archives");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // A failing assertion must not leave a sleeping test process behind.
        self.state.shutdown_service_runtime();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn create_service(
    state: &DesktopState,
    name: &str,
    service_type: &str,
    project_dir: Option<&Path>,
    start_command: Option<&str>,
    stop_command: Option<&str>,
    endpoint_url: Option<&str>,
) -> String {
    service_command_impl(
        ServiceCommandDto::Create {
            name: name.into(),
            service_type: service_type.into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url: endpoint_url.map(str::to_owned),
            dashboard_url: None,
            domain_name: None,
            plan: None,
            cost: None,
            currency: None,
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            project_dir: project_dir.map(|path| path.to_string_lossy().into_owned()),
            start_command: start_command.map(str::to_owned),
            stop_command: stop_command.map(str::to_owned),
            tags: Vec::new(),
        },
        state,
    )
    .expect("service is created")
    .asset_ids[0]
        .clone()
}

fn service<'a>(snapshot: &'a MenuBarSnapshot, asset_id: &str) -> &'a MenuBarService {
    snapshot
        .services
        .iter()
        .find(|service| service.asset_id == asset_id)
        .expect("active local service is in the menu")
}

fn wait_for_state(state: &DesktopState, asset_id: &str, expected: RuntimeState) -> RuntimeStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = service_runtime_status_impl(asset_id, state).expect("runtime status reads");
        if status.state == expected {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected:?}; last status: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn snapshot_contains_only_active_local_services_in_a_stable_order() {
    let fixture = Fixture::new();
    let last = create_service(
        &fixture.state,
        "Zebra",
        "local",
        Some(&fixture.directory),
        Some("sleep 60"),
        None,
        None,
    );
    let first = create_service(&fixture.state, "Alpha", "local", None, None, None, None);
    let same_name = create_service(&fixture.state, "Alpha", "local", None, None, None, None);
    let archived = create_service(
        &fixture.state,
        "Archived",
        "local",
        Some(&fixture.directory),
        Some("sleep 60"),
        None,
        None,
    );
    fixture.archive(&archived);
    create_service(
        &fixture.state,
        "Subscription",
        "saas",
        None,
        None,
        None,
        None,
    );

    let snapshot = fixture.snapshot();
    let mut alpha_ids = [first.clone(), same_name];
    alpha_ids.sort();
    assert_eq!(
        snapshot
            .services
            .iter()
            .map(|row| row.asset_id.as_str())
            .collect::<Vec<_>>(),
        vec![alpha_ids[0].as_str(), alpha_ids[1].as_str(), last.as_str()]
    );
    assert_eq!(snapshot.running_count(), 0);
    assert_eq!(snapshot.failed_count(), 0);
    assert_eq!(service(&snapshot, &last).state, RuntimeState::Stopped);
    assert!(service(&snapshot, &last).can_start);
    assert!(!service(&snapshot, &first).can_start);
    assert!(snapshot
        .services
        .iter()
        .all(|row| !row.can_stop && !row.can_restart && !row.can_open));
}

#[test]
fn managed_process_can_stop_without_a_configured_stop_command() {
    let fixture = Fixture::new();
    let asset_id = create_service(
        &fixture.state,
        "Managed",
        "local",
        Some(&fixture.directory),
        Some("echo menu-started; sleep 60"),
        None,
        // This address is never contacted or opened. The managed process
        // determines running status; the URL makes Open Page available.
        Some("https://example.invalid/service"),
    );
    assert!(!service(&fixture.snapshot(), &asset_id).can_open);
    ServiceMenuAction::Start
        .perform(&asset_id, &fixture.state)
        .expect("menu starts the canonical service");
    wait_for_state(&fixture.state, &asset_id, RuntimeState::Running);

    let snapshot = fixture.snapshot();
    let row = service(&snapshot, &asset_id);
    assert_eq!(row.state, RuntimeState::Running);
    assert!(!row.can_start);
    assert!(row.can_stop);
    assert!(row.can_restart);
    assert!(row.can_open);
    assert_eq!(snapshot.running_count(), 1);
    assert_eq!(snapshot.failed_count(), 0);

    ServiceMenuAction::Stop
        .perform(&asset_id, &fixture.state)
        .expect("menu stops the owned process group");
    wait_for_state(&fixture.state, &asset_id, RuntimeState::Stopped);
    let snapshot = fixture.snapshot();
    let row = service(&snapshot, &asset_id);
    assert!(row.can_start);
    assert!(!row.can_stop && !row.can_restart && !row.can_open);
    assert_eq!(snapshot.running_count(), 0);
}

#[test]
fn external_process_actions_follow_canonical_stop_and_launch_configuration() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("controlled external listener");
    let address = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("listener address").port()
    );
    let unconfigured = create_service(
        &fixture.state,
        "External without stop",
        "local",
        Some(&fixture.directory),
        Some("sleep 60"),
        None,
        Some(&address),
    );
    let configured = create_service(
        &fixture.state,
        "External configured",
        "local",
        Some(&fixture.directory),
        Some("sleep 60"),
        Some("echo stopped"),
        Some(&address),
    );
    let stop_only = create_service(
        &fixture.state,
        "External stop only",
        "local",
        Some(&fixture.directory),
        None,
        Some("echo stopped"),
        Some(&address),
    );

    let snapshot = fixture.snapshot();
    assert_eq!(snapshot.running_count(), 3);
    assert!(snapshot
        .services
        .iter()
        .all(|row| { row.state == RuntimeState::External && !row.can_start && row.can_open }));
    assert!(!service(&snapshot, &unconfigured).can_stop);
    assert!(!service(&snapshot, &unconfigured).can_restart);
    assert!(service(&snapshot, &configured).can_stop);
    assert!(service(&snapshot, &configured).can_restart);
    assert!(service(&snapshot, &stop_only).can_stop);
    assert!(!service(&snapshot, &stop_only).can_restart);

    let error = ServiceMenuAction::Stop
        .perform(&unconfigured, &fixture.state)
        .expect_err("an unconfigured menu stop cannot signal an external process");
    assert_eq!(error.category, "invalid_input");
    assert_eq!(
        service_runtime_status_impl(&unconfigured, &fixture.state)
            .expect("external listener remains intact")
            .state,
        RuntimeState::External
    );
    drop(listener);
    let snapshot = fixture.snapshot();
    assert_eq!(snapshot.running_count(), 0);
    assert!(snapshot
        .services
        .iter()
        .all(|row| row.state == RuntimeState::Stopped && !row.can_open && !row.can_stop));
    assert!(service(&snapshot, &configured).can_start);
    assert!(!service(&snapshot, &stop_only).can_start);
}

#[test]
fn failed_managed_runs_remain_visible_and_can_be_started_again() {
    let fixture = Fixture::new();
    let asset_id = create_service(
        &fixture.state,
        "Failing",
        "local",
        Some(&fixture.directory),
        Some("echo menu-failure; exit 7"),
        None,
        None,
    );
    ServiceMenuAction::Start
        .perform(&asset_id, &fixture.state)
        .expect("failing process launches");
    let status = wait_for_state(&fixture.state, &asset_id, RuntimeState::Failed);
    assert_eq!(status.exit_code, Some(7));

    let snapshot = fixture.snapshot();
    let row = service(&snapshot, &asset_id);
    assert_eq!(row.state, RuntimeState::Failed);
    assert!(row.can_start);
    assert!(!row.can_stop && !row.can_restart && !row.can_open);
    assert_eq!(snapshot.running_count(), 0);
    assert_eq!(snapshot.failed_count(), 1);
}

#[test]
fn stale_start_action_revalidates_an_archived_record_before_launching() {
    let fixture = Fixture::new();
    let asset_id = create_service(
        &fixture.state,
        "Soon archived",
        "local",
        Some(&fixture.directory),
        Some("echo started > menu-started; sleep 60"),
        None,
        None,
    );
    assert!(service(&fixture.snapshot(), &asset_id).can_start);
    fixture.archive(&asset_id);

    let error = ServiceMenuAction::Start
        .perform(&asset_id, &fixture.state)
        .expect_err("a previously enabled menu action must revalidate the current lifecycle");
    assert_eq!(error.category, "invalid_input");
    assert!(!fixture.state.service_runtime().is_active(&asset_id));
    assert!(!fixture.directory.join("menu-started").exists());
    assert!(fixture.snapshot().services.is_empty());
}

#[test]
fn stale_external_stop_cannot_execute_an_archived_records_stop_command() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("controlled external listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking accept for controlled probes");
    let address = listener.local_addr().expect("listener address");
    let asset_id = create_service(
        &fixture.state,
        "External then archived",
        "local",
        Some(&fixture.directory),
        Some("sleep 60"),
        Some("echo external-stop-ran > external-stop-ran"),
        Some(&format!("http://{address}")),
    );
    let snapshot = fixture.snapshot();
    let row = service(&snapshot, &asset_id);
    assert_eq!(row.state, RuntimeState::External);
    assert!(row.can_stop, "the old menu enabled external Stop");
    // Drain the snapshot's probe so the later accept verifies a fresh
    // connection, rather than an old connection already in the backlog.
    while listener.accept().is_ok() {}
    fixture.archive(&asset_id);

    let error = ServiceMenuAction::Stop
        .perform(&asset_id, &fixture.state)
        .expect_err("stale Stop must revalidate lifecycle before running any command");
    assert_eq!(error.category, "invalid_input");
    assert!(
        !fixture.directory.join("external-stop-ran").exists(),
        "the archived record's stop command must never execute"
    );
    let client = TcpStream::connect_timeout(&address, Duration::from_secs(1))
        .expect("external listener still accepts connections");
    // connect can complete just before a nonblocking listener sees the peer.
    // This is our established test connection; receive it without that race.
    listener
        .set_nonblocking(false)
        .expect("receive the established test connection");
    let (_, peer) = listener.accept().expect("accept the fresh connection");
    assert_eq!(peer, client.local_addr().expect("client address"));
    assert!(!fixture.state.service_runtime().is_active(&asset_id));
    assert!(fixture.snapshot().services.is_empty());
}

#[test]
fn menu_restart_uses_a_new_managed_run_and_quit_prevents_queued_starts() {
    let fixture = Fixture::new();
    let asset_id = create_service(
        &fixture.state,
        "Restartable",
        "local",
        Some(&fixture.directory),
        Some("echo menu-run; sleep 60"),
        None,
        None,
    );
    ServiceMenuAction::Start
        .perform(&asset_id, &fixture.state)
        .expect("menu starts service");
    wait_for_state(&fixture.state, &asset_id, RuntimeState::Running);
    let first_run = service_runtime_logs_impl(&asset_id, None, None, &fixture.state)
        .expect("first run logs")
        .run_id
        .expect("started run has an ID");
    assert_eq!(
        ServiceMenuAction::Start
            .perform(&asset_id, &fixture.state)
            .expect_err("duplicate menu start is refused")
            .category,
        "conflict"
    );

    ServiceMenuAction::Restart
        .perform(&asset_id, &fixture.state)
        .expect("menu restart stops then replaces the canonical run");
    let replacement = wait_for_state(&fixture.state, &asset_id, RuntimeState::Running);
    let second_run = service_runtime_logs_impl(&asset_id, None, None, &fixture.state)
        .expect("replacement run logs")
        .run_id
        .expect("replacement run has an ID");
    assert_ne!(first_run, second_run);

    fixture.state.shutdown_service_runtime();
    assert!(!fixture.state.service_runtime().is_active(&asset_id));
    assert_ne!(
        unsafe { libc::kill(replacement.pid.expect("managed PID") as i32, 0) },
        0,
        "Quit must reap the process it started"
    );
    for action in [ServiceMenuAction::Start, ServiceMenuAction::Restart] {
        assert_eq!(
            action
                .perform(&asset_id, &fixture.state)
                .expect_err("menu callbacks cannot launch after Quit")
                .category,
            "unavailable"
        );
    }
    assert!(!fixture.state.service_runtime().is_active(&asset_id));
}

#[test]
fn menu_language_tracks_persisted_backup_preferences() {
    let fixture = Fixture::new();
    assert_eq!(fixture.snapshot().language, MenuLanguage::Chinese);
    let backup = fixture
        .state
        .backup()
        .expect("backup preferences available");
    backup
        .save_preferences([("assetmesh-lang".into(), "en".into())].into())
        .expect("save English language");
    assert_eq!(fixture.snapshot().language, MenuLanguage::English);

    // Reopening demonstrates that this is durable application preference
    // state, rather than a language held only by the visible React window.
    let reopened = DesktopState::new();
    reopened
        .initialize(&fixture.directory.join("assetmesh.db"))
        .expect("database reopens");
    assert_eq!(
        MenuBarSnapshot::read(&reopened)
            .expect("reopened snapshot")
            .language,
        MenuLanguage::English
    );
    backup
        .save_preferences([("assetmesh-lang".into(), "zh".into())].into())
        .expect("save Chinese language");
    assert_eq!(fixture.snapshot().language, MenuLanguage::Chinese);
}

#[test]
fn native_exit_without_exit_requested_stops_services_and_flushes_the_final_backup() {
    use tauri::Manager;

    let directory =
        std::env::temp_dir().join(format!("assetmesh-native-exit-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&directory).unwrap();
    let state = DesktopState::new();
    state.initialize(&directory.join("library.db")).unwrap();
    let asset_id = create_service(
        &state,
        "Quit regression",
        "local",
        Some(&directory),
        Some("sleep 60"),
        None,
        None,
    );
    ServiceMenuAction::Start.perform(&asset_id, &state).unwrap();
    wait_for_state(&state, &asset_id, RuntimeState::Running);

    let app = assetmesh_desktop_lib::configure_builder(tauri::test::mock_builder())
        .manage(state)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    // Actual macOS Cmd+Q emits Exit directly. Exercise the exact callback
    // registered by run(), rather than calling shutdown manually.
    assetmesh_desktop_lib::handle_run_event(app.handle(), &tauri::RunEvent::Exit);
    let state = app.state::<DesktopState>();
    let survived = state.service_runtime().is_active(&asset_id);
    let backed_up = state
        .backup()
        .unwrap()
        .status()
        .unwrap()
        .entries
        .iter()
        .any(|entry| entry.asset_count == 1);
    // A red regression must still clean up its test-owned process.
    state.shutdown_service_runtime();
    let _ = std::fs::remove_dir_all(&directory);
    assert!(!survived, "managed service survived the native Exit event");
    assert!(
        backed_up,
        "native Exit did not flush the final library backup"
    );
}
