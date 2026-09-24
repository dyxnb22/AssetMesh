//! Portable export format V1 (docs/05-storage-and-portability).
//!
//! The portable bundle is a product contract, independent of the physical
//! SQLite schema AND of the Rust domain structs: dedicated `*V1` DTOs freeze
//! the wire format so internal refactors cannot silently change version 1.
//! Derived state (search projection), discovery snapshots, provider cache,
//! and secrets are excluded.
//!
//! Layout:
//!
//! ```text
//! assetmesh-export/
//! ├── manifest.json
//! ├── assets.jsonl
//! ├── external_refs.jsonl
//! ├── activity.jsonl
//! ├── tags.json
//! ├── asset_tags.jsonl
//! ├── relations.jsonl
//! └── modules/
//!     ├── media.jsonl
//!     ├── software.jsonl
//!     └── services.jsonl
//! ```
//!
//! Module sections are governed by manifest declaration (ADR 0008): a
//! bundle exported by a build that manages a module declares it in
//! `manifest.modules` (and its count in `record_counts`), and the section is
//! then authoritative — including reconciliation of destination state the
//! bundle no longer carries. A Phase 1 bundle predates Software and
//! Relations; a Phase 2 bundle predates Services: the absent section means
//! "contains no such state", imports cleanly, and leaves any pre-existing
//! destination data of that kind untouched. A section file present WITHOUT
//! its manifest declaration is bundle corruption and fails loudly.
//!
//! Every import (dry-run or commit) runs the same preflight: declared files
//! must be present, decoded row counts must match the manifest, identities
//! must be unique, and the reference graph (module details, refs,
//! memberships, activity, merge redirects, relations) must be internally
//! consistent — a damaged bundle can never restore "successfully" while
//! omitting records. Dry-run additionally inspects the destination, so it
//! fails exactly when commit would.

use crate::application::SharedClock;
use crate::domain::activity::ActivityEvent;
use crate::domain::asset::{Asset, AssetKind, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::{ActivityId, AssetId, ExternalRefId, RelationId, TagId};
use crate::domain::media::{MediaRecord, MediaType};
use crate::domain::relation::{Relation, RelationProvenance, RelationType};
use crate::domain::service::{BillingCadence, ServiceRecord, ServiceType};
use crate::domain::software::{InstallSource, SoftwareCategory, SoftwareRecord};
use crate::domain::tag::Tag;
use crate::domain::Timestamp;
use crate::ports::repos::{
    ActivityReader, AssetReader, ExternalRefReader, MediaReader, RelationReader, ServiceReader,
    SoftwareReader, TagReader,
};
use crate::ports::uow::UnitOfWorkFactory;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const EXPORT_FORMAT: &str = "assetmesh-portable-export";
pub const EXPORT_VERSION: i64 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableExternalRefV1 {
    pub id: String,
    pub asset_id: String,
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
    pub metadata: Option<String>,
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

fn ts_to_wire(ts: Timestamp) -> String {
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

fn id_from_wire(value: &str, field: &str) -> AppResult<uuid::Uuid> {
    uuid::Uuid::parse_str(value)
        .map_err(|e| AppError::validation(format!("bundle has invalid {field} id {value:?}: {e}")))
}

impl PortableAssetV1 {
    fn from_domain(asset: &Asset) -> Self {
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

    fn into_domain(self) -> AppResult<Asset> {
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
    fn from_domain(record: &MediaRecord) -> Self {
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

    fn into_domain(self) -> AppResult<MediaRecord> {
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
    fn from_domain(record: &SoftwareRecord) -> Self {
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

    fn into_domain(self) -> AppResult<SoftwareRecord> {
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
    fn from_domain(record: &ServiceRecord) -> Self {
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
        }
    }

    fn into_domain(self) -> AppResult<ServiceRecord> {
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
        })
    }
}

impl PortableRelationV1 {
    fn from_domain(relation: &Relation) -> Self {
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

    fn into_domain(self) -> AppResult<Relation> {
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
    fn from_domain(reference: &AssetExternalRef) -> Self {
        PortableExternalRefV1 {
            id: id_to_wire(reference.id.as_uuid()),
            asset_id: id_to_wire(reference.asset_id.as_uuid()),
            namespace: reference.namespace.clone(),
            external_id: reference.external_id.clone(),
            source_url: reference.source_url.clone(),
            metadata: reference.metadata.clone(),
            created_at: ts_to_wire(reference.created_at),
            updated_at: ts_to_wire(reference.updated_at),
        }
    }

    fn into_domain(self) -> AppResult<AssetExternalRef> {
        Ok(AssetExternalRef {
            id: ExternalRefId::from_uuid(id_from_wire(&self.id, "external ref")?),
            asset_id: AssetId::from_uuid(id_from_wire(&self.asset_id, "external ref asset")?),
            namespace: self.namespace,
            external_id: self.external_id,
            source_url: self.source_url,
            metadata: self.metadata,
            created_at: ts_from_wire(&self.created_at, "ref.created_at")?,
            updated_at: ts_from_wire(&self.updated_at, "ref.updated_at")?,
        })
    }
}

impl PortableActivityEventV1 {
    fn from_domain(event: &ActivityEvent) -> Self {
        PortableActivityEventV1 {
            id: id_to_wire(event.id.as_uuid()),
            occurred_at: ts_to_wire(event.occurred_at),
            event_type: event.event_type.clone(),
            asset_id: event.asset_id.map(|id| id_to_wire(id.as_uuid())),
            actor: event.actor.clone(),
            payload: event.payload.clone(),
        }
    }

    fn into_domain(self) -> AppResult<ActivityEvent> {
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
    fn from_domain(tag: &Tag) -> Self {
        PortableTagV1 {
            id: id_to_wire(tag.id.as_uuid()),
            name: tag.name.clone(),
            created_at: ts_to_wire(tag.created_at),
        }
    }

    fn into_domain(self) -> AppResult<Tag> {
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

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

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
pub const V1_RELATIONS_FILE_PATH: &str = "relations.jsonl";

/// Every file path a current exporter writes (a superset of what older
/// exporters wrote; readers pick up whichever exist).
pub const V1_FILE_PATHS: [&str; 9] = [
    "assets.jsonl",
    "external_refs.jsonl",
    "activity.jsonl",
    "tags.json",
    "asset_tags.jsonl",
    "relations.jsonl",
    "modules/media.jsonl",
    "modules/software.jsonl",
    "modules/services.jsonl",
];

pub struct PortableExportService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
}

impl<F: UnitOfWorkFactory> PortableExportService<F> {
    pub fn new(factory: F, clock: SharedClock) -> Self {
        PortableExportService { factory, clock }
    }

    /// Builds the whole bundle from one snapshot-consistent read scope. Rows
    /// are ordered by identity so bundles are deterministic and diffable.
    pub fn export(&mut self, app_version: &str) -> AppResult<PortableBundle> {
        let (assets, media, software, services, refs, activity, tags, memberships, relations) =
            self.factory.read(&mut |q| {
                let assets = q.assets().list(&crate::ports::repos::AssetFilter {
                    kind: None,
                    lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
                })?;
                let media = q.media().list_all()?;
                let software = q.software().list_all()?;
                let services = q.services().list_all()?;
                let refs = q.external_refs().list_all()?;
                let activity = q.activity().list_all()?;
                let tags = q.tags().list_all()?;
                let memberships = q.tags().list_memberships()?;
                let relations = q.relations().list_all()?;
                Ok((
                    assets,
                    media,
                    software,
                    services,
                    refs,
                    activity,
                    tags,
                    memberships,
                    relations,
                ))
            })?;

        // Services export through `modules/services.jsonl`: the wire DTO
        // carries the canonical vocabulary only, so no credential material can
        // reach a bundle (ADR 0010). An archived service is still exported —
        // archiving is a lifecycle state, not a deletion.

        let created_at = self.clock.now();

        let mut assets: Vec<PortableAssetV1> =
            assets.iter().map(PortableAssetV1::from_domain).collect();
        assets.sort_by(|a, b| a.id.cmp(&b.id));
        let mut media: Vec<PortableMediaRecordV1> = media
            .iter()
            .map(PortableMediaRecordV1::from_domain)
            .collect();
        media.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        let mut software: Vec<PortableSoftwareRecordV1> = software
            .iter()
            .map(PortableSoftwareRecordV1::from_domain)
            .collect();
        software.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        let mut services: Vec<PortableServiceRecordV1> = services
            .iter()
            .map(PortableServiceRecordV1::from_domain)
            .collect();
        services.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        let mut refs: Vec<PortableExternalRefV1> = refs
            .iter()
            .map(PortableExternalRefV1::from_domain)
            .collect();
        refs.sort_by(|a, b| {
            (&a.namespace, &a.external_id, &a.id).cmp(&(&b.namespace, &b.external_id, &b.id))
        });
        let mut activity: Vec<PortableActivityEventV1> = activity
            .iter()
            .map(PortableActivityEventV1::from_domain)
            .collect();
        activity.sort_by(|a, b| (&a.occurred_at, &a.id).cmp(&(&b.occurred_at, &b.id)));
        let mut tags: Vec<PortableTagV1> = tags.iter().map(PortableTagV1::from_domain).collect();
        tags.sort_by(|a, b| (&a.id, &a.name).cmp(&(&b.id, &b.name)));
        let mut relations: Vec<PortableRelationV1> = relations
            .iter()
            .map(PortableRelationV1::from_domain)
            .collect();
        relations.sort_by(|a, b| a.id.cmp(&b.id));
        let mut memberships = memberships;
        memberships.sort();

        let asset_tags_rows: Vec<AssetTagRow> = memberships
            .into_iter()
            .map(|(asset_id, tag_id)| AssetTagRow {
                asset_id: asset_id.to_string(),
                tag_id: tag_id.to_string(),
            })
            .collect();

        let manifest = PortableManifest {
            format: EXPORT_FORMAT.to_string(),
            version: EXPORT_VERSION,
            created_at: ts_to_wire(created_at),
            app_version: app_version.to_string(),
            modules: BTreeMap::from([
                (
                    "media".to_string(),
                    ModuleVersion {
                        schema_version: MEDIA_SCHEMA_VERSION,
                    },
                ),
                (
                    "software".to_string(),
                    ModuleVersion {
                        schema_version: SOFTWARE_SCHEMA_VERSION,
                    },
                ),
                (
                    "services".to_string(),
                    ModuleVersion {
                        schema_version: SERVICES_SCHEMA_VERSION,
                    },
                ),
            ]),
            record_counts: BTreeMap::from([
                ("assets".to_string(), assets.len()),
                ("external_refs".to_string(), refs.len()),
                ("activity".to_string(), activity.len()),
                ("tags".to_string(), tags.len()),
                ("asset_tags".to_string(), asset_tags_rows.len()),
                ("media".to_string(), media.len()),
                ("software".to_string(), software.len()),
                ("services".to_string(), services.len()),
                ("relations".to_string(), relations.len()),
            ]),
        };

        let files = vec![
            ExportFile {
                path: "assets.jsonl".into(),
                content: to_jsonl(&assets)?,
            },
            ExportFile {
                path: "external_refs.jsonl".into(),
                content: to_jsonl(&refs)?,
            },
            ExportFile {
                path: "activity.jsonl".into(),
                content: to_jsonl(&activity)?,
            },
            ExportFile {
                path: "tags.json".into(),
                content: serde_json::to_string_pretty(&tags).map_err(json_error)?,
            },
            ExportFile {
                path: "asset_tags.jsonl".into(),
                content: to_jsonl(&asset_tags_rows)?,
            },
            ExportFile {
                path: "relations.jsonl".into(),
                content: to_jsonl(&relations)?,
            },
            ExportFile {
                path: "modules/media.jsonl".into(),
                content: to_jsonl(&media)?,
            },
            ExportFile {
                path: "modules/software.jsonl".into(),
                content: to_jsonl(&software)?,
            },
            ExportFile {
                path: "modules/services.jsonl".into(),
                content: to_jsonl(&services)?,
            },
        ];

        Ok(PortableBundle { manifest, files })
    }
}

fn to_jsonl<T: Serialize>(rows: &[T]) -> AppResult<String> {
    let mut out = String::new();
    for row in rows {
        let line = serde_json::to_string(row).map_err(json_error)?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

fn json_error(e: serde_json::Error) -> AppError {
    AppError::storage(format!("portable serialization failed: {e}"))
}

// ---------------------------------------------------------------------------
// Filesystem bundle I/O (reference adapter; std-only, reusable by adapters)
// ---------------------------------------------------------------------------

/// Filesystem bundle I/O (std-only reference adapter).
///
/// Replacement protocol — crash-recoverable with deterministic names:
///
/// 1. any interrupted swap left behind by a previous process is recovered
///    first (roll back to the previous bundle if the swap never completed);
/// 2. the new bundle is written completely into a sibling staging directory
///    `<target>.swap/new` and fsynced;
/// 3. the existing target is moved to `<target>.swap/previous`, then the new
///    bundle is moved into place; the swap directory is removed.
///
/// A process crash can therefore only leave the deterministic swap directory
/// behind — never a missing or half-written `target` — and the next read or
/// write restores a complete old or new bundle. Note std cannot fsync
/// directories; a power-loss (as opposed to process crash) may still lose
/// the rename durability, which is why recovery runs on every operation.
/// Writes a bundle to `target`, replacing any AssetMesh bundle already there.
///
/// The whole operation runs under an exclusive cross-process lock
/// ([`with_bundle_lock`]) and refuses to clobber a directory that holds
/// unrelated data: overwriting an existing destination is only permitted
/// when that destination is itself an AssetMesh bundle (or an empty
/// directory). See [`ensure_replaceable`].
pub fn write_bundle_to_directory(bundle: &PortableBundle, target: &Path) -> AppResult<()> {
    with_bundle_lock(target, || {
        recover_interrupted_swap(target)?;
        ensure_replaceable(target)?;

        let swap = swap_dir(target);
        let new_dir = swap.join("new");
        fs::create_dir_all(new_dir.join("modules")).map_err(fs_error)?;
        for file in &bundle.files {
            let path = new_dir.join(&file.path);
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).map_err(fs_error)?;
            }
            fs::File::create(&path)
                .and_then(|mut f| {
                    f.write_all(file.content.as_bytes())?;
                    f.sync_all()
                })
                .map_err(fs_error)?;
        }
        let manifest_json = bundle.manifest_json()?;
        let manifest_path = new_dir.join("manifest.json");
        fs::File::create(&manifest_path)
            .and_then(|mut f| {
                f.write_all(manifest_json.as_bytes())?;
                f.sync_all()
            })
            .map_err(fs_error)?;

        if target.exists() {
            fs::rename(target, swap.join("previous")).map_err(fs_error)?;
        }
        fs::rename(&new_dir, target).map_err(fs_error)?;
        let _ = fs::remove_dir_all(&swap);
        Ok(())
    })
}

/// Reads a bundle from a directory, recovering any interrupted swap first so
/// a crash between the two renames can never hide the last complete bundle.
///
/// Recovery is destructive (it deletes a leftover swap directory), so it
/// happens under the same exclusive lock a write takes: without it, a reader
/// in one process could delete the staging directory of a writer in another.
pub fn read_bundle_from_directory(source: &Path) -> AppResult<PortableBundle> {
    // A bundle on read-only media cannot create the sibling lock file. It
    // also cannot be replaced by a cooperating writer on that media. Never
    // attempt recovery here: an interrupted swap needs a writable parent.
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    if fs::metadata(parent)
        .map_err(fs_error)?
        .permissions()
        .readonly()
    {
        if swap_dir(source).exists() {
            return Err(AppError::storage_busy(
                "a read-only bundle has an interrupted export; copy it to writable storage to recover",
            ));
        }
        return read_bundle_files(source);
    }
    with_bundle_lock(source, || {
        recover_interrupted_swap(source)?;
        read_bundle_files(source)
    })
}

fn read_bundle_files(source: &Path) -> AppResult<PortableBundle> {
    let manifest_text = fs::read_to_string(source.join("manifest.json")).map_err(fs_error)?;
    let manifest: PortableManifest = serde_json::from_str(&manifest_text).map_err(json_error)?;

    let mut files = Vec::new();
    for path in V1_FILE_PATHS {
        let path_buf = source.join(path);
        if path_buf.exists() {
            let content = fs::read_to_string(&path_buf).map_err(fs_error)?;
            files.push(ExportFile {
                path: path.into(),
                content,
            });
        }
    }

    Ok(PortableBundle { manifest, files })
}

/// Deterministic staging location: `<parent>/.<name>.swap/` with `new/` and
/// optionally `previous/` subdirectories.
fn swap_dir(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "assetmesh-export".to_string());
    parent.join(format!(".{file_name}.swap"))
}

/// Rolls an interrupted swap forward or back so `target` always ends up
/// pointing at a complete bundle:
/// - target missing, `previous` present → the crash happened between the two
///   renames; roll the previous (known-good) bundle back into place;
/// - target present, `previous` present → the swap completed but cleanup did
///   not; remove the swap directory;
/// - anything else → the crash happened while staging; discard the swap.
///
/// Callers must hold the bundle lock: this deletes a swap directory, and a
/// reader that raced an active writer would otherwise destroy the writer's
/// staging area.
fn recover_interrupted_swap(target: &Path) -> AppResult<()> {
    let swap = swap_dir(target);
    if !swap.exists() {
        return Ok(());
    }
    let previous = swap.join("previous");
    if !target.exists() && previous.exists() {
        fs::rename(&previous, target).map_err(fs_error)?;
    }
    let _ = fs::remove_dir_all(&swap);
    Ok(())
}

fn fs_error(e: std::io::Error) -> AppError {
    AppError::storage(format!("portable bundle I/O failed: {e}"))
}

// ---------------------------------------------------------------------------
// Cross-process bundle lock (OS advisory exclusive file lock)
// ---------------------------------------------------------------------------

use fs2::FileExt;

/// Runs `body` while holding an exclusive cross-process lock on `target`.
///
/// The lock is a sibling file (`<parent>/.<name>.lock`) using operating
/// system advisory exclusive file locks (`fs2::FileExt::try_lock_exclusive`).
/// When a process exits or crashes, the OS automatically releases the lock.
/// The lock file itself remains on disk.
pub fn with_bundle_lock<R>(target: &Path, body: impl FnOnce() -> AppResult<R>) -> AppResult<R> {
    let _lock = BundleLock::acquire(target)?;
    body()
}

/// Owned guard for the bundle lock file. Holding the open `File` keeps the
/// OS advisory lock active until dropped or until the process exits.
#[derive(Debug)]
pub struct BundleLock {
    _file: fs::File,
    path: PathBuf,
}

impl BundleLock {
    pub fn acquire(target: &Path) -> AppResult<Self> {
        let path = lock_path(target);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(fs_error)?;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(fs_error)?;

        match file.try_lock_exclusive() {
            Ok(()) => Ok(BundleLock { _file: file, path }),
            Err(e) if is_lock_contention(&e) => Err(AppError::storage_busy(format!(
                "another process is using the bundle at {}; retry later",
                target.display()
            ))),
            Err(e) => Err(fs_error(e)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BundleLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

/// Lock location: `<parent>/.<name>.lock`, a sibling of the target bundle directory.
pub fn lock_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "assetmesh-export".to_string());
    parent.join(format!(".{file_name}.lock"))
}

fn is_lock_contention(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        return code == 35 /* EWOULDBLOCK on macOS / BSD */ || code == 11 /* EAGAIN / EWOULDBLOCK on Linux */;
    }
    false
}

/// Guards the destructive rename in [`write_bundle_to_directory`]: a target
/// that already holds data is only replaced when that data is itself an
/// AssetMesh bundle. Silently `rename`-ing a user's existing directory into
/// the swap area deletes it, and the failure mode is not recoverable — the
/// swap directory is removed on success, so the old contents would be gone.
fn ensure_replaceable(target: &Path) -> AppResult<()> {
    if !target.exists() {
        return Ok(());
    }
    if is_assetmesh_bundle(target)? {
        return Ok(());
    }
    // An empty directory destroys nothing, so it is an acceptable target.
    if is_empty_directory(target) {
        return Ok(());
    }
    Err(AppError::validation(format!(
        "refusing to overwrite {}: the destination exists but is not an AssetMesh export bundle. \
         Move its contents aside or export into an empty directory.",
        target.display()
    )))
}

/// True for an empty directory. Anything that cannot be enumerated — a file,
/// or an unreadable directory — is not empty, and therefore not replaceable.
fn is_empty_directory(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

/// True when `dir` contains a manifest that parses as this format. Parsing
/// the manifest (rather than only checking the file name) keeps an unrelated
/// directory that happens to contain `manifest.json` from being replaced.
fn is_assetmesh_bundle(dir: &Path) -> AppResult<bool> {
    let Ok(text) = fs::read_to_string(dir.join("manifest.json")) else {
        return Ok(false);
    };
    match serde_json::from_str::<PortableManifest>(&text) {
        Ok(manifest) => Ok(manifest.format == EXPORT_FORMAT && manifest.version == EXPORT_VERSION),
        Err(_) => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct PortableImportReport {
    pub assets_created: usize,
    pub assets_updated: usize,
    pub media_created: usize,
    pub media_updated: usize,
    pub software_created: usize,
    pub software_updated: usize,
    pub services_created: usize,
    pub services_updated: usize,
    pub external_refs_created: usize,
    pub external_refs_deduplicated: usize,
    pub activity_created: usize,
    pub tags_created: usize,
    pub relations_created: usize,
    pub relations_updated: usize,
}

/// Fully decoded and preflighted bundle contents.
struct DecodedBundle {
    assets: Vec<Asset>,
    media: Vec<MediaRecord>,
    /// Empty when the bundle predates the Software module (undeclared).
    software: Vec<SoftwareRecord>,
    /// Empty when the bundle predates the Services module (undeclared).
    services: Vec<ServiceRecord>,
    refs: Vec<AssetExternalRef>,
    activity: Vec<ActivityEvent>,
    tags: Vec<Tag>,
    asset_tags: Vec<AssetTagRow>,
    /// Empty when the bundle predates Relations (undeclared).
    relations: Vec<Relation>,
    /// True when the manifest declares the software module. Declared
    /// sections are authoritative on import (reconciliation applies);
    /// undeclared sections are legacy compatibility and leave destination
    /// state untouched.
    software_declared: bool,
    /// Same contract for the Services module (docs/10 "Portable data"): a
    /// declared section is authoritative, an undeclared one means the bundle
    /// predates Services and must not erase destination service data.
    services_declared: bool,
    relations_declared: bool,
}

/// One consistent snapshot of the destination, taken before commit or
/// dry-run. Both compute their dispositions from the same snapshot shape, so
/// dry-run is field-for-field identical to what commit would do.
struct DestinationSnapshot {
    asset_ids: HashSet<AssetId>,
    /// Kinds of destination assets, so an import can detect a cross-module
    /// re-typing that would strand a module record (e.g. a bundle declaring a
    /// media.* kind over an asset that carries a software record).
    asset_kinds: HashMap<AssetId, AssetKind>,
    media_asset_ids: HashSet<AssetId>,
    software_asset_ids: HashSet<AssetId>,
    /// Services detail ids, so a re-type away from a service.* kind is caught
    /// here rather than only at the repository boundary during commit.
    service_asset_ids: HashSet<AssetId>,
    ref_pairs: HashMap<(String, String), AssetId>,
    ref_ids: HashMap<ExternalRefId, (String, String)>,
    activity_ids: HashSet<ActivityId>,
    tag_ids: HashSet<TagId>,
    tag_names: HashSet<String>,
    relation_ids: HashSet<RelationId>,
    /// Canonical relation triples keyed for duplicate/conflict checks,
    /// including the mirror-image key of symmetric types.
    relation_triples: HashMap<(String, String, String), RelationId>,
}

impl DestinationSnapshot {
    fn load<R: DestinationRead>(mut readers: R) -> AppResult<Self> {
        let asset_ids: HashSet<AssetId> = readers
            .assets()
            .list(&crate::ports::repos::AssetFilter {
                kind: None,
                lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
            })?
            .into_iter()
            .map(|a| a.id)
            .collect();
        let asset_kinds: HashMap<AssetId, AssetKind> = readers
            .assets()
            .list(&crate::ports::repos::AssetFilter {
                kind: None,
                lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
            })?
            .into_iter()
            .map(|a| (a.id, a.kind))
            .collect();
        let media_asset_ids: HashSet<AssetId> = readers
            .media()
            .list_all()?
            .into_iter()
            .map(|m| m.asset_id)
            .collect();
        let software_asset_ids: HashSet<AssetId> = readers
            .software()
            .list_all()?
            .into_iter()
            .map(|s| s.asset_id)
            .collect();
        let service_asset_ids: HashSet<AssetId> = readers
            .services()
            .list_all()?
            .into_iter()
            .map(|s| s.asset_id)
            .collect();
        let mut ref_pairs = HashMap::new();
        let mut ref_ids = HashMap::new();
        for reference in readers.external_refs().list_all()? {
            ref_pairs.insert(
                (reference.namespace.clone(), reference.external_id.clone()),
                reference.asset_id,
            );
            ref_ids.insert(
                reference.id,
                (reference.namespace.clone(), reference.external_id.clone()),
            );
        }
        let activity_ids: HashSet<ActivityId> = readers
            .activity()
            .list_all()?
            .into_iter()
            .map(|e| e.id)
            .collect();
        let mut tag_ids = HashSet::new();
        let mut tag_names = HashSet::new();
        for tag in readers.tags().list_all()? {
            tag_ids.insert(tag.id);
            tag_names.insert(tag.name);
        }
        let mut relation_ids = HashSet::new();
        let mut relation_triples = HashMap::new();
        for relation in readers.relations().list_all()? {
            relation_ids.insert(relation.id);
            let triple = (
                relation.source_asset_id.to_string(),
                relation.target_asset_id.to_string(),
                relation.relation_type.as_str().to_string(),
            );
            relation_triples.insert(triple, relation.id);
            if relation.relation_type.is_symmetric() {
                let mirror = (
                    relation.target_asset_id.to_string(),
                    relation.source_asset_id.to_string(),
                    relation.relation_type.as_str().to_string(),
                );
                relation_triples.insert(mirror, relation.id);
            }
        }
        Ok(DestinationSnapshot {
            asset_ids,
            asset_kinds,
            media_asset_ids,
            software_asset_ids,
            service_asset_ids,
            ref_pairs,
            ref_ids,
            activity_ids,
            tag_ids,
            tag_names,
            relation_ids,
            relation_triples,
        })
    }
}

/// Read access to the destination tables.
///
/// Implemented for both the read scope (`QueryUnitOfWork`) and the write
/// scope (`UnitOfWork`): the snapshot must be takeable *inside* the commit
/// transaction as well as from a read-only scope, and duplicating the loader
/// body for the two cases would let them drift. A write scope upcasts to its
/// reader supertraits — every repository trait extends its reader trait.
trait DestinationRead {
    fn assets(&mut self) -> &mut dyn AssetReader;
    fn media(&mut self) -> &mut dyn MediaReader;
    fn software(&mut self) -> &mut dyn SoftwareReader;
    fn services(&mut self) -> &mut dyn ServiceReader;
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader;
    fn activity(&mut self) -> &mut dyn ActivityReader;
    fn tags(&mut self) -> &mut dyn TagReader;
    fn relations(&mut self) -> &mut dyn RelationReader;
}

/// Thin newtypes over the two scopes. Passing `&mut dyn UnitOfWork` directly
/// would freeze the borrow for the whole transaction (the trait object's
/// lifetime bound is the outer borrow); a wrapper gives the loader a lifetime
/// of its own so the transaction keeps mutating after the snapshot is built.
struct ReadScope<'a>(&'a mut dyn crate::ports::uow::QueryUnitOfWork);
struct WriteScope<'a>(&'a mut dyn crate::ports::uow::UnitOfWork);

impl DestinationRead for ReadScope<'_> {
    fn assets(&mut self) -> &mut dyn AssetReader {
        self.0.assets()
    }
    fn media(&mut self) -> &mut dyn MediaReader {
        self.0.media()
    }
    fn software(&mut self) -> &mut dyn SoftwareReader {
        self.0.software()
    }
    fn services(&mut self) -> &mut dyn ServiceReader {
        self.0.services()
    }
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        self.0.external_refs()
    }
    fn activity(&mut self) -> &mut dyn ActivityReader {
        self.0.activity()
    }
    fn tags(&mut self) -> &mut dyn TagReader {
        self.0.tags()
    }
    fn relations(&mut self) -> &mut dyn RelationReader {
        self.0.relations()
    }
}

impl DestinationRead for WriteScope<'_> {
    fn assets(&mut self) -> &mut dyn AssetReader {
        self.0.assets()
    }
    fn media(&mut self) -> &mut dyn MediaReader {
        self.0.media()
    }
    fn software(&mut self) -> &mut dyn SoftwareReader {
        self.0.software()
    }
    fn services(&mut self) -> &mut dyn ServiceReader {
        self.0.services()
    }
    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        self.0.external_refs()
    }
    fn activity(&mut self) -> &mut dyn ActivityReader {
        self.0.activity()
    }
    fn tags(&mut self) -> &mut dyn TagReader {
        self.0.tags()
    }
    fn relations(&mut self) -> &mut dyn RelationReader {
        self.0.relations()
    }
}

/// Identity conflicts that make commit impossible — checked identically for
/// dry-run and commit, before any mutation.
fn check_destination(decoded: &DecodedBundle, snapshot: &DestinationSnapshot) -> AppResult<()> {
    // An external ref must not be re-pointed at a different asset than it
    // already belongs to: that would silently rewrite identity (ADR 0005).
    for reference in &decoded.refs {
        if let Some(existing) = snapshot
            .ref_pairs
            .get(&(reference.namespace.clone(), reference.external_id.clone()))
        {
            if *existing != reference.asset_id {
                return Err(AppError::import_conflict(format!(
                    "external ref {}:{} is attached to asset {}, but the bundle attaches it to {}",
                    reference.namespace, reference.external_id, existing, reference.asset_id
                )));
            }
        }
        // A ref row id already in the destination but carrying a different
        // pair would violate the primary key at commit.
        if let Some((namespace, external_id)) = snapshot.ref_ids.get(&reference.id) {
            if *namespace != reference.namespace || *external_id != reference.external_id {
                return Err(AppError::import_conflict(format!(
                    "external ref id {} already exists in the destination as {}:{}",
                    reference.id, namespace, external_id
                )));
            }
        }
    }
    // A bundled relation must not overwrite a destination relation between
    // the same endpoints with a different identity.
    for relation in &decoded.relations {
        let triple = (
            relation.source_asset_id.to_string(),
            relation.target_asset_id.to_string(),
            relation.relation_type.as_str().to_string(),
        );
        if let Some(existing) = snapshot.relation_triples.get(&triple) {
            if *existing != relation.id {
                return Err(AppError::import_conflict(format!(
                    "relation {} between {} and {} already exists in the destination as {}",
                    relation.relation_type,
                    relation.source_asset_id,
                    relation.target_asset_id,
                    existing
                )));
            }
        }
    }
    // A bundle must not re-type an asset while the destination still carries
    // the old module's record — that would strand a software record on a
    // media.* asset (or the reverse), and the repository boundary rejects ANY
    // kind change while the row remains, within a module included
    // (media.movie -> media.game strands a movie record the same way). Caught
    // here so dry-run and commit fail identically, before any canonical
    // mutation; the stranded record belongs to the OLD module.
    for asset in &decoded.assets {
        let Some(&stored_kind) = snapshot.asset_kinds.get(&asset.id) else {
            continue;
        };
        if stored_kind == asset.kind {
            continue;
        }
        let has_old_record = match stored_kind.module() {
            "media" => snapshot.media_asset_ids.contains(&asset.id),
            "software" => snapshot.software_asset_ids.contains(&asset.id),
            "services" => snapshot.service_asset_ids.contains(&asset.id),
            _ => false,
        };
        if has_old_record {
            return Err(AppError::import_conflict(format!(
                "asset {} is a {} in the destination and the bundle re-types it as {}; remove the \
                 {} record on the destination before importing",
                asset.id,
                stored_kind,
                asset.kind,
                stored_kind.module()
            )));
        }
    }
    Ok(())
}

/// Computes the exact dispositions of an import from one destination
/// snapshot. Dry-run returns this directly; commit performs these writes.
fn dispositions(decoded: &DecodedBundle, snapshot: &DestinationSnapshot) -> PortableImportReport {
    let mut report = PortableImportReport::default();
    for asset in &decoded.assets {
        if snapshot.asset_ids.contains(&asset.id) {
            report.assets_updated += 1;
        } else {
            report.assets_created += 1;
        }
    }
    // Judged by module-record existence, not asset existence: an asset can
    // be present in the destination while its module details are not.
    for record in &decoded.media {
        if snapshot.media_asset_ids.contains(&record.asset_id) {
            report.media_updated += 1;
        } else {
            report.media_created += 1;
        }
    }
    for record in &decoded.software {
        if snapshot.software_asset_ids.contains(&record.asset_id) {
            report.software_updated += 1;
        } else {
            report.software_created += 1;
        }
    }
    for record in &decoded.services {
        if snapshot.service_asset_ids.contains(&record.asset_id) {
            report.services_updated += 1;
        } else {
            report.services_created += 1;
        }
    }
    for reference in &decoded.refs {
        match snapshot
            .ref_pairs
            .get(&(reference.namespace.clone(), reference.external_id.clone()))
        {
            Some(_) => report.external_refs_deduplicated += 1,
            None => report.external_refs_created += 1,
        }
    }
    report.activity_created = decoded
        .activity
        .iter()
        .filter(|e| !snapshot.activity_ids.contains(&e.id))
        .count();
    report.tags_created = decoded
        .tags
        .iter()
        .filter(|t| !snapshot.tag_names.contains(&t.name) && !snapshot.tag_ids.contains(&t.id))
        .count();
    for relation in &decoded.relations {
        if snapshot.relation_ids.contains(&relation.id) {
            report.relations_updated += 1;
        } else {
            report.relations_created += 1;
        }
    }
    report
}

pub struct PortableImportService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> PortableImportService<F> {
    pub fn new(factory: F) -> Self {
        PortableImportService { factory }
    }

    /// Imports a portable bundle deterministically by canonical ID. The
    /// bundle is the source of truth for every asset it contains: existing
    /// rows with the same IDs are updated, and destination module state that
    /// the bundle no longer carries (e.g. a merged tombstone's old media
    /// details) is removed, keeping the destination equivalent to the bundle.
    /// Dry-run runs the identical preflight, destination checks, and
    /// disposition computation, so it fails exactly when commit would and
    /// reports field-for-field the same counts.
    pub fn import_bundle(
        &mut self,
        bundle: &PortableBundle,
        dry_run: bool,
    ) -> AppResult<PortableImportReport> {
        // Shared preflight — identical for dry-run and commit.
        let decoded = preflight(bundle)?;

        if dry_run {
            // Dry-run reads the destination without writing, so a read scope
            // is enough: it reports exactly what commit would do.
            let snapshot = self
                .factory
                .read(&mut |q| DestinationSnapshot::load(ReadScope(q)))?;
            check_destination(&decoded, &snapshot)?;
            return Ok(dispositions(&decoded, &snapshot));
        }

        // Commit. The destination snapshot and the conflict check run INSIDE
        // the write transaction, not before it: an IMMEDIATE transaction
        // holds the write lock from the snapshot through commit, so no other
        // connection can insert a conflicting row between "checked the
        // destination" and "wrote the rows". Taking the snapshot outside the
        // transaction leaves a check-then-act window the size of the whole
        // import — wide enough for a concurrent import or a running desktop
        // session to invalidate every identity check below.
        //
        // Commit. Assets first (self-referencing merged_into is resolved in
        // a second pass), then module details, refs, tags, activity,
        // relations, and finally module-state reconciliation + projections.
        self.factory.transact(&mut |uow| {
            let snapshot = DestinationSnapshot::load(WriteScope(uow))?;
            check_destination(&decoded, &snapshot)?;
            let report = dispositions(&decoded, &snapshot);

            for asset in &decoded.assets {
                let mut staged = asset.clone();
                // Intermediate state for the two-pass insert: no redirect
                // target yet (FK) and no tombstone marker (validation).
                staged.merged_into = None;
                if staged.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
                    staged.lifecycle_state = crate::domain::asset::LifecycleState::Active;
                }
                if snapshot.asset_ids.contains(&asset.id) {
                    uow.assets().update(&staged)?;
                } else {
                    uow.assets().insert(&staged)?;
                }
            }
            for asset in &decoded.assets {
                if asset.merged_into.is_some() {
                    uow.assets().update(asset)?;
                }
            }

            for record in &decoded.media {
                uow.media().upsert(record)?;
            }
            for record in &decoded.software {
                uow.software().upsert(record)?;
            }
            for record in &decoded.services {
                uow.services().upsert(record)?;
            }

            for reference in &decoded.refs {
                let known = snapshot
                    .ref_pairs
                    .get(&(reference.namespace.clone(), reference.external_id.clone()));
                if known.is_some() {
                    continue; // same owner, same pair: deduplicated
                }
                uow.external_refs().insert(reference)?;
            }

            // Tags preserve their exported identity; a name collision with a
            // different id remaps membership onto the existing tag. The map is
            // keyed by canonical UUID, and the membership lookup goes through
            // the same wire parser: bundle writers are free to emit uppercase
            // hex, and the preflight a few lines below parses `pair.tag_id`
            // with `id_from_wire` too, so string-comparing raw wire text
            // against a lowercase key would miss and report a spurious
            // "unknown tag id" for a bundle that round-trips through
            // preflight cleanly.
            let mut tag_id_map: BTreeMap<Uuid, TagId> = BTreeMap::new();
            for tag in &decoded.tags {
                let resolved = if let Some(existing) = uow.tags().find_by_name(&tag.name)? {
                    existing.id
                } else if uow.tags().get(tag.id)?.is_some() {
                    uow.tags().update(tag)?;
                    tag.id
                } else {
                    uow.tags().insert(tag)?;
                    tag.id
                };
                tag_id_map.insert(tag.id.as_uuid(), resolved);
            }

            for pair in &decoded.asset_tags {
                let asset_id = AssetId::from_uuid(id_from_wire(&pair.asset_id, "asset_tags")?);
                let tag_wire = id_from_wire(&pair.tag_id, "asset_tags")?;
                let tag_id = tag_id_map.get(&tag_wire).copied().ok_or_else(|| {
                    AppError::validation(format!(
                        "asset_tags references unknown tag id {}",
                        pair.tag_id
                    ))
                })?;
                uow.tags().insert_membership(asset_id, tag_id)?;
            }

            for event in &decoded.activity {
                if snapshot.activity_ids.contains(&event.id) {
                    uow.activity().upsert(event)?;
                } else {
                    uow.activity().append(event)?;
                }
            }

            for relation in &decoded.relations {
                if snapshot.relation_ids.contains(&relation.id) {
                    uow.relations().update(relation)?;
                } else {
                    uow.relations().insert(relation)?;
                }
            }

            // Module-state reconciliation: the bundle is authoritative for
            // every asset it contains. An asset without module details in
            // the bundle (typically a merged tombstone) must not keep stale
            // destination details or a search document; a details-bearing
            // asset is projected from its ACTUAL committed state (post-remap
            // tags, destination refs), not from the bundle subset.
            // Undeclared (legacy) sections leave destination state of that
            // kind untouched.
            let bundled_relation_ids: HashSet<RelationId> =
                decoded.relations.iter().map(|r| r.id).collect();
            for existing in uow.relations().list_all()? {
                if !decoded.relations_declared || bundled_relation_ids.contains(&existing.id) {
                    continue;
                }
                // Relations among bundled assets that the bundle no longer
                // carries are removed; relations reaching outside the bundle
                // belong to destination-local assets and stay.
                if decoded
                    .assets
                    .iter()
                    .any(|a| a.id == existing.source_asset_id)
                    && decoded
                        .assets
                        .iter()
                        .any(|a| a.id == existing.target_asset_id)
                {
                    uow.relations().delete(existing.id)?;
                }
            }

            for asset in &decoded.assets {
                let bundled_media = decoded.media.iter().any(|m| m.asset_id == asset.id);
                let bundled_software = decoded.software_declared
                    && decoded.software.iter().any(|s| s.asset_id == asset.id);
                let bundled_service = decoded.services_declared
                    && decoded.services.iter().any(|s| s.asset_id == asset.id);

                if !bundled_media {
                    uow.media().delete(asset.id)?;
                }
                if decoded.software_declared && !bundled_software {
                    uow.software().delete(asset.id)?;
                }
                if decoded.services_declared && !bundled_service {
                    uow.services().delete(asset.id)?;
                }

                if asset.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
                    uow.search_index().remove(asset.id)?;
                } else if bundled_media {
                    let record = uow
                        .media()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("media record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    let document =
                        crate::application::projection::project_media(asset, &record, &tags, &refs);
                    uow.search_index().upsert(&document)?;
                } else if bundled_software {
                    let record = uow
                        .software()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("software record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    let document = crate::application::projection::project_software(
                        asset, &record, &tags, &refs,
                    );
                    uow.search_index().upsert(&document)?;
                } else if bundled_service {
                    let record = uow
                        .services()
                        .get(asset.id)?
                        .ok_or_else(|| AppError::not_found("service record", asset.id))?;
                    let tags = uow.tags().list_for_asset(asset.id)?;
                    let refs = uow.external_refs().list_for_asset(asset.id)?;
                    let document = crate::application::projection::project_service(
                        asset, &record, &tags, &refs,
                    );
                    uow.search_index().upsert(&document)?;
                } else {
                    uow.search_index().remove(asset.id)?;
                }
            }

            Ok(report)
        })
    }
}

fn preflight(bundle: &PortableBundle) -> AppResult<DecodedBundle> {
    let manifest = &bundle.manifest;
    if manifest.format != EXPORT_FORMAT {
        return Err(AppError::unsupported_schema_version(
            "portable export format",
            manifest.format.as_str(),
            EXPORT_FORMAT,
        ));
    }
    if manifest.version != EXPORT_VERSION {
        return Err(AppError::unsupported_schema_version(
            "portable export format",
            manifest.version,
            format!("version {EXPORT_VERSION}"),
        ));
    }
    match manifest.modules.get("media") {
        Some(module) if module.schema_version == MEDIA_SCHEMA_VERSION => {}
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "media module",
                module.schema_version,
                format!("schema_version {MEDIA_SCHEMA_VERSION}"),
            ))
        }
        None => {
            return Err(AppError::validation(
                "bundle is missing the media module data",
            ))
        }
    }

    // Software module: declared → required and authoritative; undeclared →
    // a legacy (Phase 1) bundle predating Software, whose section is empty.
    let software_declared = match manifest.modules.get("software") {
        Some(module) if module.schema_version == SOFTWARE_SCHEMA_VERSION => true,
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "software module",
                module.schema_version,
                format!("schema_version {SOFTWARE_SCHEMA_VERSION}"),
            ))
        }
        None => false,
    };
    // Services module: same declaration contract (docs/10 "Portable data").
    // Declared → required, count-checked, and authoritative; undeclared → a
    // bundle predating Services, which must not erase destination service
    // state; a section file without the declaration is corruption.
    let services_declared = match manifest.modules.get("services") {
        Some(module) if module.schema_version == SERVICES_SCHEMA_VERSION => true,
        Some(module) => {
            return Err(AppError::unsupported_schema_version(
                "services module",
                module.schema_version,
                format!("schema_version {SERVICES_SCHEMA_VERSION}"),
            ))
        }
        None => false,
    };
    let relations_declared = manifest.record_counts.contains_key("relations");

    // Core files: a v1 bundle declares its full core shape; missing files
    // are corruption, not empty collections. Undeclared module sections
    // must NOT have a file present.
    let mut file_content = HashMap::new();
    for path in V1_CORE_FILE_PATHS {
        let content = bundle.file(path).ok_or_else(|| {
            AppError::validation(format!("bundle is missing required file {path:?}"))
        })?;
        file_content.insert(path, content);
    }
    let media_content = bundle.file(V1_MEDIA_FILE_PATH).ok_or_else(|| {
        AppError::validation(format!(
            "bundle is missing required file {V1_MEDIA_FILE_PATH:?}"
        ))
    })?;
    file_content.insert(V1_MEDIA_FILE_PATH, media_content);
    if software_declared {
        let content = bundle.file(V1_SOFTWARE_FILE_PATH).ok_or_else(|| {
            AppError::validation(format!(
                "bundle declares the software module but is missing required file \
                 {V1_SOFTWARE_FILE_PATH:?}"
            ))
        })?;
        file_content.insert(V1_SOFTWARE_FILE_PATH, content);
    } else if bundle.file(V1_SOFTWARE_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains modules/software.jsonl but its manifest does not declare \
             the software module",
        ));
    }
    if services_declared {
        let content = bundle.file(V1_SERVICES_FILE_PATH).ok_or_else(|| {
            AppError::validation(format!(
                "bundle declares the services module but is missing required file \
                 {V1_SERVICES_FILE_PATH:?}"
            ))
        })?;
        file_content.insert(V1_SERVICES_FILE_PATH, content);
    } else if bundle.file(V1_SERVICES_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains modules/services.jsonl but its manifest does not declare \
             the services module",
        ));
    }
    if relations_declared {
        let content = bundle.file(V1_RELATIONS_FILE_PATH).ok_or_else(|| {
            AppError::validation(format!(
                "bundle declares relations but is missing required file \
                 {V1_RELATIONS_FILE_PATH:?}"
            ))
        })?;
        file_content.insert(V1_RELATIONS_FILE_PATH, content);
    } else if bundle.file(V1_RELATIONS_FILE_PATH).is_some() {
        return Err(AppError::validation(
            "bundle contains relations.jsonl but its manifest does not declare a \
             relations count",
        ));
    }

    // Decode + verify declared record counts (catches silent truncation).
    let count_of = |name: &str| -> AppResult<usize> {
        manifest.record_counts.get(name).copied().ok_or_else(|| {
            AppError::validation(format!("manifest is missing record count for {name:?}"))
        })
    };

    let asset_rows: Vec<PortableAssetV1> =
        decode_jsonl(file_content["assets.jsonl"], "assets.jsonl")?;
    if asset_rows.len() != count_of("assets")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} assets, found {}",
            count_of("assets")?,
            asset_rows.len()
        )));
    }
    let media_rows: Vec<PortableMediaRecordV1> =
        decode_jsonl(file_content["modules/media.jsonl"], "modules/media.jsonl")?;
    if media_rows.len() != count_of("media")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} media records, found {}",
            count_of("media")?,
            media_rows.len()
        )));
    }
    let software_rows: Vec<PortableSoftwareRecordV1> = if software_declared {
        let rows = decode_jsonl(
            file_content["modules/software.jsonl"],
            "modules/software.jsonl",
        )?;
        if rows.len() != count_of("software")? {
            return Err(AppError::validation(format!(
                "bundle count mismatch: manifest declares {} software records, found {}",
                count_of("software")?,
                rows.len()
            )));
        }
        rows
    } else {
        Vec::new()
    };
    let service_rows: Vec<PortableServiceRecordV1> = if services_declared {
        let rows = decode_jsonl(
            file_content["modules/services.jsonl"],
            "modules/services.jsonl",
        )?;
        if rows.len() != count_of("services")? {
            return Err(AppError::validation(format!(
                "bundle count mismatch: manifest declares {} service records, found {}",
                count_of("services")?,
                rows.len()
            )));
        }
        rows
    } else {
        Vec::new()
    };
    let relation_rows: Vec<PortableRelationV1> = if relations_declared {
        let rows = decode_jsonl(file_content["relations.jsonl"], "relations.jsonl")?;
        if rows.len() != count_of("relations")? {
            return Err(AppError::validation(format!(
                "bundle count mismatch: manifest declares {} relations, found {}",
                count_of("relations")?,
                rows.len()
            )));
        }
        rows
    } else {
        Vec::new()
    };
    let ref_rows: Vec<PortableExternalRefV1> =
        decode_jsonl(file_content["external_refs.jsonl"], "external_refs.jsonl")?;
    if ref_rows.len() != count_of("external_refs")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} external refs, found {}",
            count_of("external_refs")?,
            ref_rows.len()
        )));
    }
    let activity_rows: Vec<PortableActivityEventV1> =
        decode_jsonl(file_content["activity.jsonl"], "activity.jsonl")?;
    if activity_rows.len() != count_of("activity")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} activity events, found {}",
            count_of("activity")?,
            activity_rows.len()
        )));
    }
    let tag_rows: Vec<PortableTagV1> =
        serde_json::from_str(file_content["tags.json"]).map_err(json_error)?;
    if tag_rows.len() != count_of("tags")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} tags, found {}",
            count_of("tags")?,
            tag_rows.len()
        )));
    }
    let asset_tag_rows: Vec<AssetTagRow> =
        decode_jsonl(file_content["asset_tags.jsonl"], "asset_tags.jsonl")?;
    if asset_tag_rows.len() != count_of("asset_tags")? {
        return Err(AppError::validation(format!(
            "bundle count mismatch: manifest declares {} asset tags, found {}",
            count_of("asset_tags")?,
            asset_tag_rows.len()
        )));
    }

    // Convert to domain and validate each row.
    let mut assets = Vec::with_capacity(asset_rows.len());
    for row in asset_rows {
        let asset = row.into_domain()?;
        asset
            .validate()
            .map_err(|e| AppError::validation(format!("bundle asset {}: {e}", asset.id)))?;
        assets.push(asset);
    }
    let mut media = Vec::with_capacity(media_rows.len());
    for row in media_rows {
        let record = row.into_domain()?;
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle media record {}: {e}", record.asset_id))
        })?;
        media.push(record);
    }
    let mut software = Vec::with_capacity(software_rows.len());
    for row in software_rows {
        let mut record = row.into_domain()?;
        // Normalize in place: trims free text and rejects control characters
        // (including user-owned purpose/notes) before the value is stored, so
        // a bundle cannot persist raw unnormalized text into canonical state.
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle software record {}: {e}", record.asset_id))
        })?;
        software.push(record);
    }
    let mut services = Vec::with_capacity(service_rows.len());
    for row in service_rows {
        let mut record = row.into_domain()?;
        // The same single canonicalization point as every other write path:
        // text normalization, URL shape, domain-name rules, money pairing, and
        // currency form are all enforced here, before any mutation.
        record.validate().map_err(|e| {
            AppError::validation(format!("bundle service record {}: {e}", record.asset_id))
        })?;
        services.push(record);
    }
    let mut relations = Vec::with_capacity(relation_rows.len());
    for row in relation_rows {
        let relation = row.into_domain()?;
        relation
            .validate()
            .map_err(|e| AppError::validation(format!("bundle relation {}: {e}", relation.id)))?;
        // Normalize to canonical storage form so a fact has exactly one
        // representation (mirrors the application-layer write path): inverse
        // pair types collapse onto their primary direction and symmetric
        // endpoints are ordered. The later duplicate-triple check therefore
        // also rejects bundles carrying both statements of one fact.
        let relation = relation.canonical_form();
        relations.push(relation);
    }
    let mut refs = Vec::with_capacity(ref_rows.len());
    for row in ref_rows {
        let reference = row.into_domain()?;
        reference.validate().map_err(|e| {
            AppError::validation(format!("bundle external ref {}: {e}", reference.id))
        })?;
        refs.push(reference);
    }
    let mut activity = Vec::with_capacity(activity_rows.len());
    for row in activity_rows {
        let event = row.into_domain()?;
        event.validate()?;
        activity.push(event);
    }
    let mut tags = Vec::with_capacity(tag_rows.len());
    for row in tag_rows {
        let tag = row.into_domain()?;
        tag.validate()
            .map_err(|e| AppError::validation(format!("bundle tag {}: {e}", tag.id)))?;
        tags.push(tag);
    }

    // Uniqueness of identities.
    let mut asset_ids = HashSet::new();
    for asset in &assets {
        if !asset_ids.insert(asset.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate asset id {}",
                asset.id
            )));
        }
    }
    let mut media_ids = HashSet::new();
    for record in &media {
        if !media_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate media record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut software_ids = HashSet::new();
    for record in &software {
        if !software_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate software record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut service_ids = HashSet::new();
    for record in &services {
        if !service_ids.insert(record.asset_id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate service record for asset {}",
                record.asset_id
            )));
        }
    }
    let mut relation_ids = HashSet::new();
    let mut relation_triples = HashSet::new();
    for relation in &relations {
        if !relation_ids.insert(relation.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate relation id {}",
                relation.id
            )));
        }
        let triple = (
            relation.source_asset_id,
            relation.target_asset_id,
            relation.relation_type,
        );
        if !relation_triples.insert(triple) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate relation {} between {} and {}",
                relation.relation_type, relation.source_asset_id, relation.target_asset_id
            )));
        }
    }
    let mut ref_keys = HashSet::new();
    let mut ref_ids = HashSet::new();
    for reference in &refs {
        if !ref_keys.insert((reference.namespace.clone(), reference.external_id.clone())) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate external ref {}:{}",
                reference.namespace, reference.external_id
            )));
        }
        if !ref_ids.insert(reference.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate external ref id {}",
                reference.id
            )));
        }
    }
    let mut event_ids = HashSet::new();
    for event in &activity {
        if !event_ids.insert(event.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate activity event id {}",
                event.id
            )));
        }
    }
    let mut tag_ids = HashSet::new();
    let mut tag_names = HashSet::new();
    for tag in &tags {
        if !tag_ids.insert(tag.id) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate tag id {}",
                tag.id
            )));
        }
        if !tag_names.insert(tag.name.clone()) {
            return Err(AppError::validation(format!(
                "bundle contains duplicate tag name {:?}",
                tag.name
            )));
        }
    }

    // Cross-record graph invariants.
    for record in &media {
        let asset = assets
            .iter()
            .find(|a| a.id == record.asset_id)
            .ok_or_else(|| {
                AppError::validation(format!(
                    "bundle media record references missing asset {}",
                    record.asset_id
                ))
            })?;
        if asset.kind != record.media_type.asset_kind() {
            return Err(AppError::validation(format!(
                "bundle asset {} has kind {} but its media record is a {}",
                asset.id, asset.kind, record.media_type
            )));
        }
    }
    for record in &software {
        let asset = assets
            .iter()
            .find(|a| a.id == record.asset_id)
            .ok_or_else(|| {
                AppError::validation(format!(
                    "bundle software record references missing asset {}",
                    record.asset_id
                ))
            })?;
        if asset.kind != record.category.asset_kind() {
            return Err(AppError::validation(format!(
                "bundle asset {} has kind {} but its software record is a {}",
                asset.id, asset.kind, record.category
            )));
        }
    }
    for record in &services {
        let asset = assets
            .iter()
            .find(|a| a.id == record.asset_id)
            .ok_or_else(|| {
                AppError::validation(format!(
                    "bundle service record references missing asset {}",
                    record.asset_id
                ))
            })?;
        // Kind ↔ type is 1:1 (docs/10): a bundle cannot attach a saas record
        // to a service.api asset, or any service record to a media/software
        // asset. The repository boundary enforces this too, but preflight must
        // fail identically for dry-run and commit, before any mutation.
        if asset.kind != record.service_type.asset_kind() {
            return Err(AppError::validation(format!(
                "bundle asset {} has kind {} but its service record is a {}",
                asset.id, asset.kind, record.service_type
            )));
        }
        // A merged identity carries no module detail (docs/10): the portable
        // contract has no "re-home this row onto the winner" rule, so a service
        // row on a tombstone is bundle corruption — not something import may
        // silently keep (stranding detail on a dead identity) or silently drop
        // (losing a fact the caller meant to move). The merge use case always
        // removes the loser's record, so a well-formed export never emits one.
        if asset.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
            return Err(AppError::validation(format!(
                "bundle service record {} is attached to merged asset {}; a merged identity \
                 carries no service detail",
                record.asset_id, asset.id
            )));
        }
    }
    for relation in &relations {
        if !asset_ids.contains(&relation.source_asset_id) {
            return Err(AppError::validation(format!(
                "bundle relation {} references missing source asset {}",
                relation.id, relation.source_asset_id
            )));
        }
        if !asset_ids.contains(&relation.target_asset_id) {
            return Err(AppError::validation(format!(
                "bundle relation {} references missing target asset {}",
                relation.id, relation.target_asset_id
            )));
        }
    }
    for reference in &refs {
        if !asset_ids.contains(&reference.asset_id) {
            return Err(AppError::validation(format!(
                "bundle external ref {}:{} references missing asset {}",
                reference.namespace, reference.external_id, reference.asset_id
            )));
        }
    }
    for event in &activity {
        if let Some(asset_id) = event.asset_id {
            if !asset_ids.contains(&asset_id) {
                return Err(AppError::validation(format!(
                    "bundle activity event {} references missing asset {}",
                    event.id, asset_id
                )));
            }
        }
    }
    for pair in &asset_tag_rows {
        let asset_id = AssetId::from_uuid(id_from_wire(&pair.asset_id, "asset_tags")?);
        let tag_id = TagId::from_uuid(id_from_wire(&pair.tag_id, "asset_tags")?);
        if !asset_ids.contains(&asset_id) {
            return Err(AppError::validation(format!(
                "bundle asset_tags references missing asset {}",
                pair.asset_id
            )));
        }
        if !tag_ids.contains(&tag_id) {
            return Err(AppError::validation(format!(
                "bundle asset_tags references missing tag {}",
                pair.tag_id
            )));
        }
    }

    // Merge redirects: target must exist, never self, never cyclic.
    for asset in &assets {
        let Some(target) = asset.merged_into else {
            continue;
        };
        if target == asset.id {
            return Err(AppError::validation(format!(
                "bundle asset {} is merged into itself",
                asset.id
            )));
        }
        if !asset_ids.contains(&target) {
            return Err(AppError::validation(format!(
                "bundle asset {} is merged into missing asset {}",
                asset.id, target
            )));
        }
        // Walk the redirect chain; a bundle must not contain cycles.
        let mut seen = HashSet::new();
        let mut cursor = asset.id;
        while let Some(next) = assets
            .iter()
            .find(|a| a.id == cursor)
            .and_then(|a| a.merged_into)
        {
            if !seen.insert(cursor) {
                return Err(AppError::validation(format!(
                    "bundle contains a merge redirect cycle at asset {}",
                    cursor
                )));
            }
            cursor = next;
        }
    }

    Ok(DecodedBundle {
        assets,
        media,
        software,
        services,
        refs,
        activity,
        tags,
        asset_tags: asset_tag_rows,
        relations,
        software_declared,
        services_declared,
        relations_declared,
    })
}

fn decode_jsonl<T: serde::de::DeserializeOwned>(text: &str, path: &str) -> AppResult<Vec<T>> {
    let mut rows = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        rows.push(serde_json::from_str(line).map_err(|e| {
            AppError::validation(format!(
                "bundle file {path} line {} is invalid: {e}",
                line_no + 1
            ))
        })?);
    }
    Ok(rows)
}
