//! Deep redirect graphs exercise preflight through the public import contract.
mod support;

use assetmesh_core::application::portable::{PortableAssetV1, PortableBundle};
use assetmesh_core::domain::asset::{Asset, AssetKind, LifecycleState};
use assetmesh_core::domain::ids::AssetId;
use support::test_env;

fn redirect_chain(count: usize, cycle: bool) -> PortableBundle {
    let env = test_env();
    let mut bundle = env.export_service().export("test").unwrap();
    let ids: Vec<_> = (0..count)
        .map(|_| AssetId::from_uuid(uuid::Uuid::now_v7()))
        .collect();
    let mut content = String::new();
    for (index, id) in ids.iter().enumerate() {
        let mut asset = Asset::new(
            *id,
            AssetKind::MediaMovie,
            format!("Identity {index}"),
            None,
            chrono::Utc::now(),
        )
        .unwrap();
        asset.merged_into = ids
            .get(index + 1)
            .copied()
            .or_else(|| cycle.then_some(ids[count / 2]));
        if asset.merged_into.is_some() {
            asset.lifecycle_state = LifecycleState::Merged;
        }
        content.push_str(&serde_json::to_string(&PortableAssetV1::from_domain(&asset)).unwrap());
        content.push('\n');
    }
    bundle
        .files
        .iter_mut()
        .find(|file| file.path == "assets.jsonl")
        .unwrap()
        .content = content;
    bundle.manifest.record_counts.insert("assets".into(), count);
    bundle
}

#[test]
fn dry_run_accepts_a_deep_redirect_chain_without_mutating_the_destination() {
    let env = test_env();
    let bundle = redirect_chain(4_000, false);
    let report = env
        .portable_import_service()
        .import_bundle(&bundle, true)
        .unwrap();
    assert_eq!(report.assets_created, 4_000);
    assert_eq!(
        env.export_service()
            .export("test")
            .unwrap()
            .manifest
            .record_counts["assets"],
        0
    );
}

#[test]
fn a_cycle_at_the_end_of_a_deep_redirect_graph_is_rejected_before_writes() {
    let env = test_env();
    let bundle = redirect_chain(4_000, true);
    for dry_run in [true, false] {
        let error = env
            .portable_import_service()
            .import_bundle(&bundle, dry_run)
            .unwrap_err();
        assert!(
            error.to_string().contains("merge redirect cycle"),
            "{error}"
        );
    }
    assert_eq!(
        env.export_service()
            .export("test")
            .unwrap()
            .manifest
            .record_counts["assets"],
        0
    );
}
