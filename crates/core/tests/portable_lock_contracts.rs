//! Multi-process and advisory lock contract tests for `BundleLock`.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use assetmesh_core::application::portable::{lock_path, with_bundle_lock, BundleLock};
use assetmesh_core::error::AppError;

/// Hidden worker test invoked as a separate process by parent tests.
#[test]
fn child_lock_worker() {
    let Ok(mode) = std::env::var("ASSETMESH_LOCK_CHILD") else {
        return; // Skipped during normal test runner pass
    };
    let target_str = std::env::var("ASSETMESH_LOCK_TARGET").expect("ASSETMESH_LOCK_TARGET");
    let target = PathBuf::from(target_str);

    match mode.as_str() {
        "hold" => {
            let _lock = BundleLock::acquire(&target).expect("child acquire lock");
            println!("ACQUIRED");
            std::io::stdout().flush().unwrap();

            // Wait on stdin or until killed
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
        }
        "try" => match BundleLock::acquire(&target) {
            Ok(_lock) => {
                println!("ACQUIRED");
                std::io::stdout().flush().unwrap();
                std::process::exit(0);
            }
            Err(AppError::StorageBusy { .. }) => {
                println!("BUSY");
                std::io::stdout().flush().unwrap();
                std::process::exit(42);
            }
            Err(err) => {
                eprintln!("UNEXPECTED ERROR: {err}");
                std::process::exit(1);
            }
        },
        other => panic!("Unknown child mode: {other}"),
    }
}

#[test]
fn test_lock_file_location_is_sibling_and_persists_on_drop() {
    let dir = std::env::temp_dir().join(format!("assetmesh-lockloc-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&dir).unwrap();
    let bundle_dir = dir.join("my_bundle");
    fs::create_dir_all(&bundle_dir).unwrap();

    let expected_lock = dir.join(".my_bundle.lock");
    assert_eq!(lock_path(&bundle_dir), expected_lock);

    // Acquire lock and drop it
    {
        let lock = BundleLock::acquire(&bundle_dir).expect("acquire");
        assert_eq!(lock.path(), &expected_lock);
        assert!(expected_lock.exists(), "lock file must exist on disk");
    }

    // Crucial safety contract: dropping the lock MUST NOT delete the lock file on disk,
    // because unlinking an advisory lock breaks race serialization across concurrent openers.
    assert!(
        expected_lock.exists(),
        "lock file must NOT be deleted upon drop"
    );

    // Can be acquired again immediately
    {
        let _lock2 = BundleLock::acquire(&bundle_dir).expect("re-acquire");
    }

    fs::remove_dir_all(&dir).ok();
    fs::remove_file(&expected_lock).ok();
}

#[test]
fn test_cross_process_lock_contention_and_automatic_os_release_on_crash() {
    let dir = std::env::temp_dir().join(format!("assetmesh-xproc-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&dir).unwrap();
    let bundle_dir = dir.join("bundle_data");
    fs::create_dir_all(&bundle_dir).unwrap();

    let exe = std::env::current_exe().expect("current_exe");

    // Spawn child holding the lock
    let mut child = Command::new(&exe)
        .arg("child_lock_worker")
        .arg("--nocapture")
        .arg("--exact")
        .env("ASSETMESH_LOCK_CHILD", "hold")
        .env("ASSETMESH_LOCK_TARGET", bundle_dir.to_str().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child worker");

    // Wait until child signals it has acquired the lock
    let stdout = child.stdout.take().expect("child stdout");
    let reader = BufReader::new(stdout);
    let mut acquired = false;
    for line in reader.lines() {
        let l = line.expect("read child line");
        if l.trim() == "ACQUIRED" {
            acquired = true;
            break;
        }
    }
    assert!(acquired, "child must acquire lock and signal ACQUIRED");

    // Parent attempts to acquire the lock while child holds it -> MUST fail with StorageBusy
    let acquire_result = BundleLock::acquire(&bundle_dir);
    match acquire_result {
        Err(AppError::StorageBusy { message }) => {
            assert!(
                message.contains("another process is using the bundle"),
                "got message: {message}"
            );
        }
        other => panic!("expected AppError::StorageBusy, got: {other:?}"),
    }

    // Simulate child process exit / crash by killing it
    child.kill().expect("kill child process");
    let _ = child.wait();

    // OS advisory lock must be automatically released on process termination.
    // Parent must now acquire the lock immediately without error!
    let parent_lock = BundleLock::acquire(&bundle_dir)
        .expect("parent acquire lock after child process crash/exit");

    assert!(parent_lock.path().exists());
    drop(parent_lock);

    // Verify with_bundle_lock runs closure
    let closure_ran = with_bundle_lock(&bundle_dir, || Ok(42)).expect("with_bundle_lock");
    assert_eq!(closure_ran, 42);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_two_concurrent_child_processes_competing_for_lock() {
    let dir = std::env::temp_dir().join(format!("assetmesh-2child-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&dir).unwrap();
    let bundle_dir = dir.join("bundle_contention");
    fs::create_dir_all(&bundle_dir).unwrap();

    let exe = std::env::current_exe().expect("current_exe");

    // 1. Start child 1 holding the lock
    let mut child1 = Command::new(&exe)
        .arg("child_lock_worker")
        .arg("--nocapture")
        .arg("--exact")
        .env("ASSETMESH_LOCK_CHILD", "hold")
        .env("ASSETMESH_LOCK_TARGET", bundle_dir.to_str().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child 1");

    let stdout1 = child1.stdout.take().expect("child1 stdout");
    let reader1 = BufReader::new(stdout1);
    let mut acquired1 = false;
    for line in reader1.lines() {
        let l = line.expect("read line");
        if l.trim() == "ACQUIRED" {
            acquired1 = true;
            break;
        }
    }
    assert!(acquired1);

    // 2. Start child 2 in 'try' mode; it must exit with code 42 (BUSY)
    let child2_status = Command::new(&exe)
        .arg("child_lock_worker")
        .arg("--nocapture")
        .arg("--exact")
        .env("ASSETMESH_LOCK_CHILD", "try")
        .env("ASSETMESH_LOCK_TARGET", bundle_dir.to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .status()
        .expect("run child 2");

    assert_eq!(
        child2_status.code(),
        Some(42),
        "child 2 must exit with code 42 when lock is busy"
    );

    // 3. Terminate child 1
    child1.kill().expect("kill child 1");
    let _ = child1.wait();

    // 4. Start child 3 in 'try' mode; lock is now free, child 3 must succeed with code 0
    let child3_status = Command::new(&exe)
        .arg("child_lock_worker")
        .arg("--nocapture")
        .arg("--exact")
        .env("ASSETMESH_LOCK_CHILD", "try")
        .env("ASSETMESH_LOCK_TARGET", bundle_dir.to_str().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .status()
        .expect("run child 3");

    assert_eq!(
        child3_status.code(),
        Some(0),
        "child 3 must exit with code 0 once lock was released"
    );

    fs::remove_dir_all(&dir).ok();
}
