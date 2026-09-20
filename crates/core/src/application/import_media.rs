//! Legacy media import pipeline (docs/08 import contract, ADR 0005).
//!
//! parse → validate/normalize → match → plan → review (dry run) → commit →
//! report. Matching precedence:
//!
//! 1. canonical AssetMesh ID (for AssetMesh-native data);
//! 2. exact namespaced external reference;
//! 3. normalized title + media type + year — reported as a **potential
//!    duplicate conflict**, never auto-applied (ADR 0005 classifies
//!    title/type/year matching as heuristic; only canonical IDs and exact
//!    external references may update canonical data in Media V1);
//! 4. heuristic (normalized title + media type) — same: reported, never
//!    written.
//!
//! External references are kept ownerless until commit and re-pointed at the
//! matched/created asset on both the create and update paths, so planning and
//! commit stay symmetric. The commit phase writes in bounded transaction
//! batches; if a batch fails, the report discloses exactly what was already
//! committed. Parsing and matching happen entirely outside write
//! transactions.

use crate::application::import_parse::{
    detect_format, normalize_title, parse_csv, parse_json, parse_timestamp, ImportCandidate,
    ImportFormat, ParsedRow, PendingExternalRef, RawMediaInput,
};
use crate::application::media_service::update_projection;
use crate::application::shared::{ensure_ref_available, normalize_tags};
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::media::{MediaRecord, MediaStatus, MediaType, Progress};
use crate::ports::repos::{AssetFilter, LifecycleFilter};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// Records per committed transaction batch (ADR 0007: bounded batches).
const BATCH_SIZE: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectedRecord {
    pub index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportConflict {
    pub index: usize,
    pub title: String,
    pub candidate_asset_id: Option<AssetId>,
    pub reason: String,
}

/// Disclosure of a partial import: batches already committed stay committed,
/// the remaining records were not applied.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportFailure {
    pub error: String,
    pub records_committed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportReport {
    pub input: usize,
    pub valid: usize,
    pub create: usize,
    pub update: usize,
    pub unchanged: usize,
    pub potential_duplicates: usize,
    pub rejected: usize,
    pub conflicts: Vec<ImportConflict>,
    pub rejected_records: Vec<RejectedRecord>,
    pub dry_run: bool,
    /// Set when a commit batch failed mid-run: earlier batches are already
    /// committed (see `records_committed`), later batches were not applied.
    pub failed: Option<ImportFailure>,
}

#[derive(Debug, Clone)]
pub enum ImportFormatHint {
    Auto,
    Json,
    Csv,
}

pub struct MediaImportService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> MediaImportService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        MediaImportService {
            factory,
            clock,
            ids,
        }
    }

    /// Imports media records from file content. With `dry_run` nothing is
    /// written and the report shows what *would* happen.
    pub fn import(
        &mut self,
        content: &str,
        format: ImportFormatHint,
        dry_run: bool,
    ) -> AppResult<ImportReport> {
        let format = match format {
            ImportFormatHint::Auto => detect_format(content, None),
            ImportFormatHint::Json => ImportFormat::Json,
            ImportFormatHint::Csv => ImportFormat::Csv,
        };

        // Parse — outside any transaction.
        let parsed = match format {
            ImportFormat::Json => parse_json(content)?,
            ImportFormat::Csv => parse_csv(content)?,
        };

        let now = self.clock.now();
        let ids = self.ids.clone();

        // Validate + normalize.
        let mut candidates: Vec<ImportCandidate> = Vec::new();
        let mut rejected_records = Vec::new();
        for row in parsed {
            match row {
                ParsedRow::Malformed { index, reason } => {
                    rejected_records.push(RejectedRecord { index, reason });
                }
                ParsedRow::Raw { index, raw } => match normalize_candidate(*raw, index, now) {
                    Ok(candidate) => candidates.push(candidate),
                    Err(reason) => rejected_records.push(RejectedRecord { index, reason }),
                },
            }
        }

        // Match + plan — read-only, outside any write transaction. Assets
        // created or updated by this run are recorded in the match index as
        // they are planned, so later rows resolve against them consistently.
        let candidate_count = candidates.len();
        let mut candidates_iter = candidates.into_iter();
        let (mut plan, conflicts) = self.factory.read(&mut |q| {
            let mut index = MatchIndex::build(q)?;
            let mut plan = Vec::with_capacity(candidate_count);
            let mut conflicts = Vec::new();
            for mut candidate in candidates_iter.by_ref() {
                let action = index.match_candidate(&candidate);
                match &action {
                    PlannedAction::Create => {
                        let planned_id = AssetId::from_uuid(ids.new_id());
                        candidate.planned_asset_id = Some(planned_id);
                        index.record_planned_create(&candidate, planned_id);
                    }
                    PlannedAction::Update { asset_id, .. } => {
                        index.record_planned_update(&candidate, *asset_id);
                    }
                    PlannedAction::Conflict {
                        reason,
                        candidate: candidate_id,
                    } => {
                        conflicts.push(ImportConflict {
                            index: candidate.index,
                            title: candidate.title.clone(),
                            candidate_asset_id: *candidate_id,
                            reason: reason.clone(),
                        });
                    }
                }
                plan.push((candidate, action));
            }
            Ok((plan, conflicts))
        })?;

        let create = plan
            .iter()
            .filter(|(_, a)| matches!(a, PlannedAction::Create))
            .count();
        let update = plan
            .iter()
            .filter(|(_, a)| matches!(a, PlannedAction::Update { .. }))
            .count();

        let mut report = ImportReport {
            input: plan.len() + rejected_records.len(),
            valid: plan.len(),
            create,
            update,
            unchanged: 0,
            potential_duplicates: conflicts.len(),
            rejected: rejected_records.len(),
            conflicts,
            rejected_records,
            dry_run,
            failed: None,
        };

        if dry_run {
            return Ok(report);
        }

        // Commit in bounded batches. If a batch fails, earlier committed
        // batches remain and the report discloses that state explicitly
        // instead of hiding partial work behind a bare error.
        let mut committed = (0usize, 0usize, 0usize);
        for chunk in plan.chunks_mut(BATCH_SIZE) {
            match self.commit_chunk(chunk, now) {
                Ok((c, u, n)) => committed = (committed.0 + c, committed.1 + u, committed.2 + n),
                Err(error) => {
                    report.failed = Some(ImportFailure {
                        error: error.to_string(),
                        records_committed: committed.0 + committed.1 + committed.2,
                    });
                    break;
                }
            }
        }

        report.create = committed.0;
        report.update = committed.1;
        report.unchanged = committed.2;
        Ok(report)
    }

    /// Applies one batch of planned records in a single short transaction.
    /// Returns (creates, updates, unchanged).
    fn commit_chunk(
        &mut self,
        chunk: &mut [(ImportCandidate, PlannedAction)],
        now: crate::domain::Timestamp,
    ) -> AppResult<(usize, usize, usize)> {
        let MediaImportService { factory, ids, .. } = self;
        let mut creates = 0usize;
        let mut updates = 0usize;
        let mut unchanged = 0usize;

        factory.transact(&mut |uow| {
            for (candidate, action) in chunk.iter_mut() {
                match action {
                    PlannedAction::Create => {
                        commit_create(uow, ids, candidate, now)?;
                        creates += 1;
                    }
                    PlannedAction::Update {
                        asset_id,
                        matched_by,
                    } => {
                        let changed = commit_update(uow, candidate, *asset_id, matched_by, now)?;
                        if changed {
                            updates += 1;
                        } else {
                            unchanged += 1;
                        }
                    }
                    PlannedAction::Conflict { .. } => {
                        // Heuristic/ambiguous matches are never committed.
                    }
                }
            }
            Ok(())
        })?;

        Ok((creates, updates, unchanged))
    }
}

fn commit_create(
    uow: &mut dyn UnitOfWork,
    ids: &SharedIdGenerator,
    candidate: &mut ImportCandidate,
    now: crate::domain::Timestamp,
) -> AppResult<()> {
    let asset_id = candidate
        .planned_asset_id
        .unwrap_or_else(|| AssetId::from_uuid(ids.new_id()));
    candidate.planned_asset_id = Some(asset_id);

    let status = candidate.status.unwrap_or(MediaStatus::Planned);
    let mut record = MediaRecord::new(asset_id, candidate.media_type, status);
    record.rating = candidate.rating;
    record.year = candidate.year;
    record.platform = candidate.platform.clone();
    record.progress = candidate.progress.clone();
    record.notes = candidate.notes.clone();
    record.started_at = candidate.started_at;
    record.completed_at = candidate.completed_at;
    record.validate()?;

    let asset = Asset::new(
        asset_id,
        candidate.media_type.asset_kind(),
        candidate.title.clone(),
        candidate.summary.clone(),
        now,
    )?;

    // Ownerless refs become real references owned by the created asset.
    let mut owned_refs = Vec::with_capacity(candidate.refs.len());
    for reference in &candidate.refs {
        let owned = AssetExternalRef::new(
            asset_id,
            reference.namespace.clone(),
            reference.external_id.clone(),
            reference.source_url.clone(),
            now,
        );
        owned.validate()?;
        ensure_ref_available(uow, &owned)?;
        owned_refs.push(owned);
    }

    uow.assets().insert(&asset)?;
    uow.media().upsert(&record)?;

    let mut tags = Vec::new();
    for name in &candidate.tags {
        let tag = uow.tags().ensure(name)?;
        uow.tags().attach(asset_id, tag.id)?;
        tags.push(tag);
    }
    for reference in &owned_refs {
        uow.external_refs().insert(reference)?;
    }

    uow.activity().append(&ActivityEvent::new(
        event_types::ASSET_CREATED,
        Some(asset_id),
        actors::IMPORT,
        json!({ "kind": asset.kind.as_str(), "name": asset.name.clone() }),
        now,
    ))?;
    uow.activity().append(&ActivityEvent::new(
        event_types::MEDIA_CREATED,
        Some(asset_id),
        actors::IMPORT,
        json!({
            "media_type": record.media_type.as_str(),
            "status": record.status.as_str(),
        }),
        now,
    ))?;

    update_projection(uow, &asset, &record)?;
    Ok(())
}

/// Applies an update plan. Field policy: imported `Some` values overwrite,
/// `None` keeps the existing value; tags and refs are unioned. Returns
/// whether anything actually changed (idempotent re-import).
fn commit_update(
    uow: &mut dyn UnitOfWork,
    candidate: &ImportCandidate,
    asset_id: AssetId,
    matched_by: &str,
    now: crate::domain::Timestamp,
) -> AppResult<bool> {
    let mut asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    asset.ensure_mutable()?;
    let mut record = uow
        .media()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("media record", asset_id))?;

    let mut changes: Vec<String> = Vec::new();

    if asset.name != candidate.title {
        asset.name = candidate.title.clone();
        changes.push("title".into());
    }
    if candidate.summary.is_some() && asset.summary != candidate.summary {
        asset.summary = candidate.summary.clone();
        changes.push("summary".into());
    }
    if let Some(year) = candidate.year {
        if record.year != Some(year) {
            record.year = Some(year);
            changes.push("year".into());
        }
    }
    if candidate.platform.is_some() && record.platform != candidate.platform {
        record.platform = candidate.platform.clone();
        changes.push("platform".into());
    }
    if candidate.notes.is_some() && record.notes != candidate.notes {
        record.notes = candidate.notes.clone();
        changes.push("notes".into());
    }
    if let Some(status) = candidate.status {
        if record.status != status {
            record.status = status;
            changes.push(format!("status:{}", status.as_str()));
        }
    }
    if candidate.rating.is_some() && record.rating != candidate.rating {
        record.rating = candidate.rating;
        changes.push("rating".into());
    }
    // Per-field progress merge; an all-None progress keeps existing data.
    if !candidate.progress.is_empty() {
        if candidate.progress.current.is_some()
            && record.progress.current != candidate.progress.current
        {
            record.progress.current = candidate.progress.current;
            changes.push("progress_current".into());
        }
        if candidate.progress.total.is_some() && record.progress.total != candidate.progress.total {
            record.progress.total = candidate.progress.total;
            changes.push("progress_total".into());
        }
        if candidate.progress.unit.is_some() && record.progress.unit != candidate.progress.unit {
            record.progress.unit = candidate.progress.unit.clone();
            changes.push("progress_unit".into());
        }
    }
    if candidate.started_at.is_some() && record.started_at != candidate.started_at {
        record.started_at = candidate.started_at;
        changes.push("started_at".into());
    }
    if candidate.completed_at.is_some() && record.completed_at != candidate.completed_at {
        record.completed_at = candidate.completed_at;
        changes.push("completed_at".into());
    }

    record.validate()?;

    let existing_tags: HashSet<String> = uow
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|t| t.name)
        .collect();
    for name in &candidate.tags {
        if !existing_tags.iter().any(|t| t.eq_ignore_ascii_case(name)) {
            let tag = uow.tags().ensure(name)?;
            uow.tags().attach(asset_id, tag.id)?;
            changes.push(format!("tag:{name}"));
        }
    }

    let existing_refs: HashSet<(String, String)> = uow
        .external_refs()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|r| (r.namespace.clone(), r.external_id.clone()))
        .collect();
    for reference in &candidate.refs {
        let key = (reference.namespace.clone(), reference.external_id.clone());
        if !existing_refs.contains(&key) {
            // Ownerless ref is owned by the matched asset on the update path
            // as well — never a placeholder ID (P1: FK-safe enrichment).
            let owned = AssetExternalRef::new(
                asset_id,
                reference.namespace.clone(),
                reference.external_id.clone(),
                reference.source_url.clone(),
                now,
            );
            owned.validate()?;
            ensure_ref_available(uow, &owned)?;
            uow.external_refs().insert(&owned)?;
            changes.push(format!(
                "ref:{}:{}",
                reference.namespace, reference.external_id
            ));
        }
    }

    if changes.is_empty() {
        return Ok(false);
    }

    asset.touch(now);
    uow.assets().update(&asset)?;
    uow.media().upsert(&record)?;
    uow.activity().append(&ActivityEvent::new(
        event_types::MEDIA_IMPORTED,
        Some(asset_id),
        actors::IMPORT,
        json!({ "matched_by": matched_by, "changes": changes }),
        now,
    ))?;
    update_projection(uow, &asset, &record)?;
    Ok(true)
}

enum PlannedAction {
    Create,
    Update {
        asset_id: AssetId,
        matched_by: &'static str,
    },
    Conflict {
        reason: String,
        candidate: Option<AssetId>,
    },
}

/// In-memory index of the live library used for deterministic matching, kept
/// up to date with planned creates and updates so records inside one batch
/// resolve consistently and duplicate reference claims are detected.
struct MatchIndex {
    active_assets: HashMap<AssetId, Asset>,
    /// Active-asset owners, used for deterministic update matching.
    by_ref: HashMap<(String, String), AssetId>,
    /// Every committed ref owner regardless of lifecycle. An external ref is
    /// globally unique, so a row claiming a ref owned by an archived or
    /// merged asset must never be planned as a create — the commit would hit
    /// the unique index.
    all_ref_owners: HashMap<(String, String), AssetId>,
    /// Normalized (title, type, year) — conflicts only (ADR 0005).
    by_key: HashMap<(String, String, Option<i32>), Vec<AssetId>>,
    /// Normalized (title, type) — heuristic conflicts.
    by_title_type: HashMap<(String, String), Vec<AssetId>>,
    /// References claimed by records planned earlier in this run, mapped to
    /// the planned owner, so duplicate claims resolve to one asset.
    planned_by_ref: HashMap<(String, String), AssetId>,
    /// Media type of every planned owner (kind-compatibility checks).
    planned_types: HashMap<AssetId, MediaType>,
}

impl MatchIndex {
    fn build(q: &mut dyn QueryUnitOfWork) -> AppResult<Self> {
        let assets = q.assets().list(&AssetFilter {
            kind: None,
            lifecycle: Some(LifecycleFilter::Active),
        })?;
        let active_assets: HashMap<AssetId, Asset> =
            assets.into_iter().map(|a| (a.id, a)).collect();
        let media = q.media().list_all()?;
        let refs = q.external_refs().list_all()?;

        let mut index = MatchIndex {
            active_assets,
            by_ref: HashMap::new(),
            all_ref_owners: HashMap::new(),
            by_key: HashMap::new(),
            by_title_type: HashMap::new(),
            planned_by_ref: HashMap::new(),
            planned_types: HashMap::new(),
        };

        for record in media {
            let asset = match index.active_assets.get(&record.asset_id) {
                Some(asset) => asset,
                None => continue,
            };
            let key = (
                normalize_title(&asset.name),
                record.media_type.as_str().to_string(),
                record.year,
            );
            index
                .by_key
                .entry(key.clone())
                .or_default()
                .push(record.asset_id);
            index
                .by_title_type
                .entry((key.0, key.1))
                .or_default()
                .push(record.asset_id);
        }

        for reference in refs {
            let key = (reference.namespace.clone(), reference.external_id.clone());
            if index.active_assets.contains_key(&reference.asset_id) {
                index.by_ref.insert(key.clone(), reference.asset_id);
            }
            // Every owner, active or not, keeps identity checks honest.
            index.all_ref_owners.insert(key, reference.asset_id);
        }

        Ok(index)
    }

    /// Resolves the committed owner of a ref, if any (active or not).
    fn committed_ref_owner(&self, key: &(String, String)) -> Option<AssetId> {
        self.by_ref
            .get(key)
            .copied()
            .or_else(|| self.all_ref_owners.get(key).copied())
    }

    /// Resolves any known owner of a ref: committed (active or not) or
    /// planned earlier in this run.
    fn known_ref_owner(&self, key: &(String, String)) -> Option<AssetId> {
        self.committed_ref_owner(key)
            .or_else(|| self.planned_by_ref.get(key).copied())
    }

    fn record_planned_create(&mut self, candidate: &ImportCandidate, asset_id: AssetId) {
        self.register_planned(candidate, asset_id);
        let key = (
            normalize_title(&candidate.title),
            candidate.media_type.as_str().to_string(),
            candidate.year,
        );
        self.by_key.entry(key.clone()).or_default().push(asset_id);
        self.by_title_type
            .entry((key.0, key.1))
            .or_default()
            .push(asset_id);
    }

    fn record_planned_update(&mut self, candidate: &ImportCandidate, asset_id: AssetId) {
        self.register_planned(candidate, asset_id);
    }

    fn register_planned(&mut self, candidate: &ImportCandidate, asset_id: AssetId) {
        self.planned_types.insert(asset_id, candidate.media_type);
        for reference in &candidate.refs {
            let key = (reference.namespace.clone(), reference.external_id.clone());
            // Only claim unclaimed refs; a differing claim would mean the
            // row should have been a conflict (checked in match_candidate).
            self.planned_by_ref.entry(key).or_insert(asset_id);
        }
    }

    fn kind_of(&self, asset_id: AssetId) -> Option<MediaType> {
        if let Some(asset) = self.active_assets.get(&asset_id) {
            return MediaType::from_asset_kind(asset.kind);
        }
        self.planned_types.get(&asset_id).copied()
    }

    fn match_candidate(&mut self, candidate: &ImportCandidate) -> PlannedAction {
        // 1. canonical AssetMesh ID. The row's refs must all agree with the
        //    named target — a ref owned by a different asset (committed or
        //    planned) would fail at commit, so dry-run must reject it too.
        if let Some(id) = candidate.asset_id {
            let target_is_active = self.active_assets.contains_key(&id);
            if !target_is_active {
                return PlannedAction::Conflict {
                    reason: format!(
                        "canonical asset id {id} was given but no active asset has this id"
                    ),
                    candidate: None,
                };
            }
            if let Some(asset) = self.active_assets.get(&id) {
                if let Some(reason) = kind_mismatch(asset.kind, candidate) {
                    return PlannedAction::Conflict {
                        reason,
                        candidate: Some(id),
                    };
                }
            }
            for reference in &candidate.refs {
                let key = (reference.namespace.clone(), reference.external_id.clone());
                if let Some(owner) = self.known_ref_owner(&key) {
                    if owner != id {
                        return PlannedAction::Conflict {
                            reason: format!(
                                "record names canonical asset {id} but its ref {}:{} belongs to asset {}",
                                reference.namespace, reference.external_id, owner
                            ),
                            // The ref owner is the reviewable candidate.
                            candidate: Some(owner),
                        };
                    }
                }
            }
            return PlannedAction::Update {
                asset_id: id,
                matched_by: "canonical_id",
            };
        }

        // 2. exact external references — committed assets first, then refs
        //    claimed by records planned earlier in this run. Ownership is
        //    checked across ALL lifecycles: a ref held by an archived or
        //    merged asset is an explicit conflict, never a silent create.
        let mut matched: Option<AssetId> = None;
        for reference in &candidate.refs {
            let key = (reference.namespace.clone(), reference.external_id.clone());
            if let Some(asset_id) = self.known_ref_owner(&key) {
                match matched {
                    None => matched = Some(asset_id),
                    Some(prev) if prev != asset_id => {
                        return PlannedAction::Conflict {
                            reason: "external references resolve to different assets".into(),
                            candidate: Some(prev),
                        };
                    }
                    _ => {}
                }
            }
        }
        if let Some(asset_id) = matched {
            if !self.active_assets.contains_key(&asset_id)
                && !self.planned_types.contains_key(&asset_id)
            {
                return PlannedAction::Conflict {
                    reason: format!(
                        "external ref belongs to a non-active (archived or merged) asset {asset_id}; resolve explicitly instead of importing"
                    ),
                    candidate: Some(asset_id),
                };
            }
            match self.kind_of(asset_id) {
                Some(kind) if kind != candidate.media_type => {
                    return PlannedAction::Conflict {
                        reason: kind_mismatch_message(kind, candidate),
                        candidate: Some(asset_id),
                    };
                }
                _ => {}
            }
            return PlannedAction::Update {
                asset_id,
                matched_by: "external_ref",
            };
        }

        // 3. deterministic normalized key (title + type + year): ADR 0005
        //    classifies this as heuristic similarity, so it is surfaced as a
        //    reviewable potential duplicate instead of silently overwriting
        //    the matched asset's canonical data.
        let key = (
            normalize_title(&candidate.title),
            candidate.media_type.as_str().to_string(),
            candidate.year,
        );
        if let Some(matches) = self.by_key.get(&key) {
            let mut unique: Vec<AssetId> = matches.clone();
            unique.dedup();
            if !unique.is_empty() {
                let reason = if unique.len() == 1 {
                    "potential duplicate: same normalized title, media type and year as an existing asset (normalized-key match requires review)"
                        .to_string()
                } else {
                    "potential duplicate: normalized key matches multiple existing assets"
                        .to_string()
                };
                return PlannedAction::Conflict {
                    reason,
                    candidate: Some(unique[0]),
                };
            }
        }

        // 4. heuristic: normalized title + type only — never auto-applied.
        let title_type = (key.0.clone(), key.1.clone());
        if let Some(matches) = self.by_title_type.get(&title_type) {
            if let Some(&first) = matches.first() {
                return PlannedAction::Conflict {
                    reason:
                        "potential duplicate: same normalized title and media type with a different or missing year"
                            .into(),
                    candidate: Some(first),
                };
            }
        }

        PlannedAction::Create
    }
}

fn kind_mismatch(
    kind: crate::domain::asset::AssetKind,
    candidate: &ImportCandidate,
) -> Option<String> {
    if kind != candidate.media_type.asset_kind() {
        return Some(kind_mismatch_message(
            MediaType::from_asset_kind(kind)?,
            candidate,
        ));
    }
    None
}

fn kind_mismatch_message(kind: MediaType, candidate: &ImportCandidate) -> String {
    format!(
        "match targets an asset of kind {}, but the record is a {}",
        kind.asset_kind(),
        candidate.media_type
    )
}

/// Validates and normalizes one raw record into a candidate. Returns the
/// rejection reason on failure.
fn normalize_candidate(
    raw: RawMediaInput,
    index: usize,
    _now: crate::domain::Timestamp,
) -> Result<ImportCandidate, String> {
    let title = raw
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("record {index} is missing a title"))?;

    let media_type = raw
        .media_type
        .as_deref()
        .and_then(MediaType::parse)
        .ok_or_else(|| {
            format!(
                "record {index} has an unsupported media_type {:?} (expected movie, tv, anime, game)",
                raw.media_type
            )
        })?;

    let status = match raw.status.as_deref() {
        None => None,
        Some(s) => Some(
            MediaStatus::parse(s)
                .ok_or_else(|| format!("record {index} has an unknown status {s:?}"))?,
        ),
    };

    let started_at = match raw.started_at.as_deref() {
        None => None,
        Some(s) => Some(
            parse_timestamp(s)
                .ok_or_else(|| format!("record {index} has an unparseable started_at {s:?}"))?,
        ),
    };
    let completed_at = match raw.completed_at.as_deref() {
        None => None,
        Some(s) => Some(
            parse_timestamp(s)
                .ok_or_else(|| format!("record {index} has an unparseable completed_at {s:?}"))?,
        ),
    };

    let asset_id = match raw
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(s) => Some(
            uuid::Uuid::parse_str(s)
                .map(AssetId::from_uuid)
                .map_err(|_| format!("record {index} has an invalid asset_id {s:?}"))?,
        ),
    };

    let mut warnings = Vec::new();

    let progress = Progress {
        current: raw.progress_current,
        total: raw.progress_total,
        unit: raw
            .progress_unit
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase),
    };

    // Validate record-level invariants before matching. For rows without an
    // explicit status (update candidates), pick the most permissive status
    // consistent with the provided timestamps so legitimate enrichment is
    // not rejected; the commit path re-validates against the real target and
    // discloses any failure in the report.
    let effective_status = match (status, completed_at, started_at) {
        (Some(s), _, _) => s,
        (None, Some(_), _) => MediaStatus::Completed,
        (None, None, Some(_)) => MediaStatus::InProgress,
        (None, None, None) => MediaStatus::Planned,
    };
    let mut probe = MediaRecord::new(
        AssetId::from_uuid(uuid::Uuid::nil()),
        media_type,
        effective_status,
    );
    probe.rating = raw.rating;
    probe.year = raw.year;
    probe.platform = raw.platform.clone();
    probe.progress = progress.clone();
    probe.notes = raw.notes.clone();
    probe.started_at = started_at;
    probe.completed_at = completed_at;
    if let Err(e) = probe.validate() {
        return Err(format!("record {index}: {e}"));
    }

    let mut refs: Vec<PendingExternalRef> = Vec::new();
    if let Some(raw_refs) = raw.external_refs {
        let flattened: Vec<(String, String, Option<String>)> = match raw_refs {
            crate::application::import_parse::RawExternalRefs::Map(map) => {
                map.into_iter().map(|(ns, id)| (ns, id, None)).collect()
            }
            crate::application::import_parse::RawExternalRefs::List(items) => items
                .into_iter()
                .map(|r| (r.namespace, r.external_id, r.source_url))
                .collect(),
        };
        for (namespace, external_id, source_url) in flattened {
            let reference = PendingExternalRef {
                namespace: namespace.trim().to_lowercase(),
                external_id: external_id.trim().to_string(),
                source_url,
            };
            if let Err(e) = reference.validate() {
                return Err(format!("record {index}: {e}"));
            }
            if refs.iter().any(|r| r.key() == reference.key()) {
                warnings.push(format!(
                    "duplicate external ref {} ignored",
                    reference.key().0
                ));
                continue;
            }
            refs.push(reference);
        }
    }

    Ok(ImportCandidate {
        index,
        asset_id,
        title: title.to_string(),
        media_type,
        status,
        rating: raw.rating,
        year: raw.year,
        platform: raw.platform,
        progress,
        notes: raw.notes,
        summary: raw.summary,
        tags: normalize_tags(&raw.tags.unwrap_or_default()),
        refs,
        started_at,
        completed_at,
        warnings,
        planned_asset_id: None,
    })
}
