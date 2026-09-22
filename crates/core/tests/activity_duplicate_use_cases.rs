//! Activity and duplicate-review use-case tests — Phase 4C (docs/11).
//!
//! These run against the in-memory port doubles; the same scenarios run
//! against real SQLite in
//! `storage-sqlite/tests/activity_duplicate_contracts.rs`.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use assetmesh_core::application::activity_service::{ActivityQuery, ActivityService};
use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::duplicate_review_service::{
    DuplicateEvidence, DuplicateQuery, DuplicateReviewService,
};
use assetmesh_core::application::media_service::CreateMedia;
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::service_service::CreateService;
use assetmesh_core::application::software_service::CreateSoftware;
use assetmesh_core::domain::activity::{event_types, ActivityEvent, ActivityModule};
use assetmesh_core::domain::asset::{AssetKind, LifecycleState};
use assetmesh_core::domain::ids::{ActivityId, AssetId};
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::clock::Clock;
use assetmesh_core::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use assetmesh_core::{AppError, AppResult};
use support::{test_env, MemFactory, TestEnv};

// ---------------------------------------------------------------------------
// Scaffolding
// ---------------------------------------------------------------------------

struct Fixture {
    env: TestEnv,
    media: AssetId,
    software: AssetId,
    cli: AssetId,
    service: AssetId,
    vps: AssetId,
}

fn fixture() -> Fixture {
    let env = test_env();
    let media = env
        .media_service()
        .create_media(media_cmd("Sousou no Frieren", MediaType::Anime))
        .unwrap();
    let software = env
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let cli = env
        .software_service()
        .create_software(software_cmd("fzf", SoftwareCategory::Cli))
        .unwrap();
    let service = env
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas))
        .unwrap();
    let vps = env
        .service_service()
        .create_service(service_cmd("Hetzner VPS", ServiceType::Vps))
        .unwrap();
    Fixture {
        env,
        media: media.entry.asset.id,
        software: software.entry.asset.id,
        cli: cli.entry.asset.id,
        service: service.entry.asset.id,
        vps: vps.entry.asset.id,
    }
}

fn media_cmd(title: &str, media_type: MediaType) -> CreateMedia {
    CreateMedia {
        title: title.into(),
        media_type,
        summary: None,
        status: None,
        rating: None,
        year: None,
        platform: None,
        progress: Default::default(),
        notes: None,
        tags: Vec::new(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

fn software_cmd(name: &str, category: SoftwareCategory) -> CreateSoftware {
    CreateSoftware {
        name: name.into(),
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

fn service_cmd(name: &str, service_type: ServiceType) -> CreateService {
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

fn activity(env: &TestEnv) -> ActivityService<MemFactory> {
    ActivityService::new(env.factory.clone())
}

fn duplicates(env: &TestEnv) -> DuplicateReviewService<MemFactory> {
    DuplicateReviewService::new(env.factory.clone())
}

fn assets(env: &TestEnv) -> AssetService<MemFactory> {
    AssetService::new(env.factory.clone(), env.clock.clone(), env.ids.clone())
}

fn types(view: &[assetmesh_core::application::activity_service::ActivityView]) -> Vec<String> {
    view.iter().map(|event| event.event_type.clone()).collect()
}

// ---------------------------------------------------------------------------
// Activity: module classification
// ---------------------------------------------------------------------------

#[test]
fn every_known_event_type_maps_to_exactly_one_module() {
    let known = [
        event_types::ASSET_CREATED,
        event_types::ASSET_ARCHIVED,
        event_types::ASSET_MERGED,
        event_types::MEDIA_CREATED,
        event_types::MEDIA_STARTED,
        event_types::MEDIA_PROGRESS_CHANGED,
        event_types::MEDIA_COMPLETED,
        event_types::MEDIA_PAUSED,
        event_types::MEDIA_DROPPED,
        event_types::MEDIA_RATING_CHANGED,
        event_types::MEDIA_IMPORTED,
        event_types::SOFTWARE_CREATED,
        event_types::SOFTWARE_ADOPTED,
        event_types::SERVICE_CREATED,
        event_types::SERVICE_RENEWED,
        event_types::RELATION_CREATED,
        event_types::RELATION_REMOVED,
    ];
    for event_type in known {
        let module = ActivityModule::of_event_type(event_type)
            .unwrap_or_else(|| panic!("{event_type} must be classified"));
        assert!(
            ActivityModule::ALL.contains(&module),
            "{event_type} mapped outside the registry"
        );
    }

    // The mapping is explicit about the actor-shaped import event.
    assert_eq!(
        ActivityModule::of_event_type(event_types::MEDIA_IMPORTED),
        Some(ActivityModule::Import)
    );
    assert_eq!(
        ActivityModule::of_event_type(event_types::MEDIA_STARTED),
        Some(ActivityModule::Media)
    );
    assert_eq!(
        ActivityModule::of_event_type(event_types::SERVICE_RENEWED),
        Some(ActivityModule::Services)
    );
    assert_eq!(
        ActivityModule::of_event_type(event_types::RELATION_CREATED),
        Some(ActivityModule::Relation)
    );
    // A future module's event is unclassified, not a panic or a wrong guess.
    assert_eq!(ActivityModule::of_event_type("knowledge.created"), None);
    assert_eq!(ActivityModule::of_event_type("nonsense"), None);
    assert_eq!(
        ActivityModule::parse("service"),
        Some(ActivityModule::Services)
    );
}

// ---------------------------------------------------------------------------
// Activity: querying
// ---------------------------------------------------------------------------

#[test]
fn activity_reports_events_from_every_module_newest_first() {
    let f = fixture();
    // Cross-module writes so every subsystem has history.
    f.env.media_service().start_media(f.media).unwrap();
    f.env
        .software_service()
        .update_metadata(
            assetmesh_core::application::software_service::UpdateSoftwareMetadata {
                asset_id: f.software,
                version: Some("14.1.0".into()),
                ..Default::default()
            },
        )
        .unwrap();
    f.env
        .service_service()
        .record_renewal(
            assetmesh_core::application::service_service::RecordRenewal {
                asset_id: f.service,
                renewed_at: f.env.clock.now(),
                charged_cost_minor: None,
                currency: None,
                next_renews_at: None,
                next_expires_at: None,
            },
        )
        .unwrap();
    RelationService::new(
        f.env.factory.clone(),
        f.env.clock.clone(),
        f.env.ids.clone(),
    )
    .attach(
        f.media,
        RelationType::Uses,
        f.service,
        None,
        RelationProvenance::Manual,
    )
    .unwrap();

    let mut service = activity(&f.env);
    let page = service.recent(&Default::default()).unwrap();

    let modules: Vec<Option<ActivityModule>> =
        page.items.iter().map(|event| event.module).collect();
    for expected in [
        Some(ActivityModule::Asset),
        Some(ActivityModule::Media),
        Some(ActivityModule::Software),
        Some(ActivityModule::Services),
        Some(ActivityModule::Relation),
    ] {
        assert!(
            modules.contains(&expected),
            "missing {expected:?} in {modules:?}"
        );
    }

    // Newest first, strictly non-increasing.
    for pair in page.items.windows(2) {
        assert!(pair[0].occurred_at >= pair[1].occurred_at);
    }

    // Every event is explained by its asset's current name.
    for event in &page.items {
        if let Some(asset_id) = event.asset_id {
            assert!(
                event.asset_name.is_some(),
                "event {} lost its asset name",
                event.event_type
            );
            assert!([f.media, f.software, f.cli, f.service, f.vps].contains(&asset_id));
        }
    }
}

#[test]
fn activity_orders_by_timestamp_then_event_id() {
    let env = test_env();
    let asset = env
        .media_service()
        .create_media(media_cmd("Ordered", MediaType::Anime))
        .unwrap()
        .entry
        .asset
        .id;

    // Three events sharing one timestamp: only the id tie-breaker gives them a
    // stable order.
    let stamp = env.clock.now();
    let mut factory = env.factory.clone();
    factory
        .transact(&mut |uow| {
            for _ in 0..3 {
                uow.activity().append(&ActivityEvent::new(
                    event_types::MEDIA_PROGRESS_CHANGED,
                    Some(asset),
                    "user",
                    serde_json::json!({}),
                    stamp,
                ))?;
            }
            Ok(())
        })
        .unwrap();

    let mut service = activity(&env);
    let first = service.recent(&Default::default()).unwrap();
    let second = service.recent(&Default::default()).unwrap();
    assert_eq!(types(&first.items), types(&second.items));

    let ids: Vec<ActivityId> = first.items.iter().map(|e| e.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.reverse();
    assert_eq!(ids, sorted, "equal timestamps break by descending event id");

    // Paging walks every event exactly once.
    let mut walked: Vec<ActivityId> = Vec::new();
    let mut offset = 0;
    loop {
        let page = service
            .recent(&assetmesh_core::application::library_service::PageRequest::new(2, offset))
            .unwrap();
        if page.items.is_empty() {
            break;
        }
        walked.extend(page.items.iter().map(|e| e.id));
        offset += page.items.len();
    }
    assert_eq!(walked.len(), ids.len());
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), walked.len(), "no event repeats across pages");
}

#[test]
fn activity_filters_by_asset_type_module_actor_and_time() {
    let f = fixture();
    f.env.media_service().start_media(f.media).unwrap();
    f.env.media_service().complete_media(f.media).unwrap();
    f.env
        .service_service()
        .record_renewal(
            assetmesh_core::application::service_service::RecordRenewal {
                asset_id: f.service,
                renewed_at: f.env.clock.now(),
                charged_cost_minor: None,
                currency: None,
                next_renews_at: None,
                next_expires_at: None,
            },
        )
        .unwrap();

    let mut service = activity(&f.env);

    let for_media = service.query(&ActivityQuery::for_asset(f.media)).unwrap();
    assert!(types(&for_media.items).contains(&event_types::MEDIA_STARTED.to_string()));
    assert!(!types(&for_media.items).contains(&event_types::SERVICE_RENEWED.to_string()));

    let by_type = service
        .query(&ActivityQuery {
            event_types: vec![event_types::MEDIA_STARTED.into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        types(&by_type.items),
        vec![event_types::MEDIA_STARTED.to_string()]
    );

    let by_module = service
        .query(&ActivityQuery {
            modules: vec![ActivityModule::Services],
            ..Default::default()
        })
        .unwrap();
    assert!(!by_module.items.is_empty());
    assert!(by_module
        .items
        .iter()
        .all(|event| event.module == Some(ActivityModule::Services)));

    let by_actor = service
        .query(&ActivityQuery {
            actors: vec!["user".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(!by_actor.items.is_empty());
    assert!(by_actor.items.iter().all(|event| event.actor == "user"));

    // A time window is inclusive on both ends and excludes everything outside.
    let now = f.env.clock.now();
    let window = service
        .query(&ActivityQuery {
            since: Some(now - chrono::Duration::seconds(1)),
            until: Some(now + chrono::Duration::seconds(1)),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(window.total, Some(by_actor.total.unwrap()));

    let empty_window = service
        .query(&ActivityQuery {
            since: Some(now + chrono::Duration::days(1)),
            ..Default::default()
        })
        .unwrap();
    assert!(empty_window.items.is_empty());
    assert_eq!(empty_window.total, Some(0));
}

#[test]
fn activity_history_survives_archiving_and_merging() {
    let f = fixture();
    f.env.media_service().start_media(f.media).unwrap();
    assets(&f.env).archive_asset(f.media).unwrap();

    let duplicate = f
        .env
        .software_service()
        .create_software(software_cmd("ripgrep copy", SoftwareCategory::Cli))
        .unwrap();
    assets(&f.env)
        .merge_assets(duplicate.entry.asset.id, f.software)
        .unwrap();

    let mut service = activity(&f.env);

    // The archived asset keeps its history and is still named.
    let archived = service.query(&ActivityQuery::for_asset(f.media)).unwrap();
    assert!(types(&archived.items).contains(&event_types::MEDIA_STARTED.to_string()));
    assert!(types(&archived.items).contains(&event_types::ASSET_ARCHIVED.to_string()));
    assert_eq!(
        archived.items[0].asset_name.as_deref(),
        Some("Sousou no Frieren")
    );

    // The merged loser keeps its own history, and the merge event is recorded
    // against the survivor.
    let loser_history = service
        .query(&ActivityQuery::for_asset(duplicate.entry.asset.id))
        .unwrap();
    assert!(!loser_history.items.is_empty());
    assert!(types(&loser_history.items).contains(&event_types::SOFTWARE_CREATED.to_string()));

    let winner_history = service
        .query(&ActivityQuery::for_asset(f.software))
        .unwrap();
    assert!(types(&winner_history.items).contains(&event_types::ASSET_MERGED.to_string()));
    // The merged payload preserves the loser's identity for traceability.
    let merge_event = winner_history
        .items
        .iter()
        .find(|event| event.event_type == event_types::ASSET_MERGED)
        .unwrap();
    assert!(merge_event.payload.get("loser_id").is_some());
}

#[test]
fn activity_of_an_empty_library_is_an_empty_page() {
    let env = test_env();
    let mut service = activity(&env);
    let page = service.recent(&Default::default()).unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));
}

// ---------------------------------------------------------------------------
// Duplicate review
// ---------------------------------------------------------------------------

#[test]
fn no_candidates_when_the_library_has_no_duplicates() {
    let f = fixture();
    let mut service = duplicates(&f.env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));
}

#[test]
fn same_normalized_name_and_kind_is_a_candidate() {
    let env = test_env();
    let first = env
        .software_service()
        .create_software(software_cmd(
            "Visual Studio Code",
            SoftwareCategory::Application,
        ))
        .unwrap();
    let second = env
        .software_service()
        .create_software(software_cmd(
            "visual   studio code",
            SoftwareCategory::Application,
        ))
        .unwrap();

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(page.total, Some(1));
    let candidate = &page.items[0];
    assert_eq!(
        candidate.left.id,
        first.entry.asset.id.min(second.entry.asset.id)
    );
    assert_eq!(
        candidate.right.id,
        first.entry.asset.id.max(second.entry.asset.id)
    );
    assert_eq!(
        candidate.evidence,
        vec![DuplicateEvidence::SameNormalizedName {
            normalized_name: "visual studio code".into(),
            kind: AssetKind::SoftwareApplication,
        }]
    );
}

#[test]
fn the_same_name_on_a_different_kind_is_not_a_candidate() {
    let env = test_env();
    env.media_service()
        .create_media(media_cmd("Python", MediaType::Movie))
        .unwrap();
    env.software_service()
        .create_software(software_cmd("Python", SoftwareCategory::Runtime))
        .unwrap();

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    assert!(
        page.items.is_empty(),
        "a film and a runtime may share a name: {:#?}",
        page.items
    );
}

#[test]
fn a_pair_is_reported_once_with_merged_evidence() {
    let env = test_env();
    let mut first_cmd = service_cmd("OpenAI", ServiceType::Saas);
    first_cmd.provider = Some("OpenAI".into());
    let first = env.service_service().create_service(first_cmd).unwrap();
    let second = env
        .service_service()
        .create_service(service_cmd("openai", ServiceType::Saas))
        .unwrap();
    // Same normalized name AND the same provider as `first`: one candidate
    // carrying two pieces of evidence, not two candidates.
    let mut provider_cmd = service_cmd("OpenAI", ServiceType::Saas);
    provider_cmd.provider = Some("OpenAI".into());
    let third = env.service_service().create_service(provider_cmd).unwrap();
    // Same provider, different name: paired with the other provider matches.
    let mut provider_cmd2 = service_cmd("Different Name", ServiceType::Saas);
    provider_cmd2.provider = Some("OpenAI".into());
    let fourth = env.service_service().create_service(provider_cmd2).unwrap();
    let _ = (second, fourth);

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();

    // Pair (first, third) shares the name AND the provider.
    let named_pair = page
        .items
        .iter()
        .find(|candidate| {
            let ids = [candidate.left.id, candidate.right.id];
            ids.contains(&first.entry.asset.id) && ids.contains(&third.entry.asset.id)
        })
        .expect("the same-name pair is a candidate");
    assert!(named_pair
        .evidence
        .contains(&DuplicateEvidence::SameNormalizedName {
            normalized_name: "openai".into(),
            kind: AssetKind::ServiceSaas,
        }));
    assert!(named_pair
        .evidence
        .iter()
        .any(|evidence| matches!(evidence, DuplicateEvidence::SameProvider { .. })));

    // (second, first) also share the normalized name, and (third, fourth) the
    // provider — but each unordered pair appears exactly once.
    let mut seen: Vec<(AssetId, AssetId)> = page
        .items
        .iter()
        .map(|candidate| (candidate.left.id, candidate.right.id))
        .collect();
    let count = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), count, "no pair is reported twice");
    assert_eq!(page.total, Some(count));

    // Deterministic ordering.
    let again = service.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(again.items, page.items);
}

#[test]
fn module_specific_evidence_comes_from_canonical_fields() {
    let env = test_env();
    let mut first = software_cmd("ripgrep", SoftwareCategory::Cli);
    first.install_source = Some(InstallSource::HomebrewFormula);
    first.install_location = Some("/opt/homebrew/bin/rg".into());
    let a = env.software_service().create_software(first).unwrap();

    let mut second = software_cmd("A Different Name", SoftwareCategory::Cli);
    second.install_location = Some("/opt/homebrew/bin/rg".into());
    let b = env.software_service().create_software(second).unwrap();

    let mut third = software_cmd("Yet Another", SoftwareCategory::Cli);
    third.install_location = Some("/usr/local/bin/rg".into());
    env.software_service().create_software(third).unwrap();

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    let candidate = page
        .items
        .iter()
        .find(|candidate| {
            let ids = [candidate.left.id, candidate.right.id];
            ids.contains(&a.entry.asset.id) && ids.contains(&b.entry.asset.id)
        })
        .expect("the same install location is a candidate");
    assert_eq!(
        candidate.evidence,
        vec![DuplicateEvidence::SameInstallLocation {
            location: "/opt/homebrew/bin/rg".into()
        }]
    );
    // The differently-located asset is not paired with them.
    assert_eq!(page.total, Some(1));

    // Domain evidence only comes from a domain service.
    let mut domain = service_cmd("example.com", ServiceType::Domain);
    domain.domain_name = Some("example.com".into());
    let d1 = env.service_service().create_service(domain).unwrap();
    let mut domain2 = service_cmd("Another Domain", ServiceType::Domain);
    domain2.domain_name = Some("example.com".into());
    let d2 = env.service_service().create_service(domain2).unwrap();
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    let domain_pair = page
        .items
        .iter()
        .find(|candidate| {
            let ids = [candidate.left.id, candidate.right.id];
            ids.contains(&d1.entry.asset.id) && ids.contains(&d2.entry.asset.id)
        })
        .expect("the same canonical domain is a candidate");
    assert!(domain_pair
        .evidence
        .contains(&DuplicateEvidence::SameDomain {
            domain: "example.com".into()
        }));
}

#[test]
fn external_reference_equality_is_not_a_candidate_signal() {
    // Two live canonical assets can never share (namespace, external_id) — the
    // storage layer enforces global uniqueness. Reporting it as duplicate
    // evidence would be a condition that cannot occur.
    let env = test_env();
    let mut first = software_cmd("ripgrep", SoftwareCategory::Cli);
    first.external_refs = vec![
        assetmesh_core::application::media_service::ExternalRefInput {
            namespace: "homebrew_formula".into(),
            external_id: "ripgrep".into(),
            source_url: None,
        },
    ];
    env.software_service().create_software(first).unwrap();

    let mut second = software_cmd("A Different Name", SoftwareCategory::Cli);
    second.external_refs = vec![
        assetmesh_core::application::media_service::ExternalRefInput {
            namespace: "homebrew_formula".into(),
            external_id: "ripgrep".into(),
            source_url: None,
        },
    ];
    let error = env.software_service().create_software(second).unwrap_err();
    assert!(
        matches!(error, AppError::Conflict { .. }),
        "the unique ref is rejected at write time: {error}"
    );

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    assert!(page.items.is_empty());
}

#[test]
fn provider_evidence_is_independent_of_row_order() {
    // The two storage adapters order module rows differently (the in-memory
    // double sorts by Reverse(updated_at) with an id tie-break, SQLite by
    // updated_at DESC, id DESC). The evidence value must come from the
    // canonically smaller asset so both adapters report the same candidate.
    let env = test_env();
    let mut first = service_cmd("First", ServiceType::Saas);
    first.provider = Some("OpenAI".into());
    let a = env.service_service().create_service(first).unwrap();
    let mut second = service_cmd("Second", ServiceType::Saas);
    second.provider = Some("openai".into());
    let b = env.service_service().create_service(second).unwrap();

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(page.total, Some(1));
    let candidate = &page.items[0];
    let (smaller, _) = if a.entry.asset.id < b.entry.asset.id {
        (a.entry.asset.id, b.entry.asset.id)
    } else {
        (b.entry.asset.id, a.entry.asset.id)
    };
    assert_eq!(candidate.left.id, smaller);
    // The provider is reported as the smaller asset spelled it, and the pair is
    // stable across repeated scans.
    let expected_provider = if a.entry.asset.id < b.entry.asset.id {
        "OpenAI"
    } else {
        "openai"
    };
    assert_eq!(
        candidate.evidence,
        vec![DuplicateEvidence::SameProvider {
            provider: expected_provider.into()
        }]
    );
    let again = service.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(again.items, page.items);
}

#[test]
fn duplicate_candidates_respect_kind_and_archived_filters() {
    let env = test_env();
    let live = env
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let archived = env
        .software_service()
        .create_software(software_cmd("RIPGREP", SoftwareCategory::Cli))
        .unwrap();
    let movie_a = env
        .media_service()
        .create_media(media_cmd("Dune", MediaType::Movie))
        .unwrap();
    let movie_b = env
        .media_service()
        .create_media(media_cmd("dune", MediaType::Movie))
        .unwrap();

    AssetService::new(env.factory.clone(), env.clock.clone(), env.ids.clone())
        .archive_asset(archived.entry.asset.id)
        .unwrap();

    let mut service = duplicates(&env);

    // Archived duplicates are reviewable by default: merging an archived loser
    // into a live survivor is a legitimate flow (ADR 0005).
    let with_archived = service.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(with_archived.total, Some(2));
    let archived_pair = with_archived
        .items
        .iter()
        .find(|candidate| {
            let ids = [candidate.left.id, candidate.right.id];
            ids.contains(&live.entry.asset.id) && ids.contains(&archived.entry.asset.id)
        })
        .expect("an archived duplicate is still reviewable");
    // The archived member of the pair is whichever one was archived; the pair
    // is canonical-ordered by AssetId, not by lifecycle.
    let lifecycles = [archived_pair.left.lifecycle, archived_pair.right.lifecycle];
    assert!(
        lifecycles.contains(&LifecycleState::Archived),
        "{lifecycles:?}"
    );
    assert!(lifecycles.contains(&LifecycleState::Active));

    let without_archived = service
        .candidates(&DuplicateQuery {
            include_archived: false,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(without_archived.total, Some(1));

    let by_kind = service
        .candidates(&DuplicateQuery {
            kinds: vec![AssetKind::MediaMovie],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_kind.total, Some(1));
    assert_eq!(by_kind.items[0].left.kind, AssetKind::MediaMovie);
    let _ = (movie_a, movie_b);
}

#[test]
fn merged_tombstones_are_never_duplicate_candidates() {
    let env = test_env();
    let winner = env
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let loser = env
        .software_service()
        .create_software(software_cmd("RIPGREP", SoftwareCategory::Cli))
        .unwrap();

    AssetService::new(env.factory.clone(), env.clock.clone(), env.ids.clone())
        .merge_assets(loser.entry.asset.id, winner.entry.asset.id)
        .unwrap();

    let mut service = duplicates(&env);
    let page = service.candidates(&DuplicateQuery::default()).unwrap();
    assert!(
        page.items.is_empty(),
        "a tombstone is a redirect, not an inventory entry: {:#?}",
        page.items
    );
}

#[test]
fn duplicate_scan_never_mutates_canonical_state() {
    let f = fixture();
    let before = f.env.factory.store().clone();

    let mut service = duplicates(&f.env);
    service.candidates(&DuplicateQuery::default()).unwrap();

    let after = f.env.factory.store();
    assert_eq!(before.assets.len(), after.assets.len());
    assert_eq!(before.software.len(), after.software.len());
    assert_eq!(before.activity.len(), after.activity.len());
    assert_eq!(before.relations.len(), after.relations.len());
}

#[test]
fn duplicate_candidates_are_pageable() {
    let env = test_env();
    // Five software assets sharing ONE normalized name → one bucket of five,
    // which is ten pairs — all from one bucket, never N² over the library.
    for _ in 0..5 {
        env.software_service()
            .create_software(software_cmd("Duplicate", SoftwareCategory::Cli))
            .unwrap();
    }
    // Assets that share nothing must not multiply the work.
    for index in 0..5 {
        env.media_service()
            .create_media(media_cmd(&format!("Unique Film {index}"), MediaType::Movie))
            .unwrap();
    }

    let mut service = duplicates(&env);
    let page = service
        .candidates(&DuplicateQuery {
            page: assetmesh_core::application::library_service::PageRequest::new(4, 0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, Some(10));
    assert_eq!(page.items.len(), 4);
    assert_eq!(page.limit, 4);

    let next = service
        .candidates(&DuplicateQuery {
            page: assetmesh_core::application::library_service::PageRequest::new(4, 4),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(next.items.len(), 4);

    let last = service
        .candidates(&DuplicateQuery {
            page: assetmesh_core::application::library_service::PageRequest::new(4, 8),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(last.items.len(), 2);

    // Walking every page yields each pair once.
    let mut walked: Vec<(AssetId, AssetId)> = Vec::new();
    let mut offset = 0;
    loop {
        let page = service
            .candidates(&DuplicateQuery {
                page: assetmesh_core::application::library_service::PageRequest::new(3, offset),
                ..Default::default()
            })
            .unwrap();
        if page.items.is_empty() {
            break;
        }
        walked.extend(page.items.iter().map(|c| (c.left.id, c.right.id)));
        offset += page.items.len();
    }
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 10);
    assert_eq!(walked.len(), 10);
}

// ---------------------------------------------------------------------------
// Consistency
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct ReadProbe {
    scopes: Rc<RefCell<usize>>,
    accesses: Rc<RefCell<Vec<&'static str>>>,
}

struct ProbeFactory {
    inner: MemFactory,
    probe: ReadProbe,
}

impl UnitOfWorkFactory for ProbeFactory {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        self.inner.transact(work)
    }

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        *self.probe.scopes.borrow_mut() += 1;
        let accesses = self.probe.accesses.clone();
        self.inner.read(&mut |inner| {
            let mut probe = ProbeQuery {
                inner,
                accesses: accesses.clone(),
            };
            work(&mut probe)
        })
    }
}

struct ProbeQuery<'a> {
    inner: &'a mut dyn QueryUnitOfWork,
    accesses: Rc<RefCell<Vec<&'static str>>>,
}

macro_rules! probe_capability {
    ($method:ident, $reader:path, $capability:literal) => {
        fn $method(&mut self) -> &mut dyn $reader {
            self.accesses.borrow_mut().push($capability);
            self.inner.$method()
        }
    };
}

impl QueryUnitOfWork for ProbeQuery<'_> {
    probe_capability!(assets, assetmesh_core::ports::repos::AssetReader, "assets");
    probe_capability!(media, assetmesh_core::ports::repos::MediaReader, "media");
    probe_capability!(
        software,
        assetmesh_core::ports::repos::SoftwareReader,
        "software"
    );
    probe_capability!(
        services,
        assetmesh_core::ports::repos::ServiceReader,
        "services"
    );
    probe_capability!(
        external_refs,
        assetmesh_core::ports::repos::ExternalRefReader,
        "external_refs"
    );
    probe_capability!(
        activity,
        assetmesh_core::ports::repos::ActivityReader,
        "activity"
    );
    probe_capability!(tags, assetmesh_core::ports::repos::TagReader, "tags");
    probe_capability!(
        relations,
        assetmesh_core::ports::repos::RelationReader,
        "relations"
    );
    probe_capability!(
        search_index,
        assetmesh_core::ports::search::SearchReader,
        "search_index"
    );
}

#[test]
fn activity_and_duplicate_reads_use_one_snapshot_and_no_per_row_queries() {
    let f = fixture();
    f.env.media_service().start_media(f.media).unwrap();
    f.env
        .service_service()
        .record_renewal(
            assetmesh_core::application::service_service::RecordRenewal {
                asset_id: f.service,
                renewed_at: f.env.clock.now(),
                charged_cost_minor: None,
                currency: None,
                next_renews_at: None,
                next_expires_at: None,
            },
        )
        .unwrap();

    let probe = ReadProbe::default();
    let mut service = ActivityService::new(ProbeFactory {
        inner: f.env.factory.clone(),
        probe: probe.clone(),
    });
    service.recent(&Default::default()).unwrap();
    assert_eq!(*probe.scopes.borrow(), 1, "one activity read, one snapshot");
    let accesses = probe.accesses.borrow();
    assert_eq!(
        accesses.iter().filter(|a| **a == "activity").count(),
        1,
        "events are read once, not per page"
    );
    assert!(
        accesses.iter().filter(|a| **a == "assets").count() <= 1,
        "asset names are hydrated from one index"
    );
    drop(accesses);

    *probe.scopes.borrow_mut() = 0;
    probe.accesses.borrow_mut().clear();
    let mut duplicates = DuplicateReviewService::new(ProbeFactory {
        inner: f.env.factory.clone(),
        probe: probe.clone(),
    });
    duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(*probe.scopes.borrow(), 1);
    let accesses = probe.accesses.borrow();
    // One call per module reader that has rows; never one per asset.
    for capability in ["media", "software", "services", "tags"] {
        assert!(
            accesses.iter().filter(|a| **a == capability).count() <= 1,
            "{capability} was read more than once"
        );
    }
}
