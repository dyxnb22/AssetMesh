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

#[test]
fn service_lifecycle_works_end_to_end() {
    let dir = unique_dir("service");
    let db = "e2e.db";
    let bin = || run(&dir, db, &[]);

    // A fresh DB creates no services and no service tables yet.
    let (out, _, ok) = run(&dir, db, &["service", "list"]);
    assert!(ok, "list on an empty DB should succeed");
    assert!(out.contains("(no service records)"), "{out}");
    let _ = bin();

    // 1. add a full SaaS subscription. Money is given as a decimal on the CLI
    // and stored as integer minor units (ADR 0010).
    let (out, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "OpenAI",
            "--type",
            "saas",
            "--provider",
            "OpenAI",
            "--account-label",
            "Work",
            "--endpoint",
            "https://api.openai.com/v1",
            "--dashboard",
            "https://platform.openai.com",
            "--plan",
            "Plus",
            "--cost",
            "19.99",
            "--currency",
            "USD",
            "--billing",
            "monthly",
            "--renews-at",
            "2026-10-01",
            "--auto-renew",
            "true",
            "--notes",
            "Team plan",
            "--tag",
            "ai",
            "--tag",
            "billing",
            "--ref",
            "openai:org-work",
        ],
    );
    assert!(ok, "add should succeed: {err}");
    assert!(out.starts_with("created "), "{out}");
    assert!(out.contains("service.saas"), "{out}");
    assert!(
        out.contains("USD 19.00/month") || out.contains("USD 19.99"),
        "{out}"
    );
    let openai_id = out
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("created ")
        .trim()
        .to_string();

    // The detail view shows every written field.
    let (out, _, ok) = run(&dir, db, &["service", "get", &openai_id]);
    assert!(ok);
    assert!(out.contains("Kind:          service.saas"), "{out}");
    assert!(out.contains("Provider:      OpenAI"), "{out}");
    assert!(out.contains("Plan:          Plus"), "{out}");
    assert!(out.contains("USD 19.99"), "{out}");
    assert!(out.contains("Auto-renew:    on"), "{out}");
    assert!(out.contains("openai:org-work"), "{out}");
    assert!(out.contains("service.created"), "{out}");

    // 2. A second service of a different type, without billing data.
    let (out, _, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "Hetzner",
            "--type",
            "vps",
            "--provider",
            "Hetzner",
            "--plan",
            "CX22",
        ],
    );
    assert!(ok, "second add should succeed: {out}");
    let hetzner_id = out
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("created ")
        .trim()
        .to_string();
    assert_ne!(openai_id, hetzner_id);

    // 3. list shows both, newest first.
    let (out, _, ok) = run(&dir, db, &["service", "list"]);
    assert!(ok);
    assert!(out.contains("OpenAI"), "{out}");
    assert!(out.contains("Hetzner"), "{out}");
    assert!(out.contains("2 record(s)"), "{out}");
    assert!(
        out.contains("service.saas") || out.contains("saas"),
        "{out}"
    );

    // Typed filters work: by type and by provider substring.
    let (out, _, ok) = run(&dir, db, &["service", "list", "--type", "vps"]);
    assert!(ok);
    assert!(out.contains("Hetzner") && !out.contains("OpenAI"), "{out}");
    assert!(out.contains("1 record(s)"), "{out}");

    let (out, _, ok) = run(&dir, db, &["service", "list", "--provider", "openai"]);
    assert!(ok);
    assert!(out.contains("OpenAI") && !out.contains("Hetzner"), "{out}");

    let (out, _, ok) = run(&dir, db, &["service", "list", "--tag", "ai"]);
    assert!(ok);
    assert!(out.contains("OpenAI") && !out.contains("Hetzner"), "{out}");

    // JSON output is machine-readable and money stays an integer.
    let (out, _, ok) = run(&dir, db, &["service", "list", "--json"]);
    assert!(ok);
    assert!(out.contains("\"cost_minor\": 1999"), "{out}");
    assert!(out.contains("\"currency\": \"USD\""), "{out}");
    assert!(out.contains("\"kind\": \"service.saas\""), "{out}");

    // 4. update with explicit patch semantics: set the plan, leave the cost.
    let (out, err, ok) = run(
        &dir,
        db,
        &["service", "update", &openai_id, "--plan", "Pro"],
    );
    assert!(ok, "update should succeed: {err}");
    assert!(out.contains("updated "), "{out}");

    let (out, _, ok) = run(&dir, db, &["service", "get", &openai_id]);
    assert!(ok);
    assert!(out.contains("Plan:          Pro"), "{out}");
    assert!(out.contains("USD 19.99"), "cost must be untouched: {out}");

    // An empty value clears a text field; other fields are untouched.
    let (out, _, ok) = run(&dir, db, &["service", "update", &openai_id, "--notes", ""]);
    assert!(ok, "{out}");
    let (out, _, ok) = run(&dir, db, &["service", "get", &openai_id]);
    assert!(ok);
    assert!(!out.contains("Team plan"), "{out}");
    assert!(out.contains("Plan:          Pro"), "{out}");

    // Money must be set or cleared as a pair.
    let (_, err, ok) = run(
        &dir,
        db,
        &["service", "update", &openai_id, "--cost", "29.99"],
    );
    assert!(!ok, "cost without currency must fail");
    assert!(
        err.contains("together") || err.contains("currency"),
        "the error must name the pairing rule: {err}"
    );

    // Clearing the pair works and takes both fields.
    let (out, _, ok) = run(
        &dir,
        db,
        &[
            "service",
            "update",
            &openai_id,
            "--clear-cost",
            "--clear-billing",
        ],
    );
    assert!(ok, "{out}");
    let (out, _, ok) = run(&dir, db, &["service", "get", &openai_id]);
    assert!(ok);
    assert!(!out.contains("USD 19.99"), "{out}");
    assert!(out.contains("Billing:       -"), "{out}");

    // Setting them again works, parsed from a decimal.
    let (out, _, ok) = run(
        &dir,
        db,
        &[
            "service",
            "update",
            &openai_id,
            "--cost",
            "100.50",
            "--currency",
            "EUR",
            "--billing",
            "yearly",
        ],
    );
    assert!(ok, "{out}");
    let (out, _, ok) = run(&dir, db, &["service", "get", &openai_id]);
    assert!(ok);
    assert!(out.contains("EUR 100.50"), "{out}");
    assert!(out.contains("yearly"), "{out}");

    // 5. search finds the service via its projection, including tags.
    let (out, _, ok) = run(&dir, db, &["service", "search", "openai"]);
    assert!(ok);
    assert!(out.contains("OpenAI"), "{out}");
    let (out, _, ok) = run(&dir, db, &["service", "search", "billing"]);
    assert!(ok);
    assert!(out.contains("OpenAI"), "{out}: a tag must be searchable");

    // 6. domain_name is rejected on a non-domain service.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "Misuse",
            "--type",
            "saas",
            "--domain",
            "assetmesh.dev",
        ],
    );
    assert!(!ok, "domain_name on a SaaS service must fail");
    assert!(err.contains("domain_name"), "{err}");

    // A domain service accepts it.
    let (out, _, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "AssetMesh",
            "--type",
            "domain",
            "--domain",
            "assetmesh.dev",
        ],
    );
    assert!(ok, "{out}");
    assert!(out.contains("Domain:        assetmesh.dev"), "{out}");

    // 7. Archiving through the shared asset lifecycle blocks further edits.
    let (out, _, ok) = run(&dir, db, &["asset", "archive", &hetzner_id]);
    assert!(ok, "{out}");
    let (_, err, ok) = run(
        &dir,
        db,
        &["service", "update", &hetzner_id, "--plan", "CX44"],
    );
    assert!(!ok, "an archived service must be read-only");
    assert!(
        err.contains("archived") || err.contains("conflict"),
        "{err}"
    );

    // The archived record is still readable for history.
    let (out, _, ok) = run(&dir, db, &["service", "get", &hetzner_id]);
    assert!(ok);
    assert!(out.contains("Hetzner"), "{out}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn service_cli_rejects_bad_money_and_credentials() {
    let dir = unique_dir("service-validation");
    let db = "e2e.db";

    // Negative cost. The `=` form keeps clap from reading the leading minus
    // as an option flag.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "Negative",
            "--type",
            "vps",
            "--cost=-5.00",
            "--currency",
            "USD",
        ],
    );
    assert!(!ok);
    assert!(err.contains("cost"), "{err}");

    // Malformed currency.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "BadCurrency",
            "--type",
            "vps",
            "--cost",
            "10.00",
            "--currency",
            "DOLLAR",
        ],
    );
    assert!(!ok);
    assert!(err.contains("currency"), "{err}");

    // A credential-bearing URL never enters canonical state (ADR 0010).
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "add",
            "--name",
            "Leaky",
            "--type",
            "saas",
            "--endpoint",
            "https://user:pass@example.com",
        ],
    );
    assert!(!ok);
    assert!(
        err.contains("credential") || err.contains("user info"),
        "{err}"
    );

    // None of the rejected writes left a row behind.
    let (out, _, ok) = run(&dir, db, &["service", "list"]);
    assert!(ok);
    assert!(out.contains("(no service records)"), "{out}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn service_renewal_and_relations_work_end_to_end() {
    let dir = unique_dir("service-renewal");
    let db = "e2e.db";

    let api = create_service(&dir, db, &["--name", "AssetMesh API", "--type", "api"]);
    let vps = create_service(
        &dir,
        db,
        &[
            "--name",
            "RackNerd VPS",
            "--type",
            "vps",
            "--plan",
            "CX22",
            "--cost",
            "49.00",
            "--currency",
            "USD",
        ],
    );

    // `hosted_on` is a first-class stored type; `relation list` resolves the
    // inverse for the target endpoint.
    let (out, err, ok) = run(&dir, db, &["relation", "add", &api, "hosted_on", &vps]);
    assert!(ok, "relation add should succeed: {err}");
    assert!(out.contains("hosted_on"), "{out}");

    let (out, _, ok) = run(&dir, db, &["relation", "list", &vps]);
    assert!(ok);
    assert!(
        out.contains("hosts"),
        "the inverse is derived at view time: {out}"
    );
    assert!(out.contains("1 relation(s)"), "{out}");

    // Re-stating the fact through the inverse type is a conflict, never a
    // second canonical row.
    let (_, err, ok) = run(&dir, db, &["relation", "add", &vps, "hosts", &api]);
    assert!(!ok, "stating an inverse must be refused");
    assert!(err.contains("conflict") || err.contains("already"), "{err}");

    // `points_to` behaves the same way.
    let domain = create_service(
        &dir,
        db,
        &[
            "--name",
            "assetmesh.dev",
            "--type",
            "domain",
            "--domain",
            "assetmesh.dev",
        ],
    );
    let (out, err, ok) = run(&dir, db, &["relation", "add", &domain, "points_to", &api]);
    assert!(ok, "points_to should succeed: {err}");
    assert!(out.contains("points_to"), "{out}");
    let (out, _, ok) = run(&dir, db, &["relation", "list", &api]);
    assert!(ok);
    assert!(out.contains("pointed_to_by"), "{out}");

    // The error text teaches the full registry, including the new types.
    let (_, err, ok) = run(&dir, db, &["relation", "add", &api, "teleports_to", &vps]);
    assert!(!ok);
    assert!(
        err.contains("hosted_on") && err.contains("points_to"),
        "{err}"
    );

    // --- record_renewal -------------------------------------------------
    // An explicit renewal applies only the facts it carries.
    let (out, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "renew",
            &vps,
            "--renewed-at",
            "2026-10-01",
            "--cost",
            "59.00",
            "--currency",
            "USD",
            "--next-renewal",
            "2026-11-01",
        ],
    );
    assert!(ok, "renew should succeed: {err}");
    assert!(out.contains("renewed "), "{out}");
    assert!(out.contains("2026-11-01"), "{out}");

    let (out, _, ok) = run(&dir, db, &["service", "get", &vps]);
    assert!(ok);
    assert!(out.contains("USD 59.00"), "{out}");
    assert!(out.contains("2026-11-01"), "{out}");
    // Exactly one provenance event for the renewal — no metadata spam.
    assert_eq!(out.matches("service.renewed").count(), 1, "{out}");
    assert!(out.contains("service.renewed"), "{out}");

    // A second renewal that carries only a new cost updates the cost and
    // leaves the boundary alone: AssetMesh never computes one.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "renew",
            &vps,
            "--renewed-at",
            "2026-11-01",
            "--cost",
            "69.00",
            "--currency",
            "USD",
        ],
    );
    assert!(ok, "second renew should succeed: {err}");
    let (out, _, ok) = run(&dir, db, &["service", "get", &vps]);
    assert!(ok);
    assert!(out.contains("USD 69.00"), "{out}");
    assert!(
        out.contains("2026-11-01"),
        "the boundary stays as last stated: {out}"
    );

    // Money is still set or cleared as a pair, and a negative amount is
    // rejected at the CLI boundary.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "renew",
            &vps,
            "--cost",
            "79.00",
            "--renewed-at",
            "2026-12-01",
        ],
    );
    assert!(!ok);
    assert!(
        err.contains("currency") || err.contains("together"),
        "{err}"
    );
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "renew",
            &vps,
            "--cost=-1.00",
            "--currency",
            "USD",
            "--renewed-at",
            "2026-12-01",
        ],
    );
    assert!(!ok);
    assert!(err.contains("cost"), "{err}");

    // --renewed-at is required: the CLI must not invent the renewal moment,
    // and a rejected command must leave the canonical record untouched.
    let (_, err, ok) = run(
        &dir,
        db,
        &[
            "service",
            "renew",
            &vps,
            "--cost",
            "89.00",
            "--currency",
            "USD",
        ],
    );
    assert!(!ok, "omitting --renewed-at must fail");
    assert!(err.contains("renewed-at"), "{err}");
    let (out, _, ok) = run(&dir, db, &["service", "get", &vps]);
    assert!(ok);
    assert!(
        out.contains("USD 69.00"),
        "a rejected renewal writes nothing: {out}"
    );
    assert!(
        out.contains("2026-11-01"),
        "the boundary is unchanged: {out}"
    );
    assert_eq!(
        out.matches("service.renewed").count(),
        2,
        "no renewal event was appended: {out}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn services_round_trip_through_the_portable_bundle_end_to_end() {
    let dir = unique_dir("service-portable");
    let db = "e2e.db";
    let bundle = dir.join("bundle");

    let api = create_service(
        &dir,
        db,
        &[
            "--name",
            "AssetMesh API",
            "--type",
            "api",
            "--provider",
            "Hetzner",
            "--plan",
            "CX22",
        ],
    );
    let vps = create_service(
        &dir,
        db,
        &[
            "--name",
            "RackNerd VPS",
            "--type",
            "vps",
            "--cost",
            "49.00",
            "--currency",
            "USD",
        ],
    );
    run(&dir, db, &["relation", "add", &api, "hosted_on", &vps]);
    run(
        &dir,
        db,
        &[
            "service",
            "renew",
            &vps,
            "--renewed-at",
            "2026-10-01",
            "--cost",
            "59.00",
            "--currency",
            "USD",
            "--next-renewal",
            "2026-11-01",
        ],
    );

    // 1. export writes the Phase 3 services section.
    let (out, err, ok) = run(&dir, db, &["export", bundle.to_str().unwrap()]);
    assert!(ok, "export should succeed: {err}");
    assert!(out.contains("services: 2"), "{out}");

    let services_file = bundle.join("modules").join("services.jsonl");
    assert!(
        services_file.exists(),
        "the bundle must carry modules/services.jsonl"
    );
    let services_text = std::fs::read_to_string(&services_file).unwrap();
    let lines: Vec<&str> = services_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 2, "{lines:#?}");
    // Each row carries the canonical asset id it belongs to (import restores
    // by id) and no credential-shaped field at all (ADR 0010: the secret
    // boundary is structural — no credential field exists to leak).
    for line in &lines {
        assert!(line.contains("\"asset_id\""), "{line}");
        for secret in ["api_key", "password", "token", "secret", "cookie"] {
            assert!(!line.contains(secret), "{line}");
        }
    }

    // The manifest declares the module with its schema version (an undeclared
    // section is corruption; an absent declaration means a pre-Phase 3 bundle).
    let manifest = std::fs::read_to_string(bundle.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(
        manifest["modules"]["services"]["schema_version"],
        serde_json::json!(1),
        "{manifest:#}"
    );
    assert_eq!(manifest["record_counts"]["services"], serde_json::json!(2));

    // 2. import into a fresh database restores services, renewal history, and
    // the service relation.
    let fresh = dir.join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    let db2 = "fresh.db";
    let (out, err, ok) = run(&fresh, db2, &["import", bundle.to_str().unwrap()]);
    assert!(ok, "import should succeed: {err}");
    assert!(out.contains("services: 2 created"), "{out}");
    assert!(out.contains("relations: 1 created"), "{out}");

    let (out, _, ok) = run(&fresh, db2, &["service", "get", &vps]);
    assert!(ok, "the restored record must be readable: {out}");
    assert!(out.contains("RackNerd VPS"), "{out}");
    assert!(out.contains("USD 59.00"), "{out}");
    assert!(out.contains("2026-11-01"), "{out}");
    assert!(
        out.contains("service.renewed"),
        "activity is restored: {out}"
    );

    let (out, _, ok) = run(&fresh, db2, &["relation", "list", &vps]);
    assert!(ok);
    assert!(out.contains("hosts"), "{out}");

    let (out, _, ok) = run(&fresh, db2, &["service", "search", "racknerd"]);
    assert!(ok, "the projection is rebuilt on import: {out}");
    assert!(out.contains("RackNerd VPS"), "{out}");

    // 3. re-importing is idempotent: nothing new is created, no activity is
    // duplicated, and the record is unchanged. (`updated` is honest — the
    // destination row is rewritten from the bundle, not skipped.)
    let (out, _, ok) = run(&fresh, db2, &["import", bundle.to_str().unwrap()]);
    assert!(ok, "{out}");
    assert!(out.contains("assets: 0 created"), "{out}");
    assert!(out.contains("services: 0 created"), "{out}");
    assert!(out.contains("activity: 0 events"), "{out}");
    let (out, _, ok) = run(&fresh, db2, &["service", "list"]);
    assert!(ok);
    assert!(out.contains("2 record(s)"), "no duplicate rows: {out}");
    let (out, _, ok) = run(&fresh, db2, &["service", "get", &vps]);
    assert!(ok);
    assert_eq!(out.matches("service.renewed").count(), 1, "{out}");
    assert!(out.contains("USD 59.00"), "{out}");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&fresh).ok();
}

/// Creates a service through the real CLI and returns its id.
fn create_service(dir: &std::path::Path, db: &str, args: &[&str]) -> String {
    let mut argv = vec!["service", "add"];
    argv.extend_from_slice(args);
    let (out, err, ok) = run(dir, db, &argv);
    assert!(ok, "service add should succeed: {err}");
    assert!(out.starts_with("created "), "{out}");
    out.lines()
        .next()
        .unwrap()
        .trim_start_matches("created ")
        .trim()
        .to_string()
}
