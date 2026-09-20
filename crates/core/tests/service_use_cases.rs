//! Services application use-case tests over in-memory port doubles.
//!
//! These cover the Phase 3B contract (docs/10): CRUD, explicit patch
//! semantics, money/currency pairing, kind↔type invariants, the shared Asset
//! lifecycle, and search projection — all without touching SQL or network I/O.

mod support;

use assetmesh_core::application::media_service::ExternalRefInput;
use assetmesh_core::application::portable::PortableExportService;
use assetmesh_core::application::service_service::{
    CreateService, Patch, RecordRenewal, UpdateService,
};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::asset::{AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::service::{BillingCadence, ServiceType};
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::repos::{ServiceFilter, ServiceSort};
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::AppError;
use support::test_env;

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
        tags: vec!["ai".into(), "billing".into()],
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

fn ts(value: &str) -> Timestamp {
    chrono::DateTime::parse_from_rfc3339(value)
        .expect("test timestamps are well-formed")
        .with_timezone(&chrono::Utc)
}

#[test]
fn create_service_round_trips_every_field() {
    let t = test_env();
    let mut services = t.service_service();

    let view = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap();

    assert_eq!(view.entry.asset.name, "OpenAI");
    assert_eq!(view.entry.asset.kind, AssetKind::ServiceSaas);
    assert_eq!(view.entry.record.service_type, ServiceType::Saas);
    assert_eq!(view.entry.record.provider.as_deref(), Some("OpenAI"));
    assert_eq!(view.entry.record.account_label.as_deref(), Some("Work"));
    assert_eq!(
        view.entry.record.endpoint_url.as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(view.entry.record.plan.as_deref(), Some("Plus"));
    assert_eq!(view.entry.record.cost_minor, Some(1999));
    assert_eq!(view.entry.record.currency.as_deref(), Some("USD"));
    assert_eq!(
        view.entry.record.billing_cadence,
        Some(BillingCadence::Monthly)
    );
    assert_eq!(view.entry.record.auto_renew, Some(true));
    assert_eq!(view.entry.record.notes.as_deref(), Some("Team plan"));
    assert_eq!(view.tags, vec!["ai", "billing"]);
    assert_eq!(
        view.external_refs
            .iter()
            .map(|r| (r.namespace.as_str(), r.external_id.as_str()))
            .collect::<Vec<_>>(),
        vec![("openai", "org-work")]
    );

    // Both lifecycle events were recorded.
    let types: Vec<&str> = view
        .activity
        .iter()
        .map(|e| e.event_type.as_str())
        .collect();
    assert!(types.contains(&"asset.created"));
    assert!(types.contains(&"service.created"));
}

#[test]
fn create_service_normalizes_and_rejects_invalid_input() {
    let t = test_env();
    let mut services = t.service_service();

    // Empty name.
    let err = services
        .create_service(create_cmd("   ", ServiceType::Saas))
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    // Money without currency.
    let mut cmd = create_cmd("Broken", ServiceType::Vps);
    cmd.cost_minor = Some(100);
    let err = services.create_service(cmd).unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
    assert_eq!(
        t.service_service()
            .list_services(&ServiceFilter::default())
            .unwrap()
            .len(),
        0
    );

    // A credential-bearing URL never enters canonical state (ADR 0010).
    let mut cmd = create_cmd("Leaky", ServiceType::Saas);
    cmd.endpoint_url = Some("https://user:pass@example.com".into());
    let err = services.create_service(cmd).unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    // domain_name is canonical metadata of a domain service only.
    let mut cmd = create_cmd("Misuse", ServiceType::Saas);
    cmd.domain_name = Some("assetmesh.dev".into());
    let err = services.create_service(cmd).unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    // Text is trimmed and normalized on the way in.
    let view = services
        .create_service(CreateService {
            provider: Some("  Cloudflare  ".into()),
            notes: Some("  trailing  ".into()),
            ..create_cmd("Cloudflare", ServiceType::Domain)
        })
        .unwrap();
    assert_eq!(view.entry.record.provider.as_deref(), Some("Cloudflare"));
    assert_eq!(view.entry.record.notes.as_deref(), Some("trailing"));
}

#[test]
fn create_service_rejects_duplicate_external_ref() {
    let t = test_env();
    let mut services = t.service_service();
    services
        .create_service(CreateService {
            external_refs: vec![ref_input("aws", "1234567890")],
            ..create_cmd("AWS Production", ServiceType::Vps)
        })
        .unwrap();

    // The same ref on a second service is a conflict, never a silent duplicate.
    let err = services
        .create_service(CreateService {
            external_refs: vec![ref_input("aws", "1234567890")],
            ..create_cmd("AWS Duplicate", ServiceType::Vps)
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
}

#[test]
fn list_services_filters_by_type_provider_and_tag_and_sorts() {
    let t = test_env();
    let mut services = t.service_service();

    let openai = services
        .create_service(CreateService {
            provider: Some("OpenAI".into()),
            tags: vec!["ai".into()],
            ..create_cmd("OpenAI", ServiceType::Saas)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let _linear = services
        .create_service(CreateService {
            provider: Some("Linear".into()),
            tags: vec!["pm".into()],
            ..create_cmd("Linear", ServiceType::Saas)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let _hetzner = services
        .create_service(CreateService {
            provider: Some("Hetzner".into()),
            ..create_cmd("Hetzner", ServiceType::Vps)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let _api = services
        .create_service(CreateService {
            provider: Some("OpenAI".into()),
            ..create_cmd("OpenAI API", ServiceType::Api)
        })
        .unwrap()
        .entry
        .asset
        .id;

    // No filter: everything.
    let all = services.list_services(&ServiceFilter::default()).unwrap();
    assert_eq!(all.len(), 4);

    // By type.
    let saas = services
        .list_services(&ServiceFilter {
            service_type: Some(ServiceType::Saas),
            ..ServiceFilter::default()
        })
        .unwrap();
    assert_eq!(saas.len(), 2);
    assert!(saas
        .iter()
        .all(|r| r.entry.record.service_type == ServiceType::Saas));

    // By provider substring, case-insensitive.
    let openai_rows = services
        .list_services(&ServiceFilter {
            provider: Some("openai".into()),
            ..ServiceFilter::default()
        })
        .unwrap();
    assert_eq!(openai_rows.len(), 2);
    assert!(openai_rows
        .iter()
        .all(|r| r.entry.record.provider.as_deref() == Some("OpenAI")));

    // A provider substring with wildcard characters must match literally.
    let escaped = services
        .list_services(&ServiceFilter {
            provider: Some("O_enAI".into()),
            ..ServiceFilter::default()
        })
        .unwrap();
    assert_eq!(
        escaped.len(),
        0,
        "'_' must match literally, not as a wildcard"
    );

    // By tag.
    let tagged = services
        .list_services(&ServiceFilter {
            tag: Some("AI".into()),
            ..ServiceFilter::default()
        })
        .unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].entry.asset.id, openai);

    // Sort by title.
    let titled = services
        .list_services(&ServiceFilter {
            sort: ServiceSort::TitleAsc,
            ..ServiceFilter::default()
        })
        .unwrap();
    let names: Vec<&str> = titled.iter().map(|r| r.entry.asset.name.as_str()).collect();
    assert_eq!(names, vec!["Hetzner", "Linear", "OpenAI", "OpenAI API"]);
}

#[test]
fn list_services_sorts_renewals_soonest_first_with_absent_last() {
    let t = test_env();
    let mut services = t.service_service();

    let far = services
        .create_service(CreateService {
            renews_at: Some(ts("2027-01-01T00:00:00Z")),
            ..create_cmd("Far Future", ServiceType::Saas)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let near = services
        .create_service(CreateService {
            renews_at: Some(ts("2026-10-01T00:00:00Z")),
            ..create_cmd("Near Term", ServiceType::Saas)
        })
        .unwrap()
        .entry
        .asset
        .id;
    let none = services
        .create_service(create_cmd("No Renewal", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    let rows = services
        .list_services(&ServiceFilter {
            sort: ServiceSort::RenewsAsc,
            ..ServiceFilter::default()
        })
        .unwrap();
    let order: Vec<_> = rows.iter().map(|r| r.entry.asset.id).collect();
    assert_eq!(
        order,
        vec![near, far, none],
        "absent renewal dates sort last"
    );
}

#[test]
fn update_service_honors_leave_set_and_clear_patches() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // An update naming nothing leaves every field exactly as stored.
    let untouched = services
        .update_service(UpdateService {
            asset_id: id,
            ..UpdateService::default()
        })
        .unwrap();
    assert_eq!(untouched.entry.record.provider.as_deref(), Some("OpenAI"));
    assert_eq!(untouched.entry.record.plan.as_deref(), Some("Plus"));
    assert_eq!(untouched.entry.record.cost_minor, Some(1999));
    assert_eq!(untouched.entry.record.currency.as_deref(), Some("USD"));
    assert_eq!(untouched.entry.record.notes.as_deref(), Some("Team plan"));

    // Set a new plan, clear the notes, leave the provider.
    let updated = services
        .update_service(UpdateService {
            asset_id: id,
            plan: Patch::Set("Pro".into()),
            notes: Patch::Clear,
            ..UpdateService::default()
        })
        .unwrap();
    assert_eq!(updated.entry.record.plan.as_deref(), Some("Pro"));
    assert!(updated.entry.record.notes.is_none());
    // Untouched fields survive.
    assert_eq!(updated.entry.record.provider.as_deref(), Some("OpenAI"));
    assert_eq!(updated.entry.record.cost_minor, Some(1999));

    // Re-reading shows the same values — the patch was durable.
    let reloaded = services.get_service(id).unwrap();
    assert_eq!(reloaded.entry.record.plan.as_deref(), Some("Pro"));
    assert!(reloaded.entry.record.notes.is_none());
}

#[test]
fn update_service_money_must_be_set_and_cleared_together() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // Cost without currency.
    let err = services
        .update_service(UpdateService {
            asset_id: id,
            cost_minor: Patch::Set(2999),
            ..UpdateService::default()
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    // Currency without cost.
    let err = services
        .update_service(UpdateService {
            asset_id: id,
            currency: Patch::Set("EUR".into()),
            ..UpdateService::default()
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    // The original money is untouched after both failures.
    let view = services.get_service(id).unwrap();
    assert_eq!(view.entry.record.cost_minor, Some(1999));
    assert_eq!(view.entry.record.currency.as_deref(), Some("USD"));

    // Clearing together works.
    let cleared = services
        .update_service(UpdateService {
            asset_id: id,
            cost_minor: Patch::Clear,
            currency: Patch::Clear,
            ..UpdateService::default()
        })
        .unwrap();
    assert!(cleared.entry.record.cost_minor.is_none());
    assert!(cleared.entry.record.currency.is_none());

    // Setting together works, and currency normalizes to uppercase.
    let set = services
        .update_service(UpdateService {
            asset_id: id,
            cost_minor: Patch::Set(500),
            currency: Patch::Set("eur".into()),
            billing_cadence: Patch::Set(BillingCadence::Yearly),
            ..UpdateService::default()
        })
        .unwrap();
    assert_eq!(set.entry.record.cost_minor, Some(500));
    assert_eq!(set.entry.record.currency.as_deref(), Some("EUR"));
    assert_eq!(
        set.entry.record.billing_cadence,
        Some(BillingCadence::Yearly)
    );
}

#[test]
fn update_service_rejects_empty_name_and_missing_asset() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(create_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    let err = services
        .update_service(UpdateService {
            asset_id: id,
            name: Some("   ".into()),
            ..UpdateService::default()
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");

    let missing = AssetId::from_uuid(uuid::Uuid::nil());
    let err = services
        .update_service(UpdateService {
            asset_id: missing,
            plan: Patch::Set("Pro".into()),
            ..UpdateService::default()
        })
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound { .. }), "{err:?}");
}

#[test]
fn update_service_on_an_asset_without_a_service_record_is_not_found() {
    let t = test_env();
    // A media asset shares the Asset table but owns no service record.
    let media_id = t
        .media_service()
        .create_media(CreateMedia {
            title: "Hades".into(),
            media_type: MediaType::Game,
            ..create_media_cmd()
        })
        .unwrap()
        .entry
        .asset
        .id;

    let err = t
        .service_service()
        .update_service(UpdateService {
            asset_id: media_id,
            plan: Patch::Set("Pro".into()),
            ..UpdateService::default()
        })
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound { .. }), "{err:?}");
}

#[test]
fn archived_service_is_read_only_but_still_listed() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(create_cmd("Old SaaS", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;
    assert_eq!(
        services
            .list_services(&ServiceFilter::default())
            .unwrap()
            .len(),
        1
    );

    // Archiving goes through the shared Asset lifecycle, not a service-specific
    // command: cancellation is represented by metadata, never by deleting the
    // durable record (docs/10). The implementation must not auto-archive a
    // service merely because `expires_at` is in the past.
    t.asset_service().archive_asset(id).unwrap();

    // The durable record stays listed for history/search; only mutation is
    // blocked — matching the Media and Software contract.
    let rows = services.list_services(&ServiceFilter::default()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].entry.asset.archived_at.is_some());

    // The detail view still resolves the archived record.
    let view = services.get_service(id).unwrap();
    assert!(view.entry.asset.archived_at.is_some());

    // Further mutation is rejected by the shared lifecycle.
    let err = services
        .update_service(UpdateService {
            asset_id: id,
            plan: Patch::Set("Pro".into()),
            ..UpdateService::default()
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
}

#[test]
fn service_search_projection_rebuilds_from_canonical_data() {
    let mut t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // The projection is written synchronously with the canonical write.
    let mut search = t.search_service();
    let hits = search.search("OpenAI", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].asset_id, id);
    assert_eq!(hits[0].kind, "service.saas");
    assert!(hits[0].subtitle.as_deref().unwrap().contains("SaaS"));
    assert!(hits[0].subtitle.as_deref().unwrap().contains("Plus"));

    // Wipe and rebuild: canonical data alone restores the projection.
    t.factory
        .transact(&mut |uow| uow.search_index().replace_all(&[]))
        .unwrap();
    assert!(search.search("OpenAI", 10).unwrap().is_empty());
    search.rebuild().unwrap();
    let rebuilt = search.search("OpenAI", 10).unwrap();
    assert_eq!(rebuilt.len(), 1);
    assert_eq!(rebuilt[0].asset_id, id);
}

#[test]
fn services_and_software_coexist_on_the_shared_search_projection() {
    let mut t = test_env();
    let _software = SoftwareService::new(t.factory.clone(), t.clock.clone(), t.ids.clone())
        .create_software(CreateSoftware {
            name: "Ollama".into(),
            category: SoftwareCategory::Runtime,
            ..create_software_cmd()
        })
        .unwrap();
    let _service = t
        .service_service()
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap();

    let mut search = t.search_service();
    let hits = search.search("ollama", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "software.runtime");

    let hits = search.search("openai", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "service.saas");

    // A rebuild over all modules restores both.
    t.factory
        .transact(&mut |uow| uow.search_index().replace_all(&[]))
        .unwrap();
    search.rebuild().unwrap();
    assert_eq!(search.search("ollama", 10).unwrap().len(), 1);
    assert_eq!(search.search("openai", 10).unwrap().len(), 1);
}

#[test]
fn service_records_carry_no_credential_material() {
    // The canonical vocabulary is closed (ADR 0010): a serialized service view
    // must never expose a key that could hold a secret. This guards a future
    // field addition by name, not by convention.
    let t = test_env();
    let view = t
        .service_service()
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap();
    let serialized = serde_json::to_value(&view.entry.record).unwrap();
    for key in serialized.as_object().unwrap().keys() {
        assert!(
            ![
                "password",
                "token",
                "secret",
                "credential",
                "cookie",
                "ssh_key",
                "api_key"
            ]
            .iter()
            .any(|forbidden| key.contains(forbidden)),
            "canonical ServiceRecord exposes a secret-shaped field: {key}"
        );
    }
}

// --- helpers re-exporting the sibling modules' command builders ---

use assetmesh_core::application::media_service::CreateMedia;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::software::SoftwareCategory;

fn create_media_cmd() -> CreateMedia {
    CreateMedia {
        title: String::new(),
        media_type: MediaType::Game,
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

fn create_software_cmd() -> CreateSoftware {
    CreateSoftware {
        name: String::new(),
        category: SoftwareCategory::Tool,
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

// ---------------------------------------------------------------------------
// Regressions from the Phase 3B review
// ---------------------------------------------------------------------------

#[test]
fn export_carries_service_records_through_the_portable_services_section() {
    // Phase 3D: `modules/services.jsonl` exists, so a bundle written today can
    // restore a ServiceRecord. The exporter no longer refuses, and the
    // manifest declares the services module with its own schema version.
    let t = test_env();
    let mut services = t.service_service();
    services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap();
    // An archived service is exported too: archiving is a lifecycle state,
    // not a deletion.
    let archived = services
        .create_service(create_cmd("Retired SaaS", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;
    t.asset_service().archive_asset(archived).unwrap();

    let mut exporter = PortableExportService::new(t.factory.clone(), t.clock.clone());
    let bundle = exporter.export("test").unwrap();
    assert_eq!(bundle.manifest.modules["services"].schema_version, 1);
    assert_eq!(bundle.manifest.record_counts["services"], 2);
    let section = bundle.file("modules/services.jsonl").unwrap();
    assert!(section.contains("OpenAI"), "{section}");
    // The archived record is exported by identity: archiving is a lifecycle
    // state, not a deletion (its name lives in assets.jsonl).
    assert!(section.contains(&archived.to_string()), "{section}");

    // The wire DTO carries the canonical vocabulary only: no key in the
    // section can hold a credential (ADR 0010).
    for line in section.lines().filter(|l| !l.trim().is_empty()) {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        for key in row.as_object().unwrap().keys() {
            assert!(
                ![
                    "password",
                    "token",
                    "secret",
                    "credential",
                    "cookie",
                    "ssh_key",
                    "api_key"
                ]
                .iter()
                .any(|forbidden| key.contains(forbidden)),
                "portable service row exposes a secret-shaped field: {key}"
            );
        }
    }

    // Both records survive the refused-export era: the library still lists two.
    assert_eq!(
        services
            .list_services(&ServiceFilter::default())
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn merge_of_two_service_records_reports_reviewable_field_conflicts() {
    // docs/10 merge rules 3/6: conflicting non-empty fields are surfaced for
    // review and never silently chosen. The merge fails loudly, names every
    // disagreeing field with both values, and leaves both records untouched.
    let t = test_env();
    let mut services = t.service_service();

    let mut loser_cmd = full_cmd("Old Provider", ServiceType::Saas);
    loser_cmd.provider = Some("OldProvider".into());
    loser_cmd.plan = Some("Starter".into());
    loser_cmd.cost_minor = Some(999);
    loser_cmd.currency = Some("USD".into());
    loser_cmd.external_refs = vec![ref_input("openai", "org-old")];
    let loser = services.create_service(loser_cmd).unwrap();

    let mut winner_cmd = full_cmd("New Provider", ServiceType::Saas);
    winner_cmd.provider = Some("NewProvider".into());
    winner_cmd.plan = Some("Pro".into());
    winner_cmd.cost_minor = Some(4999);
    winner_cmd.currency = Some("USD".into());
    winner_cmd.external_refs = vec![ref_input("openai", "org-new")];
    let winner = services.create_service(winner_cmd).unwrap();

    let err = t
        .asset_service()
        .merge_assets(loser.entry.asset.id, winner.entry.asset.id)
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
    // Every disagreeing field is named, with both values, so the conflict is
    // reviewable rather than a generic refusal.
    for field in ["provider", "plan", "cost"] {
        assert!(err.to_string().contains(field), "{err}");
    }
    assert!(err.to_string().contains("OldProvider"), "{err}");
    assert!(err.to_string().contains("NewProvider"), "{err}");
    assert!(err.to_string().contains("999 USD"), "{err}");
    assert!(err.to_string().contains("4999 USD"), "{err}");

    // The loser's commercial details were NOT discarded...
    assert_eq!(
        t.service_service()
            .get_service(loser.entry.asset.id)
            .unwrap()
            .entry
            .record
            .provider
            .as_deref(),
        Some("OldProvider")
    );
    assert_eq!(
        t.service_service()
            .get_service(loser.entry.asset.id)
            .unwrap()
            .entry
            .record
            .cost_minor,
        Some(999)
    );
    // ...and the loser is still an active asset, not a tombstone.
    assert_eq!(
        t.service_service()
            .get_service(loser.entry.asset.id)
            .unwrap()
            .entry
            .asset
            .lifecycle_state,
        LifecycleState::Active
    );
    // The winner is untouched as well.
    assert_eq!(
        t.service_service()
            .get_service(winner.entry.asset.id)
            .unwrap()
            .entry
            .record
            .plan
            .as_deref(),
        Some("Pro")
    );
}

#[test]
fn merge_of_two_service_records_deduplicates_equal_values_and_fills_gaps() {
    // docs/10 merge rules 4/5: equal values deduplicate safely and an absent
    // field is filled from the other side, so a merge of two records that do
    // not actually disagree succeeds losslessly — nothing is discarded.
    let t = test_env();
    let mut services = t.service_service();

    // Loser: shares provider/plan/cost with the winner, and is the only side
    // carrying account_label, dashboard, notes, and the auto-renew setting.
    let mut loser_cmd = create_cmd("ChatGPT Plus (old)", ServiceType::Saas);
    loser_cmd.provider = Some("OpenAI".into());
    loser_cmd.plan = Some("Plus".into());
    loser_cmd.cost_minor = Some(1999);
    loser_cmd.currency = Some("USD".into());
    loser_cmd.account_label = Some("Personal".into());
    loser_cmd.dashboard_url = Some("https://platform.openai.com".into());
    loser_cmd.notes = Some("Family plan owner".into());
    loser_cmd.auto_renew = Some(true);
    let loser = services.create_service(loser_cmd).unwrap();

    // Winner: same commercial facts, no extra metadata.
    let mut winner_cmd = create_cmd("ChatGPT Plus", ServiceType::Saas);
    winner_cmd.provider = Some("OpenAI".into());
    winner_cmd.plan = Some("Plus".into());
    winner_cmd.cost_minor = Some(1999);
    winner_cmd.currency = Some("USD".into());
    winner_cmd.renews_at = Some(ts("2026-11-01T00:00:00Z"));
    let winner = services.create_service(winner_cmd).unwrap();

    t.asset_service()
        .merge_assets(loser.entry.asset.id, winner.entry.asset.id)
        .unwrap();

    let merged = t
        .service_service()
        .get_service(winner.entry.asset.id)
        .unwrap()
        .entry
        .record;
    // Deduplicated commercial facts.
    assert_eq!(merged.provider.as_deref(), Some("OpenAI"));
    assert_eq!(merged.plan.as_deref(), Some("Plus"));
    assert_eq!(merged.cost_minor, Some(1999));
    // Winner-only fields survive.
    assert_eq!(merged.renews_at, Some(ts("2026-11-01T00:00:00Z")));
    // Loser-only fields filled the winner's gaps.
    assert_eq!(merged.account_label.as_deref(), Some("Personal"));
    assert_eq!(
        merged.dashboard_url.as_deref(),
        Some("https://platform.openai.com")
    );
    assert_eq!(merged.notes.as_deref(), Some("Family plan owner"));
    assert_eq!(merged.auto_renew, Some(true));

    // No ServiceRecord is left on the loser tombstone.
    assert!(t
        .service_service()
        .get_service(loser.entry.asset.id)
        .is_err());

    // The winner's projection reflects the merged record (notes are visible
    // in the search body).
    let mut search = t.search_service();
    let hits = search.search("Family plan owner", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].asset_id, winner.entry.asset.id);
}

#[test]
fn merge_moves_the_only_service_record_to_the_winner() {
    // docs/10 merge rule 2: when only one side carries a ServiceRecord it
    // moves to the winner (winner kind is compatible by the kind preflight).
    let t = test_env();
    let mut services = t.service_service();
    let loser = services
        .create_service(create_cmd("Old Name", ServiceType::Domain))
        .unwrap();
    let winner = services
        .create_service(create_cmd("assetmesh.dev", ServiceType::Domain))
        .unwrap();

    t.asset_service()
        .merge_assets(loser.entry.asset.id, winner.entry.asset.id)
        .unwrap();

    let moved = t
        .service_service()
        .get_service(winner.entry.asset.id)
        .unwrap()
        .entry
        .record;
    assert_eq!(moved.asset_id, winner.entry.asset.id);
    assert!(t
        .service_service()
        .get_service(loser.entry.asset.id)
        .is_err());
}

#[test]
fn record_renewal_applies_explicit_facts_and_emits_one_event() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    let view = services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-10-01T00:00:00Z"),
            charged_cost_minor: Some(2499),
            currency: Some("usd".into()),
            next_renews_at: Some(ts("2026-11-01T00:00:00Z")),
            next_expires_at: Some(ts("2027-01-01T00:00:00Z")),
        })
        .unwrap();

    // Explicit facts land on canonical state, normalized like every other
    // write path (currency uppercased).
    let record = &view.entry.record;
    assert_eq!(record.cost_minor, Some(2499));
    assert_eq!(record.currency.as_deref(), Some("USD"));
    assert_eq!(record.renews_at, Some(ts("2026-11-01T00:00:00Z")));
    assert_eq!(record.expires_at, Some(ts("2027-01-01T00:00:00Z")));
    // Fields the command did not name are untouched.
    assert_eq!(record.plan.as_deref(), Some("Plus"));
    assert_eq!(record.auto_renew, Some(true));

    // Exactly one renewal event, carrying only non-secret factual data.
    let renewed: Vec<_> = view
        .activity
        .iter()
        .filter(|e| e.event_type == "service.renewed")
        .collect();
    assert_eq!(renewed.len(), 1, "one renewal records one event");
    let payload = &renewed[0].payload;
    assert_eq!(payload["renewed_at"], "2026-10-01T00:00:00+00:00");
    assert_eq!(payload["charged_cost_minor"], 2499);
    // The payload reports the currency AS STORED. `usd` was typed; `USD` is
    // the canonical form, and an exported activity event must not disagree
    // with the record it describes.
    assert_eq!(payload["currency"], "USD");
    assert_eq!(payload["next_renews_at"], "2026-11-01T00:00:00+00:00");
    assert_eq!(payload["next_expires_at"], "2027-01-01T00:00:00+00:00");
    // No secret-shaped key may appear in the payload (ADR 0010).
    for key in payload.as_object().unwrap().keys() {
        assert!(
            ![
                "password",
                "token",
                "secret",
                "credential",
                "cookie",
                "api_key"
            ]
            .iter()
            .any(|forbidden| key.contains(forbidden)),
            "renewal payload exposes a secret-shaped field: {key}"
        );
    }

    // Durable: a fresh read shows the same values.
    let reloaded = services.get_service(id).unwrap();
    assert_eq!(
        reloaded.entry.record.renews_at,
        Some(ts("2026-11-01T00:00:00Z"))
    );
}

#[test]
fn record_renewal_never_computes_a_next_boundary() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(create_cmd("Odd Cycle", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // A caller-known boundary that no calendar arithmetic would produce (28
    // days, not a month; a 400-day cycle) is stored verbatim.
    let view = services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-10-01T00:00:00Z"),
            charged_cost_minor: None,
            currency: None,
            next_renews_at: Some(ts("2026-10-29T00:00:00Z")),
            next_expires_at: None,
        })
        .unwrap();
    assert_eq!(
        view.entry.record.renews_at,
        Some(ts("2026-10-29T00:00:00Z"))
    );

    // An unknown boundary leaves the stored one alone: it is neither cleared
    // nor guessed. Clearing stays an explicit `service update`.
    let view = services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-10-29T00:00:00Z"),
            charged_cost_minor: None,
            currency: None,
            next_renews_at: None,
            next_expires_at: None,
        })
        .unwrap();
    assert_eq!(
        view.entry.record.renews_at,
        Some(ts("2026-10-29T00:00:00Z"))
    );
    assert_eq!(view.entry.record.expires_at, None);
    // Money the command did not mention is untouched too.
    assert_eq!(view.entry.record.cost_minor, None);

    // Two renewals record two events; nothing else emits activity.
    let renewed = view
        .activity
        .iter()
        .filter(|e| e.event_type == "service.renewed")
        .count();
    assert_eq!(renewed, 2);
}

#[test]
fn record_renewal_publishes_a_currency_only_when_a_charge_was_reported() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // A renewal that reports no charge reports no currency either. The
    // record's pre-existing canonical cost is not this renewal's fact, so the
    // event must not imply money changed when it did not.
    let view = services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-11-01T00:00:00Z"),
            charged_cost_minor: None,
            currency: None,
            next_renews_at: None,
            next_expires_at: None,
        })
        .unwrap();
    let renewed: Vec<_> = view
        .activity
        .iter()
        .filter(|e| e.event_type == "service.renewed")
        .collect();
    assert_eq!(renewed.len(), 1);
    let payload = &renewed[0].payload;
    assert_eq!(payload["charged_cost_minor"], serde_json::Value::Null);
    assert_eq!(payload["currency"], serde_json::Value::Null);

    // The stored money is untouched by a renewal that reported none.
    assert_eq!(view.entry.record.cost_minor, Some(1999));
    assert_eq!(view.entry.record.currency.as_deref(), Some("USD"));
}

#[test]
fn record_renewal_validates_money_lifecycle_and_missing_records() {
    let t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(full_cmd("OpenAI", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    // Cost without currency is rejected before anything is written.
    let err = services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-10-01T00:00:00Z"),
            charged_cost_minor: Some(2499),
            currency: None,
            next_renews_at: None,
            next_expires_at: None,
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
    assert_eq!(
        services.get_service(id).unwrap().entry.record.cost_minor,
        Some(1999),
        "a rejected renewal changed nothing"
    );

    // A service asset without a record is not found, not silently created.
    let media_id = t
        .media_service()
        .create_media(CreateMedia {
            title: "Hades".into(),
            media_type: MediaType::Game,
            ..create_media_cmd()
        })
        .unwrap()
        .entry
        .asset
        .id;
    let err = services
        .record_renewal(RecordRenewal {
            asset_id: media_id,
            renewed_at: ts("2026-10-01T00:00:00Z"),
            charged_cost_minor: None,
            currency: None,
            next_renews_at: None,
            next_expires_at: None,
        })
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound { .. }), "{err:?}");

    // An archived service is read-only: the shared lifecycle still governs.
    t.asset_service().archive_asset(id).unwrap();
    let err = services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-10-01T00:00:00Z"),
            charged_cost_minor: None,
            currency: None,
            next_renews_at: None,
            next_expires_at: None,
        })
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict { .. }), "{err:?}");
}

#[test]
fn record_renewal_updates_the_search_projection() {
    let mut t = test_env();
    let mut services = t.service_service();
    let id = services
        .create_service(create_cmd("Odd Cycle", ServiceType::Saas))
        .unwrap()
        .entry
        .asset
        .id;

    services
        .record_renewal(RecordRenewal {
            asset_id: id,
            renewed_at: ts("2026-10-01T00:00:00Z"),
            charged_cost_minor: Some(500),
            currency: Some("EUR".into()),
            next_renews_at: Some(ts("2026-11-01T00:00:00Z")),
            next_expires_at: None,
        })
        .unwrap();

    // The renewal's cost/boundary are search-visible facts (projection body).
    let mut search = t.search_service();
    let hits = search.search("2026-11-01", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].asset_id, id);

    // And the projection is still rebuildable from canonical data alone.
    t.factory
        .transact(&mut |uow| uow.search_index().replace_all(&[]))
        .unwrap();
    search.rebuild().unwrap();
    let hits = search.search("2026-11-01", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn service_urls_must_have_a_real_host_and_no_credentials() {
    let t = test_env();
    let mut services = t.service_service();

    let bad = [
        "https://exa mple.com",          // whitespace in the host
        "https://:8080",                 // port without a host
        "https://exa\nmple.com",         // control character in the host
        "https://user:pass@example.com", // credentials (ADR 0010)
        "ftp://example.com",             // not http/https
        "https://[x]",                   // bracketed but not an IPv6 literal
        "https://[::1",                  // unterminated IPv6 literal
        "https://example.com:notaport",  // non-numeric port
        "https://example.com/a b",       // whitespace in the path, not the host
        "https://example.com:99999",     // port out of range
        "https://.",                     // empty labels
        "https://example..com",          // empty interior label
        "https://-bad.com",              // label starts with a hyphen
        "https://example..com.",         // trailing dot (empty final label)
        "https://[1:::2]",               // not parseable as Ipv6Addr
        "https://[::::]",                // not parseable as Ipv6Addr
        "https://[::1]x",                // junk after the closed IPv6 literal
        "https://[::1]notaport",         // port separator with a non-numeric port
    ];
    for url in bad {
        let mut cmd = create_cmd("Bad URL", ServiceType::Saas);
        cmd.endpoint_url = Some(url.into());
        let err = services.create_service(cmd).unwrap_err();
        assert!(
            matches!(err, AppError::Validation { .. }),
            "{url:?}: {err:?}"
        );
    }
    // No malformed URL left a row behind.
    assert_eq!(
        t.service_service()
            .list_services(&ServiceFilter::default())
            .unwrap()
            .len(),
        0
    );

    // Legitimate shapes still pass: plain host, host with port, bracketed IPv6.
    for url in [
        "https://api.openai.com/v1",
        "http://localhost:8080/health",
        "https://[::1]:8443/",
        "https://[2001:db8::8a2e:370:7334]/",
        "https://mixed-case.Example.com/path",
        "https://sub-domain.example.io:443/x?y=1#frag",
    ] {
        let mut cmd = create_cmd("Good URL", ServiceType::Saas);
        cmd.endpoint_url = Some(url.into());
        services.create_service(cmd).expect("well-formed URL");
    }
    assert_eq!(
        t.service_service()
            .list_services(&ServiceFilter::default())
            .unwrap()
            .len(),
        6
    );
}
