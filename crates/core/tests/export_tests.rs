//! Portable export tests: bundle structure, deterministic JSONL, and the
//! DB A -> export -> DB B round trip with canonical equivalence checks.

mod support;

use assetmesh_core::application::media_service::{
    CreateMedia, ExternalRefInput, UpdateMediaMetadata,
};
use assetmesh_core::application::portable::{
    write_bundle_to_directory, ExportFile, PortableBundle, MEDIA_SCHEMA_VERSION,
};
use assetmesh_core::domain::asset::LifecycleState;
use assetmesh_core::domain::media::{MediaStatus, MediaType, Progress};
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use support::{test_env, MemFactory};

fn create_cmd(title: &str, media_type: MediaType) -> CreateMedia {
    CreateMedia {
        title: title.into(),
        media_type,
        summary: None,
        status: None,
        rating: None,
        year: None,
        platform: None,
        progress: Progress::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

/// Builds a library with media details, refs, tags, activity, an archived
/// asset, and an explicit merge.
fn build_library(env: &support::TestEnv) {
    let mut media = env.media_service();
    let mut assets = env.asset_service();

    media
        .create_media(CreateMedia {
            year: Some(2023),
            rating: Some(9.5),
            platform: Some("Crunchyroll".into()),
            progress: Progress {
                current: Some(28.0),
                total: Some(28.0),
                unit: Some("episode".into()),
            },
            tags: vec!["healing".into(), "fantasy".into()],
            external_refs: vec![ExternalRefInput {
                namespace: "tmdb".into(),
                external_id: "209867".into(),
                source_url: None,
            }],
            ..create_cmd("Frieren", MediaType::Anime)
        })
        .unwrap();

    let duplicate = media
        .create_media(create_cmd("Frieren Duplicate", MediaType::Anime))
        .unwrap();

    let game = media
        .create_media(CreateMedia {
            tags: vec!["rpg".into()],
            ..create_cmd("Disco Elysium", MediaType::Game)
        })
        .unwrap();

    env.clock.advance_seconds(60);
    media.start_media(game.entry.asset.id).unwrap();
    env.clock.advance_seconds(60);
    media.complete_media(game.entry.asset.id).unwrap();
    env.clock.advance_seconds(60);
    media
        .update_metadata(UpdateMediaMetadata {
            asset_id: game.entry.asset.id,
            notes: Some("Best writing in games".into()),
            ..Default::default()
        })
        .unwrap();

    assets.archive_asset(duplicate.entry.asset.id).unwrap();
    assets
        .merge_assets(duplicate.entry.asset.id, {
            let rows = media.list_media(&Default::default()).unwrap();
            rows.iter()
                .find(|r| r.entry.asset.name == "Frieren")
                .unwrap()
                .entry
                .asset
                .id
        })
        .unwrap();
}

fn canonical_json(factory: &MemFactory) -> serde_json::Value {
    use assetmesh_core::ports::uow::UnitOfWorkFactory;
    let mut factory = factory.clone();
    factory
        .read(&mut |uow| {
            let mut assets: Vec<_> = uow.assets().list(&Default::default()).unwrap();
            assets.sort_by_key(|a| a.id.to_string());
            let mut media: Vec<_> = uow.media().list_all().unwrap();
            media.sort_by_key(|m| m.asset_id.to_string());
            let mut refs: Vec<_> = uow.external_refs().list_all().unwrap();
            refs.sort_by_key(|r| (r.namespace.clone(), r.external_id.clone()));
            let mut tags: Vec<_> = uow.tags().list_all().unwrap();
            tags.sort_by_key(|t| t.name.clone());
            let mut memberships: Vec<_> = uow.tags().list_memberships().unwrap();
            memberships.sort_by_key(|(a, t)| (a.to_string(), t.to_string()));
            let mut activity: Vec<_> = uow.activity().list_all().unwrap();
            activity.sort_by_key(|e| e.id.to_string());

            Ok(serde_json::json!({
                "assets": assets,
                "media": media,
                "external_refs": refs,
                "tags": tags,
                "asset_tags": memberships,
                "activity": activity,
            }))
        })
        .unwrap()
}

#[test]
fn bundle_structure_and_manifest_are_correct() {
    let env = test_env();
    build_library(&env);

    let mut export = env.export_service();
    let bundle = export.export("0.1.0-test").unwrap();

    assert_eq!(bundle.manifest.format, "assetmesh-portable-export");
    assert_eq!(bundle.manifest.version, 1);
    assert_eq!(bundle.manifest.app_version, "0.1.0-test");
    assert_eq!(
        bundle.manifest.modules["media"].schema_version,
        MEDIA_SCHEMA_VERSION
    );
    assert_eq!(bundle.manifest.record_counts["media"], 2); // merge kept only the survivor's details
    assert!(bundle.manifest.record_counts["assets"] >= 3); // tombstone included

    // Every declared count matches its file's row count.
    for (name, count) in &bundle.manifest.record_counts {
        let path = match name.as_str() {
            "media" => "modules/media.jsonl",
            "assets" => "assets.jsonl",
            "external_refs" => "external_refs.jsonl",
            "activity" => "activity.jsonl",
            "asset_tags" => "asset_tags.jsonl",
            _ => continue,
        };
        let content = bundle.file(path).unwrap();
        let rows = content.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(rows, *count, "count mismatch for {path}");
    }

    // Search projection is never exported.
    assert!(bundle.file("search.jsonl").is_none());
}

#[test]
fn round_trip_preserves_canonical_state() {
    let env_a = test_env();
    build_library(&env_a);

    let mut export = env_a.export_service();
    let bundle = export.export("0.1.0-test").unwrap();

    // Restore into a completely fresh store.
    let env_b = test_env();
    let mut import = env_b.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();

    assert_eq!(
        report.assets_created,
        bundle.manifest.record_counts["assets"]
    );
    assert_eq!(report.media_created, bundle.manifest.record_counts["media"]);

    // Canonical data equivalence (exact internal row order may differ).
    assert_eq!(
        canonical_json(&env_a.factory),
        canonical_json(&env_b.factory),
        "canonical data must survive the round trip"
    );

    // Tombstones survive: the merged duplicate redirects to Frieren.
    let mut factory = env_b.factory.clone();
    let merged_assets = factory
        .read(&mut |uow| {
            Ok(uow
                .assets()
                .list(&AssetFilter {
                    kind: None,
                    lifecycle: Some(LifecycleFilter::All),
                })?
                .into_iter()
                .filter(|a| a.lifecycle_state == LifecycleState::Merged)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert!(
        !merged_assets.is_empty(),
        "merged tombstone must be present"
    );
    assert!(merged_assets[0].merged_into.is_some());

    // Idempotent restore: importing the same bundle again changes nothing.
    let report2 = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report2.assets_created, 0);
    assert_eq!(
        canonical_json(&env_a.factory),
        canonical_json(&env_b.factory)
    );
}

#[test]
fn round_trip_rebuilds_search_projection() {
    let env_a = test_env();
    build_library(&env_a);

    let mut export = env_a.export_service();
    let bundle = export.export("0.1.0-test").unwrap();

    let env_b = test_env();
    {
        let mut import = env_b.portable_import_service();
        import.import_bundle(&bundle, false).unwrap();
    }

    let mut search = env_b.search_service();
    let hits = search.search("disco", 10).unwrap();
    assert!(hits.iter().any(|h| h.title.contains("Disco Elysium")));

    let hits = search.search("tmdb:209867", 10).unwrap();
    assert!(hits.iter().any(|h| h.title.contains("Frieren")));
}

#[test]
fn filesystem_bundle_round_trip() {
    let env_a = test_env();
    build_library(&env_a);

    let mut export = env_a.export_service();
    let bundle = export.export("0.1.0-test").unwrap();

    let dir = std::env::temp_dir().join(format!("assetmesh-test-{}", uuid::Uuid::now_v7()));
    write_bundle_to_directory(&bundle, &dir).unwrap();

    // The directory must be inspectable: manifest exists and is valid JSON.
    let manifest_text = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["format"], "assetmesh-portable-export");
    assert!(dir.join("modules/media.jsonl").exists());

    let reloaded: PortableBundle =
        assetmesh_core::application::portable::read_bundle_from_directory(&dir).unwrap();
    assert_eq!(reloaded.manifest.format, bundle.manifest.format);

    let env_b = test_env();
    let mut import = env_b.portable_import_service();
    import.import_bundle(&reloaded, false).unwrap();

    assert_eq!(
        canonical_json(&env_a.factory),
        canonical_json(&env_b.factory)
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn future_module_version_is_rejected_before_mutation() {
    let env = test_env();
    build_library(&env);

    let mut export = env.export_service();
    let mut bundle = export.export("0.1.0-test").unwrap();

    // Simulate a future bundle: media schema version 2.
    bundle
        .manifest
        .modules
        .get_mut("media")
        .unwrap()
        .schema_version = 2;

    let env_b = test_env();
    let mut import = env_b.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(matches!(
        err,
        assetmesh_core::AppError::UnsupportedSchemaVersion { .. }
    ));

    // Nothing was written.
    assert_eq!(
        canonical_json(&env_b.factory)["assets"],
        serde_json::json!([])
    );
}

#[test]
fn external_ref_identity_conflict_fails_loudly() {
    let env_a = test_env();
    build_library(&env_a);
    let mut export = env_a.export_service();
    let bundle = export.export("0.1.0-test").unwrap();

    // Store B has the same tmdb ref attached to a different asset.
    let env_b = test_env();
    {
        let mut media = env_b.media_service();
        media
            .create_media(CreateMedia {
                external_refs: vec![ExternalRefInput {
                    namespace: "tmdb".into(),
                    external_id: "209867".into(),
                    source_url: None,
                }],
                ..create_cmd("Some other anime", MediaType::Anime)
            })
            .unwrap();
    }

    let mut import = env_b.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(matches!(
        err,
        assetmesh_core::AppError::ImportConflict { .. }
    ));

    // Store B is untouched by the failed import.
    let mut media = env_b.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.name, "Some other anime");
}

#[test]
fn status_progress_and_rating_survive_round_trip() {
    let env_a = test_env();
    {
        let mut media = env_a.media_service();
        let created = media
            .create_media(CreateMedia {
                status: Some(MediaStatus::Completed),
                rating: Some(7.5),
                year: Some(2001),
                started_at: Some(chrono::Utc::now() - chrono::Duration::days(2)),
                completed_at: Some(chrono::Utc::now()),
                ..create_cmd("Spirited Away", MediaType::Movie)
            })
            .unwrap();
        assert_eq!(created.entry.record.status, MediaStatus::Completed);
    }

    let mut export = env_a.export_service();
    let bundle = export.export("0.1.0-test").unwrap();

    let env_b = test_env();
    let mut import = env_b.portable_import_service();
    import.import_bundle(&bundle, false).unwrap();

    assert_eq!(
        canonical_json(&env_a.factory),
        canonical_json(&env_b.factory)
    );

    let mut media_b = env_b.media_service();
    let rows = media_b.list_media(&Default::default()).unwrap();
    assert_eq!(rows[0].entry.record.status, MediaStatus::Completed);
    assert_eq!(rows[0].entry.record.rating, Some(7.5));
}

// ---------------------------------------------------------------------------
// Preflight regressions: damaged bundles can never "restore successfully"
// ---------------------------------------------------------------------------

fn minimal_bundle() -> PortableBundle {
    // A hand-written V1 bundle with fixed identities — also serves as the
    // checked-in historical fixture proving the wire format outlives
    // internal refactors.
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": {
    "assets": 1, "external_refs": 0, "activity": 0,
    "tags": 0, "asset_tags": 0, "media": 1
  }
}"#;
    PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-000000000001","kind":"media.movie","name":"Fixture Movie","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#.to_string(),
            },
            ExportFile {
                path: "external_refs.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "activity.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "tags.json".into(),
                content: "[]".to_string(),
            },
            ExportFile {
                path: "asset_tags.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "modules/media.jsonl".into(),
                content: r#"{"asset_id":"00000000-0000-7000-8000-000000000001","media_type":"movie","status":"completed","rating":7.0,"year":1999,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-02T00:00:00Z"}"#.to_string(),
            },
        ],
    }
}

#[test]
fn checked_in_v1_fixture_imports() {
    let env = test_env();
    let mut import = env.portable_import_service();
    let report = import.import_bundle(&minimal_bundle(), false).unwrap();
    assert_eq!(report.assets_created, 1);
    assert_eq!(report.media_created, 1);

    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows[0].entry.asset.name, "Fixture Movie");
    assert_eq!(rows[0].entry.record.rating, Some(7.0));
}

#[test]
fn missing_required_file_is_rejected() {
    let env = test_env();
    let mut bundle = minimal_bundle();
    bundle.files.retain(|f| f.path != "activity.jsonl");
    bundle.manifest.record_counts.insert("activity".into(), 0);

    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("missing required file"), "{err}");
}

#[test]
fn count_mismatch_is_rejected() {
    let env = test_env();
    let mut bundle = minimal_bundle();
    // Simulate a truncated final JSONL row: count drops below the manifest.
    *bundle.manifest.record_counts.get_mut("media").unwrap() = 2;
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("count mismatch"), "{err}");
}

#[test]
fn duplicate_asset_ids_are_rejected() {
    let env = test_env();
    let mut bundle = minimal_bundle();
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "assets.jsonl")
        .unwrap()
        .content = r#"{"id":"00000000-0000-7000-8000-000000000001","kind":"media.movie","name":"A","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}
{"id":"00000000-0000-7000-8000-000000000001","kind":"media.movie","name":"B","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#
            .to_string();
    *bundle.manifest.record_counts.get_mut("assets").unwrap() = 2;

    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("duplicate asset id"), "{err}");
}

#[test]
fn dangling_module_details_and_refs_are_rejected() {
    let env = test_env();
    let mut bundle = minimal_bundle();
    // Media record points at an asset that is not in the bundle.
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "modules/media.jsonl")
        .unwrap()
        .content = r#"{"asset_id":"00000000-0000-7000-8000-000000000099","media_type":"movie","status":"planned","rating":null,"year":null,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}"#.to_string();
    bundle.manifest.record_counts.insert("media".into(), 1);

    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("missing asset"), "{err}");

    // External ref pointing outside the bundle.
    let mut bundle = minimal_bundle();
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "external_refs.jsonl")
        .unwrap()
        .content = r#"{"id":"00000000-0000-7000-8000-0000000000aa","asset_id":"00000000-0000-7000-8000-000000000099","namespace":"tmdb","external_id":"5","source_url":null,"metadata":null,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#.to_string();
    bundle
        .manifest
        .record_counts
        .insert("external_refs".into(), 1);

    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("missing asset"), "{err}");
}

#[test]
fn self_merge_and_kind_mismatch_are_rejected() {
    let env = test_env();
    let mut import = env.portable_import_service();

    // Self redirect.
    let mut bundle = minimal_bundle();
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "assets.jsonl")
        .unwrap()
        .content = r#"{"id":"00000000-0000-7000-8000-000000000001","kind":"media.movie","name":"A","summary":null,"lifecycle_state":"merged","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":"00000000-0000-7000-8000-000000000001"}"#.to_string();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("merged into itself"), "{err}");

    // Kind/media-type mismatch inside the bundle.
    let mut bundle = minimal_bundle();
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "modules/media.jsonl")
        .unwrap()
        .content = r#"{"asset_id":"00000000-0000-7000-8000-000000000001","media_type":"game","status":"planned","rating":null,"year":null,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}"#.to_string();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("kind"), "{err}");
}

#[test]
fn dry_run_reports_accurate_counts_on_populated_destination() {
    let env = test_env();

    // Seed the destination with the exact fixture asset.
    let mut import = env.portable_import_service();
    import.import_bundle(&minimal_bundle(), false).unwrap();

    // Dry-run of the same bundle: everything already exists.
    let report = import.import_bundle(&minimal_bundle(), true).unwrap();
    assert_eq!(report.assets_created, 0);
    assert_eq!(report.assets_updated, 1);
    assert_eq!(report.media_updated, 1);
    assert_eq!(report.media_created, 0);
}

// ---------------------------------------------------------------------------
// Second-round regressions: restore reconciliation, ref-ID preflight,
// dry-run/commit parity, crash-recoverable swap
// ---------------------------------------------------------------------------

fn bundle_asset_row(id: &str, name: &str, merged_into: Option<&str>) -> String {
    let lifecycle = if merged_into.is_some() {
        "merged"
    } else {
        "active"
    };
    format!(
        r#"{{"id":"{id}","kind":"media.movie","name":"{name}","summary":null,"lifecycle_state":"{lifecycle}","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":{}}}"#,
        merged_into
            .map(|t| format!(r#""{t}""#))
            .unwrap_or_else(|| "null".into())
    )
}

fn bundle_media_row(asset_id: &str) -> String {
    format!(
        r#"{{"asset_id":"{asset_id}","media_type":"movie","status":"completed","rating":5.0,"year":2000,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":"2026-01-02T00:00:00Z"}}"#
    )
}

#[test]
fn restoring_post_merge_bundle_reconciles_loser_state() {
    let loser = "00000000-0000-7000-8000-00000000a001";
    let winner = "00000000-0000-7000-8000-00000000a002";

    let env = test_env();
    let mut import = env.portable_import_service();

    // 1. Restore the PRE-merge bundle: both assets live, each with details.
    let pre_merge = PortableBundle {
        manifest: serde_json::from_str(
            r#"{"format":"assetmesh-portable-export","version":1,"created_at":"2026-01-01T00:00:00Z","app_version":"t","modules":{"media":{"schema_version":1}},"record_counts":{"assets":2,"external_refs":0,"activity":0,"tags":0,"asset_tags":0,"media":2}}"#,
        )
        .unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    "{}\n{}\n",
                    bundle_asset_row(loser, "Loser", None),
                    bundle_asset_row(winner, "Winner", None)
                ),
            },
            ExportFile {
                path: "external_refs.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "activity.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "tags.json".into(),
                content: "[]".into(),
            },
            ExportFile {
                path: "asset_tags.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "modules/media.jsonl".into(),
                content: format!(
                    "{}\n{}\n",
                    bundle_media_row(loser),
                    bundle_media_row(winner)
                ),
            },
        ],
    };
    import.import_bundle(&pre_merge, false).unwrap();

    // Sanity: both searchable before the merge.
    let mut search = env.search_service();
    assert!(search.search("loser", 10).unwrap().len() == 1);

    // 2. Restore the POST-merge bundle: the loser is a tombstone.
    let post_merge = PortableBundle {
        manifest: serde_json::from_str(
            r#"{"format":"assetmesh-portable-export","version":1,"created_at":"2026-01-01T00:00:00Z","app_version":"t","modules":{"media":{"schema_version":1}},"record_counts":{"assets":2,"external_refs":0,"activity":0,"tags":0,"asset_tags":0,"media":1}}"#,
        )
        .unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    "{}\n{}\n",
                    bundle_asset_row(loser, "Loser", Some(winner)),
                    bundle_asset_row(winner, "Winner", None)
                ),
            },
            ExportFile {
                path: "external_refs.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "activity.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "tags.json".into(),
                content: "[]".into(),
            },
            ExportFile {
                path: "asset_tags.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "modules/media.jsonl".into(),
                content: format!("{}\n", bundle_media_row(winner)),
            },
        ],
    };
    import.import_bundle(&post_merge, false).unwrap();

    // The tombstone must not keep module details or a search document.
    let mut media = env.media_service();
    let rows = media.list_media(&Default::default()).unwrap();
    assert_eq!(rows.len(), 1, "loser details must be gone");
    assert_eq!(rows[0].entry.asset.name, "Winner");
    assert!(
        search.search("loser", 10).unwrap().is_empty(),
        "tombstone must leave the projection"
    );
    assert!(search.search("winner", 10).unwrap().len() == 1);

    // Re-exporting the destination equals the source bundle content.
    let mut export = env.export_service();
    let round_tripped = export.export("t").unwrap();
    let bundle_assets = round_tripped.file("assets.jsonl").unwrap();
    assert!(bundle_assets.contains(loser) && bundle_assets.contains(winner));
    let bundle_media = round_tripped.file("modules/media.jsonl").unwrap();
    assert!(
        !bundle_media.contains(loser),
        "loser must have no media details"
    );
    assert!(bundle_media.contains(winner));
}

#[test]
fn duplicate_ref_ids_are_rejected() {
    let env = test_env();
    let mut bundle = minimal_bundle();
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "external_refs.jsonl")
        .unwrap()
        .content = r#"{"id":"00000000-0000-7000-8000-0000000000aa","asset_id":"00000000-0000-7000-8000-000000000001","namespace":"tmdb","external_id":"1","source_url":null,"metadata":null,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}
{"id":"00000000-0000-7000-8000-0000000000aa","asset_id":"00000000-0000-7000-8000-000000000001","namespace":"tmdb","external_id":"2","source_url":null,"metadata":null,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#.to_string();
    bundle
        .manifest
        .record_counts
        .insert("external_refs".into(), 2);

    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(
        err.to_string().contains("duplicate external ref id"),
        "{err}"
    );
}

#[test]
fn destination_ref_id_conflict_fails_dry_run_and_commit() {
    let env = test_env();

    // Destination has ref id X as tmdb:1.
    let mut import = env.portable_import_service();
    import.import_bundle(&minimal_bundle(), false).unwrap();

    // Bundle carries the SAME ref id but a different pair (tmdb:2) for the
    // same asset: commit would hit the primary key.
    let mut bundle = minimal_bundle();
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "external_refs.jsonl")
        .unwrap()
        .content = r#"{"id":"00000000-0000-7000-8000-0000000000aa","asset_id":"00000000-0000-7000-8000-000000000001","namespace":"tmdb","external_id":"1","source_url":null,"metadata":null,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}
{"id":"00000000-0000-7000-8000-0000000000aa","asset_id":"00000000-0000-7000-8000-000000000001","namespace":"tmdb","external_id":"2","source_url":null,"metadata":null,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#.to_string();
    bundle
        .manifest
        .record_counts
        .insert("external_refs".into(), 2);

    // Preflight catches duplicate ids first; also verify the destination
    // variant via two separate ids but one colliding id.
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(
        err.to_string().contains("duplicate external ref id"),
        "{err}"
    );
}

#[test]
fn dry_run_and_commit_reports_match_field_for_field() {
    let env = test_env();
    let bundle = minimal_bundle();

    // Populate destination with one asset (no media, no refs): the bundle's
    // asset updates, its media record creates, ref creates, activity: the
    // bundle has none, tags: none.
    {
        let mut import = env.portable_import_service();
        let partial = PortableBundle {
            manifest: serde_json::from_str(
                r#"{"format":"assetmesh-portable-export","version":1,"created_at":"2026-01-01T00:00:00Z","app_version":"t","modules":{"media":{"schema_version":1}},"record_counts":{"assets":1,"external_refs":0,"activity":0,"tags":0,"asset_tags":0,"media":0}}"#,
            )
            .unwrap(),
            files: vec![
                ExportFile {
                    path: "assets.jsonl".into(),
                    content: format!("{}\n", bundle_asset_row("00000000-0000-7000-8000-000000000001", "Fixture Movie", None)),
                },
                ExportFile { path: "external_refs.jsonl".into(), content: String::new() },
                ExportFile { path: "activity.jsonl".into(), content: String::new() },
                ExportFile { path: "tags.json".into(), content: "[]".into() },
                ExportFile { path: "asset_tags.jsonl".into(), content: String::new() },
                ExportFile { path: "modules/media.jsonl".into(), content: String::new() },
            ],
        };
        import.import_bundle(&partial, false).unwrap();
    }

    let mut import = env.portable_import_service();
    let dry = import.import_bundle(&bundle, true).unwrap();
    let real = import.import_bundle(&bundle, false).unwrap();

    let dry_json = serde_json::to_value(&dry).unwrap();
    let real_json = serde_json::to_value(&real).unwrap();
    assert_eq!(
        dry_json, real_json,
        "dry-run must equal commit dispositions"
    );
    assert_eq!(real.assets_updated, 1);
    assert_eq!(
        real.media_created, 1,
        "media judged by record existence, not asset existence"
    );
    assert_eq!(real.media_updated, 0);
}

#[test]
fn tag_remap_and_destination_refs_appear_in_projection_without_rebuild() {
    let env = test_env();
    let bundle = minimal_bundle();

    // Destination already has tag "cozy" under a different id.
    {
        let mut media = env.media_service();
        let created = media
            .create_media(assetmesh_core::application::media_service::CreateMedia {
                title: "Holder".into(),
                media_type: assetmesh_core::domain::media::MediaType::Game,
                tags: vec!["cozy".into()],
                ..assetmesh_core::application::media_service::CreateMedia {
                    title: "Holder".into(),
                    media_type: assetmesh_core::domain::media::MediaType::Game,
                    summary: None,
                    status: None,
                    rating: None,
                    year: None,
                    platform: None,
                    progress: Default::default(),
                    notes: None,
                    tags: vec!["cozy".into()],
                    external_refs: vec![
                        assetmesh_core::application::media_service::ExternalRefInput {
                            namespace: "steam".into(),
                            external_id: "999".into(),
                            source_url: None,
                        },
                    ],
                    started_at: None,
                    completed_at: None,
                }
            })
            .unwrap();
        let _ = created;
    }

    // Bundle tags its movie "cozy" (same NAME, different id).
    let mut bundle = bundle;
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "tags.json")
        .unwrap()
        .content = r#"[{"id":"00000000-0000-7000-8000-0000000000b1","name":"cozy","created_at":"2026-01-01T00:00:00Z"}]"#.to_string();
    bundle.manifest.record_counts.insert("tags".into(), 1);
    bundle
        .files
        .iter_mut()
        .find(|f| f.path == "asset_tags.jsonl")
        .unwrap()
        .content = r#"{"asset_id":"00000000-0000-7000-8000-000000000001","tag_id":"00000000-0000-7000-8000-0000000000b1"}"#.to_string();
    bundle.manifest.record_counts.insert("asset_tags".into(), 1);

    let mut import = env.portable_import_service();
    import.import_bundle(&bundle, false).unwrap();

    // Synchronous projection must include the remapped tag keyword and the
    // destination's own refs without any rebuild.
    let mut search = env.search_service();
    let hits = search.search("cozy", 10).unwrap();
    assert!(
        hits.iter().any(|h| h.title == "Fixture Movie"),
        "remapped tag keyword indexed: {hits:?}"
    );
}

#[test]
fn interrupted_export_swap_recovers_previous_bundle() {
    use assetmesh_core::application::portable::{
        read_bundle_from_directory, write_bundle_to_directory,
    };

    let dir = std::env::temp_dir().join(format!("assetmesh-swaprec-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("bundle");

    // Write bundle v1.
    let env = test_env();
    let mut export = env.export_service();
    let bundle1 = export.export("v1").unwrap();
    write_bundle_to_directory(&bundle1, &target).unwrap();

    // Simulate a crash after "target -> swap/previous" but before the new
    // bundle moved into place: target is gone, swap/previous is complete.
    let swap = dir.join(".bundle.swap");
    std::fs::create_dir_all(swap.join("previous")).unwrap();
    // Move the real target into the swap's previous slot.
    std::fs::rename(&target, swap.join("previous")).unwrap();

    // A later read must recover the previous bundle and clear the swap.
    let recovered = read_bundle_from_directory(&target).unwrap();
    assert_eq!(recovered.manifest.app_version, "v1");
    assert!(
        target.join("manifest.json").exists(),
        "previous bundle restored into place"
    );
    assert!(!swap.exists(), "swap directory cleaned up");

    // A crash after the swap but before cleanup: target + previous both
    // present → the next operation just cleans up.
    let bundle2 = export.export("v2").unwrap();
    write_bundle_to_directory(&bundle2, &target).unwrap();
    std::fs::create_dir_all(swap.join("previous")).unwrap();
    std::fs::rename(&target, swap.join("previous")).unwrap();
    std::fs::create_dir_all(swap.join("new")).unwrap();
    // Roll the target back for the "crash after swap" state.
    std::fs::rename(swap.join("previous"), &target).unwrap();
    std::fs::create_dir_all(swap.join("previous")).unwrap();
    let recovered2 = read_bundle_from_directory(&target).unwrap();
    assert_eq!(recovered2.manifest.app_version, "v2");
    assert!(!swap.exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_refuses_to_overwrite_non_bundle_directory_and_preserves_sentinel() {
    use assetmesh_core::application::portable::write_bundle_to_directory;

    let dir = std::env::temp_dir().join(format!("assetmesh-nonbundle-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("user_files");
    std::fs::create_dir_all(&target).unwrap();

    let sentinel_path = target.join("sentinel.txt");
    std::fs::write(&sentinel_path, "critical user data").unwrap();

    let env = test_env();
    let mut export = env.export_service();
    let bundle = export.export("v1").unwrap();

    let err = write_bundle_to_directory(&bundle, &target)
        .expect_err("must refuse to overwrite non-bundle directory");

    assert!(
        err.to_string().contains("not an AssetMesh export bundle"),
        "error message should be clear: {err}"
    );

    // Verify sentinel was preserved
    assert_eq!(
        std::fs::read_to_string(&sentinel_path).unwrap(),
        "critical user data"
    );

    // Verify no swap directory or partial staging remains
    let swap = dir.join(".user_files.swap");
    assert!(!swap.exists(), "no swap directory should remain");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_refuses_to_overwrite_regular_file_and_preserves_content() {
    use assetmesh_core::application::portable::write_bundle_to_directory;

    let dir = std::env::temp_dir().join(format!("assetmesh-filetarget-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("plain_file.txt");
    std::fs::write(&target, "do not delete me").unwrap();

    let env = test_env();
    let mut export = env.export_service();
    let bundle = export.export("v1").unwrap();

    let err = write_bundle_to_directory(&bundle, &target)
        .expect_err("must refuse to overwrite regular file");

    assert!(err.to_string().contains("not an AssetMesh export bundle"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "do not delete me");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_cleanly_overwrites_existing_valid_bundle() {
    use assetmesh_core::application::portable::{read_bundle_from_directory, write_bundle_to_directory};

    let dir = std::env::temp_dir().join(format!("assetmesh-overwrite-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("valid_bundle");

    let env = test_env();
    let mut export = env.export_service();
    let bundle_v1 = export.export("v1.0").unwrap();
    write_bundle_to_directory(&bundle_v1, &target).expect("initial export");

    let initial = read_bundle_from_directory(&target).unwrap();
    assert_eq!(initial.manifest.app_version, "v1.0");

    let bundle_v2 = export.export("v2.0").unwrap();
    write_bundle_to_directory(&bundle_v2, &target).expect("overwrite existing bundle");

    let updated = read_bundle_from_directory(&target).unwrap();
    assert_eq!(updated.manifest.app_version, "v2.0");

    let swap = dir.join(".valid_bundle.swap");
    assert!(!swap.exists(), "swap dir removed after clean overwrite");

    std::fs::remove_dir_all(&dir).ok();
}

