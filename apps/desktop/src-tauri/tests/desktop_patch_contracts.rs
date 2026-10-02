//! Exercise optional updates through JSON commands and a real SQLite library.

use assetmesh_desktop_lib::commands::{
    library_get_impl, media_command_impl, service_command_impl, software_command_impl,
};
use assetmesh_desktop_lib::state::DesktopState;
use serde_json::{json, Value};

#[test]
fn wire_updates_preserve_omissions_and_clear_explicit_nulls() {
    let directory =
        std::env::temp_dir().join(format!("assetmesh-patches-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&directory).unwrap();
    let state = DesktopState::new();
    state.initialize(&directory.join("library.db")).unwrap();

    let media = media_command_impl(
        serde_json::from_value(json!({
            "action": "create", "title": "Patch movie", "media_type": "movie",
            "summary": "Old summary", "year": 2001, "platform": "Blu-ray", "notes": "Old notes"
        }))
        .unwrap(),
        &state,
    )
    .unwrap();
    let id = &media.asset_ids[0];
    let preserve = serde_json::from_value(json!({
        "action": "update_metadata", "asset_id": id, "expected_revision": 1, "title": "Renamed movie"
    })).unwrap();
    let round_trip = serde_json::to_value(&preserve).unwrap();
    assert!(round_trip.get("year").is_none());
    media_command_impl(preserve, &state).unwrap();
    let detail = serde_json::to_value(library_get_impl(id, &state).unwrap()).unwrap();
    assert_eq!(detail["details"]["year"], 2001);
    assert_eq!(detail["summary"], "Old summary");
    let error = media_command_impl(
        serde_json::from_value(json!({
            "action": "update_metadata", "asset_id": id, "expected_revision": 2,
            "title": "", "year": null
        }))
        .unwrap(),
        &state,
    )
    .unwrap_err();
    assert_eq!(error.category, "invalid_input");
    assert_eq!(library_get_impl(id, &state).unwrap().revision, 2);
    let clear = serde_json::from_value(json!({
        "action": "update_metadata", "asset_id": id, "expected_revision": 2,
        "summary": null, "year": null, "platform": null, "notes": null
    }))
    .unwrap();
    assert_eq!(serde_json::to_value(&clear).unwrap()["year"], Value::Null);
    media_command_impl(clear, &state).unwrap();
    let detail = serde_json::to_value(library_get_impl(id, &state).unwrap()).unwrap();
    for field in ["year", "platform", "notes"] {
        assert!(detail["details"][field].is_null());
    }
    assert!(detail["summary"].is_null());
    assert_eq!(detail["name"], "Renamed movie");
    assert_eq!(detail["details"]["status"], "planned");

    let software = software_command_impl(serde_json::from_value(json!({
        "action": "create", "name": "Patch tool", "category": "tool",
        "summary": "Old summary", "version": "1.0", "purpose": "Old purpose", "notes": "Old notes"
    })).unwrap(), &state).unwrap();
    let id = &software.asset_ids[0];
    software_command_impl(
        serde_json::from_value(json!({
            "action": "update_metadata", "asset_id": id, "expected_revision": 1,
            "summary": null, "purpose": null, "notes": "   "
        }))
        .unwrap(),
        &state,
    )
    .unwrap();
    let detail = serde_json::to_value(library_get_impl(id, &state).unwrap()).unwrap();
    assert!(detail["summary"].is_null());
    assert!(detail["details"]["purpose"].is_null());
    assert!(detail["details"]["notes"].is_null());
    assert_eq!(detail["details"]["version"], "1.0");

    let service = service_command_impl(
        serde_json::from_value(json!({
            "action": "create", "name": "Patch subscription", "service_type": "saas",
            "provider": "Provider", "plan": "Pro", "cost": "12.00", "currency": "USD",
            "billing_cadence": "monthly", "renews_at": "2026-10-03", "auto_renew": true,
            "notes": "Keep this note"
        }))
        .unwrap(),
        &state,
    )
    .unwrap();
    let id = &service.asset_ids[0];
    let error = service_command_impl(
        serde_json::from_value(json!({
            "action": "update", "asset_id": id, "expected_revision": 1, "cost": null
        }))
        .unwrap(),
        &state,
    )
    .unwrap_err();
    assert_eq!(error.category, "invalid_input");
    assert_eq!(library_get_impl(id, &state).unwrap().revision, 1);
    service_command_impl(
        serde_json::from_value(json!({
            "action": "update", "asset_id": id, "expected_revision": 1,
            "provider": null, "plan": null, "cost": null, "currency": null,
            "billing_cadence": null, "renews_at": null, "auto_renew": null
        }))
        .unwrap(),
        &state,
    )
    .unwrap();
    let detail = serde_json::to_value(library_get_impl(id, &state).unwrap()).unwrap();
    for field in [
        "provider",
        "plan",
        "cost_minor",
        "currency",
        "billing_cadence",
        "renews_at",
        "auto_renew",
    ] {
        assert!(detail["details"][field].is_null(), "{field}");
    }
    assert_eq!(detail["details"]["notes"], "Keep this note");
    drop(state);
    std::fs::remove_dir_all(directory).unwrap();
}
