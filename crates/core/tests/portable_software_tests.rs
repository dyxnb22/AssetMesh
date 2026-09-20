//! Portable bundle tests for the Software module and Relations:
//! round trips, legacy Phase 1 compatibility, and failure modes.

mod support;

use assetmesh_core::application::media_service::{CreateMedia, ExternalRefInput, MediaService};
use assetmesh_core::application::portable::ExportFile;
use assetmesh_core::application::portable::{PortableBundle, PortableImportReport};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter, SoftwareFilter};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::AppError;
use support::{test_env, TestEnv};

use assetmesh_core::domain::asset::{Asset, AssetKind};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory, SoftwareRecord};

fn create_software(
    env: &TestEnv,
    name: &str,
    category: SoftwareCategory,
    purpose: Option<&str>,
    refs: Vec<ExternalRefInput>,
) -> AssetId {
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    software
        .create_software(CreateSoftware {
            name: name.into(),
            category,
            purpose: purpose.map(String::from),
            version: Some("1.0.0".into()),
            install_source: Some(InstallSource::Manual),
            install_location: Some(format!("/opt/{name}")),
            external_refs: refs,
            ..software_defaults(category)
        })
        .unwrap()
        .entry
        .asset
        .id
}

fn software_defaults(category: SoftwareCategory) -> CreateSoftware {
    CreateSoftware {
        name: String::new(),
        category,
        summary: None,
        install_source: None,
        version: None,
        install_location: None,
        executable_path: None,
        purpose: None,
        notes: None,
        architecture: None,
        installed_at: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
    }
}

fn ref_input(namespace: &str, external_id: &str) -> ExternalRefInput {
    ExternalRefInput {
        namespace: namespace.into(),
        external_id: external_id.into(),
        source_url: None,
    }
}

fn canonical_json(env: &TestEnv) -> serde_json::Value {
    let store = env.factory.store();
    let assets: Vec<serde_json::Value> = store
        .assets
        .values()
        .map(|a| {
            serde_json::json!({
                "id": a.id.to_string(),
                "kind": a.kind.as_str(),
                "name": a.name,
                "summary": a.summary,
                "lifecycle": a.lifecycle_state.as_str(),
                "merged_into": a.merged_into.map(|m| m.to_string()),
            })
        })
        .collect();
    let software: Vec<serde_json::Value> = store
        .software
        .values()
        .map(|s| {
            serde_json::json!({
                "asset_id": s.asset_id.to_string(),
                "category": s.category.as_str(),
                "install_source": s.install_source.as_str(),
                "version": s.version,
                "location": s.install_location,
                "purpose": s.purpose,
                "notes": s.notes,
                "architecture": s.architecture,
            })
        })
        .collect();
    let relations: Vec<serde_json::Value> = store
        .relations
        .values()
        .map(|r| {
            serde_json::json!({
                "source": r.source_asset_id.to_string(),
                "target": r.target_asset_id.to_string(),
                "type": r.relation_type.as_str(),
                "note": r.note,
            })
        })
        .collect();
    serde_json::json!({ "assets": assets, "software": software, "relations": relations })
}

#[test]
fn software_round_trip_preserves_canonical_state() {
    let env = test_env();
    create_software(
        &env,
        "ripgrep",
        SoftwareCategory::Cli,
        Some("fast grep"),
        vec![ref_input("homebrew_formula", "ripgrep")],
    );
    create_software(
        &env,
        "Safari",
        SoftwareCategory::Application,
        None,
        vec![ref_input("bundle_id", "com.apple.Safari")],
    );

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    assert_eq!(bundle.manifest.modules["software"].schema_version, 1);
    assert_eq!(bundle.manifest.record_counts["software"], 2);
    assert!(bundle
        .file("modules/software.jsonl")
        .unwrap()
        .contains("ripgrep"));

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.software_created, 2);
    assert_eq!(report.assets_created, 2);

    let restored_json = canonical_json(&destination);
    let source_json = canonical_json(&env);
    assert_eq!(restored_json, source_json, "canonical state must match");

    // Idempotent re-import.
    let report2 = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report2.software_created, 0);
    assert_eq!(report2.software_updated, 2);
    assert_eq!(canonical_json(&destination), source_json);

    // Export → import → export is byte-stable apart from timestamps.
    let mut export2 = destination.export_service();
    let bundle2 = export2.export("test").unwrap();
    let strip = |b: &PortableBundle| b.file("modules/software.jsonl").unwrap().to_string();
    assert_eq!(strip(&bundle), strip(&bundle2));
}

#[test]
fn relations_round_trip() {
    let env = test_env();
    let rg = create_software(&env, "ripgrep", SoftwareCategory::Cli, None, vec![]);
    let jdk = create_software(&env, "OpenJDK", SoftwareCategory::Runtime, None, vec![]);

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    relations
        .attach(
            jdk,
            RelationType::Installs,
            rg,
            Some("via brew".into()),
            RelationProvenance::Manual,
        )
        .unwrap();

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    assert_eq!(bundle.manifest.record_counts["relations"], 1);

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.relations_created, 1);
    assert_eq!(canonical_json(&destination), canonical_json(&env));

    // Relations participate in list views from the restored destination.
    let mut relations2 = assetmesh_core::application::relation_service::RelationService::new(
        destination.factory.clone(),
        destination.clock.clone(),
        destination.ids.clone(),
    );
    let views = relations2.list_for_asset(rg).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].relation_type, RelationType::InstalledVia);
    assert_eq!(views[0].other_asset_name, "OpenJDK");
}

#[test]
fn legacy_phase1_bundle_without_software_module_imports() {
    // A bundle exported by Phase 1 declares only the media module and has no
    // software.jsonl / relations.jsonl — it must import cleanly and create
    // no software state (documented compatibility policy).
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "phase1-fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": {
    "assets": 1, "external_refs": 0, "activity": 0,
    "tags": 0, "asset_tags": 0, "media": 1
  }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-00000000000a","kind":"media.movie","name":"Legacy Movie","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#.to_string(),
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
                content: r#"{"asset_id":"00000000-0000-7000-8000-00000000000a","media_type":"movie","status":"completed","rating":8.0,"year":1999,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}"#.to_string(),
            },
        ],
    };

    let env = test_env();
    let mut import = env.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.assets_created, 1);
    assert_eq!(report.media_created, 1);
    assert_eq!(report.software_created, 0);
    assert_eq!(report.relations_created, 0);

    // The bundle's media asset is imported and projected as usual; no
    // software state or relations were invented.
    let store = env.factory.store();
    assert_eq!(store.software.len(), 0);
    assert_eq!(store.relations.len(), 0);
    assert_eq!(store.search_docs.len(), 1);
    assert_eq!(
        store.search_docs.values().next().unwrap().kind,
        "media.movie"
    );
}

#[test]
fn undeclared_software_section_file_is_rejected() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": { "assets": 0, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0 }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: String::new(),
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
                content: String::new(),
            },
            ExportFile {
                path: "modules/software.jsonl".into(),
                content: String::new(),
            },
        ],
    };
    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("does not declare"), "{err}");
}

#[test]
fn future_software_schema_version_is_rejected_before_mutation() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 99 } },
  "record_counts": { "assets": 0, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 0 }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: String::new(),
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
                content: String::new(),
            },
            ExportFile {
                path: "modules/software.jsonl".into(),
                content: String::new(),
            },
        ],
    };
    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(
        matches!(err, AppError::UnsupportedSchemaVersion { .. }),
        "{err}"
    );

    let store = env.factory.store();
    assert!(store.assets.is_empty(), "rejected bundle must not mutate");
}

#[test]
fn malformed_software_row_is_rejected() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 } },
  "record_counts": { "assets": 1, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 1 }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-00000000000b","kind":"software.cli","name":"Tool","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#.into(),
            },
            ExportFile { path: "external_refs.jsonl".into(), content: String::new() },
            ExportFile { path: "activity.jsonl".into(), content: String::new() },
            ExportFile { path: "tags.json".into(), content: "[]".into() },
            ExportFile { path: "asset_tags.jsonl".into(), content: String::new() },
            ExportFile { path: "modules/media.jsonl".into(), content: String::new() },
            // Unknown category is rejected.
            ExportFile {
                path: "modules/software.jsonl".into(),
                content: r#"{"asset_id":"00000000-0000-7000-8000-00000000000b","category":"hologram","install_source":"unknown","version":null,"install_location":null,"executable_path":null,"purpose":null,"notes":null,"discovered_at":null,"installed_at":null,"architecture":null}"#.into(),
            },
        ],
    };
    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("category"), "{err}");
}

#[test]
fn dangling_software_asset_and_duplicate_rows_are_rejected() {
    let make_bundle = |software_content: &str, software_count: usize| {
        PortableBundle {
        manifest: serde_json::from_str(
            r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 } },
  "record_counts": { "assets": 1, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 1 }
}"#,
        )
        .map(|mut m: serde_json::Value| {
            m["record_counts"]["software"] = software_count.into();
            m
        })
        .map(serde_json::from_value::<assetmesh_core::application::portable::PortableManifest>)
        .unwrap()
        .unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-00000000000c","kind":"software.cli","name":"Tool","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#.into(),
            },
            ExportFile { path: "external_refs.jsonl".into(), content: String::new() },
            ExportFile { path: "activity.jsonl".into(), content: String::new() },
            ExportFile { path: "tags.json".into(), content: "[]".into() },
            ExportFile { path: "asset_tags.jsonl".into(), content: String::new() },
            ExportFile { path: "modules/media.jsonl".into(), content: String::new() },
            ExportFile { path: "modules/software.jsonl".into(), content: software_content.into() },
        ],
    }
    };

    let software_row = |asset_id: &str| {
        format!(
            r#"{{"asset_id":"{asset_id}","category":"cli","install_source":"manual","version":null,"install_location":null,"executable_path":null,"purpose":null,"notes":null,"discovered_at":null,"installed_at":null,"architecture":null}}"#
        )
    };

    // Dangling asset id.
    let bundle = make_bundle(&software_row("00000000-0000-7000-8000-999999999999"), 1);
    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("missing asset"), "{err}");

    // Duplicate rows for the same asset.
    let row = software_row("00000000-0000-7000-8000-00000000000c");
    let bundle = make_bundle(&format!("{row}\n{row}"), 2);
    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(
        err.to_string().contains("duplicate software record"),
        "{err}"
    );
}

#[test]
fn relation_with_missing_endpoint_is_rejected() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": { "assets": 0, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "relations": 1 }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile { path: "assets.jsonl".into(), content: String::new() },
            ExportFile { path: "external_refs.jsonl".into(), content: String::new() },
            ExportFile { path: "activity.jsonl".into(), content: String::new() },
            ExportFile { path: "tags.json".into(), content: "[]".into() },
            ExportFile { path: "asset_tags.jsonl".into(), content: String::new() },
            ExportFile { path: "modules/media.jsonl".into(), content: String::new() },
            ExportFile {
                path: "relations.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-00000000000d","source_asset_id":"00000000-0000-7000-8000-000000000001","target_asset_id":"00000000-0000-7000-8000-000000000002","relation_type":"uses","note":null,"provenance":"manual","created_at":"2026-01-01T00:00:00Z"}"#.into(),
            },
        ],
    };
    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
}

#[test]
fn dry_run_matches_commit_for_software() {
    let env = test_env();
    create_software(&env, "ripgrep", SoftwareCategory::Cli, None, vec![]);

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let dry: PortableImportReport = import.import_bundle(&bundle, true).unwrap();
    let commit = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(dry.software_created, commit.software_created);
    assert_eq!(dry.software_created, 1);
    assert_eq!(commit.relations_created, 0);
}

#[test]
fn restored_software_is_searchable_and_rebuildable() {
    let env = test_env();
    create_software(
        &env,
        "ripgrep",
        SoftwareCategory::Cli,
        Some("fast grep"),
        vec![ref_input("homebrew_formula", "ripgrep")],
    );

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();

    let destination = test_env();
    let mut import = destination.portable_import_service();
    import.import_bundle(&bundle, false).unwrap();

    // Searchable without a rebuild...
    let mut search = destination.search_service();
    let hits = search.search("ripgrep", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "software.cli");

    // ...and rebuildable from canonical state.
    search.rebuild().unwrap();
    let hits = search.search("fast grep", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn deleting_search_state_does_not_lose_software() {
    let env = test_env();
    let id = create_software(&env, "ripgrep", SoftwareCategory::Cli, None, vec![]);

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    let destination = test_env();
    let mut import = destination.portable_import_service();
    import.import_bundle(&bundle, false).unwrap();

    destination
        .factory
        .store_borrow_mut(|store| store.search_docs.clear());

    let mut search = destination.search_service();
    assert!(search.search("ripgrep", 10).unwrap().is_empty());
    search.rebuild().unwrap();
    let hits = search.search("ripgrep", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].asset_id, id);

    // Canonical data intact.
    let mut software = SoftwareService::new(
        destination.factory.clone(),
        destination.clock.clone(),
        destination.ids.clone(),
    );
    let rows = software.list_software(&SoftwareFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn mixed_media_and_software_library_round_trips() {
    let env = test_env();
    let mut media = MediaService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    media
        .create_media(CreateMedia {
            title: "Frieren".into(),
            media_type: assetmesh_core::domain::media::MediaType::Anime,
            ..create_media_defaults()
        })
        .unwrap();
    create_software(&env, "ripgrep", SoftwareCategory::Cli, None, vec![]);

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    assert_eq!(bundle.manifest.record_counts["media"], 1);
    assert_eq!(bundle.manifest.record_counts["software"], 1);

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.media_created, 1);
    assert_eq!(report.software_created, 1);

    let mut search = destination.search_service();
    let hits = search.search("frieren", 10).unwrap();
    assert_eq!(hits.len(), 1);
    let hits = search.search("ripgrep", 10).unwrap();
    assert_eq!(hits.len(), 1);

    // All assets visible through the shared asset listing.
    let mut shared = destination.factory.clone();
    let assets = shared
        .read(&mut |q| {
            Ok(q.assets()
                .list(&AssetFilter {
                    lifecycle: Some(LifecycleFilter::All),
                    ..Default::default()
                })?
                .len())
        })
        .unwrap();
    assert_eq!(assets, 2);
}

fn create_media_defaults() -> CreateMedia {
    CreateMedia {
        title: String::new(),
        media_type: assetmesh_core::domain::media::MediaType::Movie,
        summary: None,
        status: None,
        rating: None,
        year: None,
        platform: None,
        progress: assetmesh_core::domain::media::Progress::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

#[test]
fn merge_repoints_relations_to_winner() {
    let env = test_env();
    let mut software =
        SoftwareService::new(env.factory.clone(), env.clock.clone(), env.ids.clone());
    let loser = software
        .create_software(CreateSoftware {
            name: "rg duplicate".into(),
            ..software_defaults(SoftwareCategory::Cli)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let winner = software
        .create_software(CreateSoftware {
            name: "ripgrep".into(),
            ..software_defaults(SoftwareCategory::Cli)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let jdk = create_software(&env, "OpenJDK", SoftwareCategory::Runtime, None, vec![]);

    let mut relations = assetmesh_core::application::relation_service::RelationService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    relations
        .attach(
            jdk,
            RelationType::Uses,
            loser,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();

    let mut assets = assetmesh_core::application::asset_service::AssetService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    assets.merge_assets(loser, winner).unwrap();

    let views = relations.list_for_asset(winner).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].other_asset_id, jdk);
    assert_eq!(views[0].relation_type, RelationType::UsedBy);

    // Loser tombstoned, no orphan software record or relation left.
    let store = env.factory.store();
    assert!(store.software.contains_key(&winner.to_string()));
    assert!(!store.software.contains_key(&loser.to_string()));
    assert_eq!(store.relations.len(), 1);
}

#[test]
fn bundle_carrying_both_statements_of_one_relation_fact_is_rejected() {
    // "A depends_on B" and "B dependency_of A" are the same fact; a bundle
    // carrying both statements does not import as two rows.
    let asset_a = "00000000-0000-7000-8000-000000000001";
    let asset_b = "00000000-0000-7000-8000-000000000002";
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": { "assets": 2, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "relations": 2 }
}"#;
    let relation_row = |id: &str, source: &str, target: &str, kind: &str| {
        format!(
            r#"{{"id":"{id}","source_asset_id":"{source}","target_asset_id":"{target}","relation_type":"{kind}","note":null,"provenance":"manual","created_at":"2026-01-01T00:00:00Z"}}"#
        )
    };
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    r#"{{"id":"{asset_a}","kind":"software.cli","name":"A","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}
{{"id":"{asset_b}","kind":"software.runtime","name":"B","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}"#
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
                content: String::new(),
            },
            ExportFile {
                path: "relations.jsonl".into(),
                content: format!(
                    "{}\n{}",
                    relation_row(
                        "00000000-0000-7000-8000-00000000000e",
                        asset_a,
                        asset_b,
                        "depends_on"
                    ),
                    relation_row(
                        "00000000-0000-7000-8000-00000000000f",
                        asset_b,
                        asset_a,
                        "dependency_of"
                    )
                ),
            },
        ],
    };

    let env = test_env();
    let mut import = env.portable_import_service();
    let err = import.import_bundle(&bundle, false).unwrap_err();
    assert!(err.to_string().contains("duplicate relation"), "{err}");
}

#[test]
fn bundle_relation_stated_via_inverse_type_normalizes_on_import() {
    // A single fact stated via the inverse type imports as its canonical
    // primary representation.
    let asset_a = "00000000-0000-7000-8000-000000000001";
    let asset_b = "00000000-0000-7000-8000-000000000002";
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": { "assets": 2, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "relations": 1 }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    r#"{{"id":"{asset_a}","kind":"software.cli","name":"A","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}
{{"id":"{asset_b}","kind":"software.runtime","name":"B","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}"#
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
                content: String::new(),
            },
            ExportFile {
                path: "relations.jsonl".into(),
                content: format!(
                    r#"{{"id":"00000000-0000-7000-8000-000000000011","source_asset_id":"{asset_b}","target_asset_id":"{asset_a}","relation_type":"dependency_of","note":null,"provenance":"manual","created_at":"2026-01-01T00:00:00Z"}}"#
                ),
            },
        ],
    };

    let env = test_env();
    let mut import = env.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.relations_created, 1);

    let store = env.factory.store();
    let relation = store.relations.values().next().unwrap();
    assert_eq!(
        relation.relation_type,
        assetmesh_core::domain::relation::RelationType::DependsOn
    );
    assert_eq!(relation.source_asset_id.to_string(), asset_a);
    assert_eq!(relation.target_asset_id.to_string(), asset_b);
}

fn software_normalization_bundle(purpose: &str) -> PortableBundle {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 } },
  "record_counts": {
    "assets": 1, "external_refs": 0, "activity": 0,
    "tags": 0, "asset_tags": 0, "media": 0, "software": 1
  }
}"#;
    let row = format!(
        r#"{{"asset_id":"00000000-0000-7000-8000-0000000000c1","category":"cli","install_source":"manual","version":"  9.9.9  ","install_location":"  /opt/padded  ","executable_path":null,"purpose":{purpose},"notes":"   ","discovered_at":null,"installed_at":null,"architecture":null}}"#
    );
    PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-0000000000c1","kind":"software.cli","name":"Padded","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#.to_string(),
            },
            ExportFile { path: "external_refs.jsonl".into(), content: String::new() },
            ExportFile { path: "activity.jsonl".into(), content: String::new() },
            ExportFile { path: "tags.json".into(), content: "[]".to_string() },
            ExportFile { path: "asset_tags.jsonl".into(), content: String::new() },
            ExportFile { path: "modules/media.jsonl".into(), content: String::new() },
            ExportFile { path: "modules/software.jsonl".into(), content: row },
        ],
    }
}

#[test]
fn import_normalizes_software_text_including_user_owned_fields() {
    // A bundle carrying untrimmed free text — including the user-owned
    // purpose/notes — must not persist raw values into canonical state.
    let bundle = software_normalization_bundle(r#""  why installed  ""#);
    let env = test_env();
    let report = env
        .portable_import_service()
        .import_bundle(&bundle, false)
        .unwrap();
    assert_eq!(report.software_created, 1);

    let store = env.factory.store();
    let id =
        AssetId::from_uuid(uuid::Uuid::parse_str("00000000-0000-7000-8000-0000000000c1").unwrap());
    let record = store.software.get(&id.to_string()).unwrap();
    assert_eq!(record.version.as_deref(), Some("9.9.9"));
    assert_eq!(record.install_location.as_deref(), Some("/opt/padded"));
    assert_eq!(record.purpose.as_deref(), Some("why installed"));
    // Whitespace-only text collapses to None rather than an empty string.
    assert_eq!(record.notes, None);
}

#[test]
fn import_rejects_control_characters_in_software_purpose() {
    // The purpose string carries the JSON escape backslash-u0000, which
    // decodes to a real NUL: it must be rejected rather than stored.
    let bundle = software_normalization_bundle("\"why\\u0000installed\"");
    let env = test_env();
    let err = env.portable_import_service().import_bundle(&bundle, false);
    assert!(
        err.is_err(),
        "control character in purpose must be rejected"
    );
    let store = env.factory.store();
    assert_eq!(store.software.len(), 0, "no partial state may be committed");
    assert_eq!(store.assets.len(), 0);
}

const RETYPE_ASSET: &str = "00000000-0000-7000-8000-0000000000d1";

fn legacy_media_bundle_retyping_to(kind: &str) -> PortableBundle {
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
    let media_type = if kind.starts_with("media.") {
        kind.strip_prefix("media.").unwrap()
    } else {
        "movie"
    };
    PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    r#"{{"id":"{RETYPE_ASSET}","kind":"{kind}","name":"Retyped","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}"#
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
                content: "[]".to_string(),
            },
            ExportFile {
                path: "asset_tags.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "modules/media.jsonl".into(),
                content: format!(
                    r#"{{"asset_id":"{RETYPE_ASSET}","media_type":"{media_type}","status":"completed","rating":7.0,"year":null,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}}"#
                ),
            },
        ],
    }
}

#[test]
fn legacy_bundle_cannot_retype_software_asset_to_media_while_record_exists() {
    // The destination holds a software.cli asset with a software record. A
    // legacy (media-only) bundle re-declaring the same id as media.movie must
    // fail loudly: it would strand the software record on a media asset.
    let env = test_env();
    let id = AssetId::from_uuid(uuid::Uuid::parse_str(RETYPE_ASSET).unwrap());
    let now = chrono::Utc::now();
    env.factory.store_borrow_mut(|store| {
        let mut asset = Asset::new(id, AssetKind::SoftwareCli, "Retyped", None, now).unwrap();
        asset.touch(now);
        store.assets.insert(id.to_string(), asset);
        store.software.insert(
            id.to_string(),
            SoftwareRecord::new(id, SoftwareCategory::Cli),
        );
    });

    let bundle = legacy_media_bundle_retyping_to("media.movie");
    // Dry-run and commit must both fail identically.
    let dry = env.portable_import_service().import_bundle(&bundle, true);
    let commit = env.portable_import_service().import_bundle(&bundle, false);
    for err in [dry, commit] {
        let err = err.unwrap_err();
        assert!(matches!(err, AppError::ImportConflict { .. }), "{err}");
        assert!(err.to_string().contains("re-types"), "{err}");
    }
    // No partial mutation.
    let store = env.factory.store();
    assert_eq!(
        store.assets.get(&id.to_string()).unwrap().kind,
        AssetKind::SoftwareCli
    );
    assert!(store.software.contains_key(&id.to_string()));
}

#[test]
fn direct_kind_change_stranding_module_record_is_rejected() {
    // Even a direct UnitOfWork write (bypassing every application service)
    // cannot strand a software record under a media.* kind.
    let mut env = test_env();
    let id = AssetId::generate();
    let now = chrono::Utc::now();
    env.factory.store_borrow_mut(|store| {
        let asset = Asset::new(id, AssetKind::SoftwareCli, "Strand", None, now).unwrap();
        store.assets.insert(id.to_string(), asset);
        store.software.insert(
            id.to_string(),
            SoftwareRecord::new(id, SoftwareCategory::Cli),
        );
    });

    let mut media_asset = Asset::new(id, AssetKind::MediaMovie, "Strand", None, now).unwrap();
    media_asset.touch(now);
    let err = env
        .factory
        .transact(&mut |uow| uow.assets().update(&media_asset))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("software record"), "{err}");

    // A within-module change is refused for the same reason: `cli` is the
    // record's own discriminator, so a `software.tool` asset would strand a
    // `cli` record just as a media.* asset would.
    let mut tool_asset = Asset::new(id, AssetKind::SoftwareTool, "Strand", None, now).unwrap();
    tool_asset.touch(now);
    let err = env
        .factory
        .transact(&mut |uow| uow.assets().update(&tool_asset))
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err}");
    assert!(err.to_string().contains("software record"), "{err}");
    assert_eq!(
        env.factory
            .store()
            .assets
            .get(&id.to_string())
            .unwrap()
            .kind,
        AssetKind::SoftwareCli,
        "the rejected re-type changed nothing"
    );

    // Removing the record first unlocks both the cross- and within-module
    // change: nothing is left to strand.
    env.factory
        .transact(&mut |uow| uow.software().delete(id))
        .unwrap();
    env.factory
        .transact(&mut |uow| uow.assets().update(&media_asset))
        .unwrap();
    env.factory
        .transact(&mut |uow| uow.assets().update(&tool_asset))
        .unwrap();
}

#[test]
fn same_module_retype_is_rejected_identically_by_dry_run_and_commit() {
    // A bundle that re-types an asset WITHIN its module (media.movie ->
    // media.game) strands the old detail record just as surely as a
    // cross-module re-type would: the media_type discriminator only matches
    // the kind assigned at creation. The repository boundary rejects it at
    // commit, so preflight must reject it too — the "dry-run and commit run
    // the same checks" contract is only true if this check is in the shared
    // preflight, not inferred from the destination snapshot's module names.
    let asset_id = "00000000-0000-7000-8000-00000000c001";
    let other_id = "00000000-0000-7000-8000-00000000c002";

    // Constant across both bundles: one asset row, one media row, media
    // schema v1.
    let manifest = r#"{"format":"assetmesh-portable-export","version":1,"created_at":"2026-01-01T00:00:00Z","app_version":"t","modules":{"media":{"schema_version":1}},"record_counts":{"assets":2,"external_refs":0,"activity":0,"tags":0,"asset_tags":0,"media":1}}"#;

    let bundle = |kind: &str, media_type: &str| PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    "{}\n{}\n",
                    asset_row(other_id, "Other", "media.movie", "active", None),
                    asset_row(asset_id, "Victim", kind, "active", None)
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
                content: format!("{}\n", media_row(asset_id, media_type)),
            },
        ],
    };

    // The destination holds a media.movie asset with a movie record.
    let env = test_env();
    let mut import = env.portable_import_service();
    import
        .import_bundle(&bundle("media.movie", "movie"), false)
        .unwrap();

    // A bundle re-typing it to media.game is refused by BOTH paths, with the
    // same error, and leaves the destination untouched.
    let retyped = bundle("media.game", "game");
    let dry = import.import_bundle(&retyped, true).unwrap_err();
    let commit = import.import_bundle(&retyped, false).unwrap_err();
    assert!(
        matches!(dry, AppError::ImportConflict { .. }),
        "dry-run: {dry:?}"
    );
    assert!(
        matches!(commit, AppError::ImportConflict { .. }),
        "commit: {commit:?}"
    );
    assert_eq!(dry.to_string(), commit.to_string());
    assert!(dry.to_string().contains("media record"), "{dry:?}");

    // Nothing changed: the asset is still a movie, and the record is intact.
    let mut env = env;
    let victim_kind = env.factory.read(&mut |q| {
        Ok(q.assets()
            .list(&AssetFilter::default())?
            .into_iter()
            .find(|a| a.name == "Victim")
            .unwrap()
            .kind)
    });
    assert_eq!(victim_kind.map(|k| k.as_str()).unwrap(), "media.movie");
}

fn asset_row(
    id: &str,
    name: &str,
    kind: &str,
    lifecycle: &str,
    merged_into: Option<&str>,
) -> String {
    let merged = merged_into
        .map(|t| format!(r#""{t}""#))
        .unwrap_or_else(|| "null".into());
    format!(
        r#"{{"id":"{id}","kind":"{kind}","name":"{name}","summary":null,"lifecycle_state":"{lifecycle}","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":{merged}}}"#
    )
}

fn media_row(asset_id: &str, media_type: &str) -> String {
    format!(
        r#"{{"asset_id":"{asset_id}","media_type":"{media_type}","status":"planned","rating":null,"year":null,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}}"#
    )
}
