//! Contract tests for Desktop Services workflow commands (P5-06).

use std::sync::Arc;

use assetmesh_core::ports::{SystemClock, UuidV7Generator};
use assetmesh_desktop_lib::commands::{library_get_impl, service_command_impl};
use assetmesh_desktop_lib::dto::ServiceCommandDto;
use assetmesh_desktop_lib::state::DesktopState;

fn temp_db_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetmesh-service-{}-{}.db",
        name,
        uuid::Uuid::now_v7()
    ))
}

fn setup_test_state(name: &str) -> DesktopState {
    let db_path = temp_db_path(name);
    let state = DesktopState::with_clock_and_ids(Arc::new(SystemClock), Arc::new(UuidV7Generator));
    state.initialize(&db_path).expect("initialize");
    state
}

#[test]
fn service_workflow_create_and_read_back() {
    let state = setup_test_state("create");

    let create_cmd = ServiceCommandDto::Create {
        name: "GitHub Copilot".into(),
        service_type: "saas".into(),
        summary: Some("AI pair programming tool".into()),
        provider: Some("GitHub".into()),
        account_label: Some("org-dev".into()),
        endpoint_url: None,
        dashboard_url: Some("https://github.com/settings/copilot".into()),
        domain_name: None,
        plan: Some("Business".into()),
        cost: Some("19.00".into()),
        currency: Some("USD".into()),
        billing_cadence: Some("monthly".into()),
        renews_at: Some("2024-04-01".into()),
        expires_at: None,
        auto_renew: Some(true),
        notes: Some("Auto-renewing corporate subscription".into()),
        tags: vec!["dev".into(), "ai".into()],
    };

    let receipt = service_command_impl(create_cmd, &state).expect("create service");
    assert_eq!(receipt.operation, "service.create");
    assert!(receipt.changed);
    assert_eq!(receipt.revision, Some(1));
    assert_eq!(receipt.asset_ids.len(), 1);

    let asset_id = &receipt.asset_ids[0];

    // Read back canonical
    let detail = library_get_impl(asset_id, &state).expect("library get");
    assert_eq!(detail.name, "GitHub Copilot");
    assert_eq!(detail.kind, "service.saas");
    assert_eq!(detail.revision, 1);
    assert_eq!(detail.details["module"], "services");
    assert_eq!(detail.details["service_type"], "saas");
    assert_eq!(detail.details["provider"], "GitHub");
    assert_eq!(detail.details["plan"], "Business");
    assert_eq!(detail.details["cost_minor"], 1900); // 19.00 -> 1900 minor units!
    assert_eq!(detail.details["currency"], "USD");
    assert_eq!(detail.details["billing_cadence"], "monthly");
    assert_eq!(detail.details["auto_renew"], true);
}

#[test]
fn service_workflow_update_metadata_and_conflict() {
    let state = setup_test_state("update");

    let create_cmd = ServiceCommandDto::Create {
        name: "Proton Mail".into(),
        service_type: "saas".into(),
        summary: None,
        provider: Some("Proton AG".into()),
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: Some("Mail Plus".into()),
        cost: Some("4.99".into()),
        currency: Some("EUR".into()),
        billing_cadence: Some("monthly".into()),
        renews_at: None,
        expires_at: None,
        auto_renew: Some(true),
        notes: None,
        tags: vec![],
    };
    let receipt = service_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // 1. Valid update (upgrade plan and cost)
    let update_cmd = ServiceCommandDto::Update {
        asset_id: asset_id.clone(),
        expected_revision: Some(1),
        name: Some("Proton Unlimited".into()),
        summary: Some("Encrypted email and VPN".into()),
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: Some("Unlimited".into()),
        cost: Some("9.99".into()),
        currency: Some("EUR".into()),
        billing_cadence: Some("monthly".into()),
        renews_at: None,
        expires_at: None,
        auto_renew: Some(true),
        notes: Some("Upgraded plan".into()),
    };
    let update_receipt = service_command_impl(update_cmd, &state).expect("update");
    assert_eq!(update_receipt.operation, "service.update");
    assert!(update_receipt.changed);
    assert_eq!(update_receipt.revision, Some(2));

    // Read back
    let detail = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(detail.name, "Proton Unlimited");
    assert_eq!(detail.details["plan"], "Unlimited");
    assert_eq!(detail.details["cost_minor"], 999);
    assert_eq!(detail.details["notes"], "Upgraded plan");

    // 2. Stale revision conflict
    let stale_cmd = ServiceCommandDto::Update {
        asset_id: asset_id.clone(),
        expected_revision: Some(1), // Actual is 2
        name: Some("Conflict Name".into()),
        summary: None,
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
    };
    let err = service_command_impl(stale_cmd, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // 3. No-op update
    let noop_cmd = ServiceCommandDto::Update {
        asset_id: asset_id.clone(),
        expected_revision: Some(2),
        name: None,
        summary: None,
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
    };
    let mut stale_noop = noop_cmd.clone();
    if let ServiceCommandDto::Update {
        expected_revision, ..
    } = &mut stale_noop
    {
        *expected_revision = Some(1);
    }
    assert_eq!(
        service_command_impl(stale_noop, &state)
            .unwrap_err()
            .category,
        "stale_revision"
    );
    let mut missing_noop = noop_cmd.clone();
    if let ServiceCommandDto::Update { asset_id, .. } = &mut missing_noop {
        *asset_id = uuid::Uuid::now_v7().to_string();
    }
    assert_eq!(
        service_command_impl(missing_noop, &state)
            .unwrap_err()
            .category,
        "not_found"
    );
    let noop_receipt = service_command_impl(noop_cmd, &state).expect("noop");
    assert!(!noop_receipt.changed);
    assert_eq!(noop_receipt.revision, Some(2));
}

#[test]
fn service_workflow_money_parsing_and_pairing_validation() {
    let state = setup_test_state("money_validation");

    // 1. Cost without currency fails
    let err_no_curr = service_command_impl(
        ServiceCommandDto::Create {
            name: "Cloud Service".into(),
            service_type: "saas".into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url: None,
            dashboard_url: None,
            domain_name: None,
            plan: None,
            cost: Some("15.00".into()),
            currency: None, // Missing!
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            tags: vec![],
        },
        &state,
    )
    .unwrap_err();
    assert_eq!(err_no_curr.category, "invalid_input");

    // 2. Negative cost fails
    let err_neg = service_command_impl(
        ServiceCommandDto::Create {
            name: "Cloud Service".into(),
            service_type: "saas".into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url: None,
            dashboard_url: None,
            domain_name: None,
            plan: None,
            cost: Some("-5.00".into()),
            currency: Some("USD".into()),
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            tags: vec![],
        },
        &state,
    )
    .unwrap_err();
    assert_eq!(err_neg.category, "invalid_input");

    // 3. More than 2 decimal places fails
    let err_subcent = service_command_impl(
        ServiceCommandDto::Create {
            name: "Cloud Service".into(),
            service_type: "saas".into(),
            summary: None,
            provider: None,
            account_label: None,
            endpoint_url: None,
            dashboard_url: None,
            domain_name: None,
            plan: None,
            cost: Some("10.999".into()),
            currency: Some("USD".into()),
            billing_cadence: None,
            renews_at: None,
            expires_at: None,
            auto_renew: None,
            notes: None,
            tags: vec![],
        },
        &state,
    )
    .unwrap_err();
    assert_eq!(err_subcent.category, "invalid_input");
}

#[test]
fn service_workflow_record_renewal() {
    let state = setup_test_state("renewal");

    let create_cmd = ServiceCommandDto::Create {
        name: "Domain Registration".into(),
        service_type: "domain".into(),
        summary: None,
        provider: Some("Cloudflare".into()),
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: Some("example.com".into()),
        plan: None,
        cost: Some("9.77".into()),
        currency: Some("USD".into()),
        billing_cadence: Some("yearly".into()),
        renews_at: Some("2024-05-01".into()),
        expires_at: Some("2024-05-01".into()),
        auto_renew: Some(true),
        notes: None,
        tags: vec![],
    };
    let receipt = service_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Record explicit renewal
    let renew_cmd = ServiceCommandDto::RecordRenewal {
        asset_id: asset_id.clone(),
        renews_at: "2024-05-01".into(),
        cost: Some("9.77".into()),
        currency: Some("USD".into()),
        next_renews_at: Some("2025-05-01".into()),
        next_expires_at: Some("2025-05-01".into()),
        expected_revision: receipt.revision,
    };
    let r_renew = service_command_impl(renew_cmd, &state).expect("record renewal");
    assert_eq!(r_renew.operation, "service.record_renewal");
    assert!(r_renew.changed);
    assert!(r_renew.revision.is_some());

    // Read back and check renews_at was updated to 2025-05-01
    let detail = library_get_impl(&asset_id, &state).expect("get");
    assert!(
        detail.details["renews_at"]
            .as_str()
            .unwrap()
            .starts_with("2025-05-01"),
        "renews_at must be updated to explicit next renewal date"
    );
}

#[test]
fn service_workflow_archive() {
    let state = setup_test_state("archive");

    let create_cmd = ServiceCommandDto::Create {
        name: "Deprecated VPS".into(),
        service_type: "vps".into(),
        summary: None,
        provider: Some("Hetzner".into()),
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
        tags: vec![],
    };
    let receipt = service_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // Archive
    let archive_cmd = ServiceCommandDto::Archive {
        asset_id: asset_id.clone(),
        expected_revision: receipt.revision,
    };
    let r_archive = service_command_impl(archive_cmd, &state).expect("archive");
    assert_eq!(r_archive.operation, "asset.archive");
    assert!(r_archive.revision.is_some());

    let detail = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(detail.lifecycle, "archived");
    assert!(detail.archived_at.is_some());
}

#[test]
fn service_stale_revision_is_rejected_with_stale_revision_category() {
    let state = setup_test_state("service_stale_rev");

    let create_cmd = ServiceCommandDto::Create {
        name: "Service Concurrency Test".into(),
        service_type: "saas".into(),
        summary: None,
        provider: None,
        account_label: None,
        endpoint_url: None,
        dashboard_url: None,
        domain_name: None,
        plan: None,
        cost: None,
        currency: None,
        billing_cadence: None,
        renews_at: None,
        expires_at: None,
        auto_renew: None,
        notes: None,
        tags: vec![],
    };
    let receipt = service_command_impl(create_cmd, &state).expect("create");
    let asset_id = receipt.asset_ids[0].clone();

    // RecordRenewal with wrong expected revision
    let bad_renew = ServiceCommandDto::RecordRenewal {
        asset_id: asset_id.clone(),
        renews_at: "2024-05-01".into(),
        cost: Some("10.00".into()),
        currency: Some("USD".into()),
        next_renews_at: None,
        next_expires_at: None,
        expected_revision: Some(999),
    };
    let err = service_command_impl(bad_renew, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // Archive with wrong expected revision
    let bad_archive = ServiceCommandDto::Archive {
        asset_id: asset_id.clone(),
        expected_revision: Some(999),
    };
    let err = service_command_impl(bad_archive, &state).unwrap_err();
    assert_eq!(err.category, "stale_revision");

    // Still active
    let d = library_get_impl(&asset_id, &state).expect("get");
    assert_eq!(d.lifecycle, "active");
}
