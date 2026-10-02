//! Cross-module activity query use cases — Phase 4C (docs/11).
//!
//! Activity is append-oriented historical provenance: every event was written
//! by a use case that changed canonical state, and this service only reads it
//! back. It never synthesizes an event, never repairs one, and never loses
//! history because an asset was later archived or merged.
//!
//! Rules this module upholds:
//!
//! - **One query, one snapshot.** The events and the asset names that explain
//!   them are read inside a single [`QueryUnitOfWork`], so a page cannot mix
//!   two moments of history.
//! - **Stable ordering.** Newest first, with the event id as the tie-breaker:
//!   two events sharing a timestamp still have one order, so pagination never
//!   repeats or skips a row.
//! - **Module classification is centralized.** [`ActivityModule`] is the only
//!   place an event type is mapped to a subsystem; adapters filter by it
//!   instead of string-matching prefixes.
//! - **History survives lifecycle changes.** A merged or archived asset's
//!   events are still reported, with the asset named as it stands today.

use crate::application::library_service::{Page, PageRequest};
use crate::domain::activity::{ActivityEvent, ActivityModule};
use crate::domain::asset::{Asset, AssetKind};
use crate::domain::ids::{ActivityId, AssetId};
use crate::domain::Timestamp;
use crate::ports::uow::UnitOfWorkFactory;
use crate::AppResult;
use serde::Serialize;

/// One activity event as an adapter sees it.
///
/// The payload is the canonical event's own JSON, untouched: it is recorded
/// provenance, not a substitute for typed module fields. `asset_name` is
/// resolved at query time, so a later rename, archive, or merge does not erase
/// what the event says happened.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActivityView {
    pub id: ActivityId,
    pub event_type: String,
    /// The subsystem that owns this event, when recognized.
    pub module: Option<ActivityModule>,
    pub occurred_at: Timestamp,
    pub actor: String,
    pub asset_id: Option<AssetId>,
    /// The asset's current name, when the asset still exists.
    pub asset_name: Option<String>,
    pub payload: serde_json::Value,
}

/// A cross-module activity query.
///
/// Every filter is optional and they compose with AND. An empty query is "the
/// most recent events", which is what a global activity feed wants.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActivityQuery {
    /// Only events about this asset.
    pub asset_id: Option<AssetId>,
    /// Only these event types (exact match).
    pub event_types: Vec<String>,
    /// Only these subsystems.
    pub modules: Vec<ActivityModule>,
    /// Only events about assets of these kinds (docs/12: filter by kind).
    ///
    /// Distinct from `modules`: a module owns several kinds (Media owns
    /// `media.anime`, `media.movie`, ...), so "only movie events" is a question
    /// `modules` cannot answer.
    pub kinds: Vec<AssetKind>,
    /// Only these actors (exact match).
    pub actors: Vec<String>,
    /// Inclusive lower bound on `occurred_at`.
    pub since: Option<Timestamp>,
    /// Inclusive upper bound on `occurred_at`.
    pub until: Option<Timestamp>,
    pub page: PageRequest,
}

impl ActivityQuery {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared filter semantics for adapters that evaluate events in memory.
    pub fn matches(&self, event: &ActivityEvent, asset: Option<&Asset>) -> bool {
        matches(self, event, asset)
    }

    /// The most recent events, unfiltered.
    pub fn recent() -> Self {
        Self::default()
    }

    /// Everything recorded about one asset, newest first.
    pub fn for_asset(asset_id: AssetId) -> Self {
        ActivityQuery {
            asset_id: Some(asset_id),
            ..Self::default()
        }
    }
}

/// Read-only cross-module activity queries.
#[derive(Debug, Clone)]
pub struct ActivityService<F: UnitOfWorkFactory> {
    factory: F,
}

impl<F: UnitOfWorkFactory> ActivityService<F> {
    pub fn new(factory: F) -> Self {
        ActivityService { factory }
    }

    /// One page of matching events, newest first.
    pub fn query(&mut self, query: &ActivityQuery) -> AppResult<Page<ActivityView>> {
        self.factory.read(&mut |q| q.activity().query(query))
    }

    /// The most recent events across every module.
    pub fn recent(&mut self, page: &PageRequest) -> AppResult<Page<ActivityView>> {
        self.query(&ActivityQuery {
            page: *page,
            ..ActivityQuery::default()
        })
    }

    /// The history of one asset, newest first.
    pub fn for_asset(
        &mut self,
        asset_id: AssetId,
        page: &PageRequest,
    ) -> AppResult<Page<ActivityView>> {
        self.query(&ActivityQuery {
            asset_id: Some(asset_id),
            page: *page,
            ..ActivityQuery::default()
        })
    }
}

/// Whether one event satisfies every filter of `query`.
///
/// `asset` is the event's asset as currently stored, when it still exists.
/// In-memory adapters reuse this rule; persistent adapters apply equivalent
/// filters before paging and use the centralized module classification.
fn matches(query: &ActivityQuery, event: &ActivityEvent, asset: Option<&Asset>) -> bool {
    if let Some(asset_id) = query.asset_id {
        if event.asset_id != Some(asset_id) {
            return false;
        }
    }
    if !query.event_types.is_empty() && !query.event_types.contains(&event.event_type) {
        return false;
    }
    if !query.modules.is_empty() {
        match ActivityModule::of_event_type(&event.event_type) {
            Some(module) if query.modules.contains(&module) => {}
            _ => return false,
        }
    }
    if !query.kinds.is_empty() {
        // An event with no surviving asset has no kind, so it cannot satisfy a
        // kind filter — it is excluded rather than treated as matching.
        match asset.map(|a| a.kind) {
            Some(kind) if query.kinds.contains(&kind) => {}
            _ => return false,
        }
    }
    if !query.actors.is_empty() && !query.actors.contains(&event.actor) {
        return false;
    }
    if let Some(since) = query.since {
        if event.occurred_at < since {
            return false;
        }
    }
    if let Some(until) = query.until {
        if event.occurred_at > until {
            return false;
        }
    }
    true
}

/// Projects one stored event into the adapter-facing view.
impl ActivityView {
    pub fn from_event(event: &ActivityEvent, asset_name: Option<String>) -> Self {
        Self {
            id: event.id,
            event_type: event.event_type.clone(),
            module: ActivityModule::of_event_type(&event.event_type),
            occurred_at: event.occurred_at,
            actor: event.actor.clone(),
            asset_id: event.asset_id,
            // An event about an asset that no longer exists still happened; it is
            // simply reported without a name rather than dropped.
            asset_name,
            payload: event.payload.clone(),
        }
    }
}
