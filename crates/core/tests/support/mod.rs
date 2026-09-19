//! Shared in-memory implementations of the core ports, used by application
//! tests. This mirrors the SQLite adapter's semantics (including
//! commit/rollback on `transact`) without any I/O.

#![allow(dead_code)]

use assetmesh_core::domain::activity::ActivityEvent;
use assetmesh_core::domain::asset::{Asset, LifecycleState};
use assetmesh_core::domain::external_ref::AssetExternalRef;
use assetmesh_core::domain::ids::{ActivityId, AssetId, TagId};
use assetmesh_core::domain::media::{MediaEntry, MediaRecord};
use assetmesh_core::domain::search::{SearchDocument, SearchHit};
use assetmesh_core::domain::tag::Tag;
use assetmesh_core::domain::Timestamp;
use assetmesh_core::ports::clock::Clock;
use assetmesh_core::ports::ids::IdGenerator;
use assetmesh_core::ports::repos::{
    ActivityReader, ActivityRepository, AssetFilter, AssetReader, AssetRepository,
    ExternalRefReader, ExternalRefRepository, LifecycleFilter, MediaFilter, MediaListRow,
    MediaReader, MediaRepository, MediaSort, TagReader, TagRepository,
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
    pub refs: BTreeMap<String, AssetExternalRef>,
    pub activity: Vec<ActivityEvent>,
    pub tags: BTreeMap<String, Tag>,
    pub memberships: BTreeSet<(String, String)>,
    pub search_docs: BTreeMap<String, SearchDocument>,
}

impl Default for MemStore {
    fn default() -> Self {
        MemStore {
            now: chrono::Utc::now(),
            inject_media_get_error: false,
            assets: Default::default(),
            media: Default::default(),
            refs: Default::default(),
            activity: Default::default(),
            tags: Default::default(),
            memberships: Default::default(),
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

    fn external_refs(&mut self) -> &mut dyn ExternalRefRepository {
        self
    }

    fn activity(&mut self) -> &mut dyn ActivityRepository {
        self
    }

    fn tags(&mut self) -> &mut dyn TagRepository {
        self
    }

    fn search_index(&mut self) -> &mut dyn SearchIndex {
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

    fn external_refs(&mut self) -> &mut dyn ExternalRefReader {
        self
    }

    fn activity(&mut self) -> &mut dyn ActivityReader {
        self
    }

    fn tags(&mut self) -> &mut dyn TagReader {
        self
    }

    fn search_index(&mut self) -> &mut dyn SearchReader {
        self
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
}
