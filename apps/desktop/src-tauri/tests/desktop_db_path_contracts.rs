//! Where the desktop app looks for its database, and what it says when that fails.
//!
//! A bundled `.app` opened from a file manager runs with the process working
//! directory at `/`, which is read-only on macOS, so any CWD-relative default is
//! unopenable for every real user of the packaged app.

use std::path::{Path, PathBuf};

use assetmesh_desktop_lib::resolve_db_path;
use assetmesh_desktop_lib::state::{AppStatus, DesktopState};

#[test]
fn default_database_is_absolute_under_the_data_dir() {
    let resolved = resolve_db_path(None, Path::new("/var/lib/assetmesh"));
    assert!(
        resolved.is_absolute(),
        "a relative default resolves against the process CWD: {resolved:?}"
    );
    assert_eq!(resolved, PathBuf::from("/var/lib/assetmesh/assetmesh.db"));
}

#[test]
fn environment_override_wins() {
    assert_eq!(
        resolve_db_path(Some("/tmp/other.db"), Path::new("/var/lib/assetmesh")),
        PathBuf::from("/tmp/other.db")
    );
}

#[test]
fn blank_override_falls_back_to_the_data_dir() {
    for blank in ["", "   "] {
        assert_eq!(
            resolve_db_path(Some(blank), Path::new("/var/lib/assetmesh")),
            PathBuf::from("/var/lib/assetmesh/assetmesh.db"),
            "blank {blank:?} must not become an empty relative path"
        );
    }
}

#[test]
fn setup_failure_stays_actionable_without_leaking_the_path() {
    // A regular file standing where a parent directory should be: the open fails
    // for every user, without depending on filesystem permissions.
    let root = std::env::temp_dir().join(format!("assetmesh-not-a-dir-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    let blocker = root.join("blocker");
    std::fs::write(&blocker, b"x").expect("blocker file");
    let db_path = blocker.join("assetmesh.db");

    let state = DesktopState::new();
    let status = state
        .initialize(&db_path)
        .expect("initialize reports a status, not an error");

    let message = match status {
        AppStatus::SetupFailure { message } => message,
        other => panic!("expected a setup failure, got {other:?}"),
    };
    let serialized = serde_json::to_string(&state.get_status()).unwrap();

    assert!(
        message.contains("ASSETMESH_DB"),
        "the card must tell the user what to do next, got: {message}"
    );
    assert!(
        !serialized.contains(&blocker.to_string_lossy().to_string()),
        "the database path must not cross the IPC boundary: {serialized}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_failed_initialize_recovers_when_a_retry_opens_a_valid_file() {
    // `app_init` exists so the setup card can recover: the same state object must
    // reach Ready on a later success instead of staying in the failure state.
    let root = std::env::temp_dir().join(format!("assetmesh-retry-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    let blocker = root.join("blocker");
    std::fs::write(&blocker, b"x").expect("blocker file");

    let state = DesktopState::new();
    let failed = state
        .initialize(&blocker.join("assetmesh.db"))
        .expect("initialize reports a status, not an error");
    assert!(
        matches!(failed, AppStatus::SetupFailure { .. }),
        "first attempt must fail, got {failed:?}"
    );
    assert!(state.modules().is_err());
    // `app_init` with no argument retries whatever the launch attempted, so the
    // state has to remember it even though that attempt failed.
    assert_eq!(
        state.default_db_path().as_deref(),
        Some(blocker.join("assetmesh.db").as_path())
    );

    let retried = state
        .initialize(&root.join("assetmesh.db"))
        .expect("retry reports a status, not an error");
    assert!(
        matches!(retried, AppStatus::Ready { .. }),
        "retry must reach Ready, got {retried:?}"
    );
    assert!(state.modules().is_ok());
    assert_eq!(
        state.default_db_path().as_deref(),
        Some(root.join("assetmesh.db").as_path())
    );

    let _ = std::fs::remove_dir_all(&root);
}
