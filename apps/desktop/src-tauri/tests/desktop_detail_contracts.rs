//! Contract tests for `library_get` unified detail command.
//!
//! Verifies:
//! - Media, Software, and Service typed details
//! - Archived asset details remain readable
//! - Merged tombstone asset produces clean redirect information
//! - Missing optional fields deserialize gracefully
//! - Non-existent asset returns typed NotFound failure

use std::sync::Arc;

use assetmesh_core::application::asset_service::AssetService;
use assetmesh_core::application::media_service::{CreateMedia, MediaService};
use assetmesh_core::application::service_service::{CreateService, ServiceService};
use assetmesh_core::application::software_service::{CreateSoftware, SoftwareService};
use assetmesh_core::domain::media::{MediaType, Progress};
use assetmesh_core::domain::service::ServiceType;
use assetmesh_core::domain::software::SoftwareCategory;
use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::library::library_get_impl;
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-detail-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

#[test]
fn library_get_contracts_cover_all_domains_archived_merged_and_failures() {
    let db_path = temp_db_path("detail-contracts");
    let state = DesktopState::new();
    state.initialize(&db_path).expect("failed to init state");

    let clock = Arc::new(SystemClock);
    let ids = Arc::new(UuidV7Generator);

    let factory = state.factory.read().unwrap().clone().unwrap();

    let mut media_svc = MediaService::new(factory.clone(), clock.clone(), ids.clone());
    let mut software_svc = SoftwareService::new(factory.clone(), clock.clone(), ids.clone());
    let mut service_svc = ServiceService::new(factory.clone(), clock.clone(), ids.clone());
    let mut asset_svc = AssetService::new(factory.clone(), clock.clone(), ids.clone());

    // 1. Create Media asset with rich fields
    let media = media_svc
        .create_media(CreateMedia {
            title: "Frieren: Beyond Journey's End".into(),
            media_type: MediaType::Anime,
            summary: Some("Elven mage on a nostalgic journey".into()),
            status: None,
            rating: Some(9.8),
            year: Some(2023),
            platform: Some("Crunchyroll".into()),
            progress: Progress::default(),
            notes: Some("Masterpiece fantasy work".into()),
            tags: vec!["fantasy".into(), "magic".into()],
            external_refs: Vec::new(),
            started_at: None,
            completed_at: None,
        })
        .unwrap();

    // 2. Create Software asset
    let software = software_svc
        .create_software(CreateSoftware {
            name: "Ripgrep".into(),
            category: SoftwareCategory::Tool,
            summary: Some("Line-oriented search tool".into()),
            install_source: None,
            version: Some("14.1.0".into()),
            install_location: None,
            executable_path: Some("/usr/local/bin/rg".into()),
            purpose: Some("Fast code search".into()),
            notes: Some("Essential CLI utility".into()),
            architecture: Some("arm64".into()),
            installed_at: None,
            tags: vec!["cli".into(), "search".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    // 3. Create Service asset
    let service = service_svc
        .create_service(CreateService {
            name: "Cloudflare".into(),
            service_type: ServiceType::Saas,
            summary: Some("DNS and CDN provider".into()),
            provider: Some("Cloudflare Inc.".into()),
            account_label: Some("Primary".into()),
            endpoint_url: None,
            dashboard_url: Some("https://dash.cloudflare.com".into()),
            domain_name: None,
            plan: Some("Pro".into()),
            cost_minor: Some(2000),
            currency: Some("USD".into()),
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: Some(true),
            notes: Some("Zero trust setup enabled".into()),
            tags: vec!["cdn".into(), "dns".into()],
            external_refs: Vec::new(),
        })
        .unwrap();

    // 4. Create Archived asset (Archive a media asset)
    let archived_media = media_svc
        .create_media(CreateMedia {
            title: "Old Archived Film".into(),
            media_type: MediaType::Movie,
            summary: Some("Archived item".into()),
            status: None,
            rating: None,
            year: Some(1995),
            platform: None,
            progress: Progress::default(),
            notes: None,
            tags: vec!["vintage".into()],
            external_refs: Vec::new(),
            started_at: None,
            completed_at: None,
        })
        .unwrap();
    asset_svc
        .archive_asset(archived_media.entry.asset.id)
        .unwrap();

    // 5. Create Merged asset (Merge duplicate service into Cloudflare)
    let duplicate_service = service_svc
        .create_service(CreateService {
            name: "Cloudflare DNS Old".into(),
            service_type: ServiceType::Saas,
            summary: Some("Duplicate entry".into()),
            provider: Some("Cloudflare Inc.".into()),
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
            tags: vec![],
            external_refs: Vec::new(),
        })
        .unwrap();
    asset_svc
        .merge_assets(duplicate_service.entry.asset.id, service.entry.asset.id)
        .unwrap();

    // --- TEST VERIFICATIONS ---

    // Case 1: Media Detail Verification
    let media_detail =
        library_get_impl(&media.entry.asset.id.to_string(), &state).expect("media get failed");
    assert_eq!(media_detail.id, media.entry.asset.id.to_string());
    assert_eq!(media_detail.name, "Frieren: Beyond Journey's End");
    assert_eq!(media_detail.kind, "media.anime");
    assert_eq!(media_detail.lifecycle, "active");
    assert_eq!(media_detail.tags, vec!["fantasy", "magic"]);
    assert_eq!(
        media_detail.details.get("module").and_then(|v| v.as_str()),
        Some("media")
    );
    assert_eq!(
        media_detail.details.get("rating").and_then(|v| v.as_f64()),
        Some(9.8)
    );

    // Case 2: Software Detail Verification
    let sw_detail =
        library_get_impl(&software.entry.asset.id.to_string(), &state).expect("sw get failed");
    assert_eq!(sw_detail.id, software.entry.asset.id.to_string());
    assert_eq!(sw_detail.name, "Ripgrep");
    assert_eq!(sw_detail.kind, "software.tool");
    assert_eq!(sw_detail.lifecycle, "active");
    assert_eq!(
        sw_detail.details.get("module").and_then(|v| v.as_str()),
        Some("software")
    );
    assert_eq!(
        sw_detail.details.get("version").and_then(|v| v.as_str()),
        Some("14.1.0")
    );
    assert_eq!(
        sw_detail
            .details
            .get("architecture")
            .and_then(|v| v.as_str()),
        Some("arm64")
    );

    // Case 3: Service Detail Verification
    let svc_detail =
        library_get_impl(&service.entry.asset.id.to_string(), &state).expect("svc get failed");
    assert_eq!(svc_detail.id, service.entry.asset.id.to_string());
    assert_eq!(svc_detail.name, "Cloudflare");
    assert_eq!(svc_detail.kind, "service.saas");
    assert_eq!(
        svc_detail.details.get("module").and_then(|v| v.as_str()),
        Some("services")
    );
    assert_eq!(
        svc_detail
            .details
            .get("cost_minor")
            .and_then(|v| v.as_i64()),
        Some(2000)
    );

    // Case 4: Archived Asset Detail Verification
    let arch_detail = library_get_impl(&archived_media.entry.asset.id.to_string(), &state)
        .expect("archived get failed");
    assert_eq!(arch_detail.lifecycle, "archived");
    assert!(arch_detail.archived_at.is_some());
    assert_eq!(
        arch_detail.details.get("module").and_then(|v| v.as_str()),
        Some("media")
    );

    // Case 5: Merged Tombstone Verification (Produces Clean Redirect)
    let merged_detail = library_get_impl(&duplicate_service.entry.asset.id.to_string(), &state)
        .expect("merged get failed");
    assert_eq!(merged_detail.lifecycle, "merged");
    assert_eq!(
        merged_detail.merged_into,
        Some(service.entry.asset.id.to_string())
    );
    assert_eq!(
        merged_detail.details.get("module").and_then(|v| v.as_str()),
        Some("merged_redirect")
    );
    assert_eq!(
        merged_detail
            .details
            .get("surviving_asset_id")
            .and_then(|v| v.as_str()),
        Some(service.entry.asset.id.to_string().as_str())
    );

    // Case 6: Unknown Asset Returns Typed NotFound Failure
    let unknown_id = uuid::Uuid::now_v7().to_string();
    let err = library_get_impl(&unknown_id, &state).unwrap_err();
    assert_eq!(err.category, "not_found");

    // Case 7: Invalid Asset UUID Returns InvalidInput Failure
    let bad_err = library_get_impl("not-a-valid-uuid", &state).unwrap_err();
    assert_eq!(bad_err.category, "invalid_input");

    let _ = std::fs::remove_file(&db_path);
}
