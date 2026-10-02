//! The same wire examples are consumed by Rust DTOs and the TypeScript bridge.
use assetmesh_desktop_lib::dto::{AssetDetailDto, AssetDetailsDto};

#[test]
fn frontend_wire_examples_round_trip_through_typed_backend_details() {
    let values: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../src/test/desktop-wire-fixtures.json")).unwrap();
    for value in values {
        let mut detail: AssetDetailDto = serde_json::from_value(value.clone()).unwrap();
        match &mut detail.details {
            AssetDetailsDto::Media(record) => record.validate().unwrap(),
            AssetDetailsDto::Software(record) => record.validate().unwrap(),
            AssetDetailsDto::Service(record) => record.validate().unwrap(),
            AssetDetailsDto::Info(record) => record.validate().unwrap(),
            AssetDetailsDto::MergedRedirect { surviving_asset_id } => {
                assert_eq!(detail.merged_into.as_ref(), Some(&*surviving_asset_id));
            }
        }
        assert_eq!(serde_json::to_value(detail).unwrap(), value);
    }
}
