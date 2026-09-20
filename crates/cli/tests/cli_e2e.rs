//! End-to-end test driving the real `assetmesh` binary through the full
//! Phase 1 lifecycle: fresh DB -> migrations -> create -> update -> complete
//! -> search -> export -> re-import -> verify.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "assetmesh-e2e-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &std::path::Path, db: &str, args: &[&str]) -> (String, String, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_assetmesh"))
        .arg("--db")
        .arg(dir.join(db))
        .args(args)
        .output()
        .expect("binary should run");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    )
}

#[test]
fn full_lifecycle_works_end_to_end() {
    let dir = unique_dir("lifecycle");
    let db = "e2e.db";

    // 1. add media
    let (stdout, _, ok) = run(
        &dir,
        db,
        &[
            "media",
            "add",
            "--title",
            "Sousou no Frieren",
            "--media-type",
            "anime",
            "--year",
            "2023",
            "--rating",
            "9.5",
            "--progress-current",
            "18",
            "--progress-total",
            "28",
            "--progress-unit",
            "episode",
            "--tag",
            "healing",
            "--ref",
            "tmdb:209867",
        ],
    );
    assert!(ok, "add should succeed");
    let id = stdout
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("created ")
        .trim()
        .to_string();

    // 2. start + progress
    let (out, _, ok) = run(&dir, db, &["media", "start", &id]);
    assert!(ok, "start failed: {out}");
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "media",
            "progress",
            &id,
            "--current",
            "28",
            "--total",
            "28",
            "--unit",
            "episode",
        ],
    );
    assert!(ok, "progress failed: {err}");

    // 3. complete + rate
    let (_, _, ok) = run(&dir, db, &["media", "complete", &id]);
    assert!(ok);
    let (_, _, ok) = run(&dir, db, &["media", "rate", &id, "--rating", "9.8"]);
    assert!(ok);

    // 4. get shows completed status
    let (out, _, ok) = run(&dir, db, &["media", "get", &id]);
    assert!(ok);
    assert!(out.contains("Status:   completed"), "detail: {out}");
    assert!(out.contains("media.completed"));

    // 5. search finds it by title and external ref
    let (out, _, ok) = run(&dir, db, &["media", "search", "frieren"]);
    assert!(ok && out.contains("Sousou no Frieren"), "search: {out}");
    let (out, _, _) = run(&dir, db, &["media", "search", "tmdb:209867"]);
    assert!(out.contains("Sousou no Frieren"), "ref search: {out}");

    // 6. list with filter
    let (out, _, ok) = run(
        &dir,
        db,
        &["media", "list", "--type", "anime", "--status", "completed"],
    );
    assert!(ok && out.contains("Sousou no Frieren"), "list: {out}");

    // 7. archive + merge flow: add a duplicate and merge it away
    let (dup_out, _, ok) = run(
        &dir,
        db,
        &[
            "media",
            "add",
            "--title",
            "frieren duplicate",
            "--media-type",
            "anime",
            "--ref",
            "igdb:999",
        ],
    );
    assert!(ok);
    let dup_id = dup_out
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("created ")
        .trim()
        .to_string();
    let (_, err, ok) = run(&dir, db, &["asset", "merge", &dup_id, &id]);
    assert!(ok, "merge failed: {err}");

    // merged asset rejects mutation
    let (_, err, ok) = run(&dir, db, &["media", "rate", &dup_id, "--rating", "1.0"]);
    assert!(!ok, "merged asset must reject mutations");
    assert!(err.contains("merged"), "error: {err}");

    // 8. portable export + restore into a fresh DB
    let export_dir = dir.join("bundle");
    // export takes a positional dir
    let (out, err, ok) = run(&dir, db, &["export", export_dir.to_str().unwrap()]);
    assert!(ok, "export failed: {err} {out}");
    assert!(export_dir.join("manifest.json").exists());

    let (out, err, ok) = run(
        &dir,
        "restored.db",
        &["import", export_dir.to_str().unwrap()],
    );
    assert!(ok, "restore failed: {err} {out}");
    assert!(out.contains("assets: 2 created"), "report: {out}");

    // 9. restored library is fully usable: same id resolves to same title
    let (out, _, _) = run(&dir, "restored.db", &["media", "get", &id]);
    assert!(out.contains("Sousou no Frieren"));
    assert!(out.contains("Status:   completed"));

    // 10. restore is idempotent
    let (out, _, ok) = run(
        &dir,
        "restored.db",
        &["import", export_dir.to_str().unwrap()],
    );
    assert!(ok);
    assert!(out.contains("assets: 0 created"), "second restore: {out}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_import_with_dry_run_and_conflicts() {
    let dir = unique_dir("import");
    let json_path = dir.join("legacy.json");
    std::fs::write(
        &json_path,
        r#"[
        {"title": "Perfect Blue", "media_type": "movie", "year": 1997, "rating": 8.5, "status": "completed", "external_refs": {"tmdb": "10493"}},
        {"title": "Broken", "media_type": "hologram"},
        {"title": "perfect blue", "media_type": "movie", "year": 1999}
    ]"#,
    )
    .unwrap();

    // dry run writes nothing
    let (out, _, ok) = run(
        &dir,
        "dry.db",
        &["media", "import", json_path.to_str().unwrap(), "--dry-run"],
    );
    assert!(ok, "dry run failed");
    assert!(out.contains("Would create:         1"), "report: {out}");
    assert!(out.contains("Potential duplicates: 1"));
    assert!(out.contains("Rejected:             1"));
    let (_, _, ok) = run(&dir, "dry.db", &["media", "list"]);
    assert!(ok);
    // (no media records) — via stderr/stdout
    let (list_out, _, _) = run(&dir, "dry.db", &["media", "list"]);
    assert!(list_out.contains("(no media records)"), "list: {list_out}");

    // real run creates one, reports one conflict, rejects one
    let (out, _, ok) = run(
        &dir,
        "real.db",
        &["media", "import", json_path.to_str().unwrap()],
    );
    assert!(ok);
    assert!(out.contains("Created:              1"));
    assert!(out.contains("Potential duplicates: 1"));

    // rerun is idempotent (matched by external ref, nothing changes)
    let (out, _, ok) = run(
        &dir,
        "real.db",
        &["media", "import", json_path.to_str().unwrap()],
    );
    assert!(ok);
    assert!(out.contains("Created:              0"));
    assert!(out.contains("Unchanged:            1"));

    // heuristic conflict was NOT silently applied: the year-1999 variant is
    // neither created nor merged — exactly the one imported record remains
    let (list_out, _, _) = run(&dir, "real.db", &["media", "list"]);
    assert!(list_out.contains("Perfect Blue"));
    assert!(
        list_out.contains("1 record(s)"),
        "heuristic match must not auto-apply: {list_out}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn error_messages_are_actionable() {
    let dir = unique_dir("errors");

    let (_, err, ok) = run(&dir, "err.db", &["media", "get", "not-a-uuid"]);
    assert!(!ok);
    assert!(err.contains("error:"), "stderr: {err}");

    // create an asset then reference a short ambiguous prefix

    let (_, err, ok) = run(
        &dir,
        "err.db",
        &["media", "rate", "zzzzzz", "--rating", "5"],
    );
    assert!(!ok);
    assert!(err.contains("not found"), "stderr: {err}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unicode_titles_never_panic_the_cli() {
    let dir = unique_dir("unicode");

    for title in [
        "A一二三四五六七八九十B二十三四五六七八九十C", // mixed ASCII/CJK, > display width
        "命运的命运命运的命运命运的命运命运的命运命运", // long CJK
        "🚀🛰️🛸🪐🌟⭐💫✨☄️🌌🌠",                      // emoji / multi-byte
    ] {
        let (_, err, ok) = run(
            &dir,
            "u.db",
            &["media", "add", "--title", title, "--media-type", "movie"],
        );
        assert!(ok, "add failed for {title}: {err}");

        let (list_out, _, ok) = run(&dir, "u.db", &["media", "list"]);
        assert!(ok, "list must not panic on unicode titles: {err}");
        assert!(!list_out.is_empty());

        // Search a mid-title substring: exercises the substring fallback on
        // multi-byte text that tokenization cannot serve.
        let chars: Vec<char> = title.chars().collect();
        let query: String = chars[3..7].iter().collect();
        let (search_out, _, ok) = run(&dir, "u.db", &["media", "search", &query]);
        assert!(ok, "search failed: {err}");
        assert!(
            search_out.contains(title),
            "substring search must find {title} via {query:?}: {search_out}"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Phase 2 — Software inventory end-to-end
// ---------------------------------------------------------------------------

use std::path::Path;

/// Writes a fixture .app bundle whose Info.plist is standard XML.
fn write_app_fixture(root: &Path, name: &str, bundle_id: &str, version: &str) {
    let bundle = root.join(format!("{name}.app"));
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>{bundle_id}</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleExecutable</key><string>{name}-bin</string>
</dict></plist>"#
    );
    std::fs::write(bundle.join("Contents").join("Info.plist"), plist).unwrap();
}

#[test]
fn software_lifecycle_and_discovery_work_end_to_end() {
    let dir = unique_dir("software");
    let db = "software.db";

    // 1. manual add
    let (stdout, _, ok) = run(
        &dir,
        db,
        &[
            "software",
            "add",
            "--name",
            "ripgrep",
            "--category",
            "cli",
            "--install-source",
            "homebrew-formula",
            "--version",
            "14.1.0",
            "--purpose",
            "fast search",
            "--tag",
            "dev",
            "--ref",
            "homebrew_formula:ripgrep",
        ],
    );
    assert!(ok);
    let id = stdout
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("created ")
        .trim()
        .to_string();

    // 2. get + list
    let (out, _, ok) = run(&dir, db, &["software", "get", &id]);
    assert!(ok);
    assert!(out.contains("ripgrep"), "{out}");
    assert!(out.contains("homebrew_formula"));
    assert!(out.contains("fast search"));
    assert!(out.contains("software.created"));

    let (out, _, ok) = run(
        &dir,
        db,
        &["software", "list", "--category", "cli", "--json"],
    );
    assert!(ok);
    assert!(out.contains("ripgrep"), "{out}");

    // 3. update metadata
    let (_, _, ok) = run(
        &dir,
        db,
        &["software", "update", &id, "--purpose", "faster search"],
    );
    assert!(ok);

    // 4. search finds software by name, purpose, and ref
    let (out, _, ok) = run(&dir, db, &["software", "search", "faster search"]);
    assert!(ok && out.contains("ripgrep"), "search: {out}");
    let (out, _, _) = run(
        &dir,
        db,
        &["software", "search", "homebrew_formula:ripgrep"],
    );
    assert!(out.contains("ripgrep"), "ref search: {out}");

    // 5. discovery against a fixture root (host /Applications is untouched)
    let apps_dir = dir.join("apps");
    write_app_fixture(&apps_dir, "FixtureStudio", "com.fixture.studio", "3.1.4");

    let (out, err, ok) = run(
        &dir,
        db,
        &[
            "software",
            "discover",
            "macos",
            "--root",
            apps_dir.to_str().unwrap(),
        ],
    );
    assert!(ok, "discover failed: {err}");
    assert!(out.contains("macos_applications"), "{out}");
    assert!(out.contains("[new] FixtureStudio"), "{out}");
    assert!(out.contains("bundle_id:com.fixture.studio"), "{out}");
    assert!(out.contains("not canonical assets"), "{out}");

    // 6. adopt the candidate (re-scan + adopt in one flow)
    let (out, err, ok) = run(
        &dir,
        db,
        &[
            "software",
            "adopt",
            "macos",
            "bundle_id:com.fixture.studio",
            "--root",
            apps_dir.to_str().unwrap(),
            "--purpose",
            "fixture testing",
        ],
    );
    assert!(ok, "adopt failed: {err}");
    assert!(out.contains("adopted candidate as"), "{out}");
    assert!(out.contains("disposition: new"), "{out}");
    let adopted_id = out
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("adopted candidate as ")
        .split(" ")
        .next()
        .unwrap()
        .to_string();

    let (out, _, ok) = run(&dir, db, &["software", "get", &adopted_id]);
    assert!(ok);
    assert!(out.contains("FixtureStudio"), "{out}");
    assert!(out.contains("3.1.4"), "{out}");
    assert!(out.contains("fixture testing"), "{out}");
    assert!(out.contains("software.adopted"), "{out}");

    // 7. adopting the same candidate again updates the existing record
    let (out, _, ok) = run(
        &dir,
        db,
        &[
            "software",
            "adopt",
            "macos",
            "bundle_id:com.fixture.studio",
            "--root",
            apps_dir.to_str().unwrap(),
        ],
    );
    assert!(ok);
    assert!(out.contains("disposition: exact_match"), "{out}");
    assert!(out.contains("updated"), "{out}");

    // 8. relations
    let (_, err, ok) = run(
        &dir,
        db,
        &["relation", "add", &id, "uses", &adopted_id, "--note", "e2e"],
    );
    assert!(ok, "relation add failed: {err}");
    let (out, _, ok) = run(&dir, db, &["relation", "list", &adopted_id]);
    assert!(ok);
    assert!(out.contains("used_by"), "{out}");
    assert!(out.contains("ripgrep"), "{out}");

    // 9. export + restore into a fresh DB keeps software + relations
    let export_dir = dir.join("bundle2");
    let (out, err, ok) = run(&dir, db, &["export", export_dir.to_str().unwrap()]);
    assert!(ok, "export failed: {err} {out}");
    assert!(export_dir.join("modules/software.jsonl").exists());
    assert!(export_dir.join("relations.jsonl").exists());

    let (out, err, ok) = run(
        &dir,
        "software-restored.db",
        &["import", export_dir.to_str().unwrap()],
    );
    assert!(ok, "restore failed: {err} {out}");
    assert!(out.contains("software: 2 created"), "{out}");

    let (out, _, ok) = run(
        &dir,
        "software-restored.db",
        &["software", "get", &adopted_id],
    );
    assert!(ok);
    assert!(out.contains("FixtureStudio"), "{out}");

    let (out, _, ok) = run(
        &dir,
        "software-restored.db",
        &["relation", "list", &adopted_id],
    );
    assert!(ok);
    assert!(out.contains("used_by"), "{out}");
    assert!(out.contains("ripgrep"), "{out}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn discovery_never_mutates_state_and_bad_selectors_fail_cleanly() {
    let dir = unique_dir("discover-safe");
    let db = "safe.db";

    let apps_dir = dir.join("apps");
    write_app_fixture(&apps_dir, "OnlyApp", "com.fixture.only", "1.0");

    // Discover: read-only.
    let (out, _, ok) = run(
        &dir,
        db,
        &[
            "software",
            "discover",
            "macos",
            "--root",
            apps_dir.to_str().unwrap(),
        ],
    );
    assert!(ok);
    assert!(out.contains("[new] OnlyApp"), "{out}");

    let (_, _, ok) = run(&dir, db, &["software", "list"]);
    assert!(ok);
    let (list_out, _, _) = run(&dir, db, &["software", "list"]);
    assert!(list_out.contains("(no software records)"), "{list_out}");

    // Unknown candidate selector fails with a clear error.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "software",
            "adopt",
            "macos",
            "bundle_id:com.fixture.absent",
            "--root",
            apps_dir.to_str().unwrap(),
        ],
    );
    assert!(!ok);
    assert!(err.contains("not found"), "{err}");

    // Unknown provider fails with guidance.
    let (_, err, ok) = run(&dir, db, &["software", "discover", "registry"]);
    assert!(!ok);
    assert!(err.contains("unknown discovery provider"), "{err}");

    std::fs::remove_dir_all(&dir).ok();
}
