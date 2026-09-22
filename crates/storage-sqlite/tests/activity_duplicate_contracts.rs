//! Activity and duplicate-review contract tests over the real SQLite adapter
//! (Phase 4C).
//!
//! These mirror `core/tests/activity_duplicate_use_cases.rs` scenario for
//! scenario, so the in-memory test double and the production storage adapter
//! agree at the application boundary.

use std::sync::Arc;

use assetmesh_core::application::activity_service::{ActivityQuery, ActivityService};
use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::duplicate_review_service::{
    DuplicateEvidence, DuplicateQuery, DuplicateReviewService,
};
use assetmesh_core::application::library_service::PageRequest;
use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::relation_service::RelationService;
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::activity::{event_types, ActivityModule};
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::relation::{RelationProvenance, RelationType};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::repos::LifecycleFilter;
use assetmesh_core::ports::uow::UnitOfWorkFactory;
use assetmesh_core::{AppError, SharedClock, SharedIdGenerator};
use assetmesh_storage_sqlite::SharedSqlite;

fn env() -> TestSqlite {
    let factory = SharedSqlite(Arc::new(
        assetmesh_storage_sqlite::open_in_memory().unwrap(),
    ));
    let clock: SharedClock = Arc::new(assetmesh_core::ports::clock::SystemClock);
    let ids: SharedIdGenerator = Arc::new(assetmesh_core::ports::ids::UuidV7Generator);
    TestSqlite {
        factory,
        clock,
        ids,
    }
}

struct TestSqlite {
    factory: SharedSqlite,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl TestSqlite {
    fn media_service(&self) -> MediaService<SharedSqlite> {
        MediaService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn software_service(&self) -> SoftwareService<SharedSqlite> {
        SoftwareService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn service_service(&self) -> ServiceService<SharedSqlite> {
        ServiceService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn asset_service(&self) -> AssetService<SharedSqlite> {
        AssetService::new(self.factory.clone(), self.clock.clone(), self.ids.clone())
    }
    fn activity(&self) -> ActivityService<SharedSqlite> {
        ActivityService::new(self.factory.clone())
    }
    fn duplicates(&self) -> DuplicateReviewService<SharedSqlite> {
        DuplicateReviewService::new(self.factory.clone())
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

fn renew(
    db: &TestSqlite,
    asset_id: AssetId,
) -> assetmesh_core::application::service_service::ServiceView {
    let rev = db.service_service().get_service(asset_id).unwrap().entry.asset.revision;
    db.service_service()
        .record_renewal(
            assetmesh_core::application::service_service::RecordRenewal {
                asset_id,
                renewed_at: db.clock.now(),
                charged_cost_minor: None,
                currency: None,
                next_renews_at: None,
                next_expires_at: None,
                expected_revision: Some(rev),
            },
        )
        .unwrap()
}

#[test]
fn sqlite_activity_spans_every_module_with_stable_ordering() {
    let db = env();
    let media = db
        .media_service()
        .create_media(media_cmd("Sousou no Frieren", MediaType::Anime))
        .unwrap();
    let software = db
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let service = db
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas))
        .unwrap();

    db.media_service()
        .start_media(media.entry.asset.id)
        .unwrap();
    renew(&db, service.entry.asset.id);
    RelationService::new(db.factory.clone(), db.clock.clone(), db.ids.clone())
        .attach(
            media.entry.asset.id,
            RelationType::Uses,
            service.entry.asset.id,
            None,
            RelationProvenance::Manual,
        )
        .unwrap();

    let mut activity = db.activity();
    let page = activity.recent(&PageRequest::default()).unwrap();
    assert!(page.total.unwrap() >= 5);

    let modules: Vec<Option<ActivityModule>> = page.items.iter().map(|e| e.module).collect();
    for expected in [
        Some(ActivityModule::Asset),
        Some(ActivityModule::Media),
        Some(ActivityModule::Software),
        Some(ActivityModule::Services),
        Some(ActivityModule::Relation),
    ] {
        assert!(modules.contains(&expected), "missing {expected:?}");
    }

    // Newest first and stable across repeated reads over real storage.
    for pair in page.items.windows(2) {
        assert!(pair[0].occurred_at >= pair[1].occurred_at);
        assert_ne!(pair[0].id, pair[1].id);
    }
    let again = activity.recent(&PageRequest::default()).unwrap();
    assert_eq!(
        again.items.iter().map(|e| e.id).collect::<Vec<_>>(),
        page.items.iter().map(|e| e.id).collect::<Vec<_>>()
    );

    // Every event is explained by its asset's current name.
    for event in &page.items {
        if event.asset_id.is_some() {
            assert!(event.asset_name.is_some(), "{}", event.event_type);
        }
    }

    // Filters compose.
    // One asset creation records both the kernel event and the module event.
    let for_software = activity
        .query(&ActivityQuery::for_asset(software.entry.asset.id))
        .unwrap();
    let software_types: Vec<&str> = for_software
        .items
        .iter()
        .map(|e| e.event_type.as_str())
        .collect();
    assert_eq!(
        software_types,
        vec![event_types::SOFTWARE_CREATED, event_types::ASSET_CREATED]
    );

    let services_only = activity
        .query(&ActivityQuery {
            modules: vec![ActivityModule::Services],
            ..Default::default()
        })
        .unwrap();
    assert!(services_only
        .items
        .iter()
        .all(|e| e.module == Some(ActivityModule::Services)));
    assert!(!services_only.items.is_empty());

    let by_type = activity
        .query(&ActivityQuery {
            event_types: vec![event_types::MEDIA_STARTED.into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_type.items.len(), 1);

    // Pagination walks every event exactly once.
    let mut walked: Vec<assetmesh_core::domain::ids::ActivityId> = Vec::new();
    let mut offset = 0;
    loop {
        let page = activity.recent(&PageRequest::new(2, offset)).unwrap();
        if page.items.is_empty() {
            break;
        }
        walked.extend(page.items.iter().map(|e| e.id));
        offset += page.items.len();
    }
    let total = activity
        .recent(&PageRequest::default())
        .unwrap()
        .total
        .unwrap();
    assert_eq!(walked.len(), total);
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), walked.len());
}

#[test]
fn sqlite_activity_history_survives_archive_and_merge() {
    let db = env();
    let media = db
        .media_service()
        .create_media(media_cmd("Frieren", MediaType::Anime))
        .unwrap();
    let winner = db
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let loser = db
        .software_service()
        .create_software(software_cmd("ripgrep copy", SoftwareCategory::Cli))
        .unwrap();

    db.media_service()
        .start_media(media.entry.asset.id)
        .unwrap();
    db.asset_service()
        .archive_asset(media.entry.asset.id)
        .unwrap();
    db.asset_service()
        .merge_assets(loser.entry.asset.id, winner.entry.asset.id)
        .unwrap();

    let mut activity = db.activity();

    let archived = activity
        .query(&ActivityQuery::for_asset(media.entry.asset.id))
        .unwrap();
    let types: Vec<&str> = archived
        .items
        .iter()
        .map(|e| e.event_type.as_str())
        .collect();
    assert!(types.contains(&event_types::MEDIA_STARTED));
    assert!(types.contains(&event_types::ASSET_ARCHIVED));
    assert_eq!(archived.items[0].asset_name.as_deref(), Some("Frieren"));

    let loser_history = activity
        .query(&ActivityQuery::for_asset(loser.entry.asset.id))
        .unwrap();
    assert!(!loser_history.items.is_empty());
    assert!(loser_history.items[0].asset_name.is_some());

    let winner_history = activity
        .query(&ActivityQuery::for_asset(winner.entry.asset.id))
        .unwrap();
    assert!(winner_history
        .items
        .iter()
        .any(|e| e.event_type == event_types::ASSET_MERGED));

    // A rebuild of derived state must not change what history reports.
    SearchService::new(db.factory.clone(), db.clock.clone())
        .rebuild()
        .unwrap();
    let after_rebuild = activity
        .query(&ActivityQuery::for_asset(media.entry.asset.id))
        .unwrap();
    assert_eq!(after_rebuild.total, archived.total);
}

#[test]
fn sqlite_activity_orders_equal_timestamps_by_descending_event_id() {
    let db = env();
    let media = db
        .media_service()
        .create_media(media_cmd("Ordered", MediaType::Anime))
        .unwrap();

    // Three events sharing one timestamp: only the event-id tie-breaker gives
    // them a stable order, so pagination cannot repeat or skip one.
    let stamp = db.clock.now();
    let mut factory = db.factory.clone();
    factory
        .transact(&mut |uow| {
            for _ in 0..3 {
                uow.activity()
                    .append(&assetmesh_core::domain::activity::ActivityEvent::new(
                        event_types::MEDIA_PROGRESS_CHANGED,
                        Some(media.entry.asset.id),
                        "user",
                        serde_json::json!({}),
                        stamp,
                    ))?;
            }
            Ok(())
        })
        .unwrap();

    let mut activity = db.activity();
    let page = activity.recent(&PageRequest::default()).unwrap();
    let ids: Vec<assetmesh_core::domain::ids::ActivityId> =
        page.items.iter().map(|e| e.id).collect();
    assert!(ids.len() >= 3);
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.reverse();
    assert_eq!(ids, sorted, "equal timestamps break by descending event id");

    // Walking every page yields each event exactly once.
    let mut walked: Vec<assetmesh_core::domain::ids::ActivityId> = Vec::new();
    let mut offset = 0;
    loop {
        let page = activity.recent(&PageRequest::new(2, offset)).unwrap();
        if page.items.is_empty() {
            break;
        }
        walked.extend(page.items.iter().map(|e| e.id));
        offset += page.items.len();
    }
    let total = activity
        .recent(&PageRequest::default())
        .unwrap()
        .total
        .unwrap();
    assert_eq!(walked.len(), total);
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), walked.len());
}

#[test]
fn sqlite_activity_filters_by_time_window_and_actor() {
    let db = env();
    let media = db
        .media_service()
        .create_media(media_cmd("Frieren", MediaType::Anime))
        .unwrap();
    db.media_service()
        .start_media(media.entry.asset.id)
        .unwrap();

    let mut activity = db.activity();
    let now = db.clock.now();

    let window = activity
        .query(&ActivityQuery {
            since: Some(now - chrono::Duration::seconds(1)),
            until: Some(now + chrono::Duration::seconds(1)),
            ..Default::default()
        })
        .unwrap();
    assert!(window.total.unwrap() >= 3);

    let empty_window = activity
        .query(&ActivityQuery {
            since: Some(now + chrono::Duration::days(1)),
            ..Default::default()
        })
        .unwrap();
    assert!(empty_window.items.is_empty());
    assert_eq!(empty_window.total, Some(0));

    let by_actor = activity
        .query(&ActivityQuery {
            actors: vec!["user".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(by_actor.items.iter().all(|event| event.actor == "user"));
    let no_such_actor = activity
        .query(&ActivityQuery {
            actors: vec!["nobody".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(no_such_actor.items.is_empty());

    // An empty library has no history either.
    let empty_db = env();
    let mut empty_activity = empty_db.activity();
    assert!(empty_activity
        .recent(&PageRequest::default())
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn sqlite_duplicate_candidates_respect_the_kind_filter() {
    let db = env();
    db.software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    db.software_service()
        .create_software(software_cmd("RIPGREP", SoftwareCategory::Cli))
        .unwrap();
    db.media_service()
        .create_media(media_cmd("Dune", MediaType::Movie))
        .unwrap();
    db.media_service()
        .create_media(media_cmd("dune", MediaType::Movie))
        .unwrap();

    let mut duplicates = db.duplicates();
    let all = duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(all.total, Some(2));

    let software_only = duplicates
        .candidates(&DuplicateQuery {
            kinds: vec![AssetKind::SoftwareCli],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(software_only.total, Some(1));
    assert_eq!(software_only.items[0].left.kind, AssetKind::SoftwareCli);

    let media_only = duplicates
        .candidates(&DuplicateQuery {
            kinds: vec![AssetKind::MediaMovie],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(media_only.total, Some(1));
    assert_eq!(media_only.items[0].left.kind, AssetKind::MediaMovie);

    // A kind with no candidates is an empty page, not an error.
    let none = duplicates
        .candidates(&DuplicateQuery {
            kinds: vec![AssetKind::ServiceSaas],
            ..Default::default()
        })
        .unwrap();
    assert!(none.items.is_empty());
    assert_eq!(none.total, Some(0));
}

#[test]
fn sqlite_duplicate_review_reports_deterministic_candidates() {
    let db = env();
    let first = db
        .software_service()
        .create_software(software_cmd(
            "Visual Studio Code",
            SoftwareCategory::Application,
        ))
        .unwrap();
    let second = db
        .software_service()
        .create_software(software_cmd(
            "visual   studio code",
            SoftwareCategory::Application,
        ))
        .unwrap();
    // Same name, different kind: not a duplicate.
    db.media_service()
        .create_media(media_cmd("Visual Studio Code", MediaType::Movie))
        .unwrap();

    let mut duplicates = db.duplicates();
    let page = duplicates.candidates(&DuplicateQuery::default()).unwrap();
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

    // Stable across repeated scans.
    let again = duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(again.items, page.items);

    // Archived duplicates stay reviewable; merged tombstones never appear.
    db.asset_service()
        .archive_asset(second.entry.asset.id)
        .unwrap();
    let with_archived = duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(with_archived.total, Some(1));
    let without_archived = duplicates
        .candidates(&DuplicateQuery {
            include_archived: false,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(without_archived.total, Some(0));

    let merged = db
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli))
        .unwrap();
    let merged_loser = db
        .software_service()
        .create_software(software_cmd("RIPGREP", SoftwareCategory::Cli))
        .unwrap();
    db.asset_service()
        .merge_assets(merged_loser.entry.asset.id, merged.entry.asset.id)
        .unwrap();
    let after_merge = duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(
        after_merge.total,
        Some(1),
        "the tombstone pair is gone, the archived pair remains"
    );
}

#[test]
fn sqlite_duplicate_scan_is_read_only_and_bucketed() {
    let mut db = env();
    // One bucket of five identical names → ten pairs.
    for _ in 0..5 {
        db.software_service()
            .create_software(software_cmd("Duplicate", SoftwareCategory::Cli))
            .unwrap();
    }
    // Five unrelated assets that must not multiply the work.
    for index in 0..5 {
        db.media_service()
            .create_media(media_cmd(&format!("Unique {index}"), MediaType::Movie))
            .unwrap();
    }

    let before = db
        .factory
        .read(&mut |q| {
            Ok((
                q.assets()
                    .list(&assetmesh_core::ports::repos::AssetFilter {
                        kind: None,
                        lifecycle: Some(LifecycleFilter::All),
                    })?
                    .len(),
                q.activity().list_all()?.len(),
            ))
        })
        .unwrap();

    let mut duplicates = db.duplicates();
    let page = duplicates
        .candidates(&DuplicateQuery {
            page: PageRequest::new(4, 0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, Some(10));
    assert_eq!(page.items.len(), 4);

    let next = duplicates
        .candidates(&DuplicateQuery {
            page: PageRequest::new(4, 4),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(next.items.len(), 4);
    let last = duplicates
        .candidates(&DuplicateQuery {
            page: PageRequest::new(4, 8),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(last.items.len(), 2);

    let after = db
        .factory
        .read(&mut |q| {
            Ok((
                q.assets()
                    .list(&assetmesh_core::ports::repos::AssetFilter {
                        kind: None,
                        lifecycle: Some(LifecycleFilter::All),
                    })?
                    .len(),
                q.activity().list_all()?.len(),
            ))
        })
        .unwrap();
    assert_eq!(before, after, "duplicate detection must not write");

    // Every pair is reported once.
    let mut seen: Vec<(AssetId, AssetId)> = Vec::new();
    let mut offset = 0;
    loop {
        let page = duplicates
            .candidates(&DuplicateQuery {
                page: PageRequest::new(3, offset),
                ..Default::default()
            })
            .unwrap();
        if page.items.is_empty() {
            break;
        }
        seen.extend(page.items.iter().map(|c| (c.left.id, c.right.id)));
        offset += page.items.len();
    }
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 10);
    assert_eq!(seen.len(), 10);
}

#[test]
fn sqlite_duplicate_evidence_uses_only_canonical_module_fields() {
    let db = env();
    let mut first = software_cmd("ripgrep", SoftwareCategory::Cli);
    first.install_location = Some("/opt/homebrew/bin/rg".into());
    let a = db.software_service().create_software(first).unwrap();
    let mut second = software_cmd("Another Name", SoftwareCategory::Cli);
    second.install_location = Some("/opt/homebrew/bin/rg".into());
    let b = db.software_service().create_software(second).unwrap();

    let mut domain = service_cmd(
        "example.com",
        assetmesh_core::domain::service::ServiceType::Domain,
    );
    domain.domain_name = Some("example.com".into());
    let d1 = db.service_service().create_service(domain).unwrap();
    let mut domain2 = service_cmd(
        "Other",
        assetmesh_core::domain::service::ServiceType::Domain,
    );
    domain2.domain_name = Some("example.com".into());
    let d2 = db.service_service().create_service(domain2).unwrap();

    let mut duplicates = db.duplicates();
    let page = duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert_eq!(page.total, Some(2));

    let software_pair = page
        .items
        .iter()
        .find(|c| {
            let ids = [c.left.id, c.right.id];
            ids.contains(&a.entry.asset.id) && ids.contains(&b.entry.asset.id)
        })
        .unwrap();
    assert_eq!(
        software_pair.evidence,
        vec![DuplicateEvidence::SameInstallLocation {
            location: "/opt/homebrew/bin/rg".into()
        }]
    );

    let domain_pair = page
        .items
        .iter()
        .find(|c| {
            let ids = [c.left.id, c.right.id];
            ids.contains(&d1.entry.asset.id) && ids.contains(&d2.entry.asset.id)
        })
        .unwrap();
    assert!(domain_pair
        .evidence
        .contains(&DuplicateEvidence::SameDomain {
            domain: "example.com".into()
        }));
}

#[test]
fn sqlite_shared_ref_uniqueness_prevents_an_unreachable_candidate() {
    let db = env();
    let mut first = software_cmd("ripgrep", SoftwareCategory::Cli);
    first.external_refs = vec![
        assetmesh_core::application::media_service::ExternalRefInput {
            namespace: "homebrew_formula".into(),
            external_id: "ripgrep".into(),
            source_url: None,
        },
    ];
    db.software_service().create_software(first).unwrap();

    let mut second = software_cmd("Another Name", SoftwareCategory::Cli);
    second.external_refs = vec![
        assetmesh_core::application::media_service::ExternalRefInput {
            namespace: "homebrew_formula".into(),
            external_id: "ripgrep".into(),
            source_url: None,
        },
    ];
    let error = db.software_service().create_software(second).unwrap_err();
    assert!(matches!(error, AppError::Conflict { .. }), "{error}");

    let mut duplicates = db.duplicates();
    let page = duplicates.candidates(&DuplicateQuery::default()).unwrap();
    assert!(page.items.is_empty());
    let _ = LifecycleFilter::Active;
}
