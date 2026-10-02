//! Exact identity matching and advisory conflicts, including earlier planned rows.
use super::ImportConflict;
use crate::application::import_parse::{normalize_title, ImportCandidate};
use crate::application::SharedIdGenerator;
use crate::domain::asset::Asset;
use crate::domain::ids::AssetId;
use crate::domain::media::MediaType;
use crate::ports::repos::{AssetFilter, LifecycleFilter};
use crate::ports::uow::QueryUnitOfWork;
use crate::AppResult;
use std::collections::HashMap;

type PlannedRecord = (ImportCandidate, PlannedAction);
pub(super) fn plan(
    q: &mut dyn QueryUnitOfWork,
    candidates_iter: &mut impl Iterator<Item = ImportCandidate>,
    ids: &SharedIdGenerator,
) -> AppResult<(Vec<PlannedRecord>, Vec<ImportConflict>)> {
    let candidate_count = candidates_iter.size_hint().0;
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
}

pub(super) enum PlannedAction {
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
