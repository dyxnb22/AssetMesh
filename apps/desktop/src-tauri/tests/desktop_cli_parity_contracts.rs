//! CLI vs Desktop Contract Parity Tests.
//!
//! Proves that the Desktop command handler (`library_list_impl`) and the CLI
//! query execution (`LibraryService::list_assets`) produce identical results
//! for identical queries across Media, Software, and Service domains.

use std::sync::Arc;

use assetmesh_core::application::library_service::{
    LibraryModule, LibraryQuery, LibraryService, LibrarySort, PageRequest,
};
use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::domain::media::{MediaType, Progress};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::repos::LifecycleFilter;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::library::library_list_impl;
use assetmesh_desktop_lib::dto::LibraryQueryDto;
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-parity-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

#[test]
fn cli_and_desktop_produce_identical_library_results_across_all_domains() {
    let db_path = temp_db_path("parity");
    let state = DesktopState::new();
    state.initialize(&db_path).expect("failed to init state");

    let clock = Arc::new(SystemClock);
    let ids = Arc::new(UuidV7Generator);

    let factory = state.factory.read().unwrap().clone().unwrap();

    // 1. Seed assets across Media, Software, and Services
    let mut media_svc = MediaService::new(factory.clone(), clock.clone(), ids.clone());
    let mut software_svc = SoftwareService::new(factory.clone(), clock.clone(), ids.clone());
    let mut service_svc = ServiceService::new(factory.clone(), clock.clone(), ids.clone());

    let _m1 = media_svc
        .create_media(CreateMedia {
            title: "Frieren: Beyond Journey's End".into(),
            media_type: MediaType::Anime,
            summary: Some("Elven mage journey".into()),
            status: None,
            rating: Some(9.5),
            year: Some(2023),
            platform: None,
            progress: Progress::default(),
            notes: None,
            tags: vec!["fantasy".into(), "healing".into()],
            external_refs: Vec::new(),
            started_at: None,
            completed_at: None,
        })
        .unwrap();

    let m2 = media_svc
        .create_media(CreateMedia {
            title: "Dune: Part Two".into(),
            media_type: MediaType::Movie,
            summary: Some("Paul Atreides unites with Chani".into()),
            status: None,
            rating: Some(9.0),
            year: Some(2024),
            platform: None,
            progress: Progress::default(),
            notes: None,
            tags: vec!["sci-fi".into(), "epic".into()],
            external_refs: Vec::new(),
            started_at: None,
            completed_at: None,
        })
        .unwrap();

    let mut asset_svc = assetmesh_core::application::asset_service::AssetService::new(
        factory.clone(),
        clock.clone(),
        ids.clone(),
    );
    asset_svc.archive_asset(m2.entry.asset.id).unwrap();

    let _s1 = software_svc
        .create_software(CreateSoftware {
            name: "Neovim".into(),
            category: SoftwareCategory::Tool,
            summary: Some("Vim-fork focused on extensibility".into()),
            install_source: None,
            version: Some("0.10.1".into()),
            install_location: None,
            executable_path: Some("/usr/local/bin/nvim".into()),
            purpose: Some("Modal text editor".into()),
            notes: None,
            architecture: None,
            installed_at: None,
            tags: vec!["editor".into(), "cli".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    let _s2 = software_svc
        .create_software(CreateSoftware {
            name: "Docker Desktop".into(),
            category: SoftwareCategory::Application,
            summary: Some("Container development tool".into()),
            install_source: None,
            version: Some("4.34.0".into()),
            install_location: Some("/Applications/Docker.app".into()),
            executable_path: None,
            purpose: Some("Container runtime".into()),
            notes: None,
            architecture: None,
            installed_at: None,
            tags: vec!["containers".into(), "virtualization".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    let _sv1 = service_svc
        .create_service(CreateService {
            name: "GitHub Copilot".into(),
            service_type: ServiceType::Saas,
            summary: Some("AI code completion tool".into()),
            provider: Some("GitHub".into()),
            account_label: None,
            endpoint_url: None,
            dashboard_url: Some("https://github.com/features/copilot".into()),
            domain_name: None,
            plan: Some("Individual".into()),
            cost_minor: Some(1000),
            currency: Some("USD".into()),
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            tags: vec!["ai".into(), "developer".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    let _sv2 = service_svc
        .create_service(CreateService {
            name: "Hetzner Cloud VPS".into(),
            service_type: ServiceType::Vps,
            summary: Some("Offsite build runner".into()),
            provider: Some("Hetzner".into()),
            account_label: None,
            endpoint_url: None,
            dashboard_url: Some("https://hetzner.com".into()),
            domain_name: None,
            plan: Some("CX21".into()),
            cost_minor: Some(500),
            currency: Some("EUR".into()),
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            tags: vec!["hosting".into(), "infrastructure".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    // Verification helper that compares CLI and Desktop for a given query
    let verify_parity = |query: LibraryQuery, dto: LibraryQueryDto, case_name: &str| {
        let mut cli_svc = LibraryService::new(factory.clone());
        let cli_page = cli_svc
            .list_assets(&query)
            .unwrap_or_else(|e| panic!("CLI query failed for {case_name}: {e}"));

        let desktop_page = library_list_impl(Some(dto), &state)
            .unwrap_or_else(|e| panic!("Desktop command failed for {case_name}: {e}"));

        assert_eq!(
            cli_page.total, desktop_page.total,
            "Total mismatch in case {case_name}"
        );
        assert_eq!(
            cli_page.offset, desktop_page.offset,
            "Offset mismatch in case {case_name}"
        );
        assert_eq!(
            cli_page.limit, desktop_page.limit,
            "Limit mismatch in case {case_name}"
        );
        assert_eq!(
            cli_page.items.len(),
            desktop_page.items.len(),
            "Item count mismatch in case {case_name}"
        );

        for (i, (cli_item, desktop_item)) in cli_page
            .items
            .iter()
            .zip(desktop_page.items.iter())
            .enumerate()
        {
            assert_eq!(
                cli_item.id.to_string(),
                desktop_item.id,
                "ID mismatch at index {i} in case {case_name}"
            );
            assert_eq!(
                cli_item.name, desktop_item.name,
                "Name mismatch at index {i} in case {case_name}"
            );
            assert_eq!(
                cli_item.kind.as_str(),
                desktop_item.kind,
                "Kind mismatch at index {i} in case {case_name}"
            );
            assert_eq!(
                cli_item.lifecycle.as_str(),
                desktop_item.lifecycle,
                "Lifecycle mismatch at index {i} in case {case_name}"
            );
            assert_eq!(
                cli_item.subtitle, desktop_item.subtitle,
                "Subtitle mismatch at index {i} in case {case_name}"
            );
            assert_eq!(
                cli_item.tags, desktop_item.tags,
                "Tags mismatch at index {i} in case {case_name}"
            );
            assert_eq!(
                cli_item.updated_at.to_rfc3339(),
                desktop_item.updated_at,
                "Timestamp mismatch at index {i} in case {case_name}"
            );
        }
    };

    // Case 1: Default All Assets query (active only, updated desc)
    verify_parity(
        LibraryQuery {
            lifecycle: LifecycleFilter::Active,
            modules: vec![],
            kinds: vec![],
            tags: vec![],
            sort: LibrarySort::UpdatedDesc,
            page: PageRequest::new(50, 0),
        },
        LibraryQueryDto {
            lifecycle: Some("active".into()),
            modules: None,
            kinds: None,
            tags: None,
            sort: Some("updated_desc".into()),
            limit: Some(50),
            offset: Some(0),
        },
        "default_all_active",
    );

    // Case 2: Lifecycle filter including archived assets
    verify_parity(
        LibraryQuery {
            lifecycle: LifecycleFilter::ActiveOrArchived,
            modules: vec![],
            kinds: vec![],
            tags: vec![],
            sort: LibrarySort::UpdatedDesc,
            page: PageRequest::new(50, 0),
        },
        LibraryQueryDto {
            lifecycle: Some("active_or_archived".into()),
            modules: None,
            kinds: None,
            tags: None,
            sort: Some("updated_desc".into()),
            limit: Some(50),
            offset: Some(0),
        },
        "lifecycle_active_or_archived",
    );

    // Case 3: Module filter - Software only
    verify_parity(
        LibraryQuery {
            lifecycle: LifecycleFilter::Active,
            modules: vec![LibraryModule::Software],
            kinds: vec![],
            tags: vec![],
            sort: LibrarySort::NameAsc,
            page: PageRequest::new(50, 0),
        },
        LibraryQueryDto {
            lifecycle: Some("active".into()),
            modules: Some(vec!["software".into()]),
            kinds: None,
            tags: None,
            sort: Some("name_asc".into()),
            limit: Some(50),
            offset: Some(0),
        },
        "module_software_name_asc",
    );

    // Case 4: Module filter - Services only
    verify_parity(
        LibraryQuery {
            lifecycle: LifecycleFilter::Active,
            modules: vec![LibraryModule::Services],
            kinds: vec![],
            tags: vec![],
            sort: LibrarySort::KindAsc,
            page: PageRequest::new(50, 0),
        },
        LibraryQueryDto {
            lifecycle: Some("active".into()),
            modules: Some(vec!["services".into()]),
            kinds: None,
            tags: None,
            sort: Some("kind_asc".into()),
            limit: Some(50),
            offset: Some(0),
        },
        "module_services_kind_asc",
    );

    // Case 5: Pagination - small page sizes and offsets
    for offset in [0, 2, 4] {
        verify_parity(
            LibraryQuery {
                lifecycle: LifecycleFilter::Active,
                modules: vec![],
                kinds: vec![],
                tags: vec![],
                sort: LibrarySort::NameAsc,
                page: PageRequest::new(2, offset),
            },
            LibraryQueryDto {
                lifecycle: Some("active".into()),
                modules: None,
                kinds: None,
                tags: None,
                sort: Some("name_asc".into()),
                limit: Some(2),
                offset: Some(offset),
            },
            &format!("pagination_offset_{offset}"),
        );
    }

    // Case 6: Tag filtering
    verify_parity(
        LibraryQuery {
            lifecycle: LifecycleFilter::Active,
            modules: vec![],
            kinds: vec![],
            tags: vec!["editor".into()],
            sort: LibrarySort::UpdatedDesc,
            page: PageRequest::new(50, 0),
        },
        LibraryQueryDto {
            lifecycle: Some("active".into()),
            modules: None,
            kinds: None,
            tags: Some(vec!["editor".into()]),
            sort: Some("updated_desc".into()),
            limit: Some(50),
            offset: Some(0),
        },
        "tag_filter_editor",
    );

    // Case 7: Kind filter - Tool only
    verify_parity(
        LibraryQuery {
            lifecycle: LifecycleFilter::Active,
            modules: vec![],
            kinds: vec![AssetKind::SoftwareTool],
            tags: vec![],
            sort: LibrarySort::UpdatedDesc,
            page: PageRequest::new(50, 0),
        },
        LibraryQueryDto {
            lifecycle: Some("active".into()),
            modules: None,
            kinds: Some(vec!["software.tool".into()]),
            tags: None,
            sort: Some("updated_desc".into()),
            limit: Some(50),
            offset: Some(0),
        },
        "kind_filter_software_tool",
    );

    let _ = std::fs::remove_file(&db_path);
}
