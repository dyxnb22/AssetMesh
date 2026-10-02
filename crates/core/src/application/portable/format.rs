//! Versioned portable schema, codecs, and row classification.
use crate::domain::activity::ActivityEvent;
use crate::domain::asset::{Asset, AssetKind, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::{ActivityId, AssetId, ExternalRefId, RelationId, TagId};
use crate::domain::info::{InfoRecord, InfoType};
use crate::domain::media::{MediaRecord, MediaType};
use crate::domain::relation::{Relation, RelationProvenance, RelationType};
use crate::domain::service::{BillingCadence, ServiceRecord, ServiceType};
use crate::domain::software::{InstallSource, SoftwareCategory, SoftwareRecord};
use crate::domain::tag::Tag;
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const EXPORT_FORMAT: &str = "assetmesh-portable-export";
pub const EXPORT_VERSION: i64 = 1;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableInfoRecordV1 {
    pub asset_id: String,
    pub info_type: String,
    pub value: String,
    pub notes: Option<String>,
}

impl PortableInfoRecordV1 {
    pub fn from_domain(record: &InfoRecord) -> Self {
        Self {
            asset_id: record.asset_id.to_string(),
            info_type: record.info_type.as_str().into(),
            value: record.value.clone(),
            notes: record.notes.clone(),
        }
    }
    pub(super) fn into_domain(self) -> AppResult<InfoRecord> {
        Ok(InfoRecord {
            asset_id: AssetId::from_uuid(id_from_wire(&self.asset_id, "info.asset_id")?),
            info_type: InfoType::parse(&self.info_type)
                .ok_or_else(|| AppError::validation("unknown information type in bundle"))?,
            value: self.value,
            notes: self.notes,
        })
    }
}

/// Media module data schema version — owned by the Media module
/// (`domain::media::SCHEMA_VERSION`) and re-exported here for convenience.
pub use crate::domain::media::SCHEMA_VERSION as MEDIA_SCHEMA_VERSION;

/// Software module data schema version — owned by the Software module
/// (`domain::software::SCHEMA_VERSION`).
pub use crate::domain::software::SCHEMA_VERSION as SOFTWARE_SCHEMA_VERSION;

/// Services module data schema version — owned by the Services module
/// (`domain::service::SCHEMA_VERSION`). Declared in the manifest of every
/// current export, alongside `media` and `software`.
pub use crate::domain::service::SCHEMA_VERSION as SERVICES_SCHEMA_VERSION;

/// Services wire shape before the optional launch fields arrived (schema
/// version 2). Import still accepts it: a v1 bundle is a strict subset of
/// the v2 wire shape, so legacy exports stay importable.
pub(super) const LEGACY_SERVICES_SCHEMA_VERSION: i64 = 1;

// ---------------------------------------------------------------------------
// V1 wire DTOs — the frozen interchange representation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleVersion {
    pub schema_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableManifest {
    pub format: String,
    pub version: i64,
    pub created_at: String,
    pub app_version: String,
    pub modules: BTreeMap<String, ModuleVersion>,
    pub record_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableAssetV1 {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub summary: Option<String>,
    pub lifecycle_state: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub merged_into: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableMediaRecordV1 {
    pub asset_id: String,
    pub media_type: String,
    pub status: String,
    pub rating: Option<f64>,
    pub year: Option<i32>,
    pub platform: Option<String>,
    pub progress_current: Option<f64>,
    pub progress_total: Option<f64>,
    pub progress_unit: Option<String>,
    pub notes: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableSoftwareRecordV1 {
    pub asset_id: String,
    pub category: String,
    pub install_source: String,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub executable_path: Option<String>,
    pub purpose: Option<String>,
    pub notes: Option<String>,
    pub discovered_at: Option<String>,
    pub installed_at: Option<String>,
    pub architecture: Option<String>,
}

/// Services module wire DTO (docs/10 "Portable data").
///
/// The canonical vocabulary only — no field here can hold a credential
/// (ADR 0010), and none is added without breaking format version 1 for a
/// reader that knows the field set. Money stays integer minor units.
///
/// `project_dir`/`start_command` arrived with services module schema
/// version 2 and are defaulted on read, so a schema-version 1 bundle (a
/// legacy export) parses unchanged with both fields absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableServiceRecordV1 {
    pub asset_id: String,
    pub service_type: String,
    pub provider: Option<String>,
    pub account_label: Option<String>,
    pub endpoint_url: Option<String>,
    pub dashboard_url: Option<String>,
    pub domain_name: Option<String>,
    pub plan: Option<String>,
    pub cost_minor: Option<i64>,
    pub currency: Option<String>,
    pub billing_cadence: Option<String>,
    pub renews_at: Option<String>,
    pub expires_at: Option<String>,
    pub auto_renew: Option<bool>,
    pub notes: Option<String>,
    #[serde(default)]
    pub project_dir: Option<String>,
    #[serde(default)]
    pub start_command: Option<String>,
    #[serde(default)]
    pub stop_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableRelationV1 {
    pub id: String,
    pub source_asset_id: String,
    pub target_asset_id: String,
    pub relation_type: String,
    pub note: Option<String>,
    pub provenance: String,
    pub created_at: String,
}

/// Namespaced external reference on the wire.
///
/// There is no `metadata` field: `AssetExternalRef::metadata` is provider /
/// import cache, and ADR 0009 excludes provider cache from portable export.
/// Bundles that still carry the key parse and drop it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableExternalRefV1 {
    pub id: String,
    pub asset_id: String,
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableActivityEventV1 {
    pub id: String,
    pub occurred_at: String,
    pub event_type: String,
    pub asset_id: Option<String>,
    pub actor: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableTagV1 {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetTagRow {
    pub asset_id: String,
    pub tag_id: String,
}

pub(super) fn ts_to_wire(ts: Timestamp) -> String {
    ts.to_rfc3339()
}

fn ts_from_wire(value: &str, field: &str) -> AppResult<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|ts| ts.with_timezone(&chrono::Utc))
        .map_err(|e| {
            AppError::validation(format!(
                "bundle has invalid {field} timestamp {value:?}: {e}"
            ))
        })
}

fn id_to_wire(id: impl Into<uuid::Uuid>) -> String {
    let id: uuid::Uuid = id.into();
    id.to_string()
}

pub(super) fn id_from_wire(value: &str, field: &str) -> AppResult<uuid::Uuid> {
    uuid::Uuid::parse_str(value)
        .map_err(|e| AppError::validation(format!("bundle has invalid {field} id {value:?}: {e}")))
}

impl PortableAssetV1 {
    pub fn from_domain(asset: &Asset) -> Self {
        PortableAssetV1 {
            id: id_to_wire(asset.id.as_uuid()),
            kind: asset.kind.as_str().to_string(),
            name: asset.name.clone(),
            summary: asset.summary.clone(),
            lifecycle_state: asset.lifecycle_state.as_str().to_string(),
            revision: asset.revision,
            created_at: ts_to_wire(asset.created_at),
            updated_at: ts_to_wire(asset.updated_at),
            archived_at: asset.archived_at.map(ts_to_wire),
            merged_into: asset.merged_into.map(|id| id_to_wire(id.as_uuid())),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<Asset> {
        Ok(Asset {
            id: AssetId::from_uuid(id_from_wire(&self.id, "asset")?),
            kind: AssetKind::parse(&self.kind).ok_or_else(|| {
                AppError::validation(format!("bundle asset has unknown kind {:?}", self.kind))
            })?,
            name: self.name,
            summary: self.summary,
            lifecycle_state: LifecycleState::parse(&self.lifecycle_state).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle asset has unknown lifecycle_state {:?}",
                    self.lifecycle_state
                ))
            })?,
            revision: self.revision,
            created_at: ts_from_wire(&self.created_at, "asset.created_at")?,
            updated_at: ts_from_wire(&self.updated_at, "asset.updated_at")?,
            archived_at: match &self.archived_at {
                Some(t) => Some(ts_from_wire(t, "asset.archived_at")?),
                None => None,
            },
            merged_into: match &self.merged_into {
                Some(t) => Some(AssetId::from_uuid(id_from_wire(t, "asset.merged_into")?)),
                None => None,
            },
        })
    }
}

impl PortableMediaRecordV1 {
    pub fn from_domain(record: &MediaRecord) -> Self {
        PortableMediaRecordV1 {
            asset_id: id_to_wire(record.asset_id.as_uuid()),
            media_type: record.media_type.as_str().to_string(),
            status: record.status.as_str().to_string(),
            rating: record.rating,
            year: record.year,
            platform: record.platform.clone(),
            progress_current: record.progress.current,
            progress_total: record.progress.total,
            progress_unit: record.progress.unit.clone(),
            notes: record.notes.clone(),
            started_at: record.started_at.map(ts_to_wire),
            completed_at: record.completed_at.map(ts_to_wire),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<MediaRecord> {
        Ok(MediaRecord {
            asset_id: AssetId::from_uuid(id_from_wire(&self.asset_id, "media")?),
            media_type: MediaType::parse(&self.media_type).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle media has unknown type {:?}",
                    self.media_type
                ))
            })?,
            status: crate::domain::media::MediaStatus::parse(&self.status).ok_or_else(|| {
                AppError::validation(format!("bundle media has unknown status {:?}", self.status))
            })?,
            rating: self.rating,
            year: self.year,
            platform: self.platform,
            progress: crate::domain::media::Progress {
                current: self.progress_current,
                total: self.progress_total,
                unit: self.progress_unit,
            },
            notes: self.notes,
            started_at: match &self.started_at {
                Some(t) => Some(ts_from_wire(t, "media.started_at")?),
                None => None,
            },
            completed_at: match &self.completed_at {
                Some(t) => Some(ts_from_wire(t, "media.completed_at")?),
                None => None,
            },
        })
    }
}

impl PortableSoftwareRecordV1 {
    pub fn from_domain(record: &SoftwareRecord) -> Self {
        PortableSoftwareRecordV1 {
            asset_id: id_to_wire(record.asset_id.as_uuid()),
            category: record.category.as_str().to_string(),
            install_source: record.install_source.as_str().to_string(),
            version: record.version.clone(),
            install_location: record.install_location.clone(),
            executable_path: record.executable_path.clone(),
            purpose: record.purpose.clone(),
            notes: record.notes.clone(),
            discovered_at: record.discovered_at.map(ts_to_wire),
            installed_at: record.installed_at.map(ts_to_wire),
            architecture: record.architecture.clone(),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<SoftwareRecord> {
        Ok(SoftwareRecord {
            asset_id: AssetId::from_uuid(id_from_wire(&self.asset_id, "software")?),
            category: SoftwareCategory::parse(&self.category).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle software has unknown category {:?}",
                    self.category
                ))
            })?,
            install_source: InstallSource::parse(&self.install_source).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle software has unknown install_source {:?}",
                    self.install_source
                ))
            })?,
            version: self.version,
            install_location: self.install_location,
            executable_path: self.executable_path,
            purpose: self.purpose,
            notes: self.notes,
            discovered_at: match &self.discovered_at {
                Some(t) => Some(ts_from_wire(t, "software.discovered_at")?),
                None => None,
            },
            installed_at: match &self.installed_at {
                Some(t) => Some(ts_from_wire(t, "software.installed_at")?),
                None => None,
            },
            architecture: self.architecture,
        })
    }
}

impl PortableServiceRecordV1 {
    pub fn from_domain(record: &ServiceRecord) -> Self {
        PortableServiceRecordV1 {
            asset_id: id_to_wire(record.asset_id.as_uuid()),
            service_type: record.service_type.as_str().to_string(),
            provider: record.provider.clone(),
            account_label: record.account_label.clone(),
            endpoint_url: record.endpoint_url.clone(),
            dashboard_url: record.dashboard_url.clone(),
            domain_name: record.domain_name.clone(),
            plan: record.plan.clone(),
            cost_minor: record.cost_minor,
            currency: record.currency.clone(),
            billing_cadence: record.billing_cadence.map(|c| c.as_str().to_string()),
            renews_at: record.renews_at.map(ts_to_wire),
            expires_at: record.expires_at.map(ts_to_wire),
            auto_renew: record.auto_renew,
            notes: record.notes.clone(),
            project_dir: record.project_dir.clone(),
            start_command: record.start_command.clone(),
            stop_command: record.stop_command.clone(),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<ServiceRecord> {
        Ok(ServiceRecord {
            asset_id: AssetId::from_uuid(id_from_wire(&self.asset_id, "service")?),
            service_type: ServiceType::parse(&self.service_type).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle service has unknown type {:?}",
                    self.service_type
                ))
            })?,
            provider: self.provider,
            account_label: self.account_label,
            endpoint_url: self.endpoint_url,
            dashboard_url: self.dashboard_url,
            domain_name: self.domain_name,
            plan: self.plan,
            cost_minor: self.cost_minor,
            currency: self.currency,
            billing_cadence: self
                .billing_cadence
                .as_deref()
                .map(|raw| {
                    BillingCadence::parse(raw).ok_or_else(|| {
                        AppError::validation(format!(
                            "bundle service has unknown billing_cadence {raw:?}"
                        ))
                    })
                })
                .transpose()?,
            renews_at: match &self.renews_at {
                Some(t) => Some(ts_from_wire(t, "service.renews_at")?),
                None => None,
            },
            expires_at: match &self.expires_at {
                Some(t) => Some(ts_from_wire(t, "service.expires_at")?),
                None => None,
            },
            auto_renew: self.auto_renew,
            notes: self.notes,
            project_dir: self.project_dir,
            start_command: self.start_command,
            stop_command: self.stop_command,
        })
    }
}

impl PortableRelationV1 {
    pub fn from_domain(relation: &Relation) -> Self {
        PortableRelationV1 {
            id: id_to_wire(relation.id.as_uuid()),
            source_asset_id: id_to_wire(relation.source_asset_id.as_uuid()),
            target_asset_id: id_to_wire(relation.target_asset_id.as_uuid()),
            relation_type: relation.relation_type.as_str().to_string(),
            note: relation.note.clone(),
            provenance: relation.provenance.as_str().to_string(),
            created_at: ts_to_wire(relation.created_at),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<Relation> {
        Ok(Relation {
            id: RelationId::from_uuid(id_from_wire(&self.id, "relation")?),
            source_asset_id: AssetId::from_uuid(id_from_wire(
                &self.source_asset_id,
                "relation source",
            )?),
            target_asset_id: AssetId::from_uuid(id_from_wire(
                &self.target_asset_id,
                "relation target",
            )?),
            relation_type: RelationType::parse(&self.relation_type).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle relation has unknown type {:?}",
                    self.relation_type
                ))
            })?,
            note: self.note,
            provenance: RelationProvenance::parse(&self.provenance).ok_or_else(|| {
                AppError::validation(format!(
                    "bundle relation has unknown provenance {:?}",
                    self.provenance
                ))
            })?,
            created_at: ts_from_wire(&self.created_at, "relation.created_at")?,
        })
    }
}

impl PortableExternalRefV1 {
    pub fn from_domain(reference: &AssetExternalRef) -> Self {
        PortableExternalRefV1 {
            id: id_to_wire(reference.id.as_uuid()),
            asset_id: id_to_wire(reference.asset_id.as_uuid()),
            namespace: reference.namespace.clone(),
            external_id: reference.external_id.clone(),
            source_url: reference.source_url.clone(),
            created_at: ts_to_wire(reference.created_at),
            updated_at: ts_to_wire(reference.updated_at),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<AssetExternalRef> {
        Ok(AssetExternalRef {
            id: ExternalRefId::from_uuid(id_from_wire(&self.id, "external ref")?),
            asset_id: AssetId::from_uuid(id_from_wire(&self.asset_id, "external ref asset")?),
            namespace: self.namespace,
            external_id: self.external_id,
            source_url: self.source_url,
            metadata: None,
            created_at: ts_from_wire(&self.created_at, "ref.created_at")?,
            updated_at: ts_from_wire(&self.updated_at, "ref.updated_at")?,
        })
    }
}

impl PortableActivityEventV1 {
    pub fn from_domain(event: &ActivityEvent) -> Self {
        PortableActivityEventV1 {
            id: id_to_wire(event.id.as_uuid()),
            occurred_at: ts_to_wire(event.occurred_at),
            event_type: event.event_type.clone(),
            asset_id: event.asset_id.map(|id| id_to_wire(id.as_uuid())),
            actor: event.actor.clone(),
            payload: event.payload.clone(),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<ActivityEvent> {
        Ok(ActivityEvent {
            id: ActivityId::from_uuid(id_from_wire(&self.id, "activity")?),
            occurred_at: ts_from_wire(&self.occurred_at, "activity.occurred_at")?,
            event_type: self.event_type,
            asset_id: match &self.asset_id {
                Some(t) => Some(AssetId::from_uuid(id_from_wire(t, "activity asset")?)),
                None => None,
            },
            actor: self.actor,
            payload: self.payload,
        })
    }
}

impl PortableTagV1 {
    pub fn from_domain(tag: &Tag) -> Self {
        PortableTagV1 {
            id: id_to_wire(tag.id.as_uuid()),
            name: tag.name.clone(),
            created_at: ts_to_wire(tag.created_at),
        }
    }

    pub(super) fn into_domain(self) -> AppResult<Tag> {
        Ok(Tag {
            id: TagId::from_uuid(id_from_wire(&self.id, "tag")?),
            name: self.name,
            created_at: ts_from_wire(&self.created_at, "tag.created_at")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExportFile {
    /// Path inside the bundle, e.g. `assets.jsonl` or `modules/media.jsonl`.
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct PortableBundle {
    pub manifest: PortableManifest,
    pub files: Vec<ExportFile>,
}

impl PortableBundle {
    pub fn file(&self, path: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.as_str())
    }

    pub fn manifest_json(&self) -> AppResult<String> {
        serde_json::to_string_pretty(&self.manifest).map_err(json_error)
    }

    /// Computes a deterministic SHA-256 fingerprint over manifest and all bundle files.
    ///
    /// Files are sorted by path before hashing so order cannot affect the hash.
    pub fn fingerprint(&self) -> AppResult<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();

        let manifest_text = self.manifest_json()?;
        hasher.update(b"manifest.json\0");
        hasher.update(manifest_text.as_bytes());
        hasher.update(b"\0");

        let mut sorted_files: Vec<&ExportFile> = self.files.iter().collect();
        sorted_files.sort_by(|a, b| a.path.cmp(&b.path));

        for file in sorted_files {
            hasher.update(file.path.as_bytes());
            hasher.update(b"\0");
            hasher.update(file.content.as_bytes());
            hasher.update(b"\0");
        }

        let hash = hasher.finalize();
        Ok(format!("{hash:x}"))
    }
}

/// Required core file paths of a V1 bundle (always present).
pub const V1_CORE_FILE_PATHS: [&str; 5] = [
    "assets.jsonl",
    "external_refs.jsonl",
    "activity.jsonl",
    "tags.json",
    "asset_tags.jsonl",
];

/// Module data files. `modules/media.jsonl` is required (every real export
/// has always carried Media); `modules/software.jsonl` and
/// `modules/services.jsonl` are present because every current export declares
/// those modules (docs/10).
pub const V1_MEDIA_FILE_PATH: &str = "modules/media.jsonl";
pub const V1_SOFTWARE_FILE_PATH: &str = "modules/software.jsonl";
pub const V1_SERVICES_FILE_PATH: &str = "modules/services.jsonl";
pub const V1_INFO_FILE_PATH: &str = "modules/info.jsonl";
pub const V1_RELATIONS_FILE_PATH: &str = "relations.jsonl";

/// Every file path a current exporter writes (a superset of what older
/// exporters wrote; readers pick up whichever exist).
pub const V1_FILE_PATHS: [&str; 10] = [
    "assets.jsonl",
    "external_refs.jsonl",
    "activity.jsonl",
    "tags.json",
    "asset_tags.jsonl",
    "relations.jsonl",
    "modules/media.jsonl",
    "modules/software.jsonl",
    "modules/services.jsonl",
    "modules/info.jsonl",
];

#[derive(Serialize)]
#[serde(untagged)]
pub enum PortableRow {
    Asset(PortableAssetV1),
    Media(PortableMediaRecordV1),
    Software(PortableSoftwareRecordV1),
    Service(PortableServiceRecordV1),
    Info(PortableInfoRecordV1),
    ExternalRef(PortableExternalRefV1),
    Activity(PortableActivityEventV1),
    Tag(PortableTagV1),
    Membership(AssetTagRow),
    Relation(PortableRelationV1),
}

impl PortableRow {
    pub(super) fn section(&self) -> (&'static str, &'static str) {
        match self {
            Self::Asset(_) => ("assets", "assets.jsonl"),
            Self::Media(_) => ("media", V1_MEDIA_FILE_PATH),
            Self::Software(_) => ("software", V1_SOFTWARE_FILE_PATH),
            Self::Service(_) => ("services", V1_SERVICES_FILE_PATH),
            Self::Info(_) => ("info", V1_INFO_FILE_PATH),
            Self::ExternalRef(_) => ("external_refs", "external_refs.jsonl"),
            Self::Activity(_) => ("activity", "activity.jsonl"),
            Self::Tag(_) => ("tags", "tags.json"),
            Self::Membership(_) => ("asset_tags", "asset_tags.jsonl"),
            Self::Relation(_) => ("relations", V1_RELATIONS_FILE_PATH),
        }
    }
    pub(super) fn touches(&self, excluded: &HashSet<String>) -> bool {
        match self {
            Self::Asset(row) => excluded.contains(&row.id),
            Self::Media(row) => excluded.contains(&row.asset_id),
            Self::Software(row) => excluded.contains(&row.asset_id),
            Self::Service(row) => excluded.contains(&row.asset_id),
            Self::Info(row) => excluded.contains(&row.asset_id),
            Self::ExternalRef(row) => excluded.contains(&row.asset_id),
            Self::Activity(row) => {
                row.asset_id
                    .as_ref()
                    .is_some_and(|id| excluded.contains(id))
                    || payload_has_api_key(&row.payload)
            }
            Self::Membership(row) => excluded.contains(&row.asset_id),
            Self::Relation(row) => {
                excluded.contains(&row.source_asset_id) || excluded.contains(&row.target_asset_id)
            }
            Self::Tag(_) => false,
        }
    }
}

fn payload_has_api_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields
                .get("info_type")
                .is_some_and(|kind| kind == "api_key")
                || fields.values().any(payload_has_api_key)
        }
        serde_json::Value::Array(values) => values.iter().any(payload_has_api_key),
        _ => false,
    }
}

pub(super) fn json_error(e: serde_json::Error) -> AppError {
    AppError::storage(format!("portable serialization failed: {e}"))
}
