//! Local-service runtime contracts.
//!
//! These tests drive the real process runtime through the command impls
//! against real SQLite, using controlled test processes (plain `sleep`/`echo`
//! shells) — no external service, account, or network. They pin the behavior
//! the UI depends on: lifecycle states reported by the backend, duplicate
//! start protection, subprocess cleanup on stop, bounded logs, launch-config
//! persistence across reopen, and archival stopping the process it started.

use assetmesh_desktop_lib::commands::{
    library_get_impl, service_command_impl, service_runtime_logs_impl,
    service_runtime_restart_impl, service_runtime_start_impl, service_runtime_status_impl,
    service_runtime_statuses_impl, service_runtime_stop_impl,
};
use assetmesh_desktop_lib::dto::ServiceCommandDto;
use assetmesh_desktop_lib::state::{AppStatus, DesktopState};
use std::time::{Duration, Instant};

#[cfg(unix)]
#[path = "support/loopback.rs"]
mod loopback;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let base =
        std::env::temp_dir().join(format!("assetmesh-runtime-{}-{name}", std::process::id(),));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("temp dir");
    base
}

fn initialized_state(name: &str) -> DesktopState {
    let dir = temp_dir(name);
    let state = DesktopState::new();
    let status = state
        .initialize(&dir.join("assetmesh.db"))
        .expect("database initializes");
    assert!(matches!(status, AppStatus::Ready { .. }));
    state
}

/// Binds an ephemeral port to reserve a free loopback port for the test.
fn free_local_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("ephemeral listener")
        .local_addr()
        .expect("local addr")
        .port()
}

fn create_local_service(state: &DesktopState, dir: &std::path::Path, command: &str) -> String {
    create_local_service_with_port(state, dir, command, None)
}

fn create_local_service_with_port(
    state: &DesktopState,
    dir: &std::path::Path,
    command: &str,
    port: Option<u16>,
) -> String {
    let endpoint_url = port.map(|p| format!("http://127.0.0.1:{p}"));
    let receipt = service_command_impl(
        ServiceCommandDto::Create {
            name: "gcli2api".into(),
            service_type: "local".into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url,
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
            project_dir: Some(dir.to_string_lossy().to_string()),
            start_command: Some(command.into()),
            stop_command: None,
            tags: vec![],
        },
        state,
    )
    .expect("local service is created");
    receipt.asset_ids[0].clone()
}

fn create_subscription_service(state: &DesktopState) -> String {
    let receipt = service_command_impl(
        ServiceCommandDto::Create {
            name: "GitHub Copilot".into(),
            service_type: "saas".into(),
            summary: None,
            provider: Some("GitHub".into()),
            account_label: None,
            endpoint_url: None,
            dashboard_url: None,
            domain_name: None,
            plan: Some("Business".into()),
            cost: Some("19.00".into()),
            currency: Some("USD".into()),
            billing_cadence: Some("monthly".into()),
            renews_at: Some("2024-04-01".into()),
            expires_at: None,
            auto_renew: Some(true),
            notes: None,
            project_dir: None,
            start_command: None,
            stop_command: None,
            tags: vec!["dev".into()],
        },
        state,
    )
    .expect("subscription service is created");
    receipt.asset_ids[0].clone()
}

fn update_command(
    asset_id: &str,
    expected_revision: i64,
    fields: Vec<(&str, String)>,
) -> ServiceCommandDto {
    let mut cmd = ServiceCommandDto::Update {
        asset_id: asset_id.to_string(),
        expected_revision: Some(expected_revision),
        name: None,
        summary: None.into(),
        provider: None.into(),
        account_label: None.into(),
        endpoint_url: None.into(),
        dashboard_url: None.into(),
        domain_name: None.into(),
        plan: None.into(),
        cost: None.into(),
        currency: None.into(),
        billing_cadence: None.into(),
        renews_at: None.into(),
        expires_at: None.into(),
        auto_renew: None.into(),
        notes: None.into(),
        project_dir: None.into(),
        start_command: None.into(),
        stop_command: None.into(),
    };
    for (field, value) in fields {
        match field {
            "name" => {
                if let ServiceCommandDto::Update { name, .. } = &mut cmd {
                    *name = Some(value);
                }
            }
            "project_dir" => {
                if let ServiceCommandDto::Update { project_dir, .. } = &mut cmd {
                    *project_dir = Some(value).into();
                }
            }
            "start_command" => {
                if let ServiceCommandDto::Update { start_command, .. } = &mut cmd {
                    *start_command = Some(value).into();
                }
            }
            "stop_command" => {
                if let ServiceCommandDto::Update { stop_command, .. } = &mut cmd {
                    *stop_command = Some(value).into();
                }
            }
            "notes" => {
                if let ServiceCommandDto::Update { notes, .. } = &mut cmd {
                    *notes = Some(value).into();
                }
            }
            _ => panic!("unsupported field {field}"),
        }
    }
    cmd
}

/// Waits until the runtime status satisfies `predicate`. All lifecycle facts
/// here come from the backend's watcher threads, so the test simply polls.
fn wait_for_status(
    state: &DesktopState,
    asset_id: &str,
    predicate: impl Fn(&assetmesh_desktop_lib::runtime::RuntimeStatus) -> bool,
) -> assetmesh_desktop_lib::runtime::RuntimeStatus {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = service_runtime_status_impl(asset_id, state).expect("status reads");
        if predicate(&status) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for status, last: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn start_stop_reports_lifecycle_and_cleans_up_the_whole_group() {
    let state = initialized_state("lifecycle");
    let dir = temp_dir("lifecycle-project");
    let child_pid_file = dir.join("child.pid");
    // `sleep` runs as a child of the shell, so stopping must reach both the
    // shell and the sleeping descendant through the process group.
    let command = format!(
        "echo $$ > {}; echo service-started; sleep 60",
        child_pid_file.display()
    );
    let asset_id = create_local_service(&state, &dir, &command);

    // Before any start: stopped, nothing held.
    let status = service_runtime_status_impl(&asset_id, &state).unwrap();
    assert_eq!(
        status.state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    );

    let status = service_runtime_start_impl(&asset_id, &state).expect("start");
    assert_eq!(
        status.state,
        assetmesh_desktop_lib::runtime::RuntimeState::Starting
    );

    // Output reaches the bounded log buffer.
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    wait_for_status(&state, &asset_id, |s| {
        let logs = service_runtime_logs_impl(&asset_id, None, None, &state).unwrap();
        !logs.lines.is_empty() && s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });

    // Duplicate start is refused while the process is held.
    let again = service_runtime_start_impl(&asset_id, &state);
    assert!(again.is_err(), "a second start must be refused");

    // The descendant is alive before the stop.
    let mut waited = Instant::now() + Duration::from_secs(5);
    while !child_pid_file.exists() {
        assert!(Instant::now() < waited, "child pid file never appeared");
        std::thread::sleep(Duration::from_millis(50));
    }
    let child_pid: i32 = std::fs::read_to_string(&child_pid_file)
        .expect("child pid")
        .trim()
        .parse()
        .expect("pid is a number");
    assert_eq!(unsafe { libc::kill(child_pid, 0) }, 0, "child is alive");
    waited = Instant::now() + Duration::from_secs(5);
    let _ = waited;

    // Stop: the backend escalates from SIGTERM; both shell and child die.
    let stopped = service_runtime_stop_impl(&asset_id, &state)
        .expect("stop succeeds")
        .expect("stop reports the entry it stopped");
    assert_eq!(
        stopped.state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopping
    );

    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let alive = unsafe { libc::kill(child_pid, 0) } == 0;
        if !alive {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the sleeping descendant survived the stop"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // A stopped run can be started again.
    service_runtime_start_impl(&asset_id, &state).expect("restart after stop");
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    service_runtime_stop_impl(&asset_id, &state).expect("cleanup stop");
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    });
}

#[test]
fn concurrent_starts_produce_at_most_one_process() {
    let state = initialized_state("concurrent");
    let dir = temp_dir("concurrent-project");
    let asset_id = create_local_service(&state, &dir, "sleep 60");

    let state_ref = &state;
    let (first, second) = std::thread::scope(|scope| {
        let id_a = asset_id.clone();
        let id_b = asset_id.clone();
        let a = scope.spawn(move || service_runtime_start_impl(&id_a, state_ref));
        let b = scope.spawn(move || service_runtime_start_impl(&id_b, state_ref));
        (a.join().unwrap(), b.join().unwrap())
    });

    let successes = [first.is_ok(), second.is_ok()]
        .iter()
        .filter(|ok| **ok)
        .count();
    assert_eq!(successes, 1, "exactly one concurrent start wins");
    let refused = if first.is_err() { first } else { second };
    let err = match refused {
        Err(err) => err,
        Ok(_) => unreachable!("exactly one start was refused"),
    };
    assert_eq!(err.category, "conflict");

    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    let statuses = service_runtime_statuses_impl(&state).unwrap();
    let managed: Vec<_> = statuses
        .iter()
        .filter(|s| s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running)
        .collect();
    assert_eq!(managed.len(), 1, "one process per service");

    service_runtime_stop_impl(&asset_id, &state).expect("cleanup");
}

#[test]
fn missing_directory_and_unknown_command_report_clear_reasons_and_recover() {
    let state = initialized_state("failures");
    let dir = temp_dir("failure-project");

    // Directory does not exist: refused before any process is made.
    let asset_id = create_local_service(&state, &dir.join("nope"), "sleep 5");
    let error = service_runtime_start_impl(&asset_id, &state).unwrap_err();
    assert_eq!(error.category, "invalid_input");
    assert!(
        error.message.contains("directory does not exist"),
        "{error:?}"
    );
    // Refusal leaves no managed entry behind: the record only appears as an
    // unmanaged (stopped, pid-less) read — never as a managed outcome.
    let statuses = service_runtime_statuses_impl(&state).unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].asset_id, asset_id);
    assert_eq!(
        statuses[0].state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    );
    assert_eq!(statuses[0].pid, None);

    // A command that does not exist: the child exits 127, reported as failed.
    std::fs::create_dir_all(&dir).unwrap();
    let asset_id = create_local_service(&state, &dir, "definitely-not-a-command-xyz");
    service_runtime_start_impl(&asset_id, &state).expect("spawn itself succeeds");
    let status = wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Failed
    });
    let message = status.error.expect("failure carries a reason");
    assert!(
        message.contains("127"),
        "exit code 127 is surfaced: {message}"
    );

    // Failure is terminal but recoverable: the next start works.
    let asset_id = create_local_service(&state, &dir, "echo recovered; sleep 60");
    service_runtime_start_impl(&asset_id, &state).expect("start after failure");
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    service_runtime_stop_impl(&asset_id, &state).expect("cleanup");
}

#[test]
fn abnormal_exit_is_reported_as_failure() {
    let state = initialized_state("abnormal");
    let dir = temp_dir("abnormal-project");
    let asset_id = create_local_service(&state, &dir, "exit 3");

    service_runtime_start_impl(&asset_id, &state).expect("start");
    let status = wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Failed
    });
    assert_eq!(status.exit_code, Some(3));
    let message = status.error.expect("abnormal exit carries a reason");
    assert!(message.contains("exit code 3"), "{message}");
}

#[test]
fn logs_are_bounded_and_flag_dropped_output() {
    let state = initialized_state("logs");
    let dir = temp_dir("logs-project");
    // 20k lines far exceed the backend's bounded buffer.
    let asset_id = create_local_service(
        &state,
        &dir,
        "for i in $(seq 1 20000); do echo line-$i; done; sleep 30",
    );

    service_runtime_start_impl(&asset_id, &state).expect("start");
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });

    // Drain incrementally until the producer finishes (seq stops growing).
    let mut since = 0u64;
    let mut total = 0usize;
    let mut saw_dropped = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut stable_polls = 0;
    loop {
        let logs = service_runtime_logs_impl(&asset_id, Some(since), None, &state).unwrap();
        saw_dropped = saw_dropped || logs.dropped;
        if logs.lines.is_empty() {
            stable_polls += 1;
        } else {
            stable_polls = 0;
            total += logs.lines.len();
            since = logs.lines.last().expect("non-empty").seq;
        }
        if stable_polls >= 10 {
            break;
        }
        assert!(Instant::now() < deadline, "logs never settled");
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(saw_dropped, "dropped flag reports trimmed output");
    assert!(
        total <= 1001,
        "the retained window stays bounded, saw {total}"
    );
    service_runtime_stop_impl(&asset_id, &state).expect("cleanup");
}

#[test]
fn launch_configuration_persists_and_nothing_autostarts_after_reopen() {
    let dir = temp_dir("persist");
    let db_path = dir.join("assetmesh.db");
    let project = temp_dir("persist-project");

    let state = DesktopState::new();
    state.initialize(&db_path).expect("first open");
    let port = free_local_port();
    let asset_id =
        create_local_service_with_port(&state, &project, "bash start-local.sh", Some(port));

    // Reopen the same database: the saved configuration is canonical state.
    drop(state);
    let reopened = DesktopState::new();
    reopened.initialize(&db_path).expect("second open");

    let detail = library_get_impl(&asset_id, &reopened).expect("detail after reopen");
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["service_type"],
        "local"
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["project_dir"],
        project.to_string_lossy().to_string()
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["start_command"],
        "bash start-local.sh"
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["endpoint_url"],
        format!("http://127.0.0.1:{port}")
    );

    // Runtime state is temporary: nothing is managed, nothing auto-starts.
    // The reopened record appears only as an unmanaged (stopped, pid-less)
    // read — its address is not served by anything here, so no `external`.
    let statuses = service_runtime_statuses_impl(&reopened).unwrap();
    assert_eq!(statuses.len(), 1, "a fresh session holds no processes");
    assert_eq!(statuses[0].asset_id, asset_id);
    assert_eq!(
        statuses[0].state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    );
    assert_eq!(statuses[0].pid, None);
}

#[test]
fn old_subscription_records_survive_and_still_support_renewals() {
    let dir = temp_dir("legacy");
    let db_path = dir.join("assetmesh.db");

    let state = DesktopState::new();
    state.initialize(&db_path).expect("first open");
    let asset_id = create_subscription_service(&state);

    // Reopen across the migration boundary: the old record is untouched.
    drop(state);
    let reopened = DesktopState::new();
    reopened
        .initialize(&db_path)
        .expect("reopen with new schema");

    let detail = library_get_impl(&asset_id, &reopened).expect("detail");
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["service_type"],
        "saas"
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["plan"],
        "Business"
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["cost_minor"],
        1900
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["currency"],
        "USD"
    );
    // Old records have no launch metadata and must not be converted.
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["project_dir"],
        serde_json::Value::Null
    );
    assert_eq!(
        serde_json::to_value(&detail.details).unwrap()["start_command"],
        serde_json::Value::Null
    );

    // The renewal use case keeps working on the old record.
    let renewal = service_command_impl(
        ServiceCommandDto::RecordRenewal {
            asset_id: asset_id.clone(),
            renews_at: "2024-04-15".into(),
            cost: Some("19.00".into()),
            currency: Some("USD".into()),
            next_renews_at: Some("2024-05-15".into()),
            next_expires_at: None,
            expected_revision: Some(detail.revision),
        },
        &reopened,
    )
    .expect("renewal still records");
    assert!(renewal.changed);
}

#[test]
fn running_services_cannot_be_reconfigured_and_archive_stops_the_process() {
    let state = initialized_state("guards");
    let dir = temp_dir("guards-project");
    let child_pid_file = dir.join("child.pid");
    let command = format!("sleep 60 & echo $! > {}; wait", child_pid_file.display());
    let asset_id = create_local_service(&state, &dir, &command);

    service_runtime_start_impl(&asset_id, &state).expect("start");
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });

    // Editing launch configuration while running is refused.
    let detail = library_get_impl(&asset_id, &state).expect("detail for revision");
    let err = service_command_impl(
        update_command(
            &asset_id,
            detail.revision,
            vec![("start_command", "bash other.sh".into())],
        ),
        &state,
    )
    .unwrap_err();
    assert_eq!(
        err.category, "conflict",
        "running service is frozen: {err:?}"
    );

    // Explicit null must not bypass the same guard as a replacement value.
    for field in ["project_dir", "start_command"] {
        let mut command = serde_json::json!({
            "action": "update", "asset_id": asset_id,
            "expected_revision": detail.revision
        });
        command[field] = serde_json::Value::Null;
        let error =
            service_command_impl(serde_json::from_value(command).unwrap(), &state).unwrap_err();
        assert_eq!(error.category, "conflict", "{field}: {error:?}");
    }
    assert_eq!(
        library_get_impl(&asset_id, &state).unwrap().revision,
        detail.revision
    );

    // Unrelated fields stay editable while running.
    let ok = service_command_impl(
        update_command(
            &asset_id,
            detail.revision,
            vec![("notes", "still editable".into())],
        ),
        &state,
    )
    .expect("non-launch edits work while running");
    assert!(ok.changed);

    // Archiving a running service stops its process first: no orphan.
    service_command_impl(
        ServiceCommandDto::Archive {
            asset_id: asset_id.clone(),
            expected_revision: Some(ok.revision.expect("archive returns revision")),
        },
        &state,
    )
    .expect("archive succeeds");

    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    });

    let mut waited = 0;
    while !child_pid_file.exists() && waited < 50 {
        std::thread::sleep(Duration::from_millis(100));
        waited += 1;
    }
    if child_pid_file.exists() {
        let child_pid: i32 = std::fs::read_to_string(&child_pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if unsafe { libc::kill(child_pid, 0) } != 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the sleeping descendant survived archival"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
fn subscription_services_and_launch_metadata_do_not_mix() {
    let state = initialized_state("type-guard");
    let dir = temp_dir("type-guard-project");

    // A saas service cannot carry launch metadata.
    let error = service_command_impl(
        ServiceCommandDto::Create {
            name: "Not a local service".into(),
            service_type: "saas".into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url: None,
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
            project_dir: Some(dir.to_string_lossy().to_string()),
            start_command: Some("sleep 5".into()),
            stop_command: None,
            tags: vec![],
        },
        &state,
    )
    .unwrap_err();
    assert_eq!(error.category, "invalid_input");

    // A subscription service cannot be started here even with a stray id.
    let saas_id = create_subscription_service(&state);
    let error = service_runtime_start_impl(&saas_id, &state).unwrap_err();
    assert_eq!(error.category, "invalid_input");
    assert!(error.message.contains("local service"), "{error:?}");
}

#[test]
fn start_is_refused_when_an_outside_instance_holds_the_access_port() {
    let state = initialized_state("external");
    let dir = temp_dir("external-project");
    let port = free_local_port();
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("simulate external instance");

    let asset_id = create_local_service_with_port(&state, &dir, "sleep 60", Some(port));
    let error = service_runtime_start_impl(&asset_id, &state).unwrap_err();
    assert_eq!(error.category, "conflict");
    assert!(
        error.message.contains("outside AssetMesh"),
        "the reason names the external instance: {error:?}"
    );

    // Nothing was spawned for the refused start.
    let statuses = service_runtime_statuses_impl(&state).unwrap();
    assert!(statuses
        .iter()
        .all(|s| s.state != assetmesh_desktop_lib::runtime::RuntimeState::Running));

    // Once the external instance is gone the same record starts normally.
    drop(listener);
    service_runtime_start_impl(&asset_id, &state).expect("start after external stop");
    wait_for_status(&state, &asset_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    service_runtime_stop_impl(&asset_id, &state).expect("cleanup");
}

/// An unmanaged local service (no held process) is reported from the port
/// probe alone: an access address that answers on loopback shows `external`,
/// silence shows `stopped`. A held registry entry always wins — the outcome
/// of the last managed run stays visible even when an external instance
/// later takes the port. Nothing unmanaged ever carries a pid, and nothing
/// is signalled from the read path.
#[test]
fn unmanaged_services_report_external_from_the_port_probe_only() {
    let state = initialized_state("external-status");
    let dir = temp_dir("external-status-project");
    let port = free_local_port();
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("simulate external instance");

    let asset_id = create_local_service_with_port(&state, &dir, "sleep 60", Some(port));

    // Unmanaged + an answering access address: external, with no pid to stop.
    let status = service_runtime_status_impl(&asset_id, &state).expect("status reads");
    assert_eq!(
        status.state,
        assetmesh_desktop_lib::runtime::RuntimeState::External
    );
    assert_eq!(status.pid, None);
    let statuses = service_runtime_statuses_impl(&state).expect("statuses read");
    let listed = statuses
        .iter()
        .find(|s| s.asset_id == asset_id)
        .expect("the unmanaged service is listed");
    assert_eq!(
        listed.state,
        assetmesh_desktop_lib::runtime::RuntimeState::External
    );

    // Silence on the same address reads as plain stopped.
    drop(listener);
    let status = service_runtime_status_impl(&asset_id, &state).expect("status reads");
    assert_eq!(
        status.state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    );

    // A non-local service is never probed, even with a loopback address that
    // answers: the observation is a local-service fact only.
    let saas_id = service_command_impl(
        ServiceCommandDto::Create {
            name: "Loopback SaaS".into(),
            service_type: "saas".into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url: Some(format!("http://127.0.0.1:{port}")),
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
            project_dir: None,
            start_command: None,
            stop_command: None,
            tags: vec![],
        },
        &state,
    )
    .expect("saas service is created")
    .asset_ids[0]
        .clone();
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).expect("rebind port");
    let status = service_runtime_status_impl(&saas_id, &state).expect("status reads");
    assert_eq!(
        status.state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    );
    drop(listener);

    // A held registry entry wins over the probe: after a managed run
    // finished, a later external instance on the same port cannot flip the
    // record to `external` — the last managed outcome stays visible.
    let free = free_local_port();
    let managed_id = create_local_service_with_port(&state, &dir, "sleep 60", Some(free));
    service_runtime_start_impl(&managed_id, &state).expect("managed start");
    wait_for_status(&state, &managed_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    service_runtime_stop_impl(&managed_id, &state).expect("managed stop");
    wait_for_status(&state, &managed_id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    });
    let _late = std::net::TcpListener::bind(("127.0.0.1", free)).expect("late external instance");
    let status = service_runtime_status_impl(&managed_id, &state).expect("status reads");
    assert_eq!(
        status.state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped,
        "the finished managed run keeps its outcome"
    );
}

#[cfg(unix)]
#[test]
fn blocked_earlier_services_cannot_starve_a_healthy_later_service() {
    let state = initialized_state("fair-probe-budget");
    let dir = temp_dir("fair-probe-project");
    let mut saturated = Vec::new();
    for _ in 0..5 {
        let (listener, clients) = loopback::saturated_listener();
        create_local_service_with_port(
            &state,
            &dir,
            "sleep 60",
            Some(listener.local_addr().unwrap().port()),
        );
        saturated.push((listener, clients));
    }
    let healthy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let id = create_local_service_with_port(
        &state,
        &dir,
        "sleep 60",
        Some(healthy.local_addr().unwrap().port()),
    );
    let revision = library_get_impl(&id, &state).unwrap().revision;
    service_command_impl(
        update_command(&id, revision, vec![("name", "zz healthy".into())]),
        &state,
    )
    .unwrap();

    for round in 1..=2 {
        let began = Instant::now();
        let statuses = service_runtime_statuses_impl(&state).expect("a complete probe round");
        let last = statuses.last().expect("healthy service sorts last");
        assert_eq!(last.asset_id, id);
        assert_eq!(
            last.state,
            assetmesh_desktop_lib::runtime::RuntimeState::External,
            "healthy listener was skipped in round {round}"
        );
        assert!(began.elapsed() < Duration::from_millis(1750));
    }
}

#[test]
fn stopping_remains_guarded_and_blocking_stop_waits_for_forced_cleanup() {
    let state = initialized_state("stop-wait");
    let dir = temp_dir("stop-wait-project");
    let id = create_local_service(
        &state,
        &dir,
        "trap '' TERM; echo ready; while :; do sleep 1; done",
    );
    service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    service_runtime_stop_impl(&id, &state).unwrap();
    assert!(state.service_runtime().is_active(&id));
    let revision = library_get_impl(&id, &state).unwrap().revision;
    assert_eq!(
        service_command_impl(
            update_command(
                &id,
                revision,
                vec![("start_command", "echo changed".into())]
            ),
            &state
        )
        .unwrap_err()
        .category,
        "conflict"
    );
    let before = Instant::now();
    state
        .service_runtime()
        .stop_blocking(&id, Duration::from_millis(150))
        .unwrap();
    assert!(before.elapsed() >= Duration::from_millis(150));
    assert!(!state.service_runtime().is_active(&id));
    assert_eq!(
        state.service_runtime().status(&id).state,
        assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    );
}

#[test]
fn quitting_during_stop_cleans_up_the_owned_group() {
    let state = initialized_state("quit-stopping");
    let dir = temp_dir("quit-stopping-project");
    let id = create_local_service(&state, &dir, "trap '' TERM; while :; do sleep 1; done");
    let status = service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    service_runtime_stop_impl(&id, &state).unwrap();
    state.shutdown_service_runtime();
    assert!(!state.service_runtime().is_active(&id));
    assert_ne!(unsafe { libc::kill(status.pid.unwrap() as i32, 0) }, 0);
}

#[test]
fn leader_exit_cleans_up_surviving_subprocesses_before_reporting_stopped() {
    let state = initialized_state("leader-exit");
    let dir = temp_dir("leader-exit-project");
    let pid_file = dir.join("child.pid");
    let id = create_local_service(
        &state,
        &dir,
        &format!("sleep 60 & echo $! > {}; exit 0", pid_file.display()),
    );
    service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Stopped
    });
    let pid = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "orphaned descendant survived cleanup"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn stale_archive_has_no_runtime_side_effect_and_equal_launch_fields_stay_editable() {
    let state = initialized_state("stale-archive");
    let dir = temp_dir("stale-archive-project");
    let id = create_local_service(&state, &dir, "sleep 60");
    service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    let revision = library_get_impl(&id, &state).unwrap().revision;
    let updated = service_command_impl(
        update_command(
            &id,
            revision,
            vec![
                ("notes", "updated while running".into()),
                ("project_dir", format!(" {} ", dir.display())),
                ("start_command", " sleep 60 ".into()),
            ],
        ),
        &state,
    )
    .unwrap();
    let err = service_command_impl(
        ServiceCommandDto::Archive {
            asset_id: id.clone(),
            expected_revision: Some(revision),
        },
        &state,
    )
    .unwrap_err();
    assert_eq!(err.category, "stale_revision");
    assert_eq!(
        state.service_runtime().status(&id).state,
        assetmesh_desktop_lib::runtime::RuntimeState::Running
    );
    service_command_impl(
        ServiceCommandDto::Archive {
            asset_id: id.clone(),
            expected_revision: updated.revision,
        },
        &state,
    )
    .unwrap();
    assert!(!state.service_runtime().is_active(&id));
}

#[test]
fn merge_and_import_refuse_to_retire_or_replace_running_service_records() {
    use assetmesh_desktop_lib::commands::{
        merge_apply_impl, portable_export_impl, portable_import_apply_impl,
        portable_import_preview_impl,
    };
    use assetmesh_desktop_lib::dto::MergeApplyDto;
    let state = initialized_state("replacement-guard");
    let dir = temp_dir("replacement-guard-project");
    let loser = create_local_service(&state, &dir, "sleep 60");
    let winner = create_local_service(&state, &dir, "sleep 60");
    let winner_revision = library_get_impl(&winner, &state).unwrap().revision;
    let loser_revision = library_get_impl(&loser, &state).unwrap().revision;
    let bundle = dir.join("bundle");
    portable_export_impl(&bundle.to_string_lossy(), &state).unwrap();
    let fingerprint = portable_import_preview_impl(&bundle.to_string_lossy(), &state)
        .unwrap()
        .fingerprint;
    service_runtime_start_impl(&loser, &state).unwrap();
    let input = || MergeApplyDto {
        winner_id: winner.clone(),
        loser_id: loser.clone(),
        expected_winner_revision: Some(winner_revision),
        expected_loser_revision: Some(loser_revision),
    };
    assert_eq!(
        merge_apply_impl(input(), &state).unwrap_err().category,
        "conflict"
    );
    assert_eq!(
        portable_import_apply_impl(&bundle.to_string_lossy(), &fingerprint, &state)
            .unwrap_err()
            .category,
        "conflict"
    );
    assert_eq!(
        library_get_impl(&loser, &state).unwrap().lifecycle,
        "active"
    );
    assert!(state.service_runtime().is_active(&loser));
    state
        .service_runtime()
        .stop_blocking(&loser, Duration::from_secs(1))
        .unwrap();
    portable_import_apply_impl(&bundle.to_string_lossy(), &fingerprint, &state).unwrap();
    merge_apply_impl(input(), &state).unwrap();
    assert_eq!(
        library_get_impl(&loser, &state).unwrap().lifecycle,
        "merged"
    );
}

#[test]
fn log_cursor_resets_for_a_new_run_and_page_address_comes_from_canonical_record() {
    let state = initialized_state("log-restart");
    let dir = temp_dir("log-restart-project");
    let id = create_local_service(&state, &dir, "echo startup; sleep 60");
    service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    let first = service_runtime_logs_impl(&id, None, None, &state).unwrap();
    let run = first.run_id.unwrap();
    let cursor = first.lines.last().unwrap().seq;
    assert!(
        service_runtime_logs_impl(&id, Some(cursor), Some(&run), &state)
            .unwrap()
            .lines
            .is_empty()
    );
    state
        .service_runtime()
        .stop_blocking(&id, Duration::from_secs(1))
        .unwrap();
    service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| {
        s.state == assetmesh_desktop_lib::runtime::RuntimeState::Running
    });
    let next = service_runtime_logs_impl(&id, Some(cursor), Some(&run), &state).unwrap();
    assert_ne!(next.run_id.as_deref(), Some(run.as_str()));
    assert_eq!(next.lines.first().unwrap().text, "startup");
    let revision = library_get_impl(&id, &state).unwrap().revision;
    let mut update = update_command(&id, revision, vec![]);
    if let ServiceCommandDto::Update { endpoint_url, .. } = &mut update {
        *endpoint_url = Some("http://127.0.0.1:9911".into()).into();
    }
    service_command_impl(update, &state).unwrap();
    assert_eq!(
        assetmesh_desktop_lib::commands::service_page_url_impl(&id, &state).unwrap(),
        "http://127.0.0.1:9911/"
    );
    state
        .service_runtime()
        .stop_blocking(&id, Duration::from_secs(1))
        .unwrap();
}

#[test]
fn concurrent_start_and_archive_cannot_leave_a_running_archived_record() {
    let state = initialized_state("archive-race");
    let dir = temp_dir("archive-race-project");
    let id = create_local_service(&state, &dir, "sleep 60");
    let revision = library_get_impl(&id, &state).unwrap().revision;
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let _ = service_runtime_start_impl(&id, &state);
            });
        }
        scope.spawn(|| {
            service_command_impl(
                ServiceCommandDto::Archive {
                    asset_id: id.clone(),
                    expected_revision: Some(revision),
                },
                &state,
            )
            .unwrap();
        });
    });
    assert_eq!(library_get_impl(&id, &state).unwrap().lifecycle, "archived");
    assert!(!state.service_runtime().is_active(&id));
    assert!(service_runtime_start_impl(&id, &state).is_err());
}

#[cfg(unix)]
#[test]
fn restart_reaps_the_previous_process_before_starting_a_new_one() {
    let state = initialized_state("restart");
    let dir = temp_dir("restart-project");
    let id = create_local_service(&state, &dir, "echo restarted; sleep 60");
    let first = service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| s.state.as_str() == "running");
    let second = service_runtime_restart_impl(&id, &state).unwrap();
    assert_ne!(first.pid, second.pid);
    assert_ne!(first.started_at, second.started_at);
    // The previous leader is reaped, and cannot overlap the replacement.
    assert_eq!(unsafe { libc::kill(first.pid.unwrap() as i32, 0) }, -1);
    wait_for_status(&state, &id, |s| s.state.as_str() == "running");
    state
        .service_runtime()
        .stop_blocking(&id, Duration::from_secs(1))
        .unwrap();
}

/// Spawned only by the contract below, with a test-owned port in its environment.
#[cfg(unix)]
#[test]
#[ignore = "test-owned listener subprocess"]
fn runtime_external_listener_child() {
    let Ok(port) = std::env::var("ASSETMESH_TEST_SERVICE_PORT") else {
        return;
    };
    let _listener =
        std::net::TcpListener::bind(("127.0.0.1", port.parse::<u16>().unwrap())).unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(unix)]
struct TestListener(std::process::Child);
#[cfg(unix)]
impl Drop for TestListener {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[cfg(unix)]
fn outside_listener(port: u16) -> TestListener {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "runtime_external_listener_child", "--ignored"])
        .env("ASSETMESH_TEST_SERVICE_PORT", port.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let listener = TestListener(child);
    let deadline = Instant::now() + Duration::from_secs(3);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "test listener failed to start");
        std::thread::sleep(Duration::from_millis(20));
    }
    listener
}

#[cfg(unix)]
#[test]
fn explicit_stop_command_controls_an_existing_service_and_persists() {
    let dir = temp_dir("configured-stop");
    let project = temp_dir("configured-stop-project");
    let path = dir.join("library.db");
    let state = DesktopState::new();
    state.initialize(&path).unwrap();
    let port = free_local_port();
    let listener = outside_listener(port);
    let id = create_local_service_with_port(&state, &project, "sleep 60", Some(port));
    let command = format!("pwd > stop-cwd; kill -TERM {}", listener.0.id());
    service_command_impl(
        update_command(&id, 1, vec![("stop_command", command.clone())]),
        &state,
    )
    .unwrap();
    drop(state);
    let state = DesktopState::new();
    state.initialize(&path).unwrap();
    assert_eq!(
        serde_json::to_value(library_get_impl(&id, &state).unwrap().details).unwrap()
            ["stop_command"],
        command
    );
    assert_eq!(
        service_runtime_status_impl(&id, &state)
            .unwrap()
            .state
            .as_str(),
        "external"
    );
    service_runtime_stop_impl(&id, &state).unwrap();
    assert_eq!(
        service_runtime_status_impl(&id, &state)
            .unwrap()
            .state
            .as_str(),
        "stopped"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("stop-cwd"))
            .unwrap()
            .trim(),
        project.canonicalize().unwrap().to_string_lossy()
    );
    // The service's start command was never executed during Stop.
    assert!(!state.service_runtime().holds(&id));
}

#[cfg(unix)]
#[test]
fn failed_or_ineffective_stop_commands_never_launch_a_replacement() {
    let state = initialized_state("failed-stop");
    let dir = temp_dir("failed-stop-project");
    let port = free_local_port();
    let _listener = outside_listener(port);
    let id = create_local_service_with_port(
        &state,
        &dir,
        "touch incorrectly-started; sleep 60",
        Some(port),
    );
    // No inferred kill operation: an existing service needs an explicit command.
    assert!(service_runtime_stop_impl(&id, &state)
        .unwrap_err()
        .message
        .contains("Configure a stop command"));
    service_command_impl(
        update_command(
            &id,
            1,
            vec![("stop_command", "echo deliberate-failure >&2; exit 7".into())],
        ),
        &state,
    )
    .unwrap();
    let error = service_runtime_restart_impl(&id, &state).unwrap_err();
    assert!(error.message.contains("7"));
    assert!(!dir.join("incorrectly-started").exists());
    assert_eq!(
        service_runtime_status_impl(&id, &state)
            .unwrap()
            .state
            .as_str(),
        "external"
    );
    // Exit 0 alone cannot authorize a restart while the address still answers.
    service_command_impl(
        update_command(&id, 2, vec![("stop_command", "true".into())]),
        &state,
    )
    .unwrap();
    assert!(service_runtime_restart_impl(&id, &state).is_err());
    assert!(!dir.join("incorrectly-started").exists());
}

#[cfg(unix)]
#[test]
fn restart_of_an_existing_service_uses_the_bound_stop_then_start_commands() {
    let state = initialized_state("outside-restart");
    let dir = temp_dir("outside-restart-project");
    let port = free_local_port();
    let listener = outside_listener(port);
    let id = create_local_service_with_port(&state, &dir, "echo new-run; sleep 60", Some(port));
    service_command_impl(
        update_command(
            &id,
            1,
            vec![("stop_command", format!("kill -TERM {}", listener.0.id()))],
        ),
        &state,
    )
    .unwrap();
    let status = service_runtime_restart_impl(&id, &state).unwrap();
    assert!(status.pid.is_some());
    wait_for_status(&state, &id, |s| s.state.as_str() == "running");
    // Clean up through the owned registry; the test-only custom command refers
    // to the original external PID and must not be re-executed for cleanup.
    state
        .service_runtime()
        .stop_blocking(&id, Duration::from_secs(1))
        .unwrap();
}

#[cfg(unix)]
#[test]
fn a_configured_stop_is_classified_as_an_intentional_exit_and_is_frozen_while_running() {
    let state = initialized_state("owned-custom-stop");
    let dir = temp_dir("owned-custom-stop-project");
    let id = create_local_service(&state, &dir, "echo $$ > owned-pid; exec sleep 60");
    service_command_impl(
        update_command(
            &id,
            1,
            vec![("stop_command", "kill -TERM \"$(cat owned-pid)\"".into())],
        ),
        &state,
    )
    .unwrap();
    service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| s.state.as_str() == "running");
    let error = service_command_impl(
        update_command(&id, 2, vec![("stop_command", "true".into())]),
        &state,
    )
    .unwrap_err();
    assert_eq!(error.category, "conflict");
    service_runtime_stop_impl(&id, &state).unwrap();
    let stopped = wait_for_status(&state, &id, |s| s.state.as_str() == "stopped");
    assert!(
        stopped.error.is_none(),
        "an intentional stop must not appear as a failure"
    );
}

#[cfg(unix)]
#[test]
fn a_hung_stop_command_is_bounded_without_killing_the_running_service() {
    let state = initialized_state("hung-stop");
    let dir = temp_dir("hung-stop-project");
    let id = create_local_service(&state, &dir, "sleep 60");
    service_command_impl(
        update_command(
            &id,
            1,
            vec![(
                "stop_command",
                "echo $$ > stop-command-pid; sleep 60".into(),
            )],
        ),
        &state,
    )
    .unwrap();
    let started = service_runtime_start_impl(&id, &state).unwrap();
    wait_for_status(&state, &id, |s| s.state.as_str() == "running");
    let before = Instant::now();
    let error = service_runtime_stop_impl(&id, &state).unwrap_err();
    assert!(error.message.contains("timed out"));
    assert!(before.elapsed() < Duration::from_secs(15));
    assert!(state.service_runtime().is_active(&id));
    assert_eq!(unsafe { libc::kill(started.pid.unwrap() as i32, 0) }, 0);
    let control_pid = std::fs::read_to_string(dir.join("stop-command-pid"))
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert_eq!(
        unsafe { libc::kill(control_pid, 0) },
        -1,
        "the hung control command must be cleaned up"
    );
    state
        .service_runtime()
        .stop_blocking(&id, Duration::from_secs(1))
        .unwrap();
}
