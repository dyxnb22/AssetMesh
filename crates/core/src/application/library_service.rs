//! Unified library query use cases — Phase 4A (docs/11).
//!
//! Media, Software, and Services each own typed details and their own
//! module-specific list/filter/sort API (still used by module pages). This
//! module adds the *global* library contract on top of them: one application
//! boundary an adapter can use to open a complete asset detail, list the whole
//! library, and search it, without knowing which module owns an asset.
//!
//! Rules this module upholds:
//!
//! - **Module dispatch stays here.** Adapters never branch on
//!   `asset.kind` to reach a module repository themselves.
//! - **Typed details stay typed.** [`AssetDetails`] is a union of the module
//!   records; nothing collapses into `serde_json::Value`.
//! - **One request, one snapshot.** Every unified read runs inside a single
//!   [`QueryUnitOfWork`] scope, so asset, module details, and tags cannot be
//!   torn by a concurrent commit (ADR 0007).
//! - **Deterministic ordering.** Every sort has `AssetId` as its secondary
//!   key, so pagination never repeats or drops a row.
//! - **No new business rules.** Module validation, search ranking, tag
//!   loading, lifecycle semantics, and identity rules are reused as-is.

use std::collections::HashMap;

use crate::application::projection::{media_subtitle, service_subtitle, software_subtitle};
use crate::domain::asset::{Asset, AssetKind, LifecycleState};
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::media::MediaRecord;
use crate::domain::service::ServiceRecord;
use crate::domain::software::SoftwareRecord;
use crate::domain::Timestamp;
use crate::ports::repos::{LifecycleFilter, MediaFilter, ServiceFilter, SoftwareFilter};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};

/// Page size used when a caller does not ask for one.
pub const DEFAULT_PAGE_LIMIT: usize = 50;

/// Upper bound on a page size. Adapters cannot ask the library for an
/// unbounded result set; a larger window is fetched as several pages.
pub const MAX_PAGE_LIMIT: usize = 200;

/// Upper bound on the projection window one search page hydrates.
///
/// A search page is hydrated by asking the index for `offset + limit` hits, so
/// a pathologically deep offset would otherwise ask the index for an
/// unbounded number of rows (and overflow the limit the search port takes).
/// Past this bound a search page is simply empty rather than expensive.
const MAX_SEARCH_WINDOW: usize = 10_000;

/// The coarse module grouping of an asset kind, derived from
/// [`AssetKind::module`]. It is a filter over kinds, not a second identity
/// system: every kind belongs to exactly one module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryModule {
    Media,
    Software,
    Services,
}

impl LibraryModule {
    /// All modules the unified library can dispatch to.
    pub const ALL: [LibraryModule; 3] = [
        LibraryModule::Media,
        LibraryModule::Software,
        LibraryModule::Services,
    ];

    /// The module string used by [`AssetKind::module`].
    pub const fn as_str(&self) -> &'static str {
        match self {
            LibraryModule::Media => "media",
            LibraryModule::Software => "software",
            LibraryModule::Services => "services",
        }
    }

    /// The module that owns `kind`'s typed details.
    ///
    /// Total over the three shipped modules. The wildcard arm is unreachable
    /// today — `AssetKind::module()` returns only these three strings — and is
    /// pinned by a test over every kind so that adding a fourth module fails a
    /// test here rather than silently folding its kinds into Media.
    pub fn of_kind(kind: AssetKind) -> Self {
        match kind.module() {
            "software" => LibraryModule::Software,
            "services" => LibraryModule::Services,
            _ => LibraryModule::Media,
        }
    }

    /// True when this module owns `kind`'s typed details.
    pub fn matches(&self, kind: AssetKind) -> bool {
        Self::of_kind(kind) == *self
    }
}

/// Sort order for a unified library page. Every variant uses [`AssetId`]
/// ascending as its deterministic tie-breaker, so a page boundary can never
/// repeat or skip a row whose primary sort key is equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibrarySort {
    /// Most recently updated first (default).
    #[default]
    UpdatedDesc,
    /// Least recently updated first.
    UpdatedAsc,
    /// Name ascending, case-insensitive.
    NameAsc,
    /// Name descending, case-insensitive.
    NameDesc,
    /// Asset kind ascending, then name.
    KindAsc,
}

/// One page request: a bounded size plus an offset into the sorted result set.
///
/// Offset pagination is used because the unified list is composed in the
/// application layer from the module readers; ordering is fully determined by
/// [`LibrarySort`] plus the `AssetId` tie-breaker, so offsets are stable
/// against a static library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    /// Rows per page. `0` selects [`DEFAULT_PAGE_LIMIT`]; anything above
    /// [`MAX_PAGE_LIMIT`] is clamped to it.
    pub limit: usize,
    /// Rows to skip before the page starts.
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

    /// The page size actually used after defaults and clamping. Adapters can
    /// read back what a request resolved to instead of re-deriving the rule.
    pub fn effective_limit(&self) -> usize {
        match self.limit {
            0 => DEFAULT_PAGE_LIMIT,
            limit => limit.min(MAX_PAGE_LIMIT),
        }
    }
}

/// One page of unified results.
///
/// `total` is the exact number of matching rows whenever the query can count
/// them: always for the library list, and for a library search whose index was
/// exhausted (the common case). It is `None` only when a search stopped at the
/// projection window bound, where more hits may exist beyond it. Adapters can
/// therefore treat `items.len() < limit` as "no more pages" whenever `total` is
/// `Some`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: Option<usize>,
}

impl<T> Page<T> {
    fn empty(page: &PageRequest) -> Self {
        Page {
            items: Vec::new(),
            offset: page.offset,
            limit: page.effective_limit(),
            total: Some(0),
        }
    }
}

/// One library row: shared identity, typed module details, and tags.
///
/// This is the vocabulary shared by the unified list and the unified search:
/// both return `AssetSummary`, so an adapter never has to reconcile two
/// competing shapes.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AssetSummary {
    pub id: AssetId,
    pub kind: AssetKind,
    pub name: String,
    pub lifecycle: LifecycleState,
    pub revision: i64,
    /// Concise module-aware summary, produced by the same helper the module's
    /// search projection uses.
    pub subtitle: Option<String>,
    /// Tag names, sorted by name.
    pub tags: Vec<String>,
    pub updated_at: Timestamp,
}

/// Typed module details of one asset (docs/11).
///
/// A future module adds a variant; adapters keep reading one union instead of
/// learning another repository. The records are read-only views here: the
/// library never mutates them, and nothing in this union is untyped.
///
/// Serialized as `{"module": "media", …record fields}`, so a transport
/// consumer sees the same module vocabulary as [`AssetKind::module`] without a
/// wrapper object per variant.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "module", rename_all = "snake_case")]
pub enum AssetDetails {
    Media(MediaRecord),
    Software(SoftwareRecord),
    /// Tagged `services`, matching `AssetKind::module()` for service kinds.
    #[serde(rename = "services")]
    Service(ServiceRecord),
}

/// The complete unified detail view of one asset, read from one snapshot.
///
/// Relations are deliberately not part of this view: the relation query
/// service is Phase 4B (docs/11) and will compose its own bounded views.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AssetDetailView {
    pub asset: Asset,
    pub details: AssetDetails,
    /// Tag names, sorted by name.
    pub tags: Vec<String>,
    /// External references, sorted by `(namespace, external_id)`.
    pub external_refs: Vec<AssetExternalRef>,
}

/// The remains of an asset that lost a merge (ADR 0005).
///
/// A tombstone is a redirect, not a library entry: it carries no module
/// details, only the identity the UI needs to name what went away and point at
/// the survivor. This view exists so an adapter never has to reach into
/// repositories to render one — the same rule as [`AssetDetailView`].
///
/// `tags` and `external_refs` are usually empty on purpose: a canonical merge
/// ([`AssetService::merge_assets`]) moves both onto the survivor and detaches
/// them from the loser. They are still part of the view so a tombstone created
/// by any other path renders through the same code without a special case.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MergedTombstoneView {
    pub asset: Asset,
    /// Tag names, sorted by name.
    pub tags: Vec<String>,
    /// External references, sorted by `(namespace, external_id)`.
    pub external_refs: Vec<AssetExternalRef>,
}

/// The typed outcome of requesting an asset's details from the unified library.
///
/// An asset in the library is either live (active or archived, with typed
/// module details) or a tombstone redirect (merged into a survivor, with no
/// module details of its own).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum AssetDetailOutcome {
    Live(AssetDetailView),
    MergedRedirect(MergedTombstoneView),
}

impl AssetDetailOutcome {
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    pub fn is_merged_redirect(&self) -> bool {
        matches!(self, Self::MergedRedirect(_))
    }

    pub fn asset(&self) -> &Asset {
        match self {
            Self::Live(view) => &view.asset,
            Self::MergedRedirect(tombstone) => &tombstone.asset,
        }
    }
}

/// A unified library list query over shared fields only.
///
/// Module-specific advanced filters (media rating, software install source,
/// service renewal date, ...) stay in the module services; forcing them into
/// one generic filter would be a second, weaker copy of each module's API.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryQuery {
    /// Which lifecycle states are visible. Defaults to
    /// [`LifecycleFilter::Active`]: merged tombstones are redirects and
    /// archived assets are opt-in.
    pub lifecycle: LifecycleFilter,
    /// Restrict to these modules. Empty means every module.
    pub modules: Vec<LibraryModule>,
    /// Restrict to these asset kinds. Empty means every kind. Combined with
    /// `modules`, both must match.
    pub kinds: Vec<AssetKind>,
    /// Require every one of these tags (case-insensitive). Empty means no tag
    /// filter.
    pub tags: Vec<String>,
    pub sort: LibrarySort,
    pub page: PageRequest,
}

/// A unified library search query.
///
/// The search projection stays the search engine (ADR 0006): this contract
/// only adds lifecycle/module/tag filtering and maps hits into the same
/// [`AssetSummary`] vocabulary the list uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySearchQuery {
    /// Free-text query handed to the projection unchanged.
    pub text: String,
    /// Same semantics as [`LibraryQuery::lifecycle`]. Archived assets are
    /// searchable in the projection, so they are opt-in here too.
    pub lifecycle: LifecycleFilter,
    /// Restrict to these modules. Empty means every module.
    pub modules: Vec<LibraryModule>,
    /// Restrict to these asset kinds. Empty means every kind.
    pub kinds: Vec<AssetKind>,
    /// Require every one of these tags (case-insensitive).
    pub tags: Vec<String>,
    pub page: PageRequest,
}

/// Feature flags declaring available subsystem capabilities (docs/12 Section 6.3 & 6.5).
/// Prevents UI from rendering empty shell placeholders for unreached phases.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppFeatures {
    pub runtime_enrichment: bool,
    pub projects: bool,
    pub agent_capabilities: bool,
    pub knowledge_collections: bool,
}

/// Application capabilities exposed to adapters (CLI, Desktop UI, HTTP).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppCapabilities {
    pub version: String,
    pub modules: Vec<String>,
    pub asset_kinds: Vec<String>,
    pub relation_types: Vec<String>,
    pub storable_relation_types: Vec<String>,
    pub features: AppFeatures,
}

impl AppCapabilities {
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            modules: LibraryModule::ALL
                .iter()
                .map(|m| m.as_str().to_string())
                .collect(),
            asset_kinds: AssetKind::ALL
                .iter()
                .map(|k| k.as_str().to_string())
                .collect(),
            relation_types: crate::domain::relation::ALL_TYPES
                .iter()
                .map(|r| r.as_str().to_string())
                .collect(),
            storable_relation_types: crate::domain::relation::ALL_TYPES
                .iter()
                .filter(|r| r.is_storable())
                .map(|r| r.as_str().to_string())
                .collect(),
            features: AppFeatures {
                runtime_enrichment: false,
                projects: false,
                agent_capabilities: false,
                knowledge_collections: false,
            },
        }
    }
}

/// The unified library application boundary (docs/11 Phase 4A).
#[derive(Debug, Clone)]
pub struct LibraryService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> LibraryService<F> {
    pub fn new(factory: F) -> Self {
        LibraryService { factory }
    }

    /// Exposes application-level capabilities and feature flags without
    /// touching storage.
    pub fn capabilities(&self) -> AppCapabilities {
        AppCapabilities::current()
    }

    /// Returns the typed detail outcome for an asset: [`AssetDetailOutcome::Live`]
    /// for an active or archived asset with module details, or
    /// [`AssetDetailOutcome::MergedRedirect`] for a tombstone naming the survivor.
    ///
    /// Unknown ids return `AppError::NotFound`.
    pub fn get_detail(&mut self, asset_id: AssetId) -> AppResult<AssetDetailOutcome> {
        self.factory.read(&mut |q| load_detail_outcome(q, asset_id))
    }

    /// Opens one asset's complete typed detail view from a single read
    /// snapshot.
    ///
    /// Errors:
    /// - unknown id → `not_found`;
    /// - a merged tombstone → `conflict` naming the surviving asset (a
    ///   tombstone is a redirect, not a library entry; use
    ///   [`LibraryService::resolve_merge_redirect`] to follow it);
    /// - an asset with no module details → `not_found`, because the library
    ///   only contains assets a module owns.
    ///
    /// Archived assets are readable: archiving only blocks mutation.
    pub fn get_asset(&mut self, asset_id: AssetId) -> AppResult<AssetDetailView> {
        self.factory.read(&mut |q| load_detail(q, asset_id))
    }

    /// Loads the tombstone of an asset that lost a merge, so an adapter can
    /// render the redirect to the survivor instead of hand-rolling repository
    /// reads.
    ///
    /// Errors:
    /// - unknown id → `not_found`;
    /// - an asset that is NOT a tombstone → `conflict`, because this method
    ///   is only meaningful for merged losers — call `get_asset` for a live
    ///   asset. The distinction matters: `not_found` would let a caller mistake
    ///   a programming error for a missing row.
    ///
    /// [`get_asset`](Self::get_asset) reports a tombstone as a `conflict`
    /// naming the survivor, so the pair "try the detail, fall back to the
    /// tombstone" is the intended calling pattern.
    pub fn get_merged_tombstone(&mut self, asset_id: AssetId) -> AppResult<MergedTombstoneView> {
        self.factory.read(&mut |q| load_tombstone(q, asset_id))
    }

    /// Follows an explicit merge redirect to the surviving asset (ADR 0005).
    ///
    /// Returns the input unchanged for a live asset. Chains are followed with
    /// a visited set, so a corrupt redirect cycle fails loudly instead of
    /// looping. This exists so adapters never hand-roll redirect traversal.
    pub fn resolve_merge_redirect(&mut self, asset_id: AssetId) -> AppResult<AssetId> {
        self.factory.read(&mut |q| resolve_redirect(q, asset_id))
    }

    /// Lists the whole library as one page of [`AssetSummary`].
    pub fn list_assets(&mut self, query: &LibraryQuery) -> AppResult<Page<AssetSummary>> {
        let modules = selected_modules(&query.modules, &query.kinds);
        let limit = query.page.effective_limit();
        if modules.is_empty() {
            // Contradictory module/kind filters: nothing can match, so the
            // storage is not touched at all.
            return Ok(Page::empty(&query.page));
        }

        self.factory.read(&mut |q| {
            let mut rows = load_library_rows(q, &modules)?;
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
        })
    }

    /// Searches the whole library through the existing projection and returns
    /// the same [`AssetSummary`] vocabulary the list uses.
    ///
    /// Ranking is owned by the projection and is not modified here: results
    /// keep the order the index returned.
    ///
    /// Because the filters are applied after hydration, the library widens the
    /// window it asks the index for until the index runs out (bounded by
    /// [`MAX_SEARCH_WINDOW`]). That keeps two promises at once: a filter never
    /// makes a matching row unreachable by paging, and `Page::total` is the
    /// exact number of matches. Only a search that hits the window bound
    /// reports `total: None`, because more hits may exist beyond it.
    pub fn search_assets(&mut self, query: &LibrarySearchQuery) -> AppResult<Page<AssetSummary>> {
        let text = query.text.trim().to_string();
        let limit = query.page.effective_limit();
        let needed = query.page.offset.saturating_add(limit);
        if text.is_empty() {
            // Matches SearchService: an empty query is not an error, it is
            // simply no results.
            return Ok(Page::empty(&query.page));
        }
        let modules = selected_modules(&query.modules, &query.kinds);
        if modules.is_empty() {
            return Ok(Page::empty(&query.page));
        }

        self.factory.read(&mut |q| {
            let rows = load_library_rows(q, &modules)?;
            let by_asset: HashMap<AssetId, &LibraryRow> =
                rows.iter().map(|row| (row.asset.id, row)).collect();

            // The index is asked for a growing window, but each hit is hydrated
            // exactly once: hits for a larger limit are a prefix of the hits
            // for a smaller one, so only the new tail is processed.
            let mut matched: Vec<AssetSummary> = Vec::new();
            let mut hydrated = 0usize;
            let mut window = needed.min(MAX_SEARCH_WINDOW);
            let exhausted = loop {
                let hits = q.search_index().search(&text, window)?;
                for hit in hits.iter().skip(hydrated) {
                    hydrated += 1;
                    // A projection row without a module detail is stale
                    // derived state, not a library entry.
                    let Some(row) = by_asset.get(&hit.asset_id) else {
                        continue;
                    };
                    if !matches_lifecycle(query.lifecycle, row)
                        || !matches_kinds(&query.kinds, row)
                        || !matches_tags(&query.tags, row)
                    {
                        continue;
                    }
                    matched.push(summarize_row(row));
                }
                let exhausted = hits.len() < window;
                if exhausted || window >= MAX_SEARCH_WINDOW {
                    break exhausted;
                }
                window = window.saturating_mul(2).min(MAX_SEARCH_WINDOW);
            };

            let total = matched.len();
            let items = matched
                .into_iter()
                .skip(query.page.offset)
                .take(limit)
                .collect();
            Ok(Page {
                items,
                offset: query.page.offset,
                limit,
                // An exhausted index gives an exact count; a window-bound stop
                // does not, because more hits may exist beyond it.
                total: exhausted.then_some(total),
            })
        })
    }
}

/// One asset plus its typed module details and tags, as loaded from a module
/// reader.
pub(crate) struct LibraryRow {
    pub(crate) asset: Asset,
    pub(crate) details: AssetDetails,
    pub(crate) tags: Vec<String>,
}

/// Resolves the modules a query needs to read.
///
/// Empty filters mean "every module". Contradictory filters (a module and a
/// kind that module does not own) resolve to no modules, which lets the
/// caller short-circuit without reading storage.
fn selected_modules(modules: &[LibraryModule], kinds: &[AssetKind]) -> Vec<LibraryModule> {
    let wanted_modules: Vec<LibraryModule> = dedupe(modules);
    let wanted_from_kinds: Vec<LibraryModule> = dedupe(
        &kinds
            .iter()
            .map(|kind| LibraryModule::of_kind(*kind))
            .collect::<Vec<_>>(),
    );
    match (wanted_modules.is_empty(), wanted_from_kinds.is_empty()) {
        (true, true) => LibraryModule::ALL.to_vec(),
        (false, true) => wanted_modules,
        (true, false) => wanted_from_kinds,
        (false, false) => wanted_modules
            .into_iter()
            .filter(|module| wanted_from_kinds.contains(module))
            .collect(),
    }
}

fn dedupe<T: PartialEq + Copy>(values: &[T]) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for value in values {
        if !out.contains(value) {
            out.push(*value);
        }
    }
    out
}

/// Loads every library row of the selected modules.
///
/// This is the composition that keeps the unified query free of N+1 access:
/// each module reader is called exactly once, and module list rows already
/// carry their tags — the repository resolves a whole page's tags in one query
/// rather than one per row, so no per-asset tag, detail, or asset lookup is
/// issued. Pinned by `module_lists_load_tags_for_the_whole_page_in_one_query`
/// in the SQLite contract tests. See `DEVELOPMENT.md` for the documented
/// trade-off against a SQL-side paged query.
///
/// Shared with the Phase 4B graph queries, so graph nodes hydrate through the
/// same readers the library list uses.
pub(crate) fn load_library_rows(
    q: &mut dyn QueryUnitOfWork,
    modules: &[LibraryModule],
) -> AppResult<Vec<LibraryRow>> {
    let mut rows: Vec<LibraryRow> = Vec::new();
    for module in modules {
        match module {
            LibraryModule::Media => {
                for row in q.media().list(&MediaFilter::default())? {
                    push_row(
                        &mut rows,
                        module,
                        row.entry.asset,
                        AssetDetails::Media(row.entry.record),
                        row.tags,
                    );
                }
            }
            LibraryModule::Software => {
                for row in q.software().list(&SoftwareFilter::default())? {
                    push_row(
                        &mut rows,
                        module,
                        row.entry.asset,
                        AssetDetails::Software(row.entry.record),
                        row.tags,
                    );
                }
            }
            LibraryModule::Services => {
                for row in q.services().list(&ServiceFilter::default())? {
                    push_row(
                        &mut rows,
                        module,
                        row.entry.asset,
                        AssetDetails::Service(row.entry.record),
                        row.tags,
                    );
                }
            }
        }
    }
    Ok(rows)
}

/// Adds one module row to the library, unless its kind belongs to a different
/// module.
///
/// Defense in depth: the repository boundary already refuses to store a module
/// detail on an asset of another module, so this never fires for data written
/// through AssetMesh. It exists so a corrupted or hand-edited database cannot
/// surface as a summary whose kind and typed details disagree.
fn push_row(
    rows: &mut Vec<LibraryRow>,
    module: &LibraryModule,
    asset: Asset,
    details: AssetDetails,
    mut tags: Vec<String>,
) {
    if !module.matches(asset.kind) {
        return;
    }
    tags.sort();
    tags.dedup();
    rows.push(LibraryRow {
        asset,
        details,
        tags,
    });
}

/// Lifecycle visibility for a library row.
///
/// A merged tombstone is a redirect and is never a library row under *any*
/// filter — including `All`. The rule is enforced here rather than relying on
/// `merge_assets` having deleted the loser's module details, so a hand-edited
/// or partially-migrated database cannot surface a tombstone as an inventory
/// entry. `LifecycleFilter::All` therefore means "every non-tombstone
/// lifecycle", which for a library is the same set as `ActiveOrArchived`.
fn matches_lifecycle(lifecycle: LifecycleFilter, row: &LibraryRow) -> bool {
    if row.asset.lifecycle_state == LifecycleState::Merged {
        return false;
    }
    match lifecycle {
        LifecycleFilter::All | LifecycleFilter::ActiveOrArchived => true,
        LifecycleFilter::Active => row.asset.lifecycle_state == LifecycleState::Active,
    }
}

fn matches_kinds(kinds: &[AssetKind], row: &LibraryRow) -> bool {
    kinds.is_empty() || kinds.contains(&row.asset.kind)
}

/// Tag filtering requires every requested tag, compared case-insensitively —
/// the same tag semantics the module filters use.
fn matches_tags(tags: &[String], row: &LibraryRow) -> bool {
    tags.iter().all(|wanted| {
        let wanted = wanted.trim();
        row.tags.iter().any(|tag| tag.eq_ignore_ascii_case(wanted))
    })
}

/// Sorts by the requested key, then by `AssetId` ascending so equal keys still
/// have one deterministic order.
fn sort_rows(rows: &mut [LibraryRow], sort: LibrarySort) {
    rows.sort_by(|a, b| {
        let primary = match sort {
            LibrarySort::UpdatedDesc => b.asset.updated_at.cmp(&a.asset.updated_at),
            LibrarySort::UpdatedAsc => a.asset.updated_at.cmp(&b.asset.updated_at),
            LibrarySort::NameAsc => a
                .asset
                .name
                .to_lowercase()
                .cmp(&b.asset.name.to_lowercase()),
            LibrarySort::NameDesc => b
                .asset
                .name
                .to_lowercase()
                .cmp(&a.asset.name.to_lowercase()),
            LibrarySort::KindAsc => {
                a.asset
                    .kind
                    .as_str()
                    .cmp(b.asset.kind.as_str())
                    .then_with(|| {
                        a.asset
                            .name
                            .to_lowercase()
                            .cmp(&b.asset.name.to_lowercase())
                    })
            }
        };
        primary.then_with(|| a.asset.id.cmp(&b.asset.id))
    });
}

/// Builds the unified summary of one loaded library row.
pub(crate) fn summarize_row(row: &LibraryRow) -> AssetSummary {
    AssetSummary {
        id: row.asset.id,
        kind: row.asset.kind,
        name: row.asset.name.clone(),
        lifecycle: row.asset.lifecycle_state,
        revision: row.asset.revision,
        subtitle: subtitle_of(&row.details),
        tags: row.tags.clone(),
        updated_at: row.asset.updated_at,
    }
}

/// The module's own concise summary — never a second implementation of it.
fn subtitle_of(details: &AssetDetails) -> Option<String> {
    match details {
        AssetDetails::Media(record) => media_subtitle(record),
        AssetDetails::Software(record) => software_subtitle(record),
        AssetDetails::Service(record) => service_subtitle(record),
    }
}

/// The error a merged tombstone produces, naming the surviving asset.
///
/// Shared by the library detail query and the Phase 4B graph queries so both
/// answer "what happened to this id?" the same way (ADR 0005).
pub(crate) fn merged_redirect_error(asset: &Asset) -> AppError {
    let target = asset
        .merged_into
        .map(|id| id.to_string())
        .unwrap_or_else(|| "an unknown asset".to_string());
    AppError::conflict(format!(
        "asset {} was merged into {target}; request the surviving asset instead",
        asset.id
    ))
}

/// Loads one typed detail outcome (Live or MergedRedirect) inside the caller's read scope.
fn load_detail_outcome(
    q: &mut dyn QueryUnitOfWork,
    asset_id: AssetId,
) -> AppResult<AssetDetailOutcome> {
    let asset = q
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;

    let mut tags: Vec<String> = q
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|tag| tag.name)
        .collect();
    tags.sort();
    let mut external_refs = q.external_refs().list_for_asset(asset_id)?;
    external_refs
        .sort_by(|a, b| (&a.namespace, &a.external_id).cmp(&(&b.namespace, &b.external_id)));

    if asset.lifecycle_state == LifecycleState::Merged {
        Ok(AssetDetailOutcome::MergedRedirect(MergedTombstoneView {
            asset,
            tags,
            external_refs,
        }))
    } else {
        let details = load_details(q, &asset)?;
        Ok(AssetDetailOutcome::Live(AssetDetailView {
            asset,
            details,
            tags,
            external_refs,
        }))
    }
}

/// Loads one complete detail view inside the caller's read scope.
fn load_detail(q: &mut dyn QueryUnitOfWork, asset_id: AssetId) -> AppResult<AssetDetailView> {
    let asset = q
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;

    if asset.lifecycle_state == LifecycleState::Merged {
        // A tombstone is a redirect, not a library entry: name the survivor so
        // the caller can request it instead of silently showing stale data.
        return Err(merged_redirect_error(&asset));
    }

    let details = load_details(q, &asset)?;
    let mut tags: Vec<String> = q
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|tag| tag.name)
        .collect();
    tags.sort();
    let mut external_refs = q.external_refs().list_for_asset(asset_id)?;
    external_refs
        .sort_by(|a, b| (&a.namespace, &a.external_id).cmp(&(&b.namespace, &b.external_id)));

    Ok(AssetDetailView {
        asset,
        details,
        tags,
        external_refs,
    })
}

/// Loads a merged tombstone inside the caller's read scope. Mirrors
/// [`load_detail`] field for field, minus the module details a tombstone no
/// longer owns.
fn load_tombstone(
    q: &mut dyn QueryUnitOfWork,
    asset_id: AssetId,
) -> AppResult<MergedTombstoneView> {
    let asset = q
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;

    if asset.lifecycle_state != LifecycleState::Merged {
        return Err(AppError::conflict(format!(
            "asset {asset_id} is not a merged tombstone; use the detail view for a live asset"
        )));
    }

    let mut tags: Vec<String> = q
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|tag| tag.name)
        .collect();
    tags.sort();
    let mut external_refs = q.external_refs().list_for_asset(asset_id)?;
    external_refs
        .sort_by(|a, b| (&a.namespace, &a.external_id).cmp(&(&b.namespace, &b.external_id)));

    Ok(MergedTombstoneView {
        asset,
        tags,
        external_refs,
    })
}

/// Loads the typed module details the asset's kind owns, or `None`-free error
/// when the asset has none.
///
/// Shared with the Phase 4B graph queries so module dispatch has exactly one
/// implementation.
pub(crate) fn load_details_for(
    q: &mut dyn QueryUnitOfWork,
    asset: &Asset,
) -> AppResult<AssetDetails> {
    load_details(q, asset)
}

/// Loads the typed module details the asset's kind owns. Dispatch is by
/// [`AssetKind::module`], so a new module extends one match arm here and
/// nowhere else in the library contract.
fn load_details(q: &mut dyn QueryUnitOfWork, asset: &Asset) -> AppResult<AssetDetails> {
    match asset.kind.module() {
        "media" => q
            .media()
            .get(asset.id)?
            .map(AssetDetails::Media)
            .ok_or_else(|| AppError::not_found("module details", asset.id)),
        "software" => q
            .software()
            .get(asset.id)?
            .map(AssetDetails::Software)
            .ok_or_else(|| AppError::not_found("module details", asset.id)),
        "services" => q
            .services()
            .get(asset.id)?
            .map(AssetDetails::Service)
            .ok_or_else(|| AppError::not_found("module details", asset.id)),
        other => Err(AppError::storage(format!(
            "asset kind {} belongs to unknown module {other:?}",
            asset.kind
        ))),
    }
}

/// Follows `merged_into` until a live asset is reached. The visited set makes a
/// corrupt redirect cycle a loud failure rather than an infinite loop.
fn resolve_redirect(q: &mut dyn QueryUnitOfWork, asset_id: AssetId) -> AppResult<AssetId> {
    let mut current = asset_id;
    let mut visited: Vec<AssetId> = Vec::new();
    loop {
        if visited.contains(&current) {
            return Err(AppError::conflict(format!(
                "merge redirect cycle detected at asset {current}"
            )));
        }
        visited.push(current);

        let asset = q
            .assets()
            .get(current)?
            .ok_or_else(|| AppError::not_found("asset", current))?;
        if asset.lifecycle_state != LifecycleState::Merged {
            return Ok(current);
        }
        let Some(next) = asset.merged_into else {
            // `Asset::validate` forbids this state; reaching it means the
            // stored data is corrupt, so fail loudly instead of guessing.
            return Err(AppError::storage(format!(
                "merged asset {current} has no merge target"
            )));
        };
        current = next;
    }
}
