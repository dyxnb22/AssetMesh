//! Media use cases.
//!
//! Every mutation opens one short transaction that commits the canonical
//! change, its activity event, and the synchronous search projection
//! together (ADR 0007, docs/08).

use crate::application::projection::project_media;
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::media::{MediaEntry, MediaRecord, MediaStatus, Progress};
use crate::domain::search::SearchDocument;
use crate::domain::Timestamp;
use crate::ports::repos::{MediaFilter, MediaListRow};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde_json::json;

/// Application-facing view of one media asset with its module details,
/// external references, tags, and recent activity.
#[derive(Debug, Clone)]
pub struct MediaView {
    pub entry: MediaEntry,
    pub external_refs: Vec<AssetExternalRef>,
    pub tags: Vec<String>,
    pub activity: Vec<ActivityEvent>,
}

#[derive(Debug, Clone)]
pub struct CreateMedia {
    pub title: String,
    pub media_type: crate::domain::media::MediaType,
    pub summary: Option<String>,
    pub status: Option<MediaStatus>,
    pub rating: Option<f64>,
    pub year: Option<i32>,
    pub platform: Option<String>,
    pub progress: Progress,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub external_refs: Vec<ExternalRefInput>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone)]
pub struct ExternalRefInput {
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateMediaMetadata {
    pub asset_id: AssetId,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub year: Option<i32>,
    pub platform: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MediaService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> MediaService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        MediaService {
            factory,
            clock,
            ids,
        }
    }

    pub fn factory(&mut self) -> &mut F {
        &mut self.factory
    }

    pub fn get_media(&mut self, asset_id: AssetId) -> AppResult<MediaView> {
        self.factory.read(&mut |q| build_view(q, asset_id))
    }

    pub fn list_media(&mut self, filter: &MediaFilter) -> AppResult<Vec<MediaListRow>> {
        self.factory.read(&mut |uow| uow.media().list(filter))
    }

    pub fn create_media(&mut self, cmd: CreateMedia) -> AppResult<MediaView> {
        let now = self.clock.now();

        let title = cmd.title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::validation("media title must not be empty"));
        }

        let status = cmd.status.unwrap_or(MediaStatus::Planned);
        let asset_id = AssetId::from_uuid(self.ids.new_id());

        let mut record = MediaRecord::new(asset_id, cmd.media_type, status);
        record.rating = cmd.rating;
        record.year = cmd.year;
        record.platform = cmd.platform;
        record.progress = cmd.progress;
        record.notes = cmd.notes;
        record.started_at = cmd.started_at;
        record.completed_at = cmd.completed_at;
        // Sensible defaults so a directly-completed/started creation stays
        // consistent without forcing callers to set timestamps by hand.
        if record.started_at.is_none()
            && matches!(status, MediaStatus::InProgress | MediaStatus::Completed)
        {
            record.started_at = Some(now);
        }
        if record.completed_at.is_none() && status == MediaStatus::Completed {
            record.completed_at = Some(now);
        }
        record.validate()?;

        let asset = Asset::new(
            asset_id,
            cmd.media_type.asset_kind(),
            title,
            cmd.summary,
            now,
        )?;

        let refs: Vec<AssetExternalRef> = cmd
            .external_refs
            .into_iter()
            .map(|input| {
                let r = AssetExternalRef::new(
                    asset_id,
                    input.namespace,
                    input.external_id,
                    input.source_url,
                    now,
                );
                r.validate()?;
                Ok(r)
            })
            .collect::<AppResult<Vec<_>>>()?;

        let tag_names = normalize_tags(&cmd.tags);

        self.factory.transact(&mut |uow| {
            for reference in &refs {
                ensure_ref_available(uow, reference)?;
            }
            uow.assets().insert(&asset)?;
            uow.media().upsert(&record)?;

            let mut tags = Vec::new();
            for name in &tag_names {
                let tag = uow.tags().ensure(name)?;
                uow.tags().attach(asset_id, tag.id)?;
                tags.push(tag);
            }
            for reference in &refs {
                uow.external_refs().insert(reference)?;
            }

            uow.activity().append(&ActivityEvent::new(
                event_types::ASSET_CREATED,
                Some(asset_id),
                actors::USER,
                json!({ "kind": asset.kind.as_str(), "name": asset.name.clone() }),
                now,
            ))?;
            uow.activity().append(&ActivityEvent::new(
                event_types::MEDIA_CREATED,
                Some(asset_id),
                actors::USER,
                json!({
                    "media_type": record.media_type.as_str(),
                    "status": record.status.as_str(),
                }),
                now,
            ))?;

            let document = project_media(&asset, &record, &tags, &refs);
            uow.search_index().upsert(&document)?;
            Ok(())
        })?;

        self.get_media(asset_id)
    }

    pub fn update_metadata(&mut self, cmd: UpdateMediaMetadata) -> AppResult<MediaView> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = load_active_asset(uow, cmd.asset_id)?;
            let mut record = load_media_record(uow, cmd.asset_id)?;

            if let Some(title) = cmd.title.as_deref().map(str::trim) {
                if title.is_empty() {
                    return Err(AppError::validation("media title must not be empty"));
                }
                asset.name = title.to_string();
            }
            if cmd.summary.is_some() {
                asset.summary = cmd.summary.clone();
            }
            if cmd.year.is_some() {
                record.year = cmd.year;
            }
            if cmd.platform.is_some() {
                record.platform = cmd.platform.clone();
            }
            if cmd.notes.is_some() {
                record.notes = cmd.notes.clone();
            }

            asset.validate()?;
            record.validate()?;
            // Metadata-only edits are deliberately not activity events
            // (docs/08: avoid noisy activity for trivial text edits).
            asset.touch(now);
            uow.assets().update(&asset)?;
            uow.media().upsert(&record)?;

            update_projection(uow, &asset, &record)?;
            Ok(())
        })?;

        self.get_media(cmd.asset_id)
    }

    pub fn start_media(&mut self, asset_id: AssetId) -> AppResult<MediaView> {
        self.transition(
            asset_id,
            MediaStatus::InProgress,
            event_types::MEDIA_STARTED,
        )
    }

    pub fn pause_media(&mut self, asset_id: AssetId) -> AppResult<MediaView> {
        self.transition(asset_id, MediaStatus::Paused, event_types::MEDIA_PAUSED)
    }

    pub fn drop_media(&mut self, asset_id: AssetId) -> AppResult<MediaView> {
        self.transition(asset_id, MediaStatus::Dropped, event_types::MEDIA_DROPPED)
    }

    pub fn complete_media(&mut self, asset_id: AssetId) -> AppResult<MediaView> {
        self.transition(
            asset_id,
            MediaStatus::Completed,
            event_types::MEDIA_COMPLETED,
        )
    }

    fn transition(
        &mut self,
        asset_id: AssetId,
        to: MediaStatus,
        event_type: &str,
    ) -> AppResult<MediaView> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = load_active_asset(uow, asset_id)?;
            let mut record = load_media_record(uow, asset_id)?;
            let from = record.status;

            record.transition_to(to, now)?;
            record.validate()?;
            asset.touch(now);

            uow.media().upsert(&record)?;
            uow.assets().update(&asset)?;
            uow.activity().append(&ActivityEvent::new(
                event_type,
                Some(asset_id),
                actors::USER,
                json!({ "from": from.as_str(), "to": to.as_str() }),
                now,
            ))?;

            update_projection(uow, &asset, &record)?;
            Ok(())
        })?;

        self.get_media(asset_id)
    }

    pub fn update_progress(
        &mut self,
        asset_id: AssetId,
        progress: Progress,
    ) -> AppResult<MediaView> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = load_active_asset(uow, asset_id)?;
            let mut record = load_media_record(uow, asset_id)?;
            let previous = record.progress.clone();

            record.progress = progress.clone();
            record.validate()?;
            asset.touch(now);

            uow.media().upsert(&record)?;
            uow.assets().update(&asset)?;
            if previous != record.progress {
                uow.activity().append(&ActivityEvent::new(
                    event_types::MEDIA_PROGRESS_CHANGED,
                    Some(asset_id),
                    actors::USER,
                    json!({
                        "previous": progress_payload(&previous),
                        "current": progress_payload(&record.progress),
                    }),
                    now,
                ))?;
            }

            update_projection(uow, &asset, &record)?;
            Ok(())
        })?;

        self.get_media(asset_id)
    }

    pub fn rate_media(&mut self, asset_id: AssetId, rating: f64) -> AppResult<MediaView> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = load_active_asset(uow, asset_id)?;
            let mut record = load_media_record(uow, asset_id)?;
            let previous = record.rating;

            record.rating = Some(rating);
            record.validate()?;
            asset.touch(now);

            uow.media().upsert(&record)?;
            uow.assets().update(&asset)?;
            if previous != Some(rating) {
                uow.activity().append(&ActivityEvent::new(
                    event_types::MEDIA_RATING_CHANGED,
                    Some(asset_id),
                    actors::USER,
                    json!({ "previous": previous, "rating": rating }),
                    now,
                ))?;
            }

            update_projection(uow, &asset, &record)?;
            Ok(())
        })?;

        self.get_media(asset_id)
    }
}

pub(crate) fn load_active_asset(uow: &mut dyn UnitOfWork, asset_id: AssetId) -> AppResult<Asset> {
    let asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    asset.ensure_mutable()?;
    Ok(asset)
}

pub(crate) fn load_media_record(
    uow: &mut dyn UnitOfWork,
    asset_id: AssetId,
) -> AppResult<MediaRecord> {
    uow.media()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("media record", asset_id))
}

pub(crate) fn ensure_ref_available(
    uow: &mut dyn UnitOfWork,
    reference: &AssetExternalRef,
) -> AppResult<()> {
    if let Some(existing) = uow
        .external_refs()
        .find_asset_by_ref(&reference.namespace, &reference.external_id)?
    {
        if existing != reference.asset_id {
            return Err(AppError::conflict(format!(
                "external ref {}:{} is already attached to asset {}",
                reference.namespace, reference.external_id, existing
            )));
        }
    }
    Ok(())
}

/// Rebuilds and stores the search projection for one asset from canonical
/// state, inside the caller's transaction.
pub(crate) fn update_projection(
    uow: &mut dyn UnitOfWork,
    asset: &Asset,
    record: &MediaRecord,
) -> AppResult<SearchDocument> {
    let tags = uow.tags().list_for_asset(asset.id)?;
    let refs = uow.external_refs().list_for_asset(asset.id)?;
    let document = project_media(asset, record, &tags, &refs);
    uow.search_index().upsert(&document)?;
    Ok(document)
}

pub(crate) fn build_view(uow: &mut dyn QueryUnitOfWork, asset_id: AssetId) -> AppResult<MediaView> {
    let asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    let record = uow
        .media()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("media record", asset_id))?;
    let external_refs = uow.external_refs().list_for_asset(asset_id)?;
    let tags = uow
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let activity = uow.activity().list_for_asset(asset_id, 50)?;
    Ok(MediaView {
        entry: MediaEntry { asset, record },
        external_refs,
        tags,
        activity,
    })
}

pub(crate) fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for tag in tags {
        let name = tag.trim();
        if name.is_empty() {
            continue;
        }
        if !normalized
            .iter()
            .any(|n: &String| n.eq_ignore_ascii_case(name))
        {
            normalized.push(name.to_string());
        }
    }
    normalized
}

pub(crate) fn progress_payload(progress: &Progress) -> serde_json::Value {
    json!({
        "current": progress.current,
        "total": progress.total,
        "unit": progress.unit,
    })
}
