//! Portable export format V1 (docs/05-storage-and-portability).
//!
//! The portable bundle is a product contract, independent of the physical
//! SQLite schema AND of the Rust domain structs: dedicated `*V1` DTOs freeze
//! the wire format so internal refactors cannot silently change version 1.
//! Derived state (search projection), provider cache, and secrets are
//! excluded.
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
//! └── modules/
//!     └── media.jsonl
//! ```
//!
//! Every import (dry-run or commit) runs the same preflight: declared files
//! must be present, decoded row counts must match the manifest, identities
//! must be unique, and the reference graph (module details, refs,
//! memberships, activity, merge redirects) must be internally consistent —
//! a damaged bundle can never restore "successfully" while omitting records.
//! Dry-run additionally inspects the destination, so it fails exactly when
//! commit would.

use crate::application::SharedClock;
use crate::domain::activity::ActivityEvent;
use crate::domain::asset::{Asset, AssetKind, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::{ActivityId, AssetId, ExternalRefId, TagId};
use crate::domain::media::{MediaRecord, MediaType};
use crate::domain::tag::Tag;
use crate::domain::Timestamp;
use crate::ports::uow::UnitOfWorkFactory;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const EXPORT_FORMAT: &str = "assetmesh-portable-export";
pub const EXPORT_VERSION: i64 = 1;

/// Media module data schema version — owned by the Media module
/// (`domain::media::SCHEMA_VERSION`) and re-exported here for convenience.
pub use crate::domain::media::SCHEMA_VERSION as MEDIA_SCHEMA_VERSION;

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
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Canonical file paths of a V1 bundle.
pub const V1_FILE_PATHS: [&str; 6] = [
    "assets.jsonl",
    "external_refs.jsonl",
    "activity.jsonl",
    "tags.json",
    "asset_tags.jsonl",
    "modules/media.jsonl",
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
        let (assets, media, refs, activity, tags, memberships) = self.factory.read(&mut |q| {
            let assets = q.assets().list(&crate::ports::repos::AssetFilter {
                kind: None,
                lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
            })?;
            let media = q.media().list_all()?;
            let refs = q.external_refs().list_all()?;
            let activity = q.activity().list_all()?;
            let tags = q.tags().list_all()?;
            let memberships = q.tags().list_memberships()?;
            Ok((assets, media, refs, activity, tags, memberships))
        })?;

        let created_at = self.clock.now();

        let mut assets: Vec<PortableAssetV1> =
            assets.iter().map(PortableAssetV1::from_domain).collect();
        assets.sort_by(|a, b| a.id.cmp(&b.id));
        let mut media: Vec<PortableMediaRecordV1> = media
            .iter()
            .map(PortableMediaRecordV1::from_domain)
            .collect();
        media.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
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
            modules: BTreeMap::from([(
                "media".to_string(),
                ModuleVersion {
                    schema_version: MEDIA_SCHEMA_VERSION,
                },
            )]),
            record_counts: BTreeMap::from([
                ("assets".to_string(), assets.len()),
                ("external_refs".to_string(), refs.len()),
                ("activity".to_string(), activity.len()),
                ("tags".to_string(), tags.len()),
                ("asset_tags".to_string(), asset_tags_rows.len()),
                ("media".to_string(), media.len()),
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
                path: "modules/media.jsonl".into(),
                content: to_jsonl(&media)?,
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
pub fn write_bundle_to_directory(bundle: &PortableBundle, target: &Path) -> AppResult<()> {
    recover_interrupted_swap(target)?;

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
}

/// Reads a bundle from a directory, recovering any interrupted swap first so
/// a crash between the two renames can never hide the last complete bundle.
pub fn read_bundle_from_directory(source: &Path) -> AppResult<PortableBundle> {
    recover_interrupted_swap(source)?;
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
// Import
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct PortableImportReport {
    pub assets_created: usize,
    pub assets_updated: usize,
    pub media_created: usize,
    pub media_updated: usize,
    pub external_refs_created: usize,
    pub external_refs_deduplicated: usize,
    pub activity_created: usize,
    pub tags_created: usize,
}

/// Fully decoded and preflighted bundle contents.
struct DecodedBundle {
    assets: Vec<Asset>,
    media: Vec<MediaRecord>,
    refs: Vec<AssetExternalRef>,
    activity: Vec<ActivityEvent>,
    tags: Vec<Tag>,
    asset_tags: Vec<AssetTagRow>,
}

/// One consistent snapshot of the destination, taken before commit or
/// dry-run. Both compute their dispositions from the same snapshot shape, so
/// dry-run is field-for-field identical to what commit would do.
struct DestinationSnapshot {
    asset_ids: HashSet<AssetId>,
    media_asset_ids: HashSet<AssetId>,
    ref_pairs: HashMap<(String, String), AssetId>,
    ref_ids: HashMap<ExternalRefId, (String, String)>,
    activity_ids: HashSet<ActivityId>,
    tag_ids: HashSet<TagId>,
    tag_names: HashSet<String>,
}

impl DestinationSnapshot {
    fn load(q: &mut dyn crate::ports::uow::QueryUnitOfWork) -> AppResult<Self> {
        let asset_ids: HashSet<AssetId> = q
            .assets()
            .list(&crate::ports::repos::AssetFilter {
                kind: None,
                lifecycle: Some(crate::ports::repos::LifecycleFilter::All),
            })?
            .into_iter()
            .map(|a| a.id)
            .collect();
        let media_asset_ids: HashSet<AssetId> = q
            .media()
            .list_all()?
            .into_iter()
            .map(|m| m.asset_id)
            .collect();
        let mut ref_pairs = HashMap::new();
        let mut ref_ids = HashMap::new();
        for reference in q.external_refs().list_all()? {
            ref_pairs.insert(
                (reference.namespace.clone(), reference.external_id.clone()),
                reference.asset_id,
            );
            ref_ids.insert(
                reference.id,
                (reference.namespace.clone(), reference.external_id.clone()),
            );
        }
        let activity_ids: HashSet<ActivityId> =
            q.activity().list_all()?.into_iter().map(|e| e.id).collect();
        let mut tag_ids = HashSet::new();
        let mut tag_names = HashSet::new();
        for tag in q.tags().list_all()? {
            tag_ids.insert(tag.id);
            tag_names.insert(tag.name);
        }
        Ok(DestinationSnapshot {
            asset_ids,
            media_asset_ids,
            ref_pairs,
            ref_ids,
            activity_ids,
            tag_ids,
            tag_names,
        })
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
    // Judged by MediaRecord existence, not asset existence: an asset can be
    // present in the destination while its module details are not.
    for record in &decoded.media {
        if snapshot.media_asset_ids.contains(&record.asset_id) {
            report.media_updated += 1;
        } else {
            report.media_created += 1;
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

        // One consistent destination snapshot drives checks and counts.
        let snapshot = self.factory.read(&mut |q| DestinationSnapshot::load(q))?;
        check_destination(&decoded, &snapshot)?;

        if dry_run {
            return Ok(dispositions(&decoded, &snapshot));
        }

        // Commit. Assets first (self-referencing merged_into is resolved in
        // a second pass), then module details, refs, tags, activity, and
        // finally module-state reconciliation + projections.
        self.factory.transact(&mut |uow| {
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
            // different id remaps membership onto the existing tag.
            let mut tag_id_map: BTreeMap<String, TagId> = BTreeMap::new();
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
                tag_id_map.insert(tag.id.to_string(), resolved);
            }

            for pair in &decoded.asset_tags {
                let asset_id = AssetId::from_uuid(id_from_wire(&pair.asset_id, "asset_tags")?);
                let tag_id = tag_id_map.get(&pair.tag_id).copied().ok_or_else(|| {
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

            // Module-state reconciliation: the bundle is authoritative for
            // every asset it contains. An asset without module details in
            // the bundle (typically a merged tombstone) must not keep stale
            // destination details or a search document; a details-bearing
            // asset is projected from its ACTUAL committed state (post-remap
            // tags, destination refs), not from the bundle subset.
            for asset in &decoded.assets {
                match decoded.media.iter().find(|m| m.asset_id == asset.id) {
                    None => {
                        uow.media().delete(asset.id)?;
                        uow.search_index().remove(asset.id)?;
                    }
                    Some(_record) => {
                        if asset.lifecycle_state == crate::domain::asset::LifecycleState::Merged {
                            uow.search_index().remove(asset.id)?;
                        } else {
                            let record = uow
                                .media()
                                .get(asset.id)?
                                .ok_or_else(|| AppError::not_found("media record", asset.id))?;
                            let tags = uow.tags().list_for_asset(asset.id)?;
                            let refs = uow.external_refs().list_for_asset(asset.id)?;
                            let document = crate::application::projection::project_media(
                                asset, &record, &tags, &refs,
                            );
                            uow.search_index().upsert(&document)?;
                        }
                    }
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

    // Required files: a v1 bundle declares its full shape; missing files are
    // corruption, not empty collections.
    let mut file_content = HashMap::new();
    for path in V1_FILE_PATHS {
        let content = bundle.file(path).ok_or_else(|| {
            AppError::validation(format!("bundle is missing required file {path:?}"))
        })?;
        file_content.insert(path, content);
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
        refs,
        activity,
        tags,
        asset_tags: asset_tag_rows,
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
