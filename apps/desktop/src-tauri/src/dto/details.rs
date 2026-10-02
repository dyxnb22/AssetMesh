//! Typed detail payloads; serialization preserves the existing module tags.

use assetmesh_core::application::library_service::AssetDetails;
use assetmesh_core::domain::{
    info::InfoRecord, media::MediaRecord, service::ServiceRecord, software::SoftwareRecord,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "module", rename_all = "snake_case")]
pub enum AssetDetailsDto {
    Media(MediaRecord),
    Software(SoftwareRecord),
    #[serde(rename = "services")]
    Service(ServiceRecord),
    Info(InfoRecord),
    MergedRedirect {
        surviving_asset_id: String,
    },
}

impl From<AssetDetails> for AssetDetailsDto {
    fn from(details: AssetDetails) -> Self {
        match details {
            AssetDetails::Media(record) => Self::Media(record),
            AssetDetails::Software(record) => Self::Software(record),
            AssetDetails::Service(record) => Self::Service(record),
            AssetDetails::Info(record) => Self::Info(record),
        }
    }
}
