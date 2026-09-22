//! Unified library application use-case tests — Phase 4A (docs/11).
//!
//! These run against the in-memory port doubles. The same scenarios run
//! against real SQLite in `storage-sqlite/tests/library_query_contracts.rs`
//! so the two adapters are proven to agree at the application boundary.

mod support;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::library_service::{
    AssetDetails, AssetSummary, LibraryModule, LibraryQuery, LibrarySearchQuery, LibraryService,
    LibrarySort, MergedTombstoneView, Page, PageRequest, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT,
};
use assetmesh_core::application::media_service::CreateMedia;
use assetmesh_core::application::search_service::SearchService;
use assetmesh_core::application::service_service::CreateService;
use assetmesh_core::application::software_service::CreateSoftware;
use assetmesh_core::domain::asset::{Asset, AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use assetmesh_core::domain::media::MediaType;
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::{SoftwareCategory, SoftwareRecord};
use assetmesh_core::ports::repos::{
    ActivityReader, AssetReader, ExternalRefReader, LibraryReadPort, LifecycleFilter, MediaReader,
    RelationReader, ServiceReader, SoftwareReader, TagReader,
};
use assetmesh_core::ports::search::SearchReader;
use assetmesh_core::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use assetmesh_core::AppError;
use support::{MemFactory, TestEnv};

// ---------------------------------------------------------------------------
// Test scaffolding
// ---------------------------------------------------------------------------

/// One asset of each module, plus a CJK-named service so the substring search
/// fallback is exercised across a language FTS5 tokenization cannot serve.
struct Seeded {
    env: TestEnv,
    media: AssetId,
    software: AssetId,
    service: AssetId,
    cjk: AssetId,
}

fn seed() -> Seeded {
    let env = support::test_env();

    let media = env
        .media_service()
        .create_media(media_cmd(
            "Sousou no Frieren",
            MediaType::Anime,
            &["healing"],
        ))
        .unwrap();
    let software = env
        .software_service()
        .create_software(software_cmd("ripgrep", SoftwareCategory::Cli, &["dev"]))
        .unwrap();
    let service = env
        .service_service()
        .create_service(service_cmd("OpenAI", ServiceType::Saas, &["ai"]))
        .unwrap();
    let cjk = env
        .service_service()
        .create_service(service_cmd("腾讯云", ServiceType::Saas, &["cloud"]))
        .unwrap();

    Seeded {
        env,
        media: media.entry.asset.id,
        software: software.entry.asset.id,
        service: service.entry.asset.id,
        cjk: cjk.entry.asset.id,
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

fn default_query() -> LibraryQuery {
    LibraryQuery::default()
}

fn summary_ids(page: &Page<AssetSummary>) -> Vec<AssetId> {
    page.items.iter().map(|row| row.id).collect()
}

/// Wraps [`MemFactory`] to record how many read scopes a service opened and
/// which repository capabilities it touched. This is the seam that proves a
/// unified read is one snapshot and does not degrade into per-asset
/// repository round-trips — no production code exists only for the test.
#[derive(Clone, Default)]
struct ReadProbe {
    scopes: Rc<RefCell<usize>>,
    accesses: Rc<RefCell<Vec<&'static str>>>,
}

impl ReadProbe {
    /// One read scope, and no repository capability used more than once per
    /// unified query: a second use of the same capability is the signature of
    /// a per-asset loop.
    fn assert_single_bounded_read(&self) {
        assert_eq!(
            *self.scopes.borrow(),
            1,
            "one unified read must run in exactly one QueryUnitOfWork snapshot"
        );
        let mut counts: HashMap<&'static str, usize> = HashMap::new();
        for name in self.accesses.borrow().iter() {
            *counts.entry(name).or_default() += 1;
        }
        for (capability, count) in counts {
            assert!(
                count <= 1,
                "repository capability {capability} was opened {count} times in one query; \
                 the unified library must not loop per asset"
            );
        }
    }
}

struct ProbeFactory {
    inner: MemFactory,
    probe: ReadProbe,
}

impl UnitOfWorkFactory for ProbeFactory {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> assetmesh_core::AppResult<T>,
    ) -> assetmesh_core::AppResult<T> {
        self.inner.transact(work)
    }

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> assetmesh_core::AppResult<T>,
    ) -> assetmesh_core::AppResult<T> {
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

impl ProbeQuery<'_> {
    fn note(&self, capability: &'static str) {
        self.accesses.borrow_mut().push(capability);
    }
}

macro_rules! probe_capability {
    ($method:ident, $reader:path, $capability:literal) => {
        fn $method(&mut self) -> &mut dyn $reader {
            self.note($capability);
            self.inner.$method()
        }
    };
}

impl QueryUnitOfWork for ProbeQuery<'_> {
    probe_capability!(assets, AssetReader, "assets");
    probe_capability!(media, MediaReader, "media");
    probe_capability!(software, SoftwareReader, "software");
    probe_capability!(services, ServiceReader, "services");
    probe_capability!(external_refs, ExternalRefReader, "external_refs");
    probe_capability!(activity, ActivityReader, "activity");
    probe_capability!(tags, TagReader, "tags");
    probe_capability!(relations, RelationReader, "relations");
    probe_capability!(search_index, SearchReader, "search_index");
    probe_capability!(library, LibraryReadPort, "library");
}

// ---------------------------------------------------------------------------
// Unified list
// ---------------------------------------------------------------------------

#[test]
fn list_returns_one_unified_row_per_module() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    let page = library.list_assets(&default_query()).unwrap();

    assert_eq!(page.total, Some(4));
    let kinds: Vec<AssetKind> = page.items.iter().map(|row| row.kind).collect();
    assert!(kinds.contains(&AssetKind::MediaAnime));
    assert!(kinds.contains(&AssetKind::SoftwareCli));
    assert!(kinds.contains(&AssetKind::ServiceSaas));

    let media = page
        .items
        .iter()
        .find(|row| row.id == seeded.media)
        .expect("media row");
    assert_eq!(media.name, "Sousou no Frieren");
    assert_eq!(media.lifecycle, LifecycleState::Active);
    assert_eq!(media.tags, vec!["healing".to_string()]);
    // The subtitle comes from the module's own summary helper.
    assert_eq!(media.subtitle.as_deref(), Some("Anime"));

    let software = page
        .items
        .iter()
        .find(|row| row.id == seeded.software)
        .expect("software row");
    assert_eq!(software.subtitle.as_deref(), Some("CLI Tool"));
    assert_eq!(software.tags, vec!["dev".to_string()]);

    let service = page
        .items
        .iter()
        .find(|row| row.id == seeded.service)
        .expect("service row");
    assert_eq!(service.subtitle.as_deref(), Some("SaaS"));
}

#[test]
fn empty_library_returns_an_empty_page() {
    let env = support::test_env();
    let mut library = env.library_service();

    let page = library.list_assets(&default_query()).unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));

    let search = library
        .search_assets(&LibrarySearchQuery {
            text: "anything".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(search.items.is_empty());
    // An empty result set has a known total of zero; only a relevance-ordered
    // page that actually matched rows reports no count.
    assert_eq!(search.total, Some(0));
}

#[test]
fn ordering_is_deterministic_with_an_asset_id_tie_breaker() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    // Every asset was created under one fixed clock, so `updated_at` ties for
    // all of them and the AssetId tie-breaker alone decides the order.
    let mut expected: Vec<AssetId> =
        vec![seeded.media, seeded.software, seeded.service, seeded.cjk];
    expected.sort();

    let first = library.list_assets(&default_query()).unwrap();
    let second = library.list_assets(&default_query()).unwrap();
    assert_eq!(summary_ids(&first), expected);
    assert_eq!(summary_ids(&first), summary_ids(&second));

    // Explicitly requested ascending update time keeps the same tie-breaker.
    let ascending = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::UpdatedAsc,
            ..default_query()
        })
        .unwrap();
    assert_eq!(summary_ids(&ascending), expected);
}

#[test]
fn name_and_kind_sorting_are_deterministic() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    let by_name = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::NameAsc,
            ..default_query()
        })
        .unwrap();
    let names: Vec<&str> = by_name.items.iter().map(|row| row.name.as_str()).collect();
    // Case-insensitive name order; the CJK name sorts after the ASCII names.
    assert_eq!(
        names,
        vec!["OpenAI", "ripgrep", "Sousou no Frieren", "腾讯云"]
    );

    let by_name_desc = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::NameDesc,
            ..default_query()
        })
        .unwrap();
    let reversed: Vec<&str> = by_name_desc
        .items
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(
        reversed,
        vec!["腾讯云", "Sousou no Frieren", "ripgrep", "OpenAI"]
    );

    let by_kind = library
        .list_assets(&LibraryQuery {
            sort: LibrarySort::KindAsc,
            ..default_query()
        })
        .unwrap();
    let kinds: Vec<AssetKind> = by_kind.items.iter().map(|row| row.kind).collect();
    let mut sorted_kinds = kinds.clone();
    sorted_kinds.sort_by_key(|kind| kind.as_str());
    assert_eq!(kinds, sorted_kinds);
}

#[test]
fn lifecycle_filter_controls_archived_and_merged_visibility() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    assets.archive_asset(seeded.media).unwrap();

    let active = library.list_assets(&default_query()).unwrap();
    assert_eq!(active.total, Some(3), "archived is opt-in by default");
    assert!(!summary_ids(&active).contains(&seeded.media));

    let with_archived = library
        .list_assets(&LibraryQuery {
            lifecycle: LifecycleFilter::ActiveOrArchived,
            ..default_query()
        })
        .unwrap();
    assert_eq!(with_archived.total, Some(4));
    assert!(summary_ids(&with_archived).contains(&seeded.media));

    let all = library
        .list_assets(&LibraryQuery {
            lifecycle: LifecycleFilter::All,
            ..default_query()
        })
        .unwrap();
    assert_eq!(all.total, Some(4));
}

#[test]
fn merged_tombstones_never_appear_in_the_library() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    // A second media asset merged into the first: the loser becomes a
    // tombstone and its module details move to the winner.
    let loser = seeded
        .env
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
                ..default_query()
            })
            .unwrap();
        assert!(
            !summary_ids(&page).contains(&loser.entry.asset.id),
            "merged tombstone must not be a library row ({lifecycle:?})"
        );
        assert!(
            summary_ids(&page).contains(&seeded.media),
            "the survivor stays in the library ({lifecycle:?})"
        );
    }
}

#[test]
fn module_and_kind_filters_restrict_the_library() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    let media_only = library
        .list_assets(&LibraryQuery {
            modules: vec![LibraryModule::Media],
            ..default_query()
        })
        .unwrap();
    assert_eq!(summary_ids(&media_only), vec![seeded.media]);

    let software_only = library
        .list_assets(&LibraryQuery {
            modules: vec![LibraryModule::Software],
            ..default_query()
        })
        .unwrap();
    assert_eq!(summary_ids(&software_only), vec![seeded.software]);

    let two_kinds = library
        .list_assets(&LibraryQuery {
            kinds: vec![AssetKind::MediaAnime, AssetKind::ServiceSaas],
            ..default_query()
        })
        .unwrap();
    let mut expected = vec![seeded.media, seeded.service, seeded.cjk];
    expected.sort();
    assert_eq!(summary_ids(&two_kinds), expected);

    // Contradictory filters match nothing and read no storage at all.
    let contradiction = library
        .list_assets(&LibraryQuery {
            modules: vec![LibraryModule::Media],
            kinds: vec![AssetKind::SoftwareCli],
            ..default_query()
        })
        .unwrap();
    assert!(contradiction.items.is_empty());
    assert_eq!(contradiction.total, Some(0));
}

#[test]
fn tag_filtering_requires_every_requested_tag() {
    let env = support::test_env();
    let mut library = env.library_service();
    let media = env
        .media_service()
        .create_media(media_cmd(
            "Tagged",
            MediaType::Anime,
            &["healing", "rewatch"],
        ))
        .unwrap();
    env.media_service()
        .create_media(media_cmd("Partly Tagged", MediaType::Movie, &["healing"]))
        .unwrap();

    let both = library
        .list_assets(&LibraryQuery {
            tags: vec!["healing".into(), "REWATCH".into()],
            ..default_query()
        })
        .unwrap();
    assert_eq!(summary_ids(&both), vec![media.entry.asset.id]);

    let one = library
        .list_assets(&LibraryQuery {
            tags: vec!["healing".into()],
            ..default_query()
        })
        .unwrap();
    assert_eq!(one.total, Some(2));

    let none = library
        .list_assets(&LibraryQuery {
            tags: vec!["missing".into()],
            ..default_query()
        })
        .unwrap();
    assert_eq!(none.total, Some(0));
}

#[test]
fn pagination_walks_the_whole_library_without_gaps_or_repeats() {
    let env = support::test_env();
    let mut library = env.library_service();
    let names = [
        "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf",
    ];
    for name in names {
        env.media_service()
            .create_media(media_cmd(name, MediaType::Anime, &[]))
            .unwrap();
    }

    let query = LibraryQuery {
        sort: LibrarySort::NameAsc,
        page: PageRequest::new(3, 0),
        ..default_query()
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
    let last_names: Vec<&str> = last.items.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(last_names, vec!["Golf"]);

    // Past the end is an empty page, not an error.
    let beyond = library
        .list_assets(&LibraryQuery {
            page: PageRequest::new(3, 99),
            ..query.clone()
        })
        .unwrap();
    assert!(beyond.items.is_empty());

    // Walking every page yields each row exactly once, in order.
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
    assert_eq!(walked.len(), 7);
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 7, "no row may repeat across pages");
}

#[test]
fn every_asset_kind_maps_to_exactly_one_module() {
    // A new module must extend `LibraryModule::of_kind` (and fail this test)
    // rather than have its kinds silently folded into another module.
    let expected = [
        ("media.movie", LibraryModule::Media),
        ("media.tv", LibraryModule::Media),
        ("media.anime", LibraryModule::Media),
        ("media.game", LibraryModule::Media),
        ("software.app", LibraryModule::Software),
        ("software.cli", LibraryModule::Software),
        ("software.package", LibraryModule::Software),
        ("software.runtime", LibraryModule::Software),
        ("software.tool", LibraryModule::Software),
        ("service.saas", LibraryModule::Services),
        ("service.api", LibraryModule::Services),
        ("service.vps", LibraryModule::Services),
        ("service.domain", LibraryModule::Services),
        ("service.local", LibraryModule::Services),
    ];
    for (kind, module) in expected {
        let kind = AssetKind::parse(kind).unwrap_or_else(|| panic!("{kind} must parse"));
        assert_eq!(LibraryModule::of_kind(kind), module, "{kind}");
        assert!(module.matches(kind), "{kind} must belong to {module:?}");
        assert_eq!(kind.module(), module.as_str());
    }
    // The three modules are the whole registry.
    assert_eq!(LibraryModule::ALL.len(), 3);
}

#[test]
fn page_size_is_bounded() {
    assert_eq!(PageRequest::default().limit, DEFAULT_PAGE_LIMIT);
    // Zero selects the documented default rather than an empty page.
    assert_eq!(PageRequest::new(0, 0).effective_limit(), DEFAULT_PAGE_LIMIT);
    assert_eq!(
        PageRequest::new(10_000, 0).effective_limit(),
        MAX_PAGE_LIMIT
    );
    assert_eq!(PageRequest::new(3, 6).effective_limit(), 3);
}

#[test]
fn a_filtered_search_page_loses_no_rows() {
    // Regression: the projection window was sized as `offset + limit` *before*
    // filtering, but the offset was applied *after* it — so a filter that
    // removed any hit shifted every later page and could make a matching row
    // unreachable by paging.
    let env = support::test_env();
    let mut library = env.library_service();
    for index in 0..6 {
        let mut cmd = media_cmd(&format!("Widget {index}"), MediaType::Movie, &[]);
        cmd.tags = if index % 2 == 0 {
            vec!["keep".to_string()]
        } else {
            Vec::new()
        };
        env.media_service().create_media(cmd).unwrap();
    }
    let tagged = env
        .media_service()
        .create_media(media_cmd("Widget Keep", MediaType::Movie, &["keep"]))
        .unwrap()
        .entry
        .asset
        .id;

    let query = LibrarySearchQuery {
        text: "widget".into(),
        tags: vec!["keep".into()],
        page: PageRequest::new(2, 0),
        ..Default::default()
    };
    let first = library.search_assets(&query).unwrap();
    assert_eq!(first.items.len(), 2, "page one is full despite the filter");
    assert_eq!(
        first.total,
        Some(4),
        "the index was exhausted, so the count is exact"
    );

    // Walking every page reaches all four tagged assets exactly once.
    let mut walked: Vec<AssetId> = Vec::new();
    let mut offset = 0;
    loop {
        let page = library
            .search_assets(&LibrarySearchQuery {
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
    assert_eq!(walked.len(), 4, "no matching row is dropped by paging");
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 4);
    assert!(walked.contains(&tagged));
}

#[test]
fn a_filtered_search_whose_matches_are_all_filtered_out_is_empty_not_wrong() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    // A term that matches, filtered down to nothing: the index is exhausted, so
    // the result is an honest zero rather than an unknown count.
    let page = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            tags: vec!["nothing-has-this-tag".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));
}

#[test]
fn merged_tombstones_are_excluded_under_every_lifecycle_filter() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    let loser = seeded
        .env
        .media_service()
        .create_media(media_cmd("Tombstone Candidate", MediaType::Anime, &[]))
        .unwrap();
    assets
        .merge_assets(loser.entry.asset.id, seeded.media)
        .unwrap();
    // The invariant must not depend on the merge having removed the loser's
    // module details: stage a surviving record on the tombstone directly.
    seeded.env.factory.store_borrow_mut(|store| {
        store.media.insert(
            loser.entry.asset.id.to_string(),
            assetmesh_core::domain::media::MediaRecord::new(
                loser.entry.asset.id,
                MediaType::Anime,
                assetmesh_core::domain::media::MediaStatus::Planned,
            ),
        );
    });

    for lifecycle in [
        LifecycleFilter::Active,
        LifecycleFilter::ActiveOrArchived,
        LifecycleFilter::All,
    ] {
        let page = library
            .list_assets(&LibraryQuery {
                lifecycle,
                ..default_query()
            })
            .unwrap();
        assert!(
            !summary_ids(&page).contains(&loser.entry.asset.id),
            "a tombstone must never be a library row ({lifecycle:?})"
        );
        let hits = library
            .search_assets(&LibrarySearchQuery {
                text: "tombstone".into(),
                lifecycle,
                ..Default::default()
            })
            .unwrap();
        assert!(
            !summary_ids(&hits).contains(&loser.entry.asset.id),
            "a tombstone must never be a search hit ({lifecycle:?})"
        );
    }
}

#[test]
fn an_absurd_search_offset_is_empty_not_unbounded() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    // The projection window is bounded, so a pathological offset cannot turn
    // into "ask the index for everything" — the page is simply past the end.
    let page = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            page: PageRequest::new(20, usize::MAX),
            ..Default::default()
        })
        .unwrap();
    assert!(page.items.is_empty());

    // A legitimate deep offset still returns the right page.
    let deep = library
        .list_assets(&LibraryQuery {
            page: PageRequest::new(10, usize::MAX),
            ..default_query()
        })
        .unwrap();
    assert!(deep.items.is_empty());
    assert_eq!(deep.total, Some(4));
}

// ---------------------------------------------------------------------------
// Unified detail
// ---------------------------------------------------------------------------

#[test]
fn detail_view_carries_typed_details_tags_and_refs_for_every_module() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );
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
    assert_eq!(software.asset.kind, AssetKind::SoftwareCli);
    assert!(software.external_refs.is_empty());
    match &software.details {
        AssetDetails::Software(record) => assert_eq!(record.category, SoftwareCategory::Cli),
        other => panic!("expected software details, got {other:?}"),
    }

    let service = library.get_asset(seeded.service).unwrap();
    assert_eq!(service.asset.kind, AssetKind::ServiceSaas);
    assert_eq!(service.tags, vec!["ai".to_string()]);
    match &service.details {
        AssetDetails::Service(record) => assert_eq!(record.service_type, ServiceType::Saas),
        other => panic!("expected service details, got {other:?}"),
    }
}

#[test]
fn detail_view_reports_unknown_and_detail_less_assets() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    let missing = library.get_asset(AssetId::generate());
    assert!(missing.is_err(), "an unknown id must not be found");

    // An asset with no module details is not a library entry. This state is
    // not reachable through the write paths, so it is staged directly.
    let bare = AssetId::generate();
    seeded.env.factory.store_borrow_mut(|store| {
        store.assets.insert(
            bare.to_string(),
            Asset::new(bare, AssetKind::MediaAnime, "No Details", None, store.now).unwrap(),
        );
    });
    let error = library.get_asset(bare).unwrap_err();
    assert!(
        error.to_string().contains("module details"),
        "expected a missing-details error, got: {error}"
    );
}

#[test]
fn detail_view_of_an_archived_asset_is_readable() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );
    assets.archive_asset(seeded.media).unwrap();

    let view = library.get_asset(seeded.media).unwrap();
    assert_eq!(view.asset.lifecycle_state, LifecycleState::Archived);
    match &view.details {
        AssetDetails::Media(record) => assert_eq!(record.media_type, MediaType::Anime),
        other => panic!("expected media details, got {other:?}"),
    }
}

#[test]
fn detail_view_of_a_merged_tombstone_names_the_survivor() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    let loser = seeded
        .env
        .media_service()
        .create_media(media_cmd("Duplicate", MediaType::Anime, &[]))
        .unwrap();
    assets
        .merge_assets(loser.entry.asset.id, seeded.media)
        .unwrap();

    let error = library.get_asset(loser.entry.asset.id).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains(&seeded.media.to_string()),
        "the redirect target must be named, got: {message}"
    );
}

#[test]
fn merged_tombstone_view_names_the_survivor_without_reaching_into_repositories() {
    // An adapter renders a tombstone through `get_merged_tombstone` alone —
    // it must not read repositories itself (CONTRIBUTING). This pins what the
    // use case returns and the two ways it refuses.
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    let loser = seeded
        .env
        .media_service()
        .create_media(media_cmd(
            "Duplicate",
            MediaType::Anime,
            &["shared", "loser-only"],
        ))
        .unwrap();
    assets
        .merge_assets(loser.entry.asset.id, seeded.media)
        .unwrap();

    let tombstone: MergedTombstoneView = library
        .get_merged_tombstone(loser.entry.asset.id)
        .expect("a merged loser has a tombstone");

    assert_eq!(tombstone.asset.id, loser.entry.asset.id);
    assert_eq!(tombstone.asset.lifecycle_state, LifecycleState::Merged);
    assert_eq!(tombstone.asset.merged_into, Some(seeded.media));

    // A canonical merge moves tags onto the survivor, so the tombstone keeps
    // none — the view exposes the columns anyway, and they must read empty
    // rather than silently reappearing on both sides of the merge.
    assert!(tombstone.tags.is_empty());
    assert!(tombstone.external_refs.is_empty());
    let _ = tombstone;

    // A live asset is not a tombstone — that is a conflict, not a not-found,
    // so the caller can tell "wrong method" from "no such asset".
    let live = library.get_merged_tombstone(seeded.media).unwrap_err();
    assert!(matches!(live, AppError::Conflict { .. }));

    // An unknown asset is still a not-found.
    let missing = library
        .get_merged_tombstone(AssetId::generate())
        .unwrap_err();
    assert!(matches!(missing, AppError::NotFound { .. }));
}

#[test]
fn merge_redirects_resolve_to_the_surviving_asset() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    // A live asset resolves to itself.
    assert_eq!(
        library.resolve_merge_redirect(seeded.media).unwrap(),
        seeded.media
    );

    // A chain a -> b -> c resolves to the final survivor.
    let middle = seeded
        .env
        .media_service()
        .create_media(media_cmd("Middle", MediaType::Anime, &[]))
        .unwrap();
    let tail = seeded
        .env
        .media_service()
        .create_media(media_cmd("Tail", MediaType::Anime, &[]))
        .unwrap();
    assets
        .merge_assets(middle.entry.asset.id, tail.entry.asset.id)
        .unwrap();
    assert_eq!(
        library
            .resolve_merge_redirect(middle.entry.asset.id)
            .unwrap(),
        tail.entry.asset.id
    );

    // A corrupt cycle fails loudly instead of looping.
    let first = AssetId::generate();
    let second = AssetId::generate();
    seeded.env.factory.store_borrow_mut(|store| {
        for (id, target) in [(first, second), (second, first)] {
            let mut asset =
                Asset::new(id, AssetKind::MediaAnime, "Cycle", None, store.now).unwrap();
            asset.mark_merged(target, store.now);
            store.assets.insert(id.to_string(), asset);
        }
    });
    assert!(library.resolve_merge_redirect(first).is_err());

    assert!(library.resolve_merge_redirect(AssetId::generate()).is_err());
}

#[test]
fn a_mismatched_module_detail_is_not_surfaced_as_a_library_row() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    // The repository boundary refuses this state on every write path; staging
    // it directly proves the library still refuses to publish a summary whose
    // kind and typed details disagree. The asset is given a software record
    // while its kind belongs to Media, and no media record at all.
    let mismatched = AssetId::generate();
    let mut record = SoftwareRecord::new(mismatched, SoftwareCategory::Cli);
    record.validate().unwrap();
    seeded.env.factory.store_borrow_mut(|store| {
        store.assets.insert(
            mismatched.to_string(),
            Asset::new(
                mismatched,
                AssetKind::MediaAnime,
                "Mismatched",
                None,
                store.now,
            )
            .unwrap(),
        );
        store.software.insert(mismatched.to_string(), record);
    });

    let page = library.list_assets(&default_query()).unwrap();
    assert!(
        !summary_ids(&page).contains(&mismatched),
        "a software record on a media asset must not become a library row"
    );
    // Every legitimately-modelled asset is still listed.
    assert_eq!(page.total, Some(4));

    // The detail view refuses it for the same reason instead of guessing.
    assert!(library.get_asset(mismatched).is_err());
}

// ---------------------------------------------------------------------------
// Unified search
// ---------------------------------------------------------------------------

#[test]
fn search_reaches_every_module_and_keeps_cjk_substring_behavior() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    let frieren = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&frieren), vec![seeded.media]);

    let ripgrep = library
        .search_assets(&LibrarySearchQuery {
            text: "ripgrep".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&ripgrep), vec![seeded.software]);

    let openai = library
        .search_assets(&LibrarySearchQuery {
            text: "openai".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&openai), vec![seeded.service]);

    // Substring matching must keep working for text tokenization cannot split.
    let cjk = library
        .search_assets(&LibrarySearchQuery {
            text: "腾讯".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&cjk), vec![seeded.cjk]);

    // An empty query is not an error.
    let empty = library
        .search_assets(&LibrarySearchQuery {
            text: "   ".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(empty.items.is_empty());
}

#[test]
fn search_results_use_the_same_summary_vocabulary_as_the_list() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    let listed = library.list_assets(&default_query()).unwrap();
    let found = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(found.items.len(), 1);

    let from_list = listed
        .items
        .iter()
        .find(|row| row.id == seeded.media)
        .unwrap()
        .clone();
    assert_eq!(
        found.items[0], from_list,
        "list and search must publish the same AssetSummary for one asset"
    );
}

#[test]
fn search_honors_lifecycle_module_and_tag_filters() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );
    assets.archive_asset(seeded.media).unwrap();

    let text = LibrarySearchQuery {
        text: "a".into(),
        ..Default::default()
    };

    // Archived is opt-in in search too, matching the list contract.
    let active = library.search_assets(&text).unwrap();
    assert!(!summary_ids(&active).contains(&seeded.media));

    let with_archived = library
        .search_assets(&LibrarySearchQuery {
            lifecycle: LifecycleFilter::ActiveOrArchived,
            ..text.clone()
        })
        .unwrap();
    assert!(summary_ids(&with_archived).contains(&seeded.media));

    let services_only = library
        .search_assets(&LibrarySearchQuery {
            modules: vec![LibraryModule::Services],
            ..text.clone()
        })
        .unwrap();
    assert_eq!(services_only.items.len(), 2);
    assert!(services_only
        .items
        .iter()
        .all(|row| row.kind.module() == "services"));

    let tagged = library
        .search_assets(&LibrarySearchQuery {
            tags: vec!["ai".into()],
            ..text.clone()
        })
        .unwrap();
    assert_eq!(summary_ids(&tagged), vec![seeded.service]);
}

#[test]
fn search_excludes_merged_tombstones() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = AssetService::new(
        seeded.env.factory.clone(),
        seeded.env.clock.clone(),
        seeded.env.ids.clone(),
    );

    let loser = seeded
        .env
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
            .search_assets(&LibrarySearchQuery {
                text: "frieren".into(),
                lifecycle,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            summary_ids(&page),
            vec![seeded.media],
            "only the survivor is searchable ({lifecycle:?})"
        );
    }
}

#[test]
fn search_pagination_pages_through_results() {
    let env = support::test_env();
    let mut library = env.library_service();
    for index in 0..5 {
        env.media_service()
            .create_media(media_cmd(&format!("Widget {index}"), MediaType::Movie, &[]))
            .unwrap();
    }

    let first = library
        .search_assets(&LibrarySearchQuery {
            text: "widget".into(),
            page: PageRequest::new(2, 0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(first.items.len(), 2);
    let second = library
        .search_assets(&LibrarySearchQuery {
            text: "widget".into(),
            page: PageRequest::new(2, 2),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(second.items.len(), 2);
    assert!(
        summary_ids(&first)
            .iter()
            .all(|id| !summary_ids(&second).contains(id)),
        "search pages must not repeat a row"
    );
    let third = library
        .search_assets(&LibrarySearchQuery {
            text: "widget".into(),
            page: PageRequest::new(2, 4),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(third.items.len(), 1);
}

#[test]
fn a_stale_projection_row_is_not_published_as_a_library_entry() {
    let seeded = seed();
    let mut library = seeded.env.library_service();

    // Rebuilding the projection is the documented way to keep derived state
    // fresh; a document for an asset whose module details are gone is stale.
    let stale = AssetId::generate();
    seeded.env.factory.store_borrow_mut(|store| {
        store.search_docs.insert(
            stale.to_string(),
            assetmesh_core::domain::search::SearchDocument {
                asset_id: stale,
                kind: AssetKind::MediaAnime.as_str().to_string(),
                title: "Ghost".into(),
                subtitle: None,
                body: None,
                keywords: Vec::new(),
                updated_at: store.now,
            },
        );
    });

    let page = library
        .search_assets(&LibrarySearchQuery {
            text: "ghost".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        !summary_ids(&page).contains(&stale),
        "a projection row without module details is not a library entry"
    );
}

// ---------------------------------------------------------------------------
// Consistency: one snapshot, no per-asset round-trips
// ---------------------------------------------------------------------------

#[test]
fn unified_reads_use_one_snapshot_and_no_per_asset_round_trips() {
    let seeded = seed();
    let probe = ReadProbe::default();
    let factory = ProbeFactory {
        inner: seeded.env.factory.clone(),
        probe: probe.clone(),
    };
    let mut library = LibraryService::new(factory);

    library.list_assets(&default_query()).unwrap();
    probe.assert_single_bounded_read();

    *probe.scopes.borrow_mut() = 0;
    probe.accesses.borrow_mut().clear();
    library.get_asset(seeded.media).unwrap();
    probe.assert_single_bounded_read();

    *probe.scopes.borrow_mut() = 0;
    probe.accesses.borrow_mut().clear();
    library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            ..Default::default()
        })
        .unwrap();
    probe.assert_single_bounded_read();
}

#[test]
fn the_projection_rebuild_path_still_agrees_with_the_library() {
    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut search = SearchService::new(seeded.env.factory.clone(), seeded.env.clock.clone());

    // Dropping the derived index and rebuilding it must not change what the
    // unified search finds (ADR 0006: the projection is disposable).
    seeded
        .env
        .factory
        .store_borrow_mut(|store| store.search_docs.clear());
    let before = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(before.items.is_empty());

    search.rebuild().unwrap();
    let after = library
        .search_assets(&LibrarySearchQuery {
            text: "frieren".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(summary_ids(&after), vec![seeded.media]);
}

#[test]
fn app_capabilities_exposes_modules_and_flags() {
    let seeded = seed();
    let library = seeded.env.library_service();
    let caps = library.capabilities();

    assert_eq!(caps.modules, vec!["media", "software", "services"]);
    assert!(!caps.features.runtime_enrichment);
    assert!(!caps.features.projects);
    assert!(!caps.features.agent_capabilities);
    assert!(!caps.features.knowledge_collections);
    assert!(caps.relation_types.contains(&"depends_on".to_string()));
    assert!(caps.relation_types.contains(&"dependency_of".to_string()));
    assert!(caps
        .storable_relation_types
        .contains(&"depends_on".to_string()));
    assert!(!caps
        .storable_relation_types
        .contains(&"dependency_of".to_string()));
}

#[test]
fn error_categories_map_to_stable_vocabulary() {
    use assetmesh_core::AppError;

    assert_eq!(AppError::validation("test").category(), "invalid_input");
    assert_eq!(AppError::not_found("asset", "id").category(), "not_found");
    assert_eq!(AppError::conflict("test").category(), "conflict");
    assert_eq!(AppError::stale_revision(1, 2).category(), "stale_revision");
    assert_eq!(
        AppError::setup_required("test").category(),
        "setup_required"
    );
    assert_eq!(
        AppError::provider_unavailable("test").category(),
        "unavailable"
    );
    assert_eq!(AppError::storage_busy("test").category(), "unavailable");
    assert_eq!(AppError::corrupt_data("test").category(), "corrupt_data");
    assert_eq!(AppError::storage("test").category(), "internal");
}

#[test]
fn optimistic_concurrency_stale_revision_rejected() {
    use assetmesh_core::application::software_service::UpdateSoftwareMetadata;

    let seeded = seed();
    let mut software_svc = seeded.env.software_service();

    // Initial revision is 1
    let detail = software_svc.get_software(seeded.software).unwrap();
    assert_eq!(detail.entry.asset.revision, 1);

    // Update with correct expected_revision passes and bumps revision to 2
    let updated = software_svc
        .update_metadata(UpdateSoftwareMetadata {
            asset_id: seeded.software,
            purpose: Some("testing revision bump".into()),
            expected_revision: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(updated.entry.asset.revision, 2);

    // Stale update with old expected_revision (1) is rejected
    let stale_err = software_svc
        .update_metadata(UpdateSoftwareMetadata {
            asset_id: seeded.software,
            purpose: Some("will fail".into()),
            expected_revision: Some(1),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(stale_err.category(), "stale_revision");
}

#[test]
fn asset_detail_outcome_covers_live_merged_redirect_and_not_found() {
    use assetmesh_core::application::library_service::AssetDetailOutcome;

    let seeded = seed();
    let mut library = seeded.env.library_service();
    let mut assets = seeded.env.asset_service();

    // 1. Live asset
    let live_outcome = library.get_detail(seeded.media).unwrap();
    assert!(live_outcome.is_live());
    assert!(!live_outcome.is_merged_redirect());
    assert_eq!(live_outcome.asset().id, seeded.media);
    match live_outcome {
        AssetDetailOutcome::Live(view) => {
            assert_eq!(view.asset.name, "Sousou no Frieren");
            assert_eq!(view.tags, vec!["healing".to_string()]);
        }
        AssetDetailOutcome::MergedRedirect(_) => panic!("expected Live outcome"),
    }

    // 2. Merged redirect asset
    let loser = seeded
        .env
        .media_service()
        .create_media(media_cmd("Frieren Duplicate", MediaType::Anime, &["dup"]))
        .unwrap();
    assets
        .merge_assets(loser.entry.asset.id, seeded.media)
        .unwrap();

    let merged_outcome = library.get_detail(loser.entry.asset.id).unwrap();
    assert!(!merged_outcome.is_live());
    assert!(merged_outcome.is_merged_redirect());
    assert_eq!(merged_outcome.asset().id, loser.entry.asset.id);
    assert_eq!(merged_outcome.asset().merged_into, Some(seeded.media));
    match merged_outcome {
        AssetDetailOutcome::MergedRedirect(tombstone) => {
            assert_eq!(tombstone.asset.id, loser.entry.asset.id);
            assert_eq!(tombstone.asset.lifecycle_state, LifecycleState::Merged);
            assert_eq!(tombstone.asset.merged_into, Some(seeded.media));
        }
        AssetDetailOutcome::Live(_) => panic!("expected MergedRedirect outcome"),
    }

    // 3. Not found
    let non_existent_id = AssetId::generate();
    let not_found_err = library.get_detail(non_existent_id).unwrap_err();
    assert_eq!(not_found_err.category(), "not_found");
}
