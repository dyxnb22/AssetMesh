//! Portable bundle tests for the Services module: round trips, legacy
//! Phase 2 compatibility, declaration semantics, and failure modes
//! (docs/10 "Portable data").

mod support;

use assetmesh_core::application::media_service::ExternalRefInput;
use assetmesh_core::application::portable::ExportFile;
use assetmesh_core::application::portable::{PortableBundle, PortableImportReport};
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::service::{BillingCadence, ServiceType};
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter, ServiceFilter};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::AppError;
use support::{test_env, TestEnv};

fn create_cmd(name: &str, service_type: ServiceType) -> CreateService {
    CreateService {
        name: name.into(),
        service_type,
        summary: None,
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost_minor: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
    }
}

fn full_cmd(name: &str, service_type: ServiceType) -> CreateService {
    CreateService {
        provider: Some("OpenAI".into()),
        account_label: Some("Work".into()),
        endpoint_url: Some("https://api.openai.com/v1".into()),
        dashboard_url: Some("https://platform.openai.com".into()),
        plan: Some("Plus".into()),
        cost_minor: Some(1999),
        currency: Some("USD".into()),
        billing_cadence: Some(BillingCadence::Monthly),
        renews_at: Some(ts("2026-10-01T00:00:00Z")),
        expires_at: Some(ts("2027-01-01T00:00:00Z")),
        auto_renew: Some(true),
        notes: Some("Team plan".into()),
        tags: vec!["ai".into()],
        external_refs: vec![ref_input("openai", "org-work")],
        ..create_cmd(name, service_type)
    }
}

fn ref_input(namespace: &str, external_id: &str) -> ExternalRefInput {
    ExternalRefInput {
        namespace: namespace.into(),
        external_id: external_id.into(),
        source_url: None,
    }
}

fn ts(value: &str) -> assetmesh_core::domain::Timestamp {
    chrono::DateTime::parse_from_rfc3339(value)
        .expect("test timestamps are well-formed")
        .with_timezone(&chrono::Utc)
}

fn create_service(env: &TestEnv, cmd: CreateService) -> AssetId {
    ServiceService::new(env.factory.clone(), env.clock.clone(), env.ids.clone())
        .create_service(cmd)
        .unwrap()
        .entry
        .asset
        .id
}

/// Canonical service state, comparable across environments.
fn canonical_json(env: &TestEnv) -> serde_json::Value {
    let store = env.factory.store();
    let services: Vec<serde_json::Value> = store
        .services
        .values()
        .map(|s| {
            serde_json::json!({
                "asset_id": s.asset_id.to_string(),
                "service_type": s.service_type.as_str(),
                "provider": s.provider,
                "account_label": s.account_label,
                "endpoint_url": s.endpoint_url,
                "dashboard_url": s.dashboard_url,
                "domain_name": s.domain_name,
                "plan": s.plan,
                "cost_minor": s.cost_minor,
                "currency": s.currency,
                "billing_cadence": s.billing_cadence.map(|c| c.as_str()),
                "renews_at": s.renews_at.map(|t| t.to_rfc3339()),
                "expires_at": s.expires_at.map(|t| t.to_rfc3339()),
                "auto_renew": s.auto_renew,
                "notes": s.notes,
            })
        })
        .collect();
    serde_json::json!({ "services": services })
}

#[test]
fn services_round_trip_preserves_canonical_state() {
    let env = test_env();
    create_service(&env, full_cmd("OpenAI", ServiceType::Saas));
    create_service(
        &env,
        CreateService {
            provider: Some("Cloudflare".into()),
            domain_name: Some("AssetMesh.Dev".into()),
            expires_at: Some(ts("2027-09-20T00:00:00Z")),
            auto_renew: Some(true),
            ..create_cmd("assetmesh.dev", ServiceType::Domain)
        },
    );

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    assert_eq!(bundle.manifest.modules["services"].schema_version, 1);
    assert_eq!(bundle.manifest.record_counts["services"], 2);
    assert!(bundle
        .file("modules/services.jsonl")
        .unwrap()
        .contains("assetmesh.dev"));

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.services_created, 2);
    assert_eq!(report.assets_created, 2);

    assert_eq!(
        canonical_json(&destination),
        canonical_json(&env),
        "canonical service state must match"
    );

    // Idempotent re-import.
    let report2 = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report2.services_created, 0);
    assert_eq!(report2.services_updated, 2);
    assert_eq!(canonical_json(&destination), canonical_json(&env));

    // Export → import → export is byte-stable for the services section.
    let mut export2 = destination.export_service();
    let bundle2 = export2.export("test").unwrap();
    let strip = |b: &PortableBundle| b.file("modules/services.jsonl").unwrap().to_string();
    assert_eq!(strip(&bundle), strip(&bundle2));
}

#[test]
fn restored_services_are_searchable_and_rebuildable() {
    let env = test_env();
    create_service(&env, full_cmd("OpenAI", ServiceType::Saas));

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();

    let destination = test_env();
    let mut import = destination.portable_import_service();
    import.import_bundle(&bundle, false).unwrap();

    // Searchable without a rebuild (the projection was written on import)...
    let mut search = destination.search_service();
    let hits = search.search("openai", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "service.saas");

    // ...and rebuildable from canonical state alone.
    search.rebuild().unwrap();
    let hits = search.search("Team plan", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn mixed_media_software_and_services_library_round_trips() {
    let env = test_env();
    let mut media = assetmesh_core::application::media_service::MediaService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    media
        .create_media(assetmesh_core::application::media_service::CreateMedia {
            title: "Frieren".into(),
            media_type: assetmesh_core::domain::media::MediaType::Anime,
            ..media_defaults()
        })
        .unwrap();
    let mut software = assetmesh_core::application::software_service::SoftwareService::new(
        env.factory.clone(),
        env.clock.clone(),
        env.ids.clone(),
    );
    software
        .create_software(
            assetmesh_core::application::software_service::CreateSoftware {
                name: "ripgrep".into(),
                category: assetmesh_core::domain::software::SoftwareCategory::Cli,
                ..software_defaults()
            },
        )
        .unwrap();
    create_service(&env, full_cmd("OpenAI", ServiceType::Saas));

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    assert_eq!(bundle.manifest.record_counts["media"], 1);
    assert_eq!(bundle.manifest.record_counts["software"], 1);
    assert_eq!(bundle.manifest.record_counts["services"], 1);

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.media_created, 1);
    assert_eq!(report.software_created, 1);
    assert_eq!(report.services_created, 1);

    let mut search = destination.search_service();
    for (query, kind) in [
        ("frieren", "media.anime"),
        ("ripgrep", "software.cli"),
        ("openai", "service.saas"),
    ] {
        let hits = search.search(query, 10).unwrap();
        assert_eq!(hits.len(), 1, "{query}");
        assert_eq!(hits[0].kind, kind);
    }
}

#[test]
fn declared_services_section_is_authoritative_and_reconciles() {
    // A declared section is authoritative: a service record the bundle no
    // longer carries is removed from the destination, exactly like
    // media/software (docs/10 "Import conflict policy").
    let mut source = test_env();
    let kept = create_service(&source, full_cmd("OpenAI", ServiceType::Saas));
    let dropped = create_service(&source, create_cmd("Cancelled", ServiceType::Saas));

    let mut export = source.export_service();
    let bundle_before = export.export("test").unwrap();
    assert_eq!(bundle_before.manifest.record_counts["services"], 2);

    // The destination imports the earlier bundle: both records exist.
    let destination = test_env();
    let mut import = destination.portable_import_service();
    import.import_bundle(&bundle_before, false).unwrap();
    assert_eq!(
        destination
            .service_service()
            .list_services(&ServiceFilter::default())
            .unwrap()
            .len(),
        2
    );

    // The source drops the cancelled service's record (the asset itself stays).
    source
        .factory
        .transact(&mut |uow| uow.services().delete(dropped))
        .unwrap();
    let bundle_after = source.export_service().export("test").unwrap();
    assert_eq!(bundle_after.manifest.record_counts["services"], 1);

    // Re-importing reconciles the destination back to the bundle.
    let report = import.import_bundle(&bundle_after, false).unwrap();
    assert_eq!(report.services_created, 0);
    assert_eq!(report.services_updated, 1);
    let rows = destination
        .service_service()
        .list_services(&ServiceFilter::default())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.asset.id, kept);

    // The reconciled-away record's search document is gone; the survivor's is
    // still there.
    let store = destination.factory.store();
    assert!(!store.services.contains_key(&dropped.to_string()));
    assert!(!store.search_docs.contains_key(&dropped.to_string()));
    assert!(store.search_docs.contains_key(&kept.to_string()));
}

#[test]
fn legacy_phase2_bundle_without_services_imports_and_preserves_destination() {
    // A bundle exported by Phase 2 declares media + software but no services
    // section: it imports cleanly and leaves pre-existing destination service
    // state untouched (docs/10 declaration compatibility).
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "phase2-fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 } },
  "record_counts": {
    "assets": 1, "external_refs": 0, "activity": 0,
    "tags": 0, "asset_tags": 0, "media": 1, "software": 0
  }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: r#"{"id":"00000000-0000-7000-8000-00000000000a","kind":"media.movie","name":"Legacy Movie","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}"#.to_string(),
            },
            ExportFile { path: "external_refs.jsonl".into(), content: String::new() },
            ExportFile { path: "activity.jsonl".into(), content: String::new() },
            ExportFile { path: "tags.json".into(), content: "[]".to_string() },
            ExportFile { path: "asset_tags.jsonl".into(), content: String::new() },
            ExportFile {
                path: "modules/media.jsonl".into(),
                content: r#"{"asset_id":"00000000-0000-7000-8000-00000000000a","media_type":"movie","status":"completed","rating":8.0,"year":1999,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}"#.to_string(),
            },
            ExportFile { path: "modules/software.jsonl".into(), content: String::new() },
        ],
    };

    // The destination holds one service record from before the import.
    let env = test_env();
    let existing = create_service(&env, full_cmd("OpenAI", ServiceType::Saas));

    let mut import = env.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.assets_created, 1);
    assert_eq!(report.media_created, 1);
    assert_eq!(report.services_created, 0);

    // The pre-existing service data survived the legacy import untouched.
    let store = env.factory.store();
    assert_eq!(store.services.len(), 1);
    assert!(store.services.contains_key(&existing.to_string()));
}

#[test]
fn undeclared_services_section_file_is_rejected() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 } },
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
            ExportFile {
                path: "modules/services.jsonl".into(),
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
fn declared_services_section_without_its_file_is_rejected() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 }, "services": { "schema_version": 1 } },
  "record_counts": { "assets": 0, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 0, "services": 0 }
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
    assert!(err.to_string().contains("missing required file"), "{err}");
    assert!(err.to_string().contains("services.jsonl"), "{err}");
}

#[test]
fn future_services_schema_version_is_rejected_before_mutation() {
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 }, "services": { "schema_version": 99 } },
  "record_counts": { "assets": 0, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 0, "services": 0 }
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
            ExportFile {
                path: "modules/services.jsonl".into(),
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

/// One-asset bundle declaring a services section, for failure-mode tests.
fn services_bundle(asset_id: &str, row: &str, count: usize) -> PortableBundle {
    let manifest = format!(
        r#"{{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": {{ "media": {{ "schema_version": 1 }}, "software": {{ "schema_version": 1 }}, "services": {{ "schema_version": 1 }} }},
  "record_counts": {{ "assets": 1, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 0, "services": {count} }}
}}"#
    );
    PortableBundle {
        manifest: serde_json::from_str(&manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    r#"{{"id":"{asset_id}","kind":"service.saas","name":"Bundled","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}"#
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
                path: "modules/software.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "modules/services.jsonl".into(),
                content: row.into(),
            },
        ],
    }
}

fn service_row(asset_id: &str, service_type: &str) -> String {
    format!(
        r#"{{"asset_id":"{asset_id}","service_type":"{service_type}","provider":"  Padded  ","account_label":null,"endpoint_url":null,"dashboard_url":null,"domain_name":null,"plan":"Plus","cost_minor":1999,"currency":"usd","billing_cadence":"monthly","renews_at":"2026-10-01T00:00:00Z","expires_at":null,"auto_renew":true,"notes":"  spaced  "}}"#
    )
}

#[test]
fn malformed_service_rows_fail_preflight_identically_for_dry_run_and_commit() {
    let cases: [(&str, &str, &str); 6] = [
        // Unknown service type.
        (
            "hologram",
            "00000000-0000-7000-8000-000000000001",
            "unknown type",
        ),
        // Unknown billing cadence.
        (
            "fortnightly",
            "00000000-0000-7000-8000-000000000002",
            "billing_cadence",
        ),
        // Money without its currency pair (ADR 0010).
        (
            "unpaired",
            "00000000-0000-7000-8000-000000000003",
            "both present or both absent",
        ),
        // Malformed currency.
        (
            "currency",
            "00000000-0000-7000-8000-000000000004",
            "currency",
        ),
        // A credential-bearing URL is rejected, never parsed out (ADR 0010).
        ("url", "00000000-0000-7000-8000-000000000005", "credentials"),
        // domain_name on a non-domain service is canonical-metadata misuse.
        (
            "domain",
            "00000000-0000-7000-8000-000000000006",
            "domain_name",
        ),
    ];

    for (variant, asset_id, expected) in cases {
        let row = match variant {
            "hologram" => service_row(asset_id, "hologram"),
            "fortnightly" => {
                service_row(asset_id, "saas").replace("\"monthly\"", "\"fortnightly\"")
            }
            "unpaired" => service_row(asset_id, "saas").replace(",\"currency\":\"usd\"", ""),
            "currency" => service_row(asset_id, "saas").replace("\"usd\"", "\"DOLLAR\""),
            "url" => service_row(asset_id, "saas").replace(
                "\"endpoint_url\":null",
                "\"endpoint_url\":\"https://u:p@x.io\"",
            ),
            _ => service_row(asset_id, "saas")
                .replace("\"domain_name\":null", "\"domain_name\":\"assetmesh.dev\""),
        };
        let bundle = services_bundle(asset_id, &row, 1);

        let env = test_env();
        let mut import = env.portable_import_service();
        let dry = import.import_bundle(&bundle, true).unwrap_err();
        let commit = import.import_bundle(&bundle, false).unwrap_err();
        for err in [&dry, &commit] {
            assert!(
                err.to_string().contains(expected),
                "{variant}: expected {expected:?} in {err}"
            );
        }
        assert_eq!(dry.to_string(), commit.to_string(), "{variant}");
        // No partial state.
        let store = env.factory.store();
        assert_eq!(store.services.len(), 0, "{variant}");
        assert_eq!(store.assets.len(), 0, "{variant}");
    }
}

#[test]
fn import_normalizes_service_text_including_user_owned_notes() {
    let row = service_row("00000000-0000-7000-8000-0000000000c1", "saas");
    let bundle = services_bundle("00000000-0000-7000-8000-0000000000c1", &row, 1);

    let env = test_env();
    let report = env
        .portable_import_service()
        .import_bundle(&bundle, false)
        .unwrap();
    assert_eq!(report.services_created, 1);

    let store = env.factory.store();
    let id =
        AssetId::from_uuid(uuid::Uuid::parse_str("00000000-0000-7000-8000-0000000000c1").unwrap());
    let record = store.services.get(&id.to_string()).unwrap();
    assert_eq!(record.provider.as_deref(), Some("Padded"));
    assert_eq!(record.notes.as_deref(), Some("spaced"));
    assert_eq!(record.currency.as_deref(), Some("USD"));
    assert_eq!(record.cost_minor, Some(1999));
}

#[test]
fn dangling_duplicate_and_mistyped_service_rows_are_rejected() {
    // Dangling asset reference.
    let row = service_row("00000000-0000-7000-8000-999999999999", "saas");
    let bundle = services_bundle("00000000-0000-7000-8000-00000000000c", &row, 1);
    let env = test_env();
    let err = env
        .portable_import_service()
        .import_bundle(&bundle, false)
        .unwrap_err();
    assert!(err.to_string().contains("missing asset"), "{err}");

    // Two rows for one asset.
    let asset_id = "00000000-0000-7000-8000-00000000000c";
    let row = service_row(asset_id, "saas");
    let bundle = services_bundle(asset_id, &format!("{row}\n{row}"), 2);
    let env = test_env();
    let err = env
        .portable_import_service()
        .import_bundle(&bundle, false)
        .unwrap_err();
    assert!(
        err.to_string().contains("duplicate service record"),
        "{err}"
    );

    // Kind ↔ type mismatch: a saas record on a service.api asset.
    let row = service_row(asset_id, "saas");
    let mut bundle = services_bundle(asset_id, &row, 1);
    bundle.files.iter_mut().for_each(|f| {
        if f.path == "assets.jsonl" {
            f.content = f.content.replace("service.saas", "service.api");
        }
    });
    let env = test_env();
    let err = env
        .portable_import_service()
        .import_bundle(&bundle, false)
        .unwrap_err();
    assert!(err.to_string().contains("service record is a"), "{err}");
}

#[test]
fn dry_run_matches_commit_for_services() {
    let env = test_env();
    create_service(&env, full_cmd("OpenAI", ServiceType::Saas));

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let dry: PortableImportReport = import.import_bundle(&bundle, true).unwrap();
    let commit = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(dry.services_created, commit.services_created);
    assert_eq!(dry.services_created, 1);
    assert_eq!(dry.assets_created, commit.assets_created);

    // Dry-run left nothing behind.
    let store = destination.factory.store();
    assert_eq!(store.services.len(), 1);
}

#[test]
fn a_service_record_on_a_merged_tombstone_is_rejected_before_mutation() {
    // A merged identity keeps no module detail (docs/10). The bundle below
    // tries to attach a service record to the loser tombstone, so preflight
    // must refuse it — and dry-run must fail exactly as commit does, with
    // nothing written either way.
    let winner = "00000000-0000-7000-8000-0000000000e1";
    let loser = "00000000-0000-7000-8000-0000000000e2";
    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 }, "software": { "schema_version": 1 }, "services": { "schema_version": 1 } },
  "record_counts": { "assets": 2, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 0, "software": 0, "services": 1 }
}"#
    .to_string();
    let asset_row = |id: &str, lifecycle: &str, merged_into: Option<&str>| {
        let merged = merged_into
            .map(|t| format!(r#""{t}""#))
            .unwrap_or_else(|| "null".into());
        format!(
            r#"{{"id":"{id}","kind":"service.saas","name":"Asset {id}","summary":null,"lifecycle_state":"{lifecycle}","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":{merged}}}"#
        )
    };
    let bundle = |service_asset: &str| PortableBundle {
        manifest: serde_json::from_str(&manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    "{}\n{}",
                    asset_row(winner, "active", None),
                    asset_row(loser, "merged", Some(winner))
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
                path: "modules/software.jsonl".into(),
                content: String::new(),
            },
            ExportFile {
                path: "modules/services.jsonl".into(),
                content: service_row(service_asset, "saas"),
            },
        ],
    };

    // The record on the tombstone is the corruption under test.
    let env = test_env();
    let dry_err = env
        .portable_import_service()
        .import_bundle(&bundle(loser), true)
        .unwrap_err();
    assert!(dry_err.to_string().contains("merged asset"), "{dry_err}");
    let commit_err = env
        .portable_import_service()
        .import_bundle(&bundle(loser), false)
        .unwrap_err();
    assert_eq!(dry_err.to_string(), commit_err.to_string());
    {
        let store = env.factory.store();
        assert!(store.services.is_empty(), "dry-run/commit wrote nothing");
        assert!(store.assets.is_empty(), "dry-run/commit wrote nothing");
    }

    // The same bundle with the record on the winner is legitimate: the
    // tombstone keeps no module detail, the winner keeps the record.
    let env = test_env();
    let report = env
        .portable_import_service()
        .import_bundle(&bundle(winner), false)
        .unwrap();
    assert_eq!(report.services_created, 1);

    let store = env.factory.store();
    assert_eq!(store.services.len(), 1);
    assert!(store
        .services
        .contains_key(&AssetId::from_uuid(uuid::Uuid::parse_str(winner).unwrap()).to_string()));
    // The tombstone has no service record and no search document.
    assert!(!store
        .services
        .contains_key(&AssetId::from_uuid(uuid::Uuid::parse_str(loser).unwrap()).to_string()));
    assert!(!store
        .search_docs
        .contains_key(&AssetId::from_uuid(uuid::Uuid::parse_str(loser).unwrap()).to_string()));
}

fn media_defaults() -> assetmesh_core::application::media_service::CreateMedia {
    assetmesh_core::application::media_service::CreateMedia {
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

fn software_defaults() -> assetmesh_core::application::software_service::CreateSoftware {
    assetmesh_core::application::software_service::CreateSoftware {
        name: String::new(),
        category: assetmesh_core::domain::software::SoftwareCategory::Tool,
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

#[test]
fn legacy_bundle_cannot_retype_a_service_asset_away_while_the_record_exists() {
    // The destination holds a service.saas asset with a service record. A
    // legacy bundle re-declaring the same id as media.movie must fail loudly:
    // it would strand the service record on a media asset.
    let env = test_env();
    let existing = create_service(&env, full_cmd("Retyped", ServiceType::Saas));
    let id = existing;

    let manifest = r#"{
  "format": "assetmesh-portable-export",
  "version": 1,
  "created_at": "2026-01-01T00:00:00Z",
  "app_version": "fixture",
  "modules": { "media": { "schema_version": 1 } },
  "record_counts": { "assets": 1, "external_refs": 0, "activity": 0, "tags": 0, "asset_tags": 0, "media": 1 }
}"#;
    let bundle = PortableBundle {
        manifest: serde_json::from_str(manifest).unwrap(),
        files: vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: format!(
                    r#"{{"id":"{id}","kind":"media.movie","name":"Retyped","summary":null,"lifecycle_state":"active","revision":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","archived_at":null,"merged_into":null}}"#
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
                    r#"{{"asset_id":"{id}","media_type":"movie","status":"completed","rating":null,"year":null,"platform":null,"progress_current":null,"progress_total":null,"progress_unit":null,"notes":null,"started_at":null,"completed_at":null}}"#
                ),
            },
        ],
    };

    let mut import = env.portable_import_service();
    for dry_run in [true, false] {
        let err = import.import_bundle(&bundle, dry_run).unwrap_err();
        assert!(matches!(err, AppError::ImportConflict { .. }), "{err}");
        assert!(err.to_string().contains("re-types"), "{err}");
    }
    // No partial mutation.
    let store = env.factory.store();
    assert_eq!(
        store.assets.get(&id.to_string()).unwrap().kind.as_str(),
        "service.saas"
    );
    assert!(store.services.contains_key(&id.to_string()));
}

#[test]
fn service_records_of_every_type_round_trip() {
    // Every ServiceType ↔ AssetKind pair survives the wire format.
    let env = test_env();
    let mut ids: Vec<AssetId> = [
        ServiceType::Saas,
        ServiceType::Api,
        ServiceType::Vps,
        ServiceType::Local,
    ]
    .iter()
    .map(|ty| create_service(&env, create_cmd(&format!("{ty}"), *ty)))
    .collect();
    // A domain service carries its canonical domain_name.
    ids.push(create_service(
        &env,
        CreateService {
            domain_name: Some("AssetMesh.Dev".into()),
            ..create_cmd("assetmesh.dev", ServiceType::Domain)
        },
    ));

    let mut export = env.export_service();
    let bundle = export.export("test").unwrap();
    assert_eq!(bundle.manifest.record_counts["services"], 5);

    let destination = test_env();
    let mut import = destination.portable_import_service();
    let report = import.import_bundle(&bundle, false).unwrap();
    assert_eq!(report.services_created, 5);
    assert_eq!(canonical_json(&destination), canonical_json(&env));

    // Each restored record still satisfies the module invariants (the
    // repository boundary re-validates on read paths via list).
    let rows = destination
        .service_service()
        .list_services(&ServiceFilter::default())
        .unwrap();
    assert_eq!(rows.len(), 5);
    let domain = rows
        .iter()
        .find(|r| r.entry.record.service_type == ServiceType::Domain)
        .unwrap();
    assert_eq!(
        domain.entry.record.domain_name.as_deref(),
        Some("assetmesh.dev")
    );

    // And every restored asset is visible through the shared listing.
    let mut factory = destination.factory.clone();
    let count = factory
        .read(&mut |q| {
            Ok(q.assets()
                .list(&AssetFilter {
                    lifecycle: Some(LifecycleFilter::All),
                    ..Default::default()
                })?
                .len())
        })
        .unwrap();
    assert_eq!(count, 5);
}
