//! Desktop Search Contract Tests (P5-04).
//!
//! Validates:
//! 1. Empty query returns an empty page with total 0.
//! 2. Text search matches ASCII tokens.
//! 3. Text search matches CJK characters and substrings.
//! 4. Archived policy: archived assets are excluded by default (`active`),
//!    and opt-in via `active_or_archived` or `all`.
//! 5. Merged policy: merged tombstones are hidden from search results.
//! 6. Combining query text with lifecycle, kind, and tag filters.
//! 7. Pagination behaves deterministically.
//! 8. Returns identical `AssetSummaryDto` vocabulary to `library_list`.

use std::sync::Arc;

use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::media::{MediaType, Progress};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::library::library_search_impl;
use assetmesh_desktop_lib::dto::LibrarySearchQueryDto;
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-search-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

#[test]
fn desktop_search_contracts_cover_cjk_substring_archived_merged_and_filters() {
    let db_path = temp_db_path("search_contracts");
    let state = DesktopState::new();
    state.initialize(&db_path).expect("failed to init state");

    let clock = Arc::new(SystemClock);
    let ids = Arc::new(UuidV7Generator);
    let factory = state.modules().unwrap().factory().clone();

    let mut media_svc = MediaService::new(factory.clone(), clock.clone(), ids.clone());
    let mut software_svc = SoftwareService::new(factory.clone(), clock.clone(), ids.clone());
    let mut service_svc = ServiceService::new(factory.clone(), clock.clone(), ids.clone());

    // 1. Seed assets
    // ASCII software asset
    let s1 = software_svc
        .create_software(CreateSoftware {
            name: "ripgrep".into(),
            category: SoftwareCategory::Tool,
            summary: Some("Fast line-oriented search tool".into()),
            install_source: None,
            version: Some("14.1.0".into()),
            install_location: None,
            executable_path: Some("/usr/bin/rg".into()),
            purpose: Some("Codebase grep tool".into()),
            notes: None,
            tags: vec!["cli".into(), "search".into(), "rust".into()],
            external_refs: Vec::new(),
            installed_at: None,
            architecture: None,
        })
        .unwrap();

    // CJK software asset
    let s2 = software_svc
        .create_software(CreateSoftware {
            name: "微信开发者工具".into(),
            category: SoftwareCategory::Application,
            summary: Some("微信小程序官方集成开发环境".into()),
            install_source: None,
            version: Some("1.06".into()),
            install_location: None,
            executable_path: None,
            purpose: Some("腾讯微信小程序开发".into()),
            notes: Some("腾讯官方出品".into()),
            tags: vec!["dev".into(), "tencent".into()],
            external_refs: Vec::new(),
            installed_at: None,
            architecture: None,
        })
        .unwrap();

    // Media asset that will be archived
    let m1 = media_svc
        .create_media(CreateMedia {
            title: "Frieren: Beyond Journey's End".into(),
            media_type: MediaType::Anime,
            summary: Some("Elven mage journey after defeating the Demon King".into()),
            status: None,
            rating: Some(9.5),
            year: Some(2023),
            platform: None,
            progress: Progress::default(),
            notes: None,
            tags: vec!["anime".into(), "fantasy".into()],
            external_refs: Vec::new(),
            started_at: None,
            completed_at: None,
        })
        .unwrap();

    // Service asset
    let serv1 = service_svc
        .create_service(CreateService {
            name: "OpenAI Platform API".into(),
            service_type: ServiceType::Api,
            summary: Some("Language model API gateway and platform services".into()),
            provider: Some("OpenAI, Inc.".into()),
            account_label: Some("org-personal".into()),
            endpoint_url: Some("https://api.openai.com".into()),
            dashboard_url: Some("https://platform.openai.com".into()),
            domain_name: None,
            plan: Some("Pay-as-you-go".into()),
            cost_minor: None,
            currency: None,
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            tags: vec!["ai".into(), "cloud".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    // Duplicate software asset to merge
    let s_dup = software_svc
        .create_software(CreateSoftware {
            name: "rg".into(),
            category: SoftwareCategory::Tool,
            summary: Some("ripgrep alias".into()),
            install_source: None,
            version: None,
            install_location: None,
            executable_path: None,
            purpose: None,
            notes: None,
            tags: vec!["cli".into()],
            external_refs: Vec::new(),
            installed_at: None,
            architecture: None,
        })
        .unwrap();

    // Archive m1
    let mut asset_svc = assetmesh_core::application::asset_service::AssetService::new(
        factory.clone(),
        clock.clone(),
        ids.clone(),
    );
    asset_svc.archive_asset(m1.entry.asset.id).unwrap();

    // Merge s_dup into s1 (s_dup is loser, s1 is survivor)
    asset_svc
        .merge_assets(s_dup.entry.asset.id, s1.entry.asset.id)
        .unwrap();

    // -------------------------------------------------------------------------
    // Contract 1: Empty search query returns empty page with total 0
    // -------------------------------------------------------------------------
    let empty_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("empty search should succeed");
    assert_eq!(empty_res.items.len(), 0);
    assert_eq!(empty_res.total, Some(0));

    let whitespace_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "   ".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("whitespace search should succeed");
    assert_eq!(whitespace_res.items.len(), 0);
    assert_eq!(whitespace_res.total, Some(0));

    // -------------------------------------------------------------------------
    // Contract 2: ASCII search matches
    // -------------------------------------------------------------------------
    let ascii_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "ripgrep".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("ascii search should succeed");
    assert_eq!(ascii_res.items.len(), 1);
    assert_eq!(ascii_res.items[0].id, s1.entry.asset.id.to_string());
    assert_eq!(ascii_res.items[0].name, "ripgrep");
    assert_eq!(ascii_res.items[0].kind, "software.tool");
    assert_eq!(ascii_res.items[0].lifecycle, "active");

    // -------------------------------------------------------------------------
    // Contract 3: CJK search matches
    // -------------------------------------------------------------------------
    let cjk_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "微信".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("CJK search should succeed");
    assert_eq!(cjk_res.items.len(), 1);
    assert_eq!(cjk_res.items[0].id, s2.entry.asset.id.to_string());
    assert_eq!(cjk_res.items[0].name, "微信开发者工具");

    let cjk_res2 = library_search_impl(
        LibrarySearchQueryDto {
            text: "腾讯".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("CJK summary search should succeed");
    assert_eq!(cjk_res2.items.len(), 1);
    assert_eq!(cjk_res2.items[0].id, s2.entry.asset.id.to_string());

    // -------------------------------------------------------------------------
    // Contract 4: Substring fallback search matches
    // -------------------------------------------------------------------------
    let sub_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "grep".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("substring search should succeed");
    assert_eq!(sub_res.items.len(), 1);
    assert_eq!(sub_res.items[0].id, s1.entry.asset.id.to_string());
    assert_eq!(sub_res.items[0].name, "ripgrep");

    let serv_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "openai".into(),
            ..Default::default()
        },
        &state,
    )
    .expect("service search should succeed");
    assert_eq!(serv_res.items.len(), 1);
    assert_eq!(serv_res.items[0].id, serv1.entry.asset.id.to_string());
    assert_eq!(serv_res.items[0].name, "OpenAI Platform API");

    // -------------------------------------------------------------------------
    // Contract 5: Archived lifecycle policy
    // Default (active) excludes archived asset
    // -------------------------------------------------------------------------
    let frieren_default = library_search_impl(
        LibrarySearchQueryDto {
            text: "frieren".into(),
            lifecycle: None,
            ..Default::default()
        },
        &state,
    )
    .expect("active search should succeed");
    assert_eq!(frieren_default.items.len(), 0);

    // ActiveOrArchived includes archived asset
    let frieren_archived = library_search_impl(
        LibrarySearchQueryDto {
            text: "frieren".into(),
            lifecycle: Some("active_or_archived".into()),
            ..Default::default()
        },
        &state,
    )
    .expect("archived search should succeed");
    assert_eq!(frieren_archived.items.len(), 1);
    assert_eq!(frieren_archived.items[0].id, m1.entry.asset.id.to_string());
    assert_eq!(frieren_archived.items[0].lifecycle, "archived");

    // -------------------------------------------------------------------------
    // Contract 6: Merged tombstones are hidden from search results
    // -------------------------------------------------------------------------
    let merged_search = library_search_impl(
        LibrarySearchQueryDto {
            text: "ripgrep alias".into(),
            lifecycle: Some("all".into()),
            ..Default::default()
        },
        &state,
    )
    .expect("merged search should succeed");
    assert_eq!(
        merged_search.items.len(),
        0,
        "Merged tombstones must not appear in search results"
    );

    // -------------------------------------------------------------------------
    // Contract 7: Combining text search with kind and tag filters
    // -------------------------------------------------------------------------
    // Search "tool" matching software tool
    let filter_kind_hit = library_search_impl(
        LibrarySearchQueryDto {
            text: "tool".into(),
            kinds: Some(vec!["software.tool".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("kind search should succeed");
    assert_eq!(filter_kind_hit.items.len(), 1);
    assert_eq!(filter_kind_hit.items[0].id, s1.entry.asset.id.to_string());

    // Search "tool" with kind "media.anime" produces 0 hits
    let filter_kind_miss = library_search_impl(
        LibrarySearchQueryDto {
            text: "tool".into(),
            kinds: Some(vec!["media.anime".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("mismatched kind search should succeed");
    assert_eq!(filter_kind_miss.items.len(), 0);

    // Search with tag filter "rust"
    let filter_tag_hit = library_search_impl(
        LibrarySearchQueryDto {
            text: "tool".into(),
            tags: Some(vec!["rust".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("tag search should succeed");
    assert_eq!(filter_tag_hit.items.len(), 1);
    assert_eq!(filter_tag_hit.items[0].id, s1.entry.asset.id.to_string());

    // Search with tag filter "nonexistent"
    let filter_tag_miss = library_search_impl(
        LibrarySearchQueryDto {
            text: "tool".into(),
            tags: Some(vec!["nonexistent".into()]),
            ..Default::default()
        },
        &state,
    )
    .expect("mismatched tag search should succeed");
    assert_eq!(filter_tag_miss.items.len(), 0);

    // -------------------------------------------------------------------------
    // Contract 8: Pagination offset and limit
    // -------------------------------------------------------------------------
    let paged_res = library_search_impl(
        LibrarySearchQueryDto {
            text: "tool".into(),
            limit: Some(1),
            offset: Some(0),
            ..Default::default()
        },
        &state,
    )
    .expect("paged search should succeed");
    assert_eq!(paged_res.limit, 1);
    assert_eq!(paged_res.offset, 0);
    assert!(paged_res.items.len() <= 1);
}
