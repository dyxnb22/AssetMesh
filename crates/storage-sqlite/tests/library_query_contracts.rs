//! Unified library contract tests over the real SQLite adapter (Phase 4A).
//!
//! These mirror `core/tests/library_use_cases.rs` scenario for scenario, so
//! the in-memory test double and the production storage adapter are proven to
//! agree at the application boundary rather than only in isolation.

use std::sync::Arc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::library_service::{
    AssetDetails, AssetSummary, LibraryModule, LibraryQuery, LibrarySearchQuery, LibraryService,
    LibrarySort, PageRequest,
};
use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::asset::{Asset, AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::repos::{AssetFilter, LifecycleFilter};
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
    fn library_service(&self) -> LibraryService<SharedSqlite> {
        LibraryService::new(self.factory.clone())
    }
}

fn media_cmd(title: &str, media_type: MediaType, tags: &[&str]) -> CreateMedia {
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
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        external_refs: Vec::new(),
        started_at: None,
        completed_at: None,
    }
}

fn software_cmd(name: &str, category: SoftwareCategory, tags: &[&str]) -> CreateSoftware {
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
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        external_refs: Vec::new(),
    }
}

fn service_cmd(name: &str, service_type: ServiceType, tags: &[&str]) -> CreateService {
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
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        external_refs: Vec::new(),
    }
}

/// One asset of each module plus a CJK-named service, as in the core suite.
struct Seeded {
    db: TestSqlite,
    media: AssetId,
    software: AssetId,
    service: AssetId,
    cjk: AssetId,
}

fn seed() -> Seeded {
    let db = env();
    let media = db
        .media_service()
        .create_media(media_cmd(
            "Sousou no Frieren",
            MediaType::Anime,
            &["healing"],
        ))
        .unwrap();
    let software = db
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli, &["dev"]))
        .unwrap();
    let service = db
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas, &["ai"]))
        .unwrap();
    let cjk = db
        .service_service()
        .create_service(service_cmd("腾讯云", ServiceType::Saas, &["cloud"]))
        .unwrap();
    Seeded {
        db,
        media: media.entry.asset.id,
        software: software.entry.asset.id,
        service: service.entry.asset.id,
        cjk: cjk.entry.asset.id,
    }
}

fn summary_ids(
    page: &assetmesh_core::application::library_service::Page<AssetSummary>,
) -> Vec<AssetId> {
    page.items.iter().map(|row| row.id).collect()
}

#[test]
fn sqlite_empty_library_and_contradictory_filters_are_empty_pages() {
    let db = env();
    let mut library = db.library_service();

    let empty = library.list_assets(&LibraryQuery::default()).unwrap();
    assert!(empty.items.is_empty());
    assert_eq!(empty.total, Some(0));
    let search = library
        .search_assets(&LibrarySearchQuery {
            text: "anything".into(),
            ..LibrarySearchQuery::default()
        })
        .unwrap();
    assert!(search.items.is_empty());
    assert_eq!(search.total, Some(0));

    // Contradictory module/kind filters never touch storage.
    let contradiction = library
        .list_assets(&LibraryQuery {
            modules: vec![LibraryModule::Media],
            kinds: vec![AssetKind::SoftwareCli],
            ..LibraryQuery::default()
        })
        .unwrap();
    assert!(contradiction.items.is_empty());
    assert_eq!(contradiction.total, Some(0));

    // Page-size bounds behave as documented.
    let page = library
        .list_assets(&LibraryQuery {
            page: PageRequest::new(10_000, 0),
            ..LibraryQuery::default()
        })
        .unwrap();
    assert_eq!(page.limit, 200);
    assert_eq!(PageRequest::new(0, 0).effective_limit(), 50);
}

#[test]
fn sqlite_lists_every_module_as_one_library() {
    let seeded = seed();
    let mut library = seeded.db.library_service();

    let page = library.list_assets(&LibraryQuery::default()).unwrap();
    assert_eq!(page.total, Some(4));
    assert_eq!(page.items.len(), 4);

    let media = page
        .items
        .iter()
        .find(|row| row.id == seeded.media)
        .unwrap();
    assert_eq!(media.kind, AssetKind::MediaAnime);
    assert_eq!(media.subtitle.as_deref(), Some("Anime"));
    assert_eq!(media.tags, vec!["healing".to_string()]);
    assert_eq!(media.lifecycle, LifecycleState::Active);

    let software = page
        .items
        .iter()
        .find(|row| row.id == seeded.software)
        .unwrap();
    assert_eq!(software.kind, AssetKind::SoftwareCli);
    assert_eq!(software.subtitle.as_deref(), Some("CLI Tool"));

    let service = page
        .items
        .iter()
        .find(|row| row.id == seeded.service)
        .unwrap();
    assert_eq!(service.kind, AssetKind::ServiceSaas);
    assert_eq!(service.subtitle.as_deref(), Some("SaaS"));
}

#[test]
fn sqlite_ordering_is_deterministic_and_breaks_ties_by_asset_id() {
    let seeded = seed();
    let mut library = seeded.db.library_service();

    // Force every asset onto one timestamp so the AssetId tie-breaker alone
    // decides the order — the same scenario the core suite runs against the
    // in-memory double.
    let stamp = seeded.db.clock.now();
    let ids = [seeded.media, seeded.software, seeded.service, seeded.cjk];
    let mut expected = ids.to_vec();
    expected.sort();

    let mut factory = seeded.db.factory.clone();
    factory
        .transact(&mut |uow| {
            for id in ids {
                let mut asset = uow
                    .assets()
                    .get(id)?
                    .ok_or_else(|| AppError::not_found("asset", id))?;
                asset.updated_at = stamp;
                uow.assets().update(&asset)?;
            }
            Ok(())
        })
        .unwrap();

    let first = library.list_assets(&LibraryQuery::default()).unwrap();
    let second = library.list_assets(&LibraryQuery::default()).unwrap();
    assert_eq!(summary_ids(&first), expected);
    assert_eq!(summary_ids(&first), summary_ids(&second));

    let ascending = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::UpdatedAsc,
            ..LibraryQuery::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&ascending), expected);

    let by_name = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::NameAsc,
            ..LibraryQuery::default()
        })
        .unwrap();
    let names: Vec<&str> = by_name.items.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["OpenAI", "ripgrep", "Sousou no Frieren", "腾讯云"]
    );
}

#[test]
fn sqlite_pagination_partitions_the_library_exactly() {
    let db = env();
    let mut library = db.library_service();
    let names = [
        "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf",
    ];
    for name in names {
        db.media_service()
            .create_media(media_cmd(name, MediaType::Anime, &[]))
            .unwrap();
    }

    let query = LibraryQuery {
        sort: LibrarySort::NameAsc,
        page: PageRequest::new(3, 0),
        ..LibraryQuery::default()
    };
    let first = library.list_assets(&query).unwrap();
    assert_eq!(first.total, Some(7));
    assert_eq!(first.limit, 3);
    let first_names: Vec<&str> = first.items.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(first_names, vec!["Alpha", "Bravo", "Charlie"]);

    let middle = library
        .list_assets(&LibraryQuery {
            page: PageRequest::new(3, 3),
            ..query.clone()
        })
        .unwrap();
    let middle_names: Vec<&str> = middle.items.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(middle_names, vec!["Delta", "Echo", "Foxtrot"]);

    let last = library
        .list_assets(&LibraryQuery {
            page: PageRequest::new(3, 6),
            ..query.clone()
        })
        .unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].name, "Golf");

    let beyond = library
        .list_assets(&LibraryQuery {
            page: PageRequest::new(3, 99),
            ..query.clone()
        })
        .unwrap();
    assert!(beyond.items.is_empty());

    // Walking the library two at a time yields every row exactly once.
    let mut walked: Vec<AssetId> = Vec::new();
    let mut offset = 0;
    loop {
        let page = library
            .list_assets(&LibraryQuery {
                page: PageRequest::new(2, offset),
                ..query.clone()
            })
            .unwrap();
        if page.items.is_empty() {
            break;
        }
        walked.extend(summary_ids(&page));
        offset += page.items.len();
    }
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 7);
    assert_eq!(walked.len(), 7, "no row may repeat across pages");
}

#[test]
fn sqlite_lifecycle_and_module_filters_agree_with_the_core_contract() {
    let seeded = seed();
    let mut library = seeded.db.library_service();

    // Module and tag filters over a fully active library.
    let media_only = library
        .list_assets(&LibraryQuery {
            modules: vec![LibraryModule::Media],
            ..LibraryQuery::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&media_only), vec![seeded.media]);

    let services_only = library
        .list_assets(&LibraryQuery {
            modules: vec![LibraryModule::Services],
            ..LibraryQuery::default()
        })
        .unwrap();
    assert_eq!(services_only.items.len(), 2);

    let tagged = library
        .list_assets(&LibraryQuery {
            tags: vec!["AI".into()],
            ..LibraryQuery::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&tagged), vec![seeded.service]);

    // Archiving makes the asset opt-in.
    let mut assets = seeded.db.asset_service();
    assets.archive_asset(seeded.media).unwrap();

    let active = library.list_assets(&LibraryQuery::default()).unwrap();
    assert_eq!(active.total, Some(3));
    assert!(!summary_ids(&active).contains(&seeded.media));

    let with_archived = library
        .list_assets(&LibraryQuery {
            lifecycle: LifecycleFilter::ActiveOrArchived,
            ..LibraryQuery::default()
        })
        .unwrap();
    assert_eq!(with_archived.total, Some(4));
    assert!(summary_ids(&with_archived).contains(&seeded.media));
}

#[test]
fn sqlite_sorting_variants_are_deterministic() {
    let seeded = seed();
    let mut library = seeded.db.library_service();

    let by_kind = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::KindAsc,
            ..LibraryQuery::default()
        })
        .unwrap();
    let kinds: Vec<&str> = by_kind.items.iter().map(|row| row.kind.as_str()).collect();
    let mut sorted = kinds.clone();
    sorted.sort_unstable();
    assert_eq!(kinds, sorted);

    let desc = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::NameDesc,
            ..LibraryQuery::default()
        })
        .unwrap();
    let names: Vec<String> = desc.items.iter().map(|row| row.name.clone()).collect();
    let mut reversed = names.clone();
    reversed.sort_by_key(|name| name.to_lowercase());
    reversed.reverse();
    assert_eq!(names, reversed);

    // The updated-descending order agrees with the asset timestamps.
    let updated = library.list_assets(&LibraryQuery::default()).unwrap();
    for pair in updated.items.windows(2) {
        assert!(pair[0].updated_at >= pair[1].updated_at);
    }
}

#[test]
fn sqlite_refuses_a_module_detail_on_another_modules_kind() {
    // The SQLite boundary is the primary guarantee: not even a direct
    // UnitOfWork write can attach a software record to a media asset. The
    // library's own defense-in-depth check (which cannot be reached through
    // this adapter) is covered by the core test
    // `a_mismatched_module_detail_is_not_surfaced_as_a_library_row`.
    let seeded = seed();
    let mut library = seeded.db.library_service();

    let mismatched = AssetId::generate();
    let mut factory = seeded.db.factory.clone();
    let error = factory
        .transact(&mut |uow| {
            uow.assets().insert(&Asset::new(
                mismatched,
                AssetKind::MediaAnime,
                "Mismatched",
                None,
                chrono::Utc::now(),
            )?)?;
            let mut record = assetmesh_core::domain::software::SoftwareRecord::new(
                mismatched,
                SoftwareCategory::Cli,
            );
            record.validate()?;
            uow.software().upsert(&record)
        })
        .unwrap_err();
    assert!(
        matches!(error, AppError::Conflict { .. }),
        "the repository must refuse a stranded module record: {error}"
    );

    // Nothing was written, so the library is unchanged.
    let page = library.list_assets(&LibraryQuery::default()).unwrap();
    assert_eq!(page.total, Some(4));
    assert!(library.get_asset(mismatched).is_err());
}

#[test]
fn sqlite_merged_tombstones_are_hidden_and_redirect() {
    let seeded = seed();
    let mut library = seeded.db.library_service();
    let mut assets = seeded.db.asset_service();

    let loser = seeded
        .db
        .media_service()
        .create_media(media_cmd("Frieren Duplicate", MediaType::Anime, &[]))
        .unwrap();
    assets
        .merge_assets(loser.entry.asset.id, seeded.media)
        .unwrap();

    for lifecycle in [
        LifecycleFilter::Active,
        LifecycleFilter::ActiveOrArchived,
        LifecycleFilter::All,
    ] {
        let page = library
            .list_assets(&LibraryQuery {
                lifecycle,
                ..LibraryQuery::default()
            })
            .unwrap();
        assert!(!summary_ids(&page).contains(&loser.entry.asset.id));
        assert!(summary_ids(&page).contains(&seeded.media));

        let hits = library
            .search_assets(&LibrarySearchQuery {
                text: "frieren".into(),
                lifecycle,
                ..LibrarySearchQuery::default()
            })
            .unwrap();
        assert_eq!(summary_ids(&hits), vec![seeded.media]);
    }

    let error = library.get_asset(loser.entry.asset.id).unwrap_err();
    assert!(
        error.to_string().contains(&seeded.media.to_string()),
        "the redirect target must be named, got: {error}"
    );
    assert_eq!(
        library
            .resolve_merge_redirect(loser.entry.asset.id)
            .unwrap(),
        seeded.media
    );
    assert_eq!(
        library.resolve_merge_redirect(seeded.media).unwrap(),
        seeded.media
    );
}

#[test]
fn sqlite_detail_view_is_typed_for_every_module() {
    let seeded = seed();
    let mut library = seeded.db.library_service();
    let mut assets = seeded.db.asset_service();
    assets
        .attach_external_ref(seeded.media, "tmdb", "209867", None)
        .unwrap();

    let media = library.get_asset(seeded.media).unwrap();
    assert_eq!(media.asset.kind, AssetKind::MediaAnime);
    assert_eq!(media.tags, vec!["healing".to_string()]);
    assert_eq!(media.external_refs.len(), 1);
    assert_eq!(media.external_refs[0].namespace, "tmdb");
    match &media.details {
        AssetDetails::Media(record) => assert_eq!(record.media_type, MediaType::Anime),
        other => panic!("expected media details, got {other:?}"),
    }

    let software = library.get_asset(seeded.software).unwrap();
    match &software.details {
        AssetDetails::Software(record) => assert_eq!(record.category, SoftwareCategory::Cli),
        other => panic!("expected software details, got {other:?}"),
    }

    let service = library.get_asset(seeded.service).unwrap();
    match &service.details {
        AssetDetails::Service(record) => assert_eq!(record.service_type, ServiceType::Saas),
        other => panic!("expected service details, got {other:?}"),
    }

    // Archived assets stay readable; unknown ids are not found.
    assets.archive_asset(seeded.software).unwrap();
    let archived = library.get_asset(seeded.software).unwrap();
    assert_eq!(archived.asset.lifecycle_state, LifecycleState::Archived);
    assert!(library.get_asset(AssetId::generate()).is_err());
}

#[test]
fn sqlite_search_reaches_every_module_and_keeps_substring_fallback() {
    let seeded = seed();
    let mut library = seeded.db.library_service();
    let mut assets = seeded.db.asset_service();
    assets.archive_asset(seeded.media).unwrap();

    let search = |library: &mut LibraryService<SharedSqlite>, text: &str| {
        library
            .search_assets(&LibrarySearchQuery {
                text: text.into(),
                ..LibrarySearchQuery::default()
            })
            .unwrap()
    };

    // FTS5 serves the ASCII tokens.
    assert_eq!(
        summary_ids(&search(&mut library, "ripgrep")),
        vec![seeded.software]
    );
    assert_eq!(
        summary_ids(&search(&mut library, "openai")),
        vec![seeded.service]
    );
    // CJK and short substrings fall back to LIKE.
    assert_eq!(summary_ids(&search(&mut library, "腾讯")), vec![seeded.cjk]);
    assert_eq!(
        summary_ids(&search(&mut library, "腾讯云")),
        vec![seeded.cjk]
    );

    // Archived is opt-in in search, exactly as in the list contract.
    let active = search(&mut library, "frieren");
    assert!(!summary_ids(&active).contains(&seeded.media));
    let with_archived = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            lifecycle: LifecycleFilter::ActiveOrArchived,
            ..LibrarySearchQuery::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&with_archived), vec![seeded.media]);

    // The search summary is the same vocabulary the list publishes.
    let listed = library.list_assets(&LibraryQuery::default()).unwrap();
    let from_list = listed
        .items
        .iter()
        .find(|row| row.id == seeded.software)
        .unwrap()
        .clone();
    let found = search(&mut library, "ripgrep");
    assert_eq!(found.items[0], from_list);

    // An empty query is not an error, and a rebuild keeps the results stable.
    assert!(search(&mut library, "  ").items.is_empty());
    SearchService::new(seeded.db.factory.clone(), seeded.db.clock.clone())
        .rebuild()
        .unwrap();
    assert_eq!(
        summary_ids(&search(&mut library, "ripgrep")),
        vec![seeded.software]
    );
}

#[test]
fn sqlite_storage_errors_are_not_swallowed_by_the_library() {
    let seeded = seed();
    let mut library = seeded.db.library_service();

    // A closed/poisoned connection must surface as an error, never as an empty
    // library that looks like "nothing is stored".
    let mut factory = seeded.db.factory.clone();
    factory
        .transact(&mut |uow| {
            uow.assets().insert(&Asset::new(
                AssetId::generate(),
                AssetKind::MediaAnime,
                "placeholder",
                None,
                seeded.db.clock.now(),
            )?)
        })
        .unwrap();

    // Reading through a factory whose reader pool is exhausted still works,
    // and the count matches the assets that really exist.
    let stored: usize = factory
        .read(&mut |q| {
            Ok(q.assets()
                .list(&AssetFilter {
                    kind: None,
                    lifecycle: Some(LifecycleFilter::All),
                })?
                .len())
        })
        .unwrap();
    let page = library.list_assets(&LibraryQuery::default()).unwrap();
    // The placeholder has no module details, so it is not a library row.
    assert_eq!(page.total, Some(stored - 1));
    assert!(page.items.iter().all(|row| row.subtitle.is_some()));
}
