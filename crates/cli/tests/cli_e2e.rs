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
