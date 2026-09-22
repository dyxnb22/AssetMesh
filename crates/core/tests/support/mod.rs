//! Shared in-memory implementations of the core ports, used by application
//! tests. This mirrors the SQLite adapter's semantics (including
//! commit/rollback on `transact`) without any I/O.

#![allow(dead_code)]

use assetmesh_core::domain::activity::ActivityEvent;
use assetmesh_core::domain::asset::{Asset, LifecycleState};
use assetmesh_core::domain::external_ref::AssetExternalRef;
use assetmesh_core::domain::ids::{ActivityId, AssetId, RelationId, TagId};
use assetmesh_core::domain::media::{MediaEntry, MediaRecord};
use assetmesh_core::domain::relation::Relation;
use assetmesh_core::domain::search::{SearchDocument, SearchHit};
use assetmesh_core::domain::service::{ServiceEntry, ServiceRecord};
use assetmesh_core::domain::software::{SoftwareEntry, SoftwareRecord};
use assetmesh_core::domain::tag::Tag;
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::clock::Clock;
use assetmesh_core::ports::ids::IdGenerator;
use assetmesh_core::ports::repos::{
    ActivityReader, ActivityRepository, AssetFilter, AssetReader, AssetRepository, AssetSummary,
    ExternalRefReader, ExternalRefRepository, LibraryQuery, LibraryReadPort,
    LifecycleFilter, MediaFilter, MediaListRow, MediaReader, MediaRepository, MediaSort, Page,
    RelationReader, RelationRepository, ServiceFilter, ServiceListRow, ServiceReader,
    ServiceRepository, ServiceSort, SoftwareFilter, SoftwareListRow, SoftwareReader,
    SoftwareRepository, SoftwareSort, TagReader, TagRepository,
};
use assetmesh_core::ports::search::{SearchIndex, SearchReader};
use assetmesh_core::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use assetmesh_core::{AppError, AppResult};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct MemStore {
    /// Clock used for tag creation inside `ensure`.
    pub now: Timestamp,
    /// Test hook: when set, every `media().get` fails with a storage error,
    /// emulating a repository fault for rollback/propagation tests.
    pub inject_media_get_error: bool,
    pub assets: BTreeMap<String, Asset>,
    pub media: BTreeMap<String, MediaRecord>,
    pub software: BTreeMap<String, SoftwareRecord>,
    pub services: BTreeMap<String, ServiceRecord>,
    pub refs: BTreeMap<String, AssetExternalRef>,
    pub activity: Vec<ActivityEvent>,
    pub tags: BTreeMap<String, Tag>,
    pub memberships: BTreeSet<(String, String)>,
    pub relations: BTreeMap<String, Relation>,
    pub search_docs: BTreeMap<String, SearchDocument>,
}

impl Default for MemStore {
    fn default() -> Self {
        MemStore {
            now: chrono::Utc::now(),
            inject_media_get_error: false,
            assets: Default::default(),
            media: Default::default(),
            software: Default::default(),
            services: Default::default(),
            refs: Default::default(),
            activity: Default::default(),
            tags: Default::default(),
            memberships: Default::default(),
            relations: Default::default(),
            search_docs: Default::default(),
        }
    }
}

fn not_found(entity: &'static str, id: impl std::fmt::Display) -> AppError {
    AppError::not_found(entity, id)
}

impl AssetReader for MemStore {
    fn get(&mut self, id: AssetId) -> AppResult<Option<Asset>> {
        Ok(self.assets.get(&id.to_string()).cloned())
    }
    fn list(&mut self, filter: &AssetFilter) -> AppResult<Vec<Asset>> {
        let assets = self
            .assets
            .values()
            .filter(|a| {
                if let Some(kind) = filter.kind {
                    if a.kind != kind {
                        return false;
                    }
                }
                match filter.lifecycle.unwrap_or(LifecycleFilter::Active) {
                    LifecycleFilter::All => true,
                    LifecycleFilter::Active => a.lifecycle_state == LifecycleState::Active,
                    LifecycleFilter::ActiveOrArchived => {
                        a.lifecycle_state != LifecycleState::Merged
                    }
                }
            })
            .cloned()
            .collect();
        Ok(assets)
    }
}

impl AssetRepository for MemStore {
    fn insert(&mut self, asset: &Asset) -> AppResult<()> {
        if self.assets.contains_key(asset.id.to_string().as_str()) {
            return Err(AppError::conflict(format!(
                "asset {} already exists",
                asset.id
            )));
        }
        self.assets.insert(asset.id.to_string(), asset.clone());
        Ok(())
    }
    fn update(&mut self, asset: &Asset) -> AppResult<()> {
        if !self.assets.contains_key(asset.id.to_string().as_str()) {
            return Err(not_found("asset", asset.id));
        }
        // Mirrors the SQLite adapter: a kind change must not strand the
        // existing module record (e.g. a software record left on a media.*
        // asset, or a `saas` record left on a `service.api` asset — the typed
        // detail's own discriminator is only consistent with the kind assigned
        // at creation, and no write path re-types an asset).
        if let Some(existing) = self.assets.get(&asset.id.to_string()) {
            if existing.kind != asset.kind {
                let stranded = match existing.kind.module() {
                    "media" => self.media.contains_key(&asset.id.to_string()),
                    "software" => self.software.contains_key(&asset.id.to_string()),
                    "services" => self.services.contains_key(&asset.id.to_string()),
                    _ => false,
                };
                if stranded {
                    return Err(AppError::conflict(format!(
                        "cannot change asset {} from {} to {} while it has a {} record; remove \
                         the {} record first",
                        asset.id,
                        existing.kind,
                        asset.kind,
                        existing.kind.module(),
                        existing.kind.module()
                    )));
                }
            }
        }
        self.assets.insert(asset.id.to_string(), asset.clone());
        Ok(())
    }
}

impl MediaReader for MemStore {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<MediaRecord>> {
        if self.inject_media_get_error {
            return Err(AppError::storage("injected media read failure"));
        }
        Ok(self.media.get(&asset_id.to_string()).cloned())
    }
    fn list(&mut self, filter: &MediaFilter) -> AppResult<Vec<MediaListRow>> {
        let mut rows: Vec<MediaListRow> = self
            .media
            .values()
            .filter_map(|record| {
                let asset = self.assets.get(&record.asset_id.to_string())?;
                if let Some(media_type) = filter.media_type {
                    if record.media_type != media_type {
                        return None;
                    }
                }
                if let Some(status) = filter.status {
                    if record.status != status {
                        return None;
                    }
                }
                if let Some(platform) = &filter.platform {
                    if record.platform.as_deref() != Some(platform.as_str()) {
                        return None;
                    }
                }
                let tags: Vec<String> = self
                    .memberships
                    .iter()
                    .filter(|(a, _)| a == &asset.id.to_string())
                    .filter_map(|(_, t)| self.tags.get(t).map(|tag| tag.name.clone()))
                    .collect();
                if let Some(tag) = &filter.tag {
                    if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                        return None;
                    }
                }
                Some(MediaListRow {
                    entry: MediaEntry {
                        asset: asset.clone(),
                        record: record.clone(),
                    },
                    tags,
                })
            })
            .collect();

        match filter.sort {
            MediaSort::UpdatedDesc => {
                rows.sort_by_key(|row| std::cmp::Reverse(row.entry.asset.updated_at))
            }
            MediaSort::TitleAsc => rows.sort_by(|a, b| {
                a.entry
                    .asset
                    .name
                    .to_lowercase()
                    .cmp(&b.entry.asset.name.to_lowercase())
            }),
            MediaSort::RatingDesc => rows.sort_by(|a, b| {
                b.entry
                    .record
                    .rating
                    .partial_cmp(&a.entry.record.rating)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            MediaSort::CompletedDesc => rows.sort_by(|a, b| {
                b.entry
                    .record
                    .completed_at
                    .cmp(&a.entry.record.completed_at)
            }),
        }
        Ok(rows)
    }
    fn list_all(&mut self) -> AppResult<Vec<MediaRecord>> {
        Ok(self.media.values().cloned().collect())
    }
}

impl MediaRepository for MemStore {
    fn upsert(&mut self, record: &MediaRecord) -> AppResult<()> {
        self.media
            .insert(record.asset_id.to_string(), record.clone());
        Ok(())
    }
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.media.remove(&asset_id.to_string());
        Ok(())
    }
}

impl SoftwareReader for MemStore {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<SoftwareRecord>> {
        Ok(self.software.get(&asset_id.to_string()).cloned())
    }
    fn list(&mut self, filter: &SoftwareFilter) -> AppResult<Vec<SoftwareListRow>> {
        let mut rows: Vec<SoftwareListRow> = self
            .software
            .values()
            .filter_map(|record| {
                let asset = self.assets.get(&record.asset_id.to_string())?;
                if let Some(category) = filter.category {
                    if record.category != category {
                        return None;
                    }
                }
                if let Some(install_source) = filter.install_source {
                    if record.install_source != install_source {
                        return None;
                    }
                }
                let tags: Vec<String> = self
                    .memberships
                    .iter()
                    .filter(|(a, _)| a == &asset.id.to_string())
                    .filter_map(|(_, t)| self.tags.get(t).map(|tag| tag.name.clone()))
                    .collect();
                if let Some(tag) = &filter.tag {
                    if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                        return None;
                    }
                }
                Some(SoftwareListRow {
                    entry: SoftwareEntry {
                        asset: asset.clone(),
                        record: record.clone(),
                    },
                    tags,
                })
            })
            .collect();

        match filter.sort {
            SoftwareSort::UpdatedDesc => {
                rows.sort_by_key(|row| std::cmp::Reverse(row.entry.asset.updated_at))
            }
            SoftwareSort::TitleAsc => rows.sort_by(|a, b| {
                a.entry
                    .asset
                    .name
                    .to_lowercase()
                    .cmp(&b.entry.asset.name.to_lowercase())
            }),
        }
        Ok(rows)
    }
    fn list_all(&mut self) -> AppResult<Vec<SoftwareRecord>> {
        Ok(self.software.values().cloned().collect())
    }
}

impl SoftwareRepository for MemStore {
    fn upsert(&mut self, record: &SoftwareRecord) -> AppResult<()> {
        // Mirrors the SQLite adapter: invariants enforced and normalized at
        // the repository boundary, so even a direct UnitOfWork write cannot
        // store raw unnormalized text.
        let mut normalized = record.clone();
        normalized.validate()?;
        // Mirrors the SQLite adapter: category must match the owning asset's
        // kind, even for direct UnitOfWork writes.
        if let Some(asset) = self.assets.get(&normalized.asset_id.to_string()) {
            if asset.kind != normalized.category.asset_kind() {
                return Err(AppError::conflict(format!(
                    "software record category {} requires asset kind {}, but asset {} has kind {}",
                    normalized.category,
                    normalized.category.asset_kind(),
                    normalized.asset_id,
                    asset.kind
                )));
            }
        }
        self.software
            .insert(normalized.asset_id.to_string(), normalized);
        Ok(())
    }
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.software.remove(&asset_id.to_string());
        Ok(())
    }
}

impl ServiceReader for MemStore {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<ServiceRecord>> {
        Ok(self.services.get(&asset_id.to_string()).cloned())
    }
    fn list(&mut self, filter: &ServiceFilter) -> AppResult<Vec<ServiceListRow>> {
        let mut rows: Vec<ServiceListRow> = self
            .services
            .values()
            .filter_map(|record| {
                let asset = self.assets.get(&record.asset_id.to_string())?;
                if let Some(service_type) = filter.service_type {
                    if record.service_type != service_type {
                        return None;
                    }
                }
                if let Some(provider) = &filter.provider {
                    if !record
                        .provider
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&provider.to_lowercase())
                    {
                        return None;
                    }
                }
                let tags: Vec<String> = self
                    .memberships
                    .iter()
                    .filter(|(a, _)| a == &asset.id.to_string())
                    .filter_map(|(_, t)| self.tags.get(t).map(|tag| tag.name.clone()))
                    .collect();
                if let Some(tag) = &filter.tag {
                    if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                        return None;
                    }
                }
                Some(ServiceListRow {
                    entry: ServiceEntry {
                        asset: asset.clone(),
                        record: record.clone(),
                    },
                    tags,
                })
            })
            .collect();

        match filter.sort {
            ServiceSort::UpdatedDesc => {
                rows.sort_by_key(|row| std::cmp::Reverse(row.entry.asset.updated_at))
            }
            ServiceSort::TitleAsc => rows.sort_by(|a, b| {
                a.entry
                    .asset
                    .name
                    .to_lowercase()
                    .cmp(&b.entry.asset.name.to_lowercase())
            }),
            ServiceSort::RenewsAsc => rows.sort_by(|a, b| {
                // Services without a renewal date sort last, so the nullability
                // dominates the comparison — mirroring the SQLite adapter's
                // `ORDER BY s.renews_at IS NULL, s.renews_at ASC`.
                a.entry
                    .record
                    .renews_at
                    .is_none()
                    .cmp(&b.entry.record.renews_at.is_none())
                    .then_with(|| a.entry.record.renews_at.cmp(&b.entry.record.renews_at))
                    .then_with(|| {
                        a.entry
                            .asset
                            .name
                            .to_lowercase()
                            .cmp(&b.entry.asset.name.to_lowercase())
                    })
            }),
        }
        Ok(rows)
    }
    fn list_all(&mut self) -> AppResult<Vec<ServiceRecord>> {
        Ok(self.services.values().cloned().collect())
    }
}

impl ServiceRepository for MemStore {
    fn upsert(&mut self, record: &ServiceRecord) -> AppResult<()> {
        // Mirrors the SQLite adapter: invariants enforced and normalized at
        // the repository boundary, so even a direct UnitOfWork write cannot
        // store raw unnormalized text or an incompatible kind/type pair.
        let mut normalized = record.clone();
        normalized.validate()?;
        if let Some(asset) = self.assets.get(&normalized.asset_id.to_string()) {
            if asset.kind != normalized.service_type.asset_kind() {
                return Err(AppError::conflict(format!(
                    "service record type {} requires asset kind {}, but asset {} has kind {}",
                    normalized.service_type,
                    normalized.service_type.asset_kind(),
                    normalized.asset_id,
                    asset.kind
                )));
            }
        }
        self.services
            .insert(normalized.asset_id.to_string(), normalized);
        Ok(())
    }
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.services.remove(&asset_id.to_string());
        Ok(())
    }
}

impl RelationReader for MemStore {
    fn get(&mut self, id: RelationId) -> AppResult<Option<Relation>> {
        Ok(self.relations.get(&id.to_string()).cloned())
    }
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<Relation>> {
        let mut relations: Vec<Relation> = self
            .relations
            .values()
            .filter(|r| r.source_asset_id == asset_id || r.target_asset_id == asset_id)
            .cloned()
            .collect();
        relations.sort_by_key(|r| r.id.as_uuid());
        Ok(relations)
    }
    fn list_for_assets(&mut self, asset_ids: &[AssetId]) -> AppResult<Vec<Relation>> {
        // Mirrors the SQLite adapter: every relation touching any of the given
        // assets, ordered by identity, and an empty slice yields no rows.
        let wanted: std::collections::HashSet<String> =
            asset_ids.iter().map(|id| id.to_string()).collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let mut relations: Vec<Relation> = self
            .relations
            .values()
            .filter(|r| {
                wanted.contains(&r.source_asset_id.to_string())
                    || wanted.contains(&r.target_asset_id.to_string())
            })
            .cloned()
            .collect();
        relations.sort_by_key(|r| r.id.as_uuid());
        Ok(relations)
    }
    fn list_all(&mut self) -> AppResult<Vec<Relation>> {
        Ok(self.relations.values().cloned().collect())
    }
}

impl RelationRepository for MemStore {
    fn insert(&mut self, relation: &Relation) -> AppResult<()> {
        relation.validate()?;
        relation.ensure_canonical()?;
        let duplicate = self.relations.values().any(|existing| {
            existing.relation_type == relation.relation_type
                && existing.source_asset_id == relation.source_asset_id
                && existing.target_asset_id == relation.target_asset_id
        });
        if duplicate {
            return Err(AppError::conflict(format!(
                "relation between {} and {} already exists",
                relation.source_asset_id, relation.target_asset_id
            )));
        }
        self.relations
            .insert(relation.id.to_string(), relation.clone());
        Ok(())
    }
    fn update(&mut self, relation: &Relation) -> AppResult<()> {
        relation.ensure_canonical()?;
        if !self
            .relations
            .contains_key(relation.id.to_string().as_str())
        {
            return Err(not_found("relation", relation.id));
        }
        self.relations
            .insert(relation.id.to_string(), relation.clone());
        Ok(())
    }
    fn delete(&mut self, id: RelationId) -> AppResult<()> {
        self.relations.remove(&id.to_string());
        Ok(())
    }
}

impl ExternalRefReader for MemStore {
    fn get(
        &mut self,
        id: assetmesh_core::domain::ids::ExternalRefId,
    ) -> AppResult<Option<AssetExternalRef>> {
        Ok(self.refs.get(&id.to_string()).cloned())
    }
    fn find_asset_by_ref(
        &mut self,
        namespace: &str,
        external_id: &str,
    ) -> AppResult<Option<AssetId>> {
        Ok(self
            .refs
            .values()
            .find(|r| r.namespace == namespace && r.external_id == external_id)
            .map(|r| r.asset_id))
    }
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<AssetExternalRef>> {
        Ok(self
            .refs
            .values()
            .filter(|r| r.asset_id == asset_id)
            .cloned()
            .collect())
    }
    fn list_all(&mut self) -> AppResult<Vec<AssetExternalRef>> {
        Ok(self.refs.values().cloned().collect())
    }
}

impl ExternalRefRepository for MemStore {
    fn insert(&mut self, reference: &AssetExternalRef) -> AppResult<()> {
        let taken = self
            .refs
            .values()
            .any(|r| r.namespace == reference.namespace && r.external_id == reference.external_id);
        if taken {
            return Err(AppError::conflict(format!(
                "external ref {}:{} already exists",
                reference.namespace, reference.external_id
            )));
        }
        self.refs
            .insert(reference.id.to_string(), reference.clone());
        Ok(())
    }
    fn update(&mut self, reference: &AssetExternalRef) -> AppResult<()> {
        if !self.refs.contains_key(reference.id.to_string().as_str()) {
            return Err(not_found("external ref", reference.id));
        }
        self.refs
            .insert(reference.id.to_string(), reference.clone());
        Ok(())
    }
    fn delete(&mut self, id: assetmesh_core::domain::ids::ExternalRefId) -> AppResult<()> {
        self.refs.remove(&id.to_string());
        Ok(())
    }
}

impl ActivityReader for MemStore {
    fn list_for_asset(&mut self, asset_id: AssetId, limit: usize) -> AppResult<Vec<ActivityEvent>> {
        let mut events: Vec<ActivityEvent> = self
            .activity
            .iter()
            .filter(|e| e.asset_id == Some(asset_id))
            .cloned()
            .collect();
        events.sort_by_key(|e| std::cmp::Reverse((e.occurred_at, e.id.as_uuid())));
        events.truncate(limit);
        Ok(events)
    }
    fn list_recent(&mut self, limit: usize) -> AppResult<Vec<ActivityEvent>> {
        let mut events = self.activity.clone();
        events.sort_by_key(|e| std::cmp::Reverse((e.occurred_at, e.id.as_uuid())));
        events.truncate(limit);
        Ok(events)
    }
    fn list_all(&mut self) -> AppResult<Vec<ActivityEvent>> {
        let mut events = self.activity.clone();
        events.sort_by_key(|e| (e.occurred_at, e.id.as_uuid()));
        Ok(events)
    }
    fn get(&mut self, id: ActivityId) -> AppResult<Option<ActivityEvent>> {
        Ok(self.activity.iter().find(|e| e.id == id).cloned())
    }
}

impl ActivityRepository for MemStore {
    fn append(&mut self, event: &ActivityEvent) -> AppResult<()> {
        self.activity.push(event.clone());
        Ok(())
    }
    fn upsert(&mut self, event: &ActivityEvent) -> AppResult<()> {
        match self.activity.iter_mut().find(|e| e.id == event.id) {
            Some(existing) => *existing = event.clone(),
            None => self.activity.push(event.clone()),
        }
        Ok(())
    }
}

impl TagReader for MemStore {
    fn find_by_name(&mut self, name: &str) -> AppResult<Option<Tag>> {
        Ok(self.tags.values().find(|t| t.name == name).cloned())
    }
    fn get(&mut self, id: TagId) -> AppResult<Option<Tag>> {
        Ok(self.tags.get(&id.to_string()).cloned())
    }
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<Tag>> {
        Ok(self
            .memberships
            .iter()
            .filter(|(a, _)| a == &asset_id.to_string())
            .filter_map(|(_, t)| self.tags.get(t).cloned())
            .collect())
    }
    fn list_for_assets(&mut self, asset_ids: &[AssetId]) -> AppResult<Vec<(AssetId, Tag)>> {
        // Mirrors the SQLite adapter: one pass over a filterable set, ordered by
        // (asset_id, name), and an empty slice yields no rows. The set is built
        // from the id strings so the grouping matches the stored membership keys.
        let wanted: std::collections::HashSet<String> =
            asset_ids.iter().map(|id| id.to_string()).collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let mut out: Vec<(String, Tag)> = self
            .memberships
            .iter()
            .filter(|(a, _)| wanted.contains(a))
            .filter_map(|(a, t)| self.tags.get(t).map(|tag| (a.clone(), tag.clone())))
            .collect();
        out.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.name.cmp(&b.1.name))
                .then_with(|| a.1.id.as_uuid().cmp(&b.1.id.as_uuid()))
        });
        Ok(out
            .into_iter()
            .filter_map(|(a, tag)| {
                uuid::Uuid::parse_str(&a)
                    .ok()
                    .map(AssetId::from_uuid)
                    .map(|id| (id, tag))
            })
            .collect())
    }
    fn list_all(&mut self) -> AppResult<Vec<Tag>> {
        Ok(self.tags.values().cloned().collect())
    }
    fn list_memberships(&mut self) -> AppResult<Vec<(AssetId, TagId)>> {
        Ok(self
            .memberships
            .iter()
            .filter_map(|(a, t)| {
                let asset_id = uuid::Uuid::parse_str(a).ok().map(AssetId::from_uuid)?;
                let tag_id = uuid::Uuid::parse_str(t).ok().map(TagId::from_uuid)?;
                Some((asset_id, tag_id))
            })
            .collect())
    }
}

impl TagRepository for MemStore {
    fn ensure(&mut self, name: &str) -> AppResult<Tag> {
        if let Some(tag) = self.tags.values().find(|t| t.name == name) {
            return Ok(tag.clone());
        }
        let tag = Tag::new(name, self.now);
        self.tags.insert(tag.id.to_string(), tag.clone());
        Ok(tag)
    }
    fn insert(&mut self, tag: &Tag) -> AppResult<()> {
        if self.tags.values().any(|t| t.name == tag.name) {
            return Err(AppError::conflict(format!(
                "tag {} already exists",
                tag.name
            )));
        }
        self.tags.insert(tag.id.to_string(), tag.clone());
        Ok(())
    }
    fn update(&mut self, tag: &Tag) -> AppResult<()> {
        if !self.tags.contains_key(tag.id.to_string().as_str()) {
            return Err(not_found("tag", tag.id));
        }
        self.tags.insert(tag.id.to_string(), tag.clone());
        Ok(())
    }
    fn attach(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()> {
        self.memberships
            .insert((asset_id.to_string(), tag_id.to_string()));
        Ok(())
    }
    fn detach(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()> {
        self.memberships
            .remove(&(asset_id.to_string(), tag_id.to_string()));
        Ok(())
    }
    fn insert_membership(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()> {
        self.memberships
            .insert((asset_id.to_string(), tag_id.to_string()));
        Ok(())
    }
}

impl SearchReader for MemStore {
    fn search(&mut self, query: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        let query = query.to_lowercase();
        let tokens: Vec<&str> = query.split_whitespace().collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits: Vec<SearchHit> = self
            .search_docs
            .values()
            .filter(|doc| {
                let haystack = format!(
                    "{} {} {} {}",
                    doc.title.to_lowercase(),
                    doc.subtitle.as_deref().unwrap_or("").to_lowercase(),
                    doc.body.as_deref().unwrap_or("").to_lowercase(),
                    doc.keywords
                        .iter()
                        .map(|k| k.to_lowercase())
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                tokens.iter().all(|token| haystack.contains(token))
            })
            .map(|doc| SearchHit {
                asset_id: doc.asset_id,
                kind: doc.kind.clone(),
                title: doc.title.clone(),
                subtitle: doc.subtitle.clone(),
            })
            .collect();
        hits.sort_by_key(|hit| hit.title.to_lowercase());
        hits.truncate(limit);
        Ok(hits)
    }
}

impl SearchIndex for MemStore {
    fn upsert(&mut self, document: &SearchDocument) -> AppResult<()> {
        self.search_docs
            .insert(document.asset_id.to_string(), document.clone());
        Ok(())
    }
    fn remove(&mut self, asset_id: AssetId) -> AppResult<()> {
        self.search_docs.remove(&asset_id.to_string());
        Ok(())
    }
    fn clear(&mut self) -> AppResult<()> {
        self.search_docs.clear();
        Ok(())
    }
    fn replace_all(&mut self, documents: &[SearchDocument]) -> AppResult<()> {
        self.search_docs.clear();
        for document in documents {
            self.search_docs
                .insert(document.asset_id.to_string(), document.clone());
        }
        Ok(())
    }
}

impl UnitOfWork for MemStore {
    fn assets(&mut self) -> &mut dyn AssetRepository {
        self
    }

    fn media(&mut self) -> &mut dyn MediaRepository {
        self
    }

    fn software(&mut self) -> &mut dyn SoftwareRepository {
        self
    }

    fn services(&mut self) -> &mut dyn ServiceRepository {
        self
    }

    fn external_refs(&mut self) -> &mut dyn ExternalRefRepository {
        self
    }

    fn activity(&mut self) -> &mut dyn ActivityRepository {
        self
    }

    fn tags(&mut self) -> &mut dyn TagRepository {
        self
    }

    fn relations(&mut self) -> &mut dyn RelationRepository {
        self
    }

    fn search_index(&mut self) -> &mut dyn SearchIndex {
        self
    }

    fn library(&mut self) -> &mut dyn LibraryReadPort {
        self
    }
}

impl QueryUnitOfWork for MemStore {
    fn assets(&mut self) -> &mut dyn AssetReader {
        self
    }

    fn media(&mut self) -> &mut dyn MediaReader {
        self
    }

    fn software(&mut self) -> &mut dyn SoftwareReader {
        self
    }

    fn services(&mut self) -> &mut dyn ServiceReader {
        self
    }

    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        self
    }

    fn activity(&mut self) -> &mut dyn ActivityReader {
        self
    }

    fn tags(&mut self) -> &mut dyn TagReader {
        self
    }

    fn relations(&mut self) -> &mut dyn RelationReader {
        self
    }

    fn search_index(&mut self) -> &mut dyn SearchReader {
        self
    }

    fn library(&mut self) -> &mut dyn LibraryReadPort {
        self
    }
}

impl LibraryReadPort for MemStore {
    fn query_library(&mut self, query: &LibraryQuery) -> AppResult<Page<AssetSummary>> {
        use assetmesh_core::application::library_service::{
            load_library_rows, matches_kinds, matches_lifecycle, matches_tags, selected_modules,
            sort_rows, summarize_row,
        };
        let modules = selected_modules(&query.modules, &query.kinds);
        let limit = query.page.effective_limit();
        if modules.is_empty() {
            return Ok(Page::empty(&query.page));
        }
        let mut rows = load_library_rows(self, &modules)?;
        rows.retain(|row| matches_lifecycle(query.lifecycle, row));
        rows.retain(|row| matches_kinds(&query.kinds, row));
        rows.retain(|row| matches_tags(&query.tags, row));
        sort_rows(&mut rows, query.sort);
        let total = rows.len();
        let items = rows
            .into_iter()
            .skip(query.page.offset)
            .take(limit)
            .map(|row| summarize_row(&row))
            .collect();
        Ok(Page {
            items,
            offset: query.page.offset,
            limit,
            total: Some(total),
        })
    }
}

/// Shareable in-memory factory. `transact` rolls back on error by restoring a
/// snapshot, mirroring SQLite transaction semantics.
#[derive(Clone, Default)]
pub struct MemFactory {
    store: Rc<RefCell<MemStore>>,
}

impl MemFactory {
    pub fn new() -> Self {
        MemFactory {
            store: Rc::new(RefCell::new(MemStore::default())),
        }
    }

    pub fn store(&self) -> std::cell::Ref<'_, MemStore> {
        self.store.borrow()
    }

    /// Test hook: mutate the store directly (e.g. inject faults).
    pub fn store_borrow_mut(&self, f: impl FnOnce(&mut MemStore)) {
        f(&mut self.store.borrow_mut())
    }
}

impl UnitOfWorkFactory for MemFactory {
    fn transact<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn UnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        let snapshot = self.store.borrow().clone();
        let mut store = self.store.borrow_mut();
        match work(&mut *store) {
            Ok(value) => Ok(value),
            Err(error) => {
                drop(store);
                *self.store.borrow_mut() = snapshot;
                Err(error)
            }
        }
    }

    fn read<T>(
        &mut self,
        work: &mut dyn FnMut(&mut dyn QueryUnitOfWork) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut store = self.store.borrow_mut();
        work(&mut *store)
    }
}

/// Fixed clock for deterministic tests.
#[derive(Debug, Clone)]
pub struct FakeClock {
    now: Arc<Mutex<Timestamp>>,
}

impl FakeClock {
    pub fn new() -> Self {
        FakeClock {
            now: Arc::new(Mutex::new(chrono::Utc::now())),
        }
    }

    pub fn advance_seconds(&self, seconds: i64) {
        let mut now = self.now.lock().unwrap();
        *now += chrono::Duration::seconds(seconds);
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().unwrap()
    }
}

/// Sequential deterministic IDs. The counter is process-global so IDs stay
/// unique across different test environments (important for round trips).
#[derive(Debug, Clone, Default)]
pub struct FakeIdGen {
    _marker: (),
}

impl IdGenerator for FakeIdGen {
    fn new_id(&self) -> uuid::Uuid {
        static COUNTER: Mutex<u128> = Mutex::new(0);
        let mut counter = COUNTER.lock().unwrap();
        *counter += 1;
        uuid::Uuid::from_u128(*counter)
    }
}

/// Bundles services over one shared in-memory store for tests.
pub fn test_env() -> TestEnv {
    let factory = MemFactory::new();
    let clock = Arc::new(FakeClock::new());
    let ids = Arc::new(FakeIdGen::default());
    TestEnv {
        factory,
        clock,
        ids,
    }
}

pub struct TestEnv {
    pub factory: MemFactory,
    pub clock: Arc<FakeClock>,
    pub ids: Arc<FakeIdGen>,
}

impl TestEnv {
    pub fn media_service(
        &self,
    ) -> assetmesh_core::application::media_service::MediaService<MemFactory> {
        assetmesh_core::application::media_service::MediaService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
    }

    pub fn asset_service(
        &self,
    ) -> assetmesh_core::application::asset_service::AssetService<MemFactory> {
        assetmesh_core::application::asset_service::AssetService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
    }

    pub fn search_service(
        &self,
    ) -> assetmesh_core::application::search_service::SearchService<MemFactory> {
        assetmesh_core::application::search_service::SearchService::new(
            self.factory.clone(),
            self.clock.clone(),
        )
    }

    pub fn import_service(
        &self,
    ) -> assetmesh_core::application::import_media::MediaImportService<MemFactory> {
        assetmesh_core::application::import_media::MediaImportService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
    }

    pub fn export_service(
        &self,
    ) -> assetmesh_core::application::portable::PortableExportService<MemFactory> {
        assetmesh_core::application::portable::PortableExportService::new(
            self.factory.clone(),
            self.clock.clone(),
        )
    }

    pub fn portable_import_service(
        &self,
    ) -> assetmesh_core::application::portable::PortableImportService<MemFactory> {
        assetmesh_core::application::portable::PortableImportService::new(self.factory.clone())
    }

    pub fn service_service(
        &self,
    ) -> assetmesh_core::application::service_service::ServiceService<MemFactory> {
        assetmesh_core::application::service_service::ServiceService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
    }

    pub fn library_service(
        &self,
    ) -> assetmesh_core::application::library_service::LibraryService<MemFactory> {
        assetmesh_core::application::library_service::LibraryService::new(self.factory.clone())
    }

    pub fn software_service(
        &self,
    ) -> assetmesh_core::application::software_service::SoftwareService<MemFactory> {
        assetmesh_core::application::software_service::SoftwareService::new(
            self.factory.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
    }
}
