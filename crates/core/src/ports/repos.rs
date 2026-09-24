//! Repository contracts, split into read (query) and write capabilities.
//!
//! Read scopes (`QueryUnitOfWork`) hand out only `*Reader` trait objects, so
//! mutation methods are unavailable at the type level; write scopes
//! (`UnitOfWork`) hand out the full repositories (ADR 0007).
//!
//! Implementations live in infrastructure adapters (SQLite) and test doubles;
//! application services never touch SQL.

use crate::domain::activity::ActivityEvent;
use crate::domain::asset::{Asset, AssetKind, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::{AssetId, ExternalRefId, RelationId, TagId};
use crate::domain::media::{MediaEntry, MediaRecord, MediaStatus, MediaType};
use crate::domain::relation::Relation;
use crate::domain::service::{ServiceEntry, ServiceRecord, ServiceType};
use crate::domain::software::{InstallSource, SoftwareCategory, SoftwareEntry, SoftwareRecord};
use crate::domain::tag::Tag;
use crate::domain::Timestamp;
use crate::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PAGE_LIMIT: usize = 50;
pub const MAX_PAGE_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryModule {
    Media,
    Software,
    Services,
}

impl LibraryModule {
    pub const ALL: [LibraryModule; 3] = [
        LibraryModule::Media,
        LibraryModule::Software,
        LibraryModule::Services,
    ];

    pub const fn as_str(&self) -> &'static str {
        match self {
            LibraryModule::Media => "media",
            LibraryModule::Software => "software",
            LibraryModule::Services => "services",
        }
    }

    pub fn of_kind(kind: AssetKind) -> Self {
        match kind.module() {
            "software" => LibraryModule::Software,
            "services" => LibraryModule::Services,
            _ => LibraryModule::Media,
        }
    }

    pub fn matches(&self, kind: AssetKind) -> bool {
        Self::of_kind(kind) == *self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibrarySort {
    #[default]
    UpdatedDesc,
    UpdatedAsc,
    NameAsc,
    NameDesc,
    KindAsc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub limit: usize,
    pub offset: usize,
}

impl Default for PageRequest {
    fn default() -> Self {
        PageRequest {
            limit: DEFAULT_PAGE_LIMIT,
            offset: 0,
        }
    }
}

impl PageRequest {
    pub fn new(limit: usize, offset: usize) -> Self {
        PageRequest { limit, offset }
    }

    pub fn effective_limit(&self) -> usize {
        match self.limit {
            0 => DEFAULT_PAGE_LIMIT,
            limit => limit.min(MAX_PAGE_LIMIT),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: Option<usize>,
}

impl<T> Page<T> {
    pub fn empty(page: &PageRequest) -> Self {
        Page {
            items: Vec::new(),
            offset: page.offset,
            limit: page.effective_limit(),
            total: Some(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSummary {
    pub id: AssetId,
    pub kind: AssetKind,
    pub name: String,
    pub lifecycle: LifecycleState,
    pub revision: i64,
    pub subtitle: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryQuery {
    pub lifecycle: LifecycleFilter,
    pub modules: Vec<LibraryModule>,
    pub kinds: Vec<AssetKind>,
    pub tags: Vec<String>,
    pub sort: LibrarySort,
    pub page: PageRequest,
}

pub trait LibraryReadPort {
    fn query_library(&mut self, query: &LibraryQuery) -> AppResult<Page<AssetSummary>>;
    /// Hydrate only the indexed candidates, never the whole library.
    fn hydrate_candidates(&mut self, ids: &[AssetId]) -> AppResult<Vec<AssetSummary>>;
}

/// Filter for base-asset listing.
#[derive(Debug, Clone, Default)]
pub struct AssetFilter {
    pub kind: Option<crate::domain::asset::AssetKind>,
    pub lifecycle: Option<LifecycleFilter>,
}

/// Lifecycle filter. `Active` (the default) excludes merged tombstones and
/// archived assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifecycleFilter {
    All,
    #[default]
    Active,
    ActiveOrArchived,
}

pub trait AssetReader {
    fn get(&mut self, id: AssetId) -> AppResult<Option<Asset>>;
    fn list(&mut self, filter: &AssetFilter) -> AppResult<Vec<Asset>>;
}

pub trait AssetRepository: AssetReader {
    fn insert(&mut self, asset: &Asset) -> AppResult<()>;
    fn update(&mut self, asset: &Asset) -> AppResult<()>;
}

/// Sort orders for media listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaSort {
    /// Most recently updated first.
    #[default]
    UpdatedDesc,
    TitleAsc,
    RatingDesc,
    CompletedDesc,
}

/// Typed media queries. Structured filtering remains typed application
/// behavior; full-text search never replaces these filters (ADR 0006).
#[derive(Debug, Clone, Default)]
pub struct MediaFilter {
    pub media_type: Option<MediaType>,
    pub status: Option<MediaStatus>,
    pub tag: Option<String>,
    pub platform: Option<String>,
    pub sort: MediaSort,
}

/// Media list rows carry their tags so list views avoid N+1 lookups at the
/// application layer: the repository resolves a whole page's tags in one query
/// (see `TagReader::list_for_assets`), not one per row.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaListRow {
    pub entry: MediaEntry,
    pub tags: Vec<String>,
}

pub trait MediaReader {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<MediaRecord>>;
    fn list(&mut self, filter: &MediaFilter) -> AppResult<Vec<MediaListRow>>;
    /// All media records, unsorted — used by projection rebuild and export.
    fn list_all(&mut self) -> AppResult<Vec<MediaRecord>>;
}

pub trait MediaRepository: MediaReader {
    fn upsert(&mut self, record: &MediaRecord) -> AppResult<()>;
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()>;
}

/// Sort orders for software listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoftwareSort {
    /// Most recently updated first.
    #[default]
    UpdatedDesc,
    TitleAsc,
}

/// Typed software queries. Structured filtering remains typed application
/// behavior; full-text search never replaces these filters (ADR 0006).
#[derive(Debug, Clone, Default)]
pub struct SoftwareFilter {
    pub category: Option<SoftwareCategory>,
    pub install_source: Option<InstallSource>,
    pub tag: Option<String>,
    pub sort: SoftwareSort,
}

/// Software list rows carry their tags so list views avoid N+1 lookups at the
/// application layer: the repository resolves a whole page's tags in one query
/// (see `TagReader::list_for_assets`), not one per row.
#[derive(Debug, Clone, PartialEq)]
pub struct SoftwareListRow {
    pub entry: SoftwareEntry,
    pub tags: Vec<String>,
}

pub trait SoftwareReader {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<SoftwareRecord>>;
    fn list(&mut self, filter: &SoftwareFilter) -> AppResult<Vec<SoftwareListRow>>;
    /// All software records, unsorted — used by projection rebuild and export.
    fn list_all(&mut self) -> AppResult<Vec<SoftwareRecord>>;
}

pub trait SoftwareRepository: SoftwareReader {
    fn upsert(&mut self, record: &SoftwareRecord) -> AppResult<()>;
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()>;
}

/// Sort orders for service listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServiceSort {
    /// Most recently updated first.
    #[default]
    UpdatedDesc,
    TitleAsc,
    /// Soonest renewal first; services without a renewal date sort last.
    RenewsAsc,
}

/// Typed service queries. Structured filtering remains typed application
/// behavior; full-text search never replaces these filters (ADR 0006).
#[derive(Debug, Clone, Default)]
pub struct ServiceFilter {
    pub service_type: Option<ServiceType>,
    /// Substring match on the provider name (case-insensitive).
    pub provider: Option<String>,
    pub tag: Option<String>,
    pub sort: ServiceSort,
}

/// Service list rows carry their tags so list views avoid N+1 lookups at the
/// application layer: the repository resolves a whole page's tags in one query
/// (see `TagReader::list_for_assets`), not one per row.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceListRow {
    pub entry: ServiceEntry,
    pub tags: Vec<String>,
}

pub trait ServiceReader {
    fn get(&mut self, asset_id: AssetId) -> AppResult<Option<ServiceRecord>>;
    fn list(&mut self, filter: &ServiceFilter) -> AppResult<Vec<ServiceListRow>>;
    /// All service records, unsorted — used by projection rebuild.
    fn list_all(&mut self) -> AppResult<Vec<ServiceRecord>>;
}

pub trait ServiceRepository: ServiceReader {
    fn upsert(&mut self, record: &ServiceRecord) -> AppResult<()>;
    fn delete(&mut self, asset_id: AssetId) -> AppResult<()>;
}

pub trait RelationReader {
    fn get(&mut self, id: RelationId) -> AppResult<Option<Relation>>;
    /// Relations stored with this asset as either endpoint.
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<Relation>>;
    /// Every relation touching any of these assets, ordered by identity.
    ///
    /// The batch form graph traversal needs: expanding one BFS frontier is a
    /// single query instead of one per visited node. An empty slice yields no
    /// rows rather than an error.
    fn list_for_assets(&mut self, asset_ids: &[AssetId]) -> AppResult<Vec<Relation>>;
    /// Every relation, ordered by identity — used by export and merge.
    fn list_all(&mut self) -> AppResult<Vec<Relation>>;
}

pub trait RelationRepository: RelationReader {
    /// Inserts a relation. Fails with [`AppError::Conflict`] when the same
    /// (source, target, type) relation — including the mirror-image row of a
    /// symmetric type — already exists.
    fn insert(&mut self, relation: &Relation) -> AppResult<()>;
    /// Re-points one endpoint at another asset (merge move).
    fn update(&mut self, relation: &Relation) -> AppResult<()>;
    fn delete(&mut self, id: RelationId) -> AppResult<()>;
}

pub trait ExternalRefReader {
    fn get(&mut self, id: ExternalRefId) -> AppResult<Option<AssetExternalRef>>;
    /// Deterministic lookup used by import/discovery matching (ADR 0005).
    fn find_asset_by_ref(
        &mut self,
        namespace: &str,
        external_id: &str,
    ) -> AppResult<Option<AssetId>>;
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<AssetExternalRef>>;
    fn list_all(&mut self) -> AppResult<Vec<AssetExternalRef>>;
}

pub trait ExternalRefRepository: ExternalRefReader {
    /// Inserts a reference. Fails with [`AppError::Conflict`] when the
    /// `(namespace, external_id)` pair is already attached to any asset.
    fn insert(&mut self, reference: &AssetExternalRef) -> AppResult<()>;
    /// Re-points an existing reference at a different asset (merge move).
    fn update(&mut self, reference: &AssetExternalRef) -> AppResult<()>;
    fn delete(&mut self, id: ExternalRefId) -> AppResult<()>;
}

pub trait ActivityReader {
    fn list_for_asset(&mut self, asset_id: AssetId, limit: usize) -> AppResult<Vec<ActivityEvent>>;
    /// Most recent events first.
    fn list_recent(&mut self, limit: usize) -> AppResult<Vec<ActivityEvent>>;
    /// Every event, oldest first — used by export.
    fn list_all(&mut self) -> AppResult<Vec<ActivityEvent>>;
    fn get(&mut self, id: crate::domain::ids::ActivityId) -> AppResult<Option<ActivityEvent>>;
}

pub trait ActivityRepository: ActivityReader {
    fn append(&mut self, event: &ActivityEvent) -> AppResult<()>;
    /// Replaces an event wholesale (portable import upsert by id).
    fn upsert(&mut self, event: &ActivityEvent) -> AppResult<()>;
}

pub trait TagReader {
    fn find_by_name(&mut self, name: &str) -> AppResult<Option<Tag>>;
    fn get(&mut self, id: TagId) -> AppResult<Option<Tag>>;
    fn list_for_asset(&mut self, asset_id: AssetId) -> AppResult<Vec<Tag>>;
    /// Every tag attached to any of these assets, as `(asset_id, tag)` pairs.
    ///
    /// The batch form the module list queries need: rendering a page of rows
    /// with their tags is one query instead of one per row, which is the
    /// difference between a constant and a linear number of round-trips. An
    /// empty slice yields no rows rather than an error.
    ///
    /// Ordered by `(asset_id, name)` so a caller can group the result by
    /// walking it in order, and so the grouping is stable across adapters.
    fn list_for_assets(&mut self, asset_ids: &[AssetId]) -> AppResult<Vec<(AssetId, Tag)>>;
    fn list_all(&mut self) -> AppResult<Vec<Tag>>;
    /// `(asset_id, tag_id)` membership pairs — used by export and merge.
    fn list_memberships(&mut self) -> AppResult<Vec<(AssetId, TagId)>>;
}

pub trait TagRepository: TagReader {
    /// Returns the tag with exactly this name, creating it if needed.
    fn ensure(&mut self, name: &str) -> AppResult<Tag>;
    /// Inserts a tag with a specific id (portable import preserves identity).
    fn insert(&mut self, tag: &Tag) -> AppResult<()>;
    /// Replaces a tag by id.
    fn update(&mut self, tag: &Tag) -> AppResult<()>;
    fn attach(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()>;
    fn detach(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()>;
    fn insert_membership(&mut self, asset_id: AssetId, tag_id: TagId) -> AppResult<()>;
}

pub trait SearchReader {
    /// Full-text query with relevance ordering; implementations must serve
    /// substring/CJK queries too and never silently drop storage errors.
    fn search(
        &mut self,
        query: &str,
        limit: usize,
    ) -> AppResult<Vec<crate::domain::search::SearchHit>>;
}

/// Search index writer, implemented by the SQLite adapter over an FTS5
/// table; the index is derived state and must be droppable/rebuildable.
pub trait SearchIndex: SearchReader {
    fn upsert(&mut self, document: &crate::domain::search::SearchDocument) -> AppResult<()>;
    fn remove(&mut self, asset_id: AssetId) -> AppResult<()>;
    /// Removes every document. The index must be rebuildable afterwards.
    fn clear(&mut self) -> AppResult<()>;
    /// Atomically replaces the whole projection (rebuild path).
    fn replace_all(&mut self, documents: &[crate::domain::search::SearchDocument])
        -> AppResult<()>;
}

/// Helper for repositories that check uniqueness of external refs.
pub fn ref_conflict(namespace: &str, external_id: &str) -> AppError {
    AppError::conflict(format!(
        "external ref {namespace}:{external_id} is already attached to another asset"
    ))
}
