//! Software use cases: CRUD/query plus the discovery boundary.
//!
//! Every mutation opens one short transaction that commits the canonical
//! change, its activity event, and the synchronous search projection together
//! (ADR 0007). Provider scans run entirely OUTSIDE transactions; only
//! adoption writes canonical state, and it never overwrites user-owned
//! purpose/notes except through explicit user overrides (docs/09).

use crate::application::projection::project_software;
use crate::application::shared::{ensure_ref_available, normalize_tags, ExternalRefInput};
use crate::application::software_discovery::{
    classify_candidate, CandidateDisposition, ClassifiedCandidate, SoftwareCandidate,
};
use crate::application::{SharedClock, SharedIdGenerator};
use crate::domain::activity::{actors, event_types, ActivityEvent};
use crate::domain::asset::Asset;
use crate::domain::external_ref::AssetExternalRef;
use crate::domain::ids::AssetId;
use crate::domain::software::{InstallSource, SoftwareCategory, SoftwareEntry, SoftwareRecord};
use crate::domain::validation::optional_text;
use crate::domain::Timestamp;
use crate::ports::providers::SoftwareDiscoveryProvider;
use crate::ports::repos::{SoftwareFilter, SoftwareListRow};
use crate::ports::uow::{QueryUnitOfWork, UnitOfWork, UnitOfWorkFactory};
use crate::{AppError, AppResult};
use serde::Serialize;
use serde_json::json;

/// Application-facing view of one software asset with its module details,
/// external references, tags, and recent activity.
#[derive(Debug, Clone)]
pub struct SoftwareView {
    pub entry: SoftwareEntry,
    pub external_refs: Vec<AssetExternalRef>,
    pub tags: Vec<String>,
    pub activity: Vec<crate::domain::activity::ActivityEvent>,
}

#[derive(Debug, Clone)]
pub struct CreateSoftware {
    pub name: String,
    pub category: SoftwareCategory,
    pub summary: Option<String>,
    pub install_source: Option<InstallSource>,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub executable_path: Option<String>,
    pub purpose: Option<String>,
    pub notes: Option<String>,
    pub architecture: Option<String>,
    pub installed_at: Option<Timestamp>,
    pub tags: Vec<String>,
    pub external_refs: Vec<ExternalRefInput>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateSoftwareMetadata {
    pub asset_id: AssetId,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub executable_path: Option<String>,
    pub purpose: Option<String>,
    pub notes: Option<String>,
    pub architecture: Option<String>,
    pub expected_revision: Option<i64>,
}

/// Explicit adoption decision for a candidate that heuristics flagged as a
/// potential duplicate (never resolved automatically, ADR 0005).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum AdoptTarget {
    /// Exact match updates the matched asset; new candidates create.
    #[default]
    Auto,
    /// Create a separate canonical record even when a match exists.
    CreateNew,
    /// Adopt into the given existing software asset.
    Existing(AssetId),
}

/// User-supplied overrides for adoption. Every `Some` value is an explicit
/// user decision and takes precedence over candidate data; absent overrides
/// fall back to candidate values (fill-if-empty on the update path).
#[derive(Debug, Clone, Default)]
pub struct AdoptOverrides {
    pub target: AdoptTarget,
    pub name: Option<String>,
    /// Only applies when a new record is created (category is fixed by the
    /// asset kind afterwards).
    pub category: Option<SoftwareCategory>,
    pub install_source: Option<InstallSource>,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub executable_path: Option<String>,
    pub purpose: Option<String>,
    pub notes: Option<String>,
    pub architecture: Option<String>,
    pub tags: Vec<String>,
}

/// Result of one adoption: what happened and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdoptionOutcome {
    pub asset_id: AssetId,
    pub created: bool,
    pub disposition: &'static str,
    /// Canonical fields refreshed from the candidate on the update path.
    pub updated_fields: Vec<String>,
    /// Candidate refs not attached because another asset owns them (only
    /// possible with an explicit `CreateNew` target).
    pub skipped_refs: Vec<String>,
}

/// One provider scan: candidates classified against canonical state. Purely
/// advisory — producing a report never writes canonical data or activity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanReport {
    pub provider: String,
    pub candidates: Vec<ClassifiedCandidate>,
}

pub struct SoftwareService<F: UnitOfWorkFactory> {
    factory: F,
    clock: SharedClock,
    ids: SharedIdGenerator,
}

impl<F: UnitOfWorkFactory> SoftwareService<F> {
    pub fn new(factory: F, clock: SharedClock, ids: SharedIdGenerator) -> Self {
        SoftwareService {
            factory,
            clock,
            ids,
        }
    }

    pub fn factory(&mut self) -> &mut F {
        &mut self.factory
    }

    pub fn get_software(&mut self, asset_id: AssetId) -> AppResult<SoftwareView> {
        self.factory.read(&mut |q| build_view(q, asset_id))
    }

    pub fn list_software(&mut self, filter: &SoftwareFilter) -> AppResult<Vec<SoftwareListRow>> {
        self.factory.read(&mut |uow| uow.software().list(filter))
    }

    pub fn create_software(&mut self, cmd: CreateSoftware) -> AppResult<SoftwareView> {
        let now = self.clock.now();

        let name = cmd.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::validation("software name must not be empty"));
        }

        let asset_id = AssetId::from_uuid(self.ids.new_id());
        let mut record = SoftwareRecord::new(asset_id, cmd.category);
        record.install_source = cmd.install_source.unwrap_or(InstallSource::Unknown);
        record.version = optional_text(&cmd.version, "version")?;
        record.install_location = optional_text(&cmd.install_location, "install_location")?;
        record.executable_path = optional_text(&cmd.executable_path, "executable_path")?;
        record.purpose = optional_text(&cmd.purpose, "purpose")?;
        record.notes = optional_text(&cmd.notes, "notes")?;
        record.architecture = optional_text(&cmd.architecture, "architecture")?;
        record.installed_at = cmd.installed_at;
        record.discovered_at = None; // manual creation is not discovery
        record.validate()?;

        let asset = Asset::new(asset_id, cmd.category.asset_kind(), name, cmd.summary, now)?;

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
            uow.software().upsert(&record)?;

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
                event_types::SOFTWARE_CREATED,
                Some(asset_id),
                actors::USER,
                json!({
                    "category": record.category.as_str(),
                    "install_source": record.install_source.as_str(),
                }),
                now,
            ))?;

            let document = project_software(&asset, &record, &tags, &refs);
            uow.search_index().upsert(&document)?;
            Ok(())
        })?;

        self.get_software(asset_id)
    }

    pub fn update_metadata(&mut self, cmd: UpdateSoftwareMetadata) -> AppResult<SoftwareView> {
        let now = self.clock.now();

        self.factory.transact(&mut |uow| {
            let mut asset = crate::application::shared::load_active_asset(uow, cmd.asset_id)?;
            if let Some(expected) = cmd.expected_revision {
                if asset.revision != expected {
                    return Err(AppError::stale_revision(expected, asset.revision));
                }
            }
            let mut record = load_software_record(uow, cmd.asset_id)?;

            if let Some(name) = cmd.name.as_deref().map(str::trim) {
                if name.is_empty() {
                    return Err(AppError::validation("software name must not be empty"));
                }
                asset.name = name.to_string();
            }
            if cmd.summary.is_some() {
                asset.summary = cmd.summary.clone();
            }
            if let Some(value) = optional_text(&cmd.version, "version")? {
                record.version = Some(value);
            }
            if let Some(value) = optional_text(&cmd.install_location, "install_location")? {
                record.install_location = Some(value);
            }
            if let Some(value) = optional_text(&cmd.executable_path, "executable_path")? {
                record.executable_path = Some(value);
            }
            if let Some(value) = optional_text(&cmd.purpose, "purpose")? {
                record.purpose = Some(value);
            }
            if let Some(value) = optional_text(&cmd.notes, "notes")? {
                record.notes = Some(value);
            }
            if let Some(value) = optional_text(&cmd.architecture, "architecture")? {
                record.architecture = Some(value);
            }

            asset.validate()?;
            record.validate()?;
            // Metadata-only edits are deliberately not activity events,
            // matching the Media module policy (docs/08).
            asset.touch(now);
            uow.assets().update(&asset)?;
            uow.software().upsert(&record)?;

            update_software_projection(uow, &asset, &record)?;
            Ok(())
        })?;

        self.get_software(cmd.asset_id)
    }

    /// Runs one provider scan and classifies every candidate against
    /// canonical state. Provider I/O happens BEFORE any scope is opened
    /// (ADR 0007: no filesystem/process work inside transactions). This use
    /// case never writes canonical data, activity, or projections.
    pub fn discover(&mut self, provider: &dyn SoftwareDiscoveryProvider) -> AppResult<ScanReport> {
        let candidates = provider.scan()?;
        self.factory.read(&mut |q| {
            let mut classified = Vec::with_capacity(candidates.len());
            for candidate in &candidates {
                let disposition = classify_candidate(q, candidate)?;
                classified.push(ClassifiedCandidate {
                    candidate: candidate.clone(),
                    disposition,
                });
            }
            Ok(ScanReport {
                provider: provider.name().to_string(),
                candidates: classified,
            })
        })
    }

    /// Explicitly adopts a discovered candidate (docs/09 adoption workflow).
    ///
    /// The candidate is re-classified inside the commit transaction, so the
    /// decision and the write observe the same state. The update path only
    /// FILLS fields the canonical record does not have yet; user-owned
    /// purpose/notes are written exclusively from explicit overrides.
    /// Potential-duplicate and conflict classifications require an explicit
    /// target (`CreateNew`/`Existing`) or fail — nothing is merged silently.
    pub fn adopt_candidate(
        &mut self,
        candidate: SoftwareCandidate,
        overrides: AdoptOverrides,
    ) -> AppResult<AdoptionOutcome> {
        self.adopt_candidate_internal(candidate, overrides, None, false)
    }

    /// Desktop write path: an adoption that resolves to an existing asset
    /// must prove which revision was reviewed. New-record adoption needs none.
    pub fn adopt_candidate_with_revision(
        &mut self,
        candidate: SoftwareCandidate,
        overrides: AdoptOverrides,
        expected_revision: Option<i64>,
    ) -> AppResult<AdoptionOutcome> {
        self.adopt_candidate_internal(candidate, overrides, expected_revision, true)
    }

    fn adopt_candidate_internal(
        &mut self,
        candidate: SoftwareCandidate,
        overrides: AdoptOverrides,
        expected_revision: Option<i64>,
        require_existing_revision: bool,
    ) -> AppResult<AdoptionOutcome> {
        let now = self.clock.now();
        let ids = self.ids.clone();
        candidate.validate()?;

        // Normalize all override text outside the transaction.
        let overrides = AdoptOverrides {
            target: overrides.target,
            name: overrides
                .name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
            category: overrides.category,
            install_source: overrides.install_source,
            version: optional_text(&overrides.version, "version")?,
            install_location: optional_text(&overrides.install_location, "install_location")?,
            executable_path: optional_text(&overrides.executable_path, "executable_path")?,
            purpose: optional_text(&overrides.purpose, "purpose")?,
            notes: optional_text(&overrides.notes, "notes")?,
            architecture: optional_text(&overrides.architecture, "architecture")?,
            tags: overrides.tags,
        };

        self.factory.transact(&mut |uow| {
            let disposition = {
                let mut query = WriteAsQuery(uow);
                classify_candidate(&mut query, &candidate)?
            };

            // Resolve the adoption target. Conflict-like dispositions fail
            // loudly instead of guessing (ADR 0005).
            let target: Option<AssetId> = match (&overrides.target, &disposition) {
                (AdoptTarget::CreateNew, _) => None,
                (AdoptTarget::Auto, CandidateDisposition::ExactMatch { asset_id }) => {
                    Some(*asset_id)
                }
                (AdoptTarget::Auto, CandidateDisposition::New) => None,
                (AdoptTarget::Auto, CandidateDisposition::PotentialDuplicate { asset_ids }) => {
                    let ids: Vec<String> = asset_ids.iter().map(|id| id.to_string()).collect();
                    return Err(AppError::conflict(format!(
                        "candidate resembles existing software ({}); review them and \
                         pass an explicit target (--new or --as <asset_id>)",
                        ids.join(", ")
                    )));
                }
                (AdoptTarget::Auto, CandidateDisposition::Conflict { message }) => {
                    return Err(AppError::conflict(format!(
                        "candidate cannot be adopted automatically: {message}"
                    )));
                }
                (AdoptTarget::Existing(_), CandidateDisposition::Conflict { message }) => {
                    return Err(AppError::conflict(format!(
                        "candidate cannot be adopted into the requested asset: {message}"
                    )));
                }
                (AdoptTarget::Existing(asset_id), _) => Some(*asset_id),
            };

            if let Some(asset_id) = target {
                let asset = uow
                    .assets()
                    .get(asset_id)?
                    .ok_or_else(|| AppError::not_found("asset", asset_id))?;
                asset.ensure_mutable()?;
                if require_existing_revision {
                    let expected = expected_revision.ok_or_else(|| {
                        AppError::validation(
                            "expected_revision is required when adopting into an existing asset",
                        )
                    })?;
                    crate::application::shared::check_asset_revision(&asset, expected)?;
                }
                if asset.kind.module() != "software" {
                    return Err(AppError::conflict(format!(
                        "asset {asset_id} is a {} — adoption targets must be software assets",
                        asset.kind
                    )));
                }
                return apply_update(uow, asset, &candidate, &overrides, &disposition, now);
            }

            apply_create(uow, &candidate, &overrides, &disposition, now, &ids)
        })
    }
}

/// Read-view projection of a write scope, letting read-only helpers run
/// inside a commit transaction (trait-object upcast from repositories to
/// their reader supertraits).
struct WriteAsQuery<'a>(&'a mut dyn UnitOfWork);

impl QueryUnitOfWork for WriteAsQuery<'_> {
    fn assets(&mut self) -> &mut dyn crate::ports::repos::AssetReader {
        self.0.assets()
    }
    fn media(&mut self) -> &mut dyn crate::ports::repos::MediaReader {
        self.0.media()
    }
    fn software(&mut self) -> &mut dyn crate::ports::repos::SoftwareReader {
        self.0.software()
    }
    fn services(&mut self) -> &mut dyn crate::ports::repos::ServiceReader {
        self.0.services()
    }
    fn external_refs(&mut self) -> &mut dyn crate::ports::repos::ExternalRefReader {
        self.0.external_refs()
    }
    fn activity(&mut self) -> &mut dyn crate::ports::repos::ActivityReader {
        self.0.activity()
    }
    fn tags(&mut self) -> &mut dyn crate::ports::repos::TagReader {
        self.0.tags()
    }
    fn relations(&mut self) -> &mut dyn crate::ports::repos::RelationReader {
        self.0.relations()
    }
    fn search_index(&mut self) -> &mut dyn crate::ports::search::SearchReader {
        self.0.search_index()
    }
    fn library(&mut self) -> &mut dyn crate::ports::repos::LibraryReadPort {
        self.0.library()
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_create(
    uow: &mut dyn UnitOfWork,
    candidate: &SoftwareCandidate,
    overrides: &AdoptOverrides,
    disposition: &CandidateDisposition,
    now: Timestamp,
    ids: &SharedIdGenerator,
) -> AppResult<AdoptionOutcome> {
    let asset_id = AssetId::from_uuid(ids.new_id());
    let category = overrides.category.unwrap_or(candidate.category);
    let name = overrides
        .name
        .clone()
        .unwrap_or_else(|| candidate.display_name.trim().to_string());
    if name.is_empty() {
        return Err(AppError::validation("software name must not be empty"));
    }

    let mut record = SoftwareRecord::new(asset_id, category);
    record.install_source = overrides.install_source.unwrap_or(candidate.install_source);
    record.version = overrides
        .version
        .clone()
        .or_else(|| candidate.version.clone());
    record.install_location = overrides
        .install_location
        .clone()
        .or_else(|| candidate.install_location.clone());
    record.executable_path = overrides
        .executable_path
        .clone()
        .or_else(|| candidate.executable_path.clone());
    // Architecture is not part of the candidate model; it comes only from
    // explicit user overrides.
    record.architecture = overrides.architecture.clone();
    // User-owned fields come ONLY from explicit overrides — never from the
    // provider (docs/09).
    record.purpose = overrides.purpose.clone();
    record.notes = overrides.notes.clone();
    record.discovered_at = Some(now);
    record.installed_at = None; // never guessed from discovery
    record.validate()?;

    let asset = Asset::new(asset_id, category.asset_kind(), name, None, now)?;

    let mut skipped_refs = Vec::new();
    let mut tags = Vec::new();

    uow.assets().insert(&asset)?;
    uow.software().upsert(&record)?;

    for tag_name in normalize_tags(&overrides.tags) {
        let tag = uow.tags().ensure(&tag_name)?;
        uow.tags().attach(asset_id, tag.id)?;
        tags.push(tag);
    }

    for reference in &candidate.external_refs {
        let existing = uow
            .external_refs()
            .find_asset_by_ref(&reference.namespace, &reference.external_id)?;
        match existing {
            None => {
                let created = AssetExternalRef::new(
                    asset_id,
                    reference.namespace.clone(),
                    reference.external_id.clone(),
                    None,
                    now,
                );
                created.validate()?;
                uow.external_refs().insert(&created)?;
            }
            Some(owner) if owner == asset_id => {}
            Some(owner) => {
                // Reachable only with an explicit CreateNew target on a
                // candidate whose refs are already claimed; reported, never
                // re-pointed (ADR 0005).
                skipped_refs.push(format!(
                    "{}:{} → {}",
                    reference.namespace, reference.external_id, owner
                ));
            }
        }
    }

    uow.activity().append(&ActivityEvent::new(
        event_types::ASSET_CREATED,
        Some(asset_id),
        actors::USER,
        json!({ "kind": asset.kind.as_str(), "name": asset.name.clone() }),
        now,
    ))?;
    uow.activity().append(&ActivityEvent::new(
        event_types::SOFTWARE_ADOPTED,
        Some(asset_id),
        actors::USER,
        json!({
            "provider": candidate.provider,
            "disposition": disposition.kind(),
            "category": record.category.as_str(),
            "install_source": record.install_source.as_str(),
            "skipped_refs": skipped_refs,
        }),
        now,
    ))?;

    let refs = uow.external_refs().list_for_asset(asset_id)?;
    let document = project_software(&asset, &record, &tags, &refs);
    uow.search_index().upsert(&document)?;

    Ok(AdoptionOutcome {
        asset_id,
        created: true,
        disposition: disposition.kind(),
        updated_fields: Vec::new(),
        skipped_refs,
    })
}

fn apply_update(
    uow: &mut dyn UnitOfWork,
    asset: Asset,
    candidate: &SoftwareCandidate,
    overrides: &AdoptOverrides,
    disposition: &CandidateDisposition,
    now: Timestamp,
) -> AppResult<AdoptionOutcome> {
    let mut asset = asset;
    let mut record = load_software_record(uow, asset.id)?;
    let mut updated_fields: Vec<String> = Vec::new();

    // Category changes would silently rewrite the asset kind; they are not
    // part of the adoption update path.
    if let Some(category) = overrides.category {
        if category != record.category {
            return Err(AppError::conflict(format!(
                "cannot change category of {} from {} to {} during adoption; \
                 create a new record instead",
                asset.id, record.category, category
            )));
        }
    }

    // --- Compute the proposed change set (reads only) ----------------------
    if let Some(name) = &overrides.name {
        if *name != asset.name {
            asset.name = name.clone();
            updated_fields.push("name".into());
        }
    }
    if record.version.is_none() {
        if let Some(value) = overrides
            .version
            .clone()
            .or_else(|| candidate.version.clone())
        {
            record.version = Some(value);
            updated_fields.push("version".into());
        }
    }
    if record.install_location.is_none() {
        if let Some(value) = overrides
            .install_location
            .clone()
            .or_else(|| candidate.install_location.clone())
        {
            record.install_location = Some(value);
            updated_fields.push("install_location".into());
        }
    }
    if record.executable_path.is_none() {
        if let Some(value) = overrides
            .executable_path
            .clone()
            .or_else(|| candidate.executable_path.clone())
        {
            record.executable_path = Some(value);
            updated_fields.push("executable_path".into());
        }
    }
    if record.architecture.is_none() && overrides.architecture.is_some() {
        record.architecture = overrides.architecture.clone();
        updated_fields.push("architecture".into());
    }
    if record.install_source == InstallSource::Unknown {
        let source = overrides.install_source.unwrap_or(candidate.install_source);
        if source != InstallSource::Unknown {
            record.install_source = source;
            updated_fields.push("install_source".into());
        }
    }
    // User-owned fields: only explicit overrides may write them, and an
    // override always replaces (the user asked for it). The candidate NEVER
    // touches purpose/notes (docs/09).
    if let Some(purpose) = overrides.purpose.clone() {
        if record.purpose.as_deref() != Some(purpose.as_str()) {
            record.purpose = Some(purpose);
            updated_fields.push("purpose".into());
        }
    }
    if let Some(notes) = overrides.notes.clone() {
        if record.notes.as_deref() != Some(notes.as_str()) {
            record.notes = Some(notes);
            updated_fields.push("notes".into());
        }
    }
    if record.discovered_at.is_none() {
        record.discovered_at = Some(now);
        updated_fields.push("discovered_at".into());
    }

    // Candidate refs: conflict on third-party ownership, otherwise pending
    // insertion (owned refs are already attached and change nothing).
    let mut new_refs: Vec<&crate::ports::providers::CandidateRef> = Vec::new();
    for reference in &candidate.external_refs {
        let existing = uow
            .external_refs()
            .find_asset_by_ref(&reference.namespace, &reference.external_id)?;
        match existing {
            Some(owner) if owner == asset.id => {}
            Some(owner) => {
                return Err(AppError::conflict(format!(
                    "external ref {}:{} is owned by asset {owner}; it cannot be \
                     attached to {} — resolve the conflict first",
                    reference.namespace, reference.external_id, asset.id
                )));
            }
            None => new_refs.push(reference),
        }
    }
    for reference in &new_refs {
        updated_fields.push(format!(
            "ref {}:{}",
            reference.namespace, reference.external_id
        ));
    }

    // Only tag names NOT already attached would change state; computing them
    // without `ensure` keeps a no-op adoption from creating orphan tags.
    let attached: std::collections::BTreeSet<String> = uow
        .tags()
        .list_for_asset(asset.id)?
        .into_iter()
        .map(|t| t.name.to_lowercase())
        .collect();
    let new_tag_names: Vec<String> = normalize_tags(&overrides.tags)
        .into_iter()
        .filter(|name| !attached.contains(&name.to_lowercase()))
        .collect();

    // --- True no-op: nothing would change, so write nothing ---------------
    // Re-adoption of an unchanged candidate must not bump the revision or
    // append activity (docs/09 idempotent adoption).
    if updated_fields.is_empty() && new_tag_names.is_empty() {
        return Ok(AdoptionOutcome {
            asset_id: asset.id,
            created: false,
            disposition: disposition.kind(),
            updated_fields: Vec::new(),
            skipped_refs: Vec::new(),
        });
    }

    // --- Commit the change set atomically ---------------------------------
    record.validate()?;
    asset.touch(now);
    uow.assets().update(&asset)?;
    uow.software().upsert(&record)?;

    let mut tags = uow.tags().list_for_asset(asset.id)?;
    for tag_name in new_tag_names {
        let tag = uow.tags().ensure(&tag_name)?;
        uow.tags().attach(asset.id, tag.id)?;
        if !tags.iter().any(|t| t.id == tag.id) {
            tags.push(tag);
        }
    }

    for reference in &new_refs {
        let created = AssetExternalRef::new(
            asset.id,
            reference.namespace.clone(),
            reference.external_id.clone(),
            None,
            now,
        );
        created.validate()?;
        uow.external_refs().insert(&created)?;
    }

    uow.activity().append(&ActivityEvent::new(
        event_types::SOFTWARE_ADOPTED,
        Some(asset.id),
        actors::USER,
        json!({
            "provider": candidate.provider,
            "disposition": disposition.kind(),
            "updated_fields": updated_fields,
        }),
        now,
    ))?;

    let refs = uow.external_refs().list_for_asset(asset.id)?;
    let document = project_software(&asset, &record, &tags, &refs);
    uow.search_index().upsert(&document)?;

    Ok(AdoptionOutcome {
        asset_id: asset.id,
        created: false,
        disposition: disposition.kind(),
        updated_fields,
        skipped_refs: Vec::new(),
    })
}

pub(crate) fn load_software_record(
    uow: &mut dyn UnitOfWork,
    asset_id: AssetId,
) -> AppResult<SoftwareRecord> {
    uow.software()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("software record", asset_id))
}

/// Rebuilds and stores the search projection for one software asset from
/// canonical state, inside the caller's transaction.
pub(crate) fn update_software_projection(
    uow: &mut dyn UnitOfWork,
    asset: &Asset,
    record: &SoftwareRecord,
) -> AppResult<crate::domain::search::SearchDocument> {
    let tags = uow.tags().list_for_asset(asset.id)?;
    let refs = uow.external_refs().list_for_asset(asset.id)?;
    let document = project_software(asset, record, &tags, &refs);
    uow.search_index().upsert(&document)?;
    Ok(document)
}

pub(crate) fn build_view(
    uow: &mut dyn QueryUnitOfWork,
    asset_id: AssetId,
) -> AppResult<SoftwareView> {
    let asset = uow
        .assets()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("asset", asset_id))?;
    let record = uow
        .software()
        .get(asset_id)?
        .ok_or_else(|| AppError::not_found("software record", asset_id))?;
    let external_refs = uow.external_refs().list_for_asset(asset_id)?;
    let tags = uow
        .tags()
        .list_for_asset(asset_id)?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let activity = uow.activity().list_for_asset(asset_id, 50)?;
    Ok(SoftwareView {
        entry: SoftwareEntry { asset, record },
        external_refs,
        tags,
        activity,
    })
}

#[cfg(test)]
mod tests {

    use crate::application::software_discovery::normalize_name;

    #[test]
    fn normalize_name_collapses_whitespace_and_case() {
        assert_eq!(
            normalize_name("  Visual   Studio Code "),
            "visual studio code"
        );
    }
}
