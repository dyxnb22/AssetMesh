//! Transport Data Transfer Objects for Tauri commands.

use assetmesh_core::application::library_service::{
    AssetDetailOutcome, AssetDetailView, AssetSummary, LibraryModule, LibraryQuery,
    LibrarySearchQuery, LibrarySort, MergedTombstoneView, Page, PageRequest,
};
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::ports::repos::LifecycleFilter;
use serde::{Deserialize, Serialize};

use crate::error::DesktopError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalRefDto {
    pub namespace: String,
    pub external_id: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetDetailDto {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub summary: Option<String>,
    pub lifecycle: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub merged_into: Option<String>,
    pub details: serde_json::Value,
    pub tags: Vec<String>,
    pub external_refs: Vec<ExternalRefDto>,
}

impl From<AssetDetailView> for AssetDetailDto {
    fn from(view: AssetDetailView) -> Self {
        let details = serde_json::to_value(&view.details).unwrap_or(serde_json::json!({
            "module": "unknown"
        }));

        let external_refs = view
            .external_refs
            .into_iter()
            .map(|r| ExternalRefDto {
                namespace: r.namespace,
                external_id: r.external_id,
                source_url: r.source_url,
            })
            .collect();

        Self {
            id: view.asset.id.to_string(),
            kind: view.asset.kind.as_str().to_string(),
            name: view.asset.name,
            summary: view.asset.summary,
            lifecycle: view.asset.lifecycle_state.as_str().to_string(),
            revision: view.asset.revision,
            created_at: view.asset.created_at.to_rfc3339(),
            updated_at: view.asset.updated_at.to_rfc3339(),
            archived_at: view.asset.archived_at.map(|t| t.to_rfc3339()),
            merged_into: view.asset.merged_into.map(|id| id.to_string()),
            details,
            tags: view.tags,
            external_refs,
        }
    }
}

/// Renders a merged tombstone (ADR 0005): the identity of the asset that lost
/// a merge plus the tags and references that moved to the survivor. The details
/// payload is a redirect, not module data — the loser no longer owns any.
impl From<MergedTombstoneView> for AssetDetailDto {
    fn from(view: MergedTombstoneView) -> Self {
        let surviving_asset_id = view
            .asset
            .merged_into
            .map(|id| id.to_string())
            .unwrap_or_default();

        let external_refs = view
            .external_refs
            .into_iter()
            .map(|r| ExternalRefDto {
                namespace: r.namespace,
                external_id: r.external_id,
                source_url: r.source_url,
            })
            .collect();

        Self {
            id: view.asset.id.to_string(),
            kind: view.asset.kind.as_str().to_string(),
            name: view.asset.name,
            summary: view.asset.summary,
            lifecycle: view.asset.lifecycle_state.as_str().to_string(),
            revision: view.asset.revision,
            created_at: view.asset.created_at.to_rfc3339(),
            updated_at: view.asset.updated_at.to_rfc3339(),
            archived_at: view.asset.archived_at.map(|t| t.to_rfc3339()),
            merged_into: view.asset.merged_into.map(|id| id.to_string()),
            details: serde_json::json!({
                "module": "merged_redirect",
                "surviving_asset_id": surviving_asset_id,
            }),
            tags: view.tags,
            external_refs,
        }
    }
}

impl From<AssetDetailOutcome> for AssetDetailDto {
    fn from(outcome: AssetDetailOutcome) -> Self {
        match outcome {
            AssetDetailOutcome::Live(view) => Self::from(view),
            AssetDetailOutcome::MergedRedirect(tombstone) => Self::from(tombstone),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSummaryDto {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub lifecycle: String,
    pub revision: i64,
    pub subtitle: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: String,
}

impl From<AssetSummary> for AssetSummaryDto {
    fn from(s: AssetSummary) -> Self {
        Self {
            id: s.id.to_string(),
            kind: s.kind.as_str().to_string(),
            name: s.name,
            lifecycle: s.lifecycle.as_str().to_string(),
            revision: s.revision,
            subtitle: s.subtitle,
            tags: s.tags,
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageDto<T> {
    pub items: Vec<T>,
    pub offset: usize,
    pub limit: usize,
    pub total: Option<usize>,
}

impl<T, U: From<T>> From<Page<T>> for PageDto<U> {
    fn from(p: Page<T>) -> Self {
        Self {
            items: p.items.into_iter().map(U::from).collect(),
            offset: p.offset,
            limit: p.limit,
            total: p.total,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryQueryDto {
    pub lifecycle: Option<String>,
    pub modules: Option<Vec<String>>,
    pub kinds: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub sort: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn parse_lifecycle(opt: Option<&str>) -> Result<LifecycleFilter, DesktopError> {
    match opt {
        None | Some("active") => Ok(LifecycleFilter::Active),
        Some("archived") | Some("active_or_archived") => Ok(LifecycleFilter::ActiveOrArchived),
        Some("all") => Ok(LifecycleFilter::All),
        Some(other) => Err(DesktopError::invalid_input(format!(
            "Unknown lifecycle filter: {}",
            other
        ))),
    }
}

fn parse_modules(opt: Option<Vec<String>>) -> Result<Vec<LibraryModule>, DesktopError> {
    let mut modules = Vec::new();
    if let Some(mods) = opt {
        for m in mods {
            match m.trim().to_ascii_lowercase().as_str() {
                "media" => modules.push(LibraryModule::Media),
                "software" => modules.push(LibraryModule::Software),
                "services" => modules.push(LibraryModule::Services),
                other => {
                    return Err(DesktopError::invalid_input(format!(
                        "Unknown module: {}",
                        other
                    )))
                }
            }
        }
    }
    Ok(modules)
}

fn parse_kinds(opt: Option<Vec<String>>) -> Result<Vec<AssetKind>, DesktopError> {
    let mut kinds = Vec::new();
    if let Some(k_list) = opt {
        for k in k_list {
            match AssetKind::parse(&k) {
                Some(kind) => kinds.push(kind),
                None => {
                    return Err(DesktopError::invalid_input(format!(
                        "Unknown asset kind: {}",
                        k
                    )))
                }
            }
        }
    }
    Ok(kinds)
}

impl TryFrom<LibraryQueryDto> for LibraryQuery {
    type Error = DesktopError;

    fn try_from(dto: LibraryQueryDto) -> Result<Self, Self::Error> {
        let lifecycle = parse_lifecycle(dto.lifecycle.as_deref())?;
        let modules = parse_modules(dto.modules)?;
        let kinds = parse_kinds(dto.kinds)?;

        let sort = match dto.sort.as_deref() {
            None | Some("updated_desc") | Some("updated") => LibrarySort::UpdatedDesc,
            Some("updated_asc") | Some("updated-asc") => LibrarySort::UpdatedAsc,
            Some("name_asc") | Some("name") => LibrarySort::NameAsc,
            Some("name_desc") | Some("name-desc") => LibrarySort::NameDesc,
            Some("kind_asc") | Some("kind") => LibrarySort::KindAsc,
            Some(other) => {
                return Err(DesktopError::invalid_input(format!(
                    "Unknown sort order: {}",
                    other
                )))
            }
        };

        let page = PageRequest::new(dto.limit.unwrap_or(50), dto.offset.unwrap_or(0));

        Ok(LibraryQuery {
            lifecycle,
            modules,
            kinds,
            tags: dto.tags.unwrap_or_default(),
            sort,
            page,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibrarySearchQueryDto {
    pub text: String,
    pub lifecycle: Option<String>,
    pub modules: Option<Vec<String>>,
    pub kinds: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl TryFrom<LibrarySearchQueryDto> for LibrarySearchQuery {
    type Error = DesktopError;

    fn try_from(dto: LibrarySearchQueryDto) -> Result<Self, Self::Error> {
        let lifecycle = parse_lifecycle(dto.lifecycle.as_deref())?;
        let modules = parse_modules(dto.modules)?;
        let kinds = parse_kinds(dto.kinds)?;
        let page = PageRequest::new(dto.limit.unwrap_or(50), dto.offset.unwrap_or(0));

        Ok(LibrarySearchQuery {
            text: dto.text,
            lifecycle,
            modules,
            kinds,
            tags: dto.tags.unwrap_or_default(),
            page,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationReceiptDto {
    pub operation: String,
    pub asset_ids: Vec<String>,
    pub revision: Option<i64>,
    pub changed: bool,
    pub warnings: Vec<String>,
}

use assetmesh_core::application::software_discovery::{CandidateDisposition, ClassifiedCandidate};
use assetmesh_core::domain::software::{InstallSource, SoftwareCategory};
use assetmesh_core::ports::providers::{CandidateRef, SoftwareCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRefDto {
    pub namespace: String,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoftwareCandidateDto {
    pub provider: String,
    pub display_name: String,
    pub category: String,
    pub install_source: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub install_location: Option<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub external_refs: Vec<CandidateRefDto>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

impl TryFrom<SoftwareCandidateDto> for SoftwareCandidate {
    type Error = DesktopError;

    fn try_from(dto: SoftwareCandidateDto) -> Result<Self, Self::Error> {
        let category = SoftwareCategory::parse(&dto.category).ok_or_else(|| {
            DesktopError::invalid_input(format!("unknown category: {}", dto.category))
        })?;
        let install_source = InstallSource::parse(&dto.install_source).ok_or_else(|| {
            DesktopError::invalid_input(format!("unknown install source: {}", dto.install_source))
        })?;
        let external_refs = dto
            .external_refs
            .into_iter()
            .map(|r| {
                CandidateRef::new(r.namespace, r.external_id)
                    .map_err(|e| DesktopError::invalid_input(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SoftwareCandidate {
            provider: dto.provider,
            display_name: dto.display_name,
            category,
            install_source,
            version: dto.version,
            install_location: dto.install_location,
            executable_path: dto.executable_path,
            external_refs,
            metadata: dto.metadata,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedCandidateDto {
    pub candidate: SoftwareCandidateDto,
    pub disposition: String,
    pub matched_asset_ids: Vec<String>,
    pub message: Option<String>,
}

impl From<ClassifiedCandidate> for ClassifiedCandidateDto {
    fn from(cc: ClassifiedCandidate) -> Self {
        let (disposition, matched_asset_ids, message) = match cc.disposition {
            CandidateDisposition::New => ("new".to_string(), vec![], None),
            CandidateDisposition::ExactMatch { asset_id } => {
                ("exact_match".to_string(), vec![asset_id.to_string()], None)
            }
            CandidateDisposition::PotentialDuplicate { asset_ids } => (
                "potential_duplicate".to_string(),
                asset_ids.into_iter().map(|id| id.to_string()).collect(),
                None,
            ),
            CandidateDisposition::Conflict { message } => {
                ("conflict".to_string(), vec![], Some(message))
            }
        };

        ClassifiedCandidateDto {
            candidate: SoftwareCandidateDto {
                provider: cc.candidate.provider,
                display_name: cc.candidate.display_name,
                category: cc.candidate.category.as_str().to_string(),
                install_source: cc.candidate.install_source.as_str().to_string(),
                version: cc.candidate.version,
                install_location: cc.candidate.install_location,
                executable_path: cc.candidate.executable_path,
                external_refs: cc
                    .candidate
                    .external_refs
                    .into_iter()
                    .map(|r| CandidateRefDto {
                        namespace: r.namespace,
                        external_id: r.external_id,
                    })
                    .collect(),
                metadata: cc.candidate.metadata,
            },
            disposition,
            matched_asset_ids,
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SoftwareCommandDto {
    Create {
        name: String,
        category: String,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        install_source: Option<String>,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        install_location: Option<String>,
        #[serde(default)]
        executable_path: Option<String>,
        #[serde(default)]
        purpose: Option<String>,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        architecture: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    },
    UpdateMetadata {
        asset_id: String,
        #[serde(default)]
        expected_revision: Option<i64>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        install_location: Option<String>,
        #[serde(default)]
        executable_path: Option<String>,
        #[serde(default)]
        purpose: Option<String>,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        architecture: Option<String>,
    },
    AdoptCandidate {
        candidate: SoftwareCandidateDto,
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        purpose: Option<String>,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    },
    Archive {
        asset_id: String,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MediaCommandDto {
    Create {
        title: String,
        media_type: String,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        rating: Option<f64>,
        #[serde(default)]
        year: Option<i32>,
        #[serde(default)]
        platform: Option<String>,
        #[serde(default)]
        progress_unit: Option<String>,
        #[serde(default)]
        progress_current: Option<f64>,
        #[serde(default)]
        progress_total: Option<f64>,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    },
    UpdateMetadata {
        asset_id: String,
        #[serde(default)]
        expected_revision: Option<i64>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        year: Option<i32>,
        #[serde(default)]
        platform: Option<String>,
        #[serde(default)]
        notes: Option<String>,
    },
    TransitionStatus {
        asset_id: String,
        status: String,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
    UpdateProgress {
        asset_id: String,
        #[serde(default)]
        unit: Option<String>,
        #[serde(default)]
        current: Option<f64>,
        #[serde(default)]
        total: Option<f64>,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
    Rate {
        asset_id: String,
        rating: f64,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
    Archive {
        asset_id: String,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ServiceCommandDto {
    Create {
        name: String,
        service_type: String,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        account_label: Option<String>,
        #[serde(default)]
        endpoint_url: Option<String>,
        #[serde(default)]
        dashboard_url: Option<String>,
        #[serde(default)]
        domain_name: Option<String>,
        #[serde(default)]
        plan: Option<String>,
        #[serde(default)]
        cost: Option<String>,
        #[serde(default)]
        currency: Option<String>,
        #[serde(default)]
        billing_cadence: Option<String>,
        #[serde(default)]
        renews_at: Option<String>,
        #[serde(default)]
        expires_at: Option<String>,
        #[serde(default)]
        auto_renew: Option<bool>,
        #[serde(default)]
        notes: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    },
    Update {
        asset_id: String,
        #[serde(default)]
        expected_revision: Option<i64>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        account_label: Option<String>,
        #[serde(default)]
        endpoint_url: Option<String>,
        #[serde(default)]
        dashboard_url: Option<String>,
        #[serde(default)]
        domain_name: Option<String>,
        #[serde(default)]
        plan: Option<String>,
        #[serde(default)]
        cost: Option<String>,
        #[serde(default)]
        currency: Option<String>,
        #[serde(default)]
        billing_cadence: Option<String>,
        #[serde(default)]
        renews_at: Option<String>,
        #[serde(default)]
        expires_at: Option<String>,
        #[serde(default)]
        auto_renew: Option<bool>,
        #[serde(default)]
        notes: Option<String>,
    },
    RecordRenewal {
        asset_id: String,
        renews_at: String,
        #[serde(default)]
        cost: Option<String>,
        #[serde(default)]
        currency: Option<String>,
        #[serde(default)]
        next_renews_at: Option<String>,
        #[serde(default)]
        next_expires_at: Option<String>,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
    Archive {
        asset_id: String,
        #[serde(default)]
        expected_revision: Option<i64>,
    },
}

// ---------------------------------------------------------------------------
// Relation DTOs (P5-07)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationViewDto {
    pub relation_id: String,
    pub other_asset_id: String,
    pub other_asset_name: String,
    pub relation_type: String,
    pub outgoing: bool,
    pub note: Option<String>,
    pub provenance: String,
    pub created_at: String,
}

impl From<assetmesh_core::application::relation_service::RelationView> for RelationViewDto {
    fn from(v: assetmesh_core::application::relation_service::RelationView) -> Self {
        Self {
            relation_id: v.relation_id.to_string(),
            other_asset_id: v.other_asset_id.to_string(),
            other_asset_name: v.other_asset_name,
            relation_type: v.relation_type.as_str().to_string(),
            outgoing: v.outgoing,
            note: v.note,
            provenance: v.provenance.as_str().to_string(),
            created_at: v.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighborViewDto {
    pub asset: AssetSummaryDto,
    pub edge: RelationViewDto,
}

impl From<assetmesh_core::application::relation_query_service::NeighborView> for NeighborViewDto {
    fn from(v: assetmesh_core::application::relation_query_service::NeighborView) -> Self {
        Self {
            asset: AssetSummaryDto::from(v.asset),
            edge: RelationViewDto::from(v.edge),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPathHopDto {
    pub from_asset_id: String,
    pub to_asset_id: String,
    pub relation_type: String,
}

impl From<assetmesh_core::application::relation_query_service::RelationPathHop>
    for RelationPathHopDto
{
    fn from(h: assetmesh_core::application::relation_query_service::RelationPathHop) -> Self {
        Self {
            from_asset_id: h.from_asset_id.to_string(),
            to_asset_id: h.to_asset_id.to_string(),
            relation_type: h.relation_type.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraversalNodeDto {
    pub asset: AssetSummaryDto,
    pub depth: usize,
    pub path: Vec<RelationPathHopDto>,
}

impl From<assetmesh_core::application::relation_query_service::TraversalNode> for TraversalNodeDto {
    fn from(n: assetmesh_core::application::relation_query_service::TraversalNode) -> Self {
        Self {
            asset: AssetSummaryDto::from(n.asset),
            depth: n.depth,
            path: n.path.into_iter().map(RelationPathHopDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraversalViewDto {
    pub root: AssetSummaryDto,
    pub nodes: Vec<TraversalNodeDto>,
    pub truncated: bool,
}

impl From<assetmesh_core::application::relation_query_service::TraversalView> for TraversalViewDto {
    fn from(v: assetmesh_core::application::relation_query_service::TraversalView) -> Self {
        Self {
            root: AssetSummaryDto::from(v.root),
            nodes: v.nodes.into_iter().map(TraversalNodeDto::from).collect(),
            truncated: v.truncated,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RelationNeighborsQueryDto {
    pub asset_id: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub relation_types: Option<Vec<String>>,
    #[serde(default)]
    pub include_archived: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RelationTraverseQueryDto {
    pub asset_id: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub relation_types: Option<Vec<String>>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub include_archived: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationAttachDto {
    pub source_asset_id: String,
    pub relation_type: String,
    pub target_asset_id: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub expected_source_revision: Option<i64>,
    #[serde(default)]
    pub expected_target_revision: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationRemoveDto {
    pub relation_id: String,
    #[serde(default)]
    pub context_asset_id: Option<String>,
    #[serde(default)]
    pub expected_context_revision: Option<i64>,
}

// =========================================================================
// Activity DTOs (P5-08)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityViewDto {
    pub id: String,
    pub event_type: String,
    pub module: Option<String>,
    pub occurred_at: String,
    pub actor: String,
    pub asset_id: Option<String>,
    pub asset_name: Option<String>,
    pub payload: serde_json::Value,
}

impl From<assetmesh_core::application::activity_service::ActivityView> for ActivityViewDto {
    fn from(v: assetmesh_core::application::activity_service::ActivityView) -> Self {
        Self {
            id: v.id.to_string(),
            event_type: v.event_type,
            module: v.module.map(|m| m.as_str().to_string()),
            occurred_at: v.occurred_at.to_rfc3339(),
            actor: v.actor,
            asset_id: v.asset_id.map(|id| id.to_string()),
            asset_name: v.asset_name,
            payload: v.payload,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActivityQueryDto {
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub event_types: Option<Vec<String>>,
    #[serde(default)]
    pub modules: Option<Vec<String>>,
    /// Only events about assets of these kinds, e.g. `media.anime`.
    #[serde(default)]
    pub kinds: Option<Vec<String>>,
    #[serde(default)]
    pub actors: Option<Vec<String>>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

// =========================================================================
// Duplicate Review & Merge DTOs (P5-08)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateCandidateDto {
    pub left: AssetSummaryDto,
    pub right: AssetSummaryDto,
    pub evidence: Vec<serde_json::Value>,
    pub evidence_labels: Vec<String>,
}

impl From<assetmesh_core::application::duplicate_review_service::DuplicateCandidate>
    for DuplicateCandidateDto
{
    fn from(c: assetmesh_core::application::duplicate_review_service::DuplicateCandidate) -> Self {
        let evidence_labels = c.evidence.iter().map(|e| e.label()).collect();
        let evidence = c
            .evidence
            .into_iter()
            .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null))
            .collect();
        Self {
            left: AssetSummaryDto::from(c.left),
            right: AssetSummaryDto::from(c.right),
            evidence,
            evidence_labels,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DuplicateQueryDto {
    #[serde(default)]
    pub kinds: Option<Vec<String>>,
    #[serde(default)]
    pub include_archived: Option<bool>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergePreviewQueryDto {
    pub winner_id: String,
    pub loser_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergePreviewDto {
    pub winner: AssetSummaryDto,
    pub loser: AssetSummaryDto,
    pub winner_revision: i64,
    pub loser_revision: i64,
    pub can_merge: bool,
    pub conflicts: Vec<String>,
    pub transferred_tags: Vec<String>,
    pub transferred_external_refs: Vec<ExternalRefDto>,
    pub redundant_external_refs: Vec<ExternalRefDto>,
    pub transferred_relations_count: usize,
    pub redundant_relations_count: usize,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeApplyDto {
    pub winner_id: String,
    pub loser_id: String,
    #[serde(default)]
    pub expected_winner_revision: Option<i64>,
    #[serde(default)]
    pub expected_loser_revision: Option<i64>,
}

/// The adapter shapes the domain preview for the wire; every field is a plain
/// conversion, and the transfer/redundant split computed in the core is
/// preserved as-is rather than being recomputed here.
impl From<assetmesh_core::application::merge_preview_service::MergePreviewView>
    for MergePreviewDto
{
    fn from(v: assetmesh_core::application::merge_preview_service::MergePreviewView) -> Self {
        fn ref_dto(r: assetmesh_core::domain::external_ref::AssetExternalRef) -> ExternalRefDto {
            ExternalRefDto {
                namespace: r.namespace,
                external_id: r.external_id,
                source_url: r.source_url,
            }
        }

        let winner_revision = v.winner.revision;
        let loser_revision = v.loser.revision;

        Self {
            winner: AssetSummaryDto::from(v.winner),
            loser: AssetSummaryDto::from(v.loser),
            winner_revision,
            loser_revision,
            can_merge: v.can_merge,
            conflicts: v.conflicts,
            transferred_tags: v.transferred_tags,
            transferred_external_refs: v
                .transferred_external_refs
                .into_iter()
                .map(ref_dto)
                .collect(),
            redundant_external_refs: v.redundant_external_refs.into_iter().map(ref_dto).collect(),
            transferred_relations_count: v.transferred_relations_count,
            redundant_relations_count: v.redundant_relations_count,
            notes: v.notes,
        }
    }
}

// =========================================================================
// Import / Export & Settings DTOs (P5-09)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportReceiptDto {
    pub target_dir: String,
    pub format: String,
    pub version: i64,
    pub app_version: String,
    pub created_at: String,
    pub record_counts: std::collections::BTreeMap<String, usize>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportReportDto {
    pub assets_created: usize,
    pub assets_updated: usize,
    pub media_created: usize,
    pub media_updated: usize,
    pub software_created: usize,
    pub software_updated: usize,
    pub services_created: usize,
    pub services_updated: usize,
    pub relations_created: usize,
    pub relations_updated: usize,
    pub external_refs_created: usize,
    pub external_refs_deduplicated: usize,
    pub activity_created: usize,
    pub tags_created: usize,
}

impl From<assetmesh_core::application::portable::PortableImportReport> for ImportReportDto {
    fn from(r: assetmesh_core::application::portable::PortableImportReport) -> Self {
        Self {
            assets_created: r.assets_created,
            assets_updated: r.assets_updated,
            media_created: r.media_created,
            media_updated: r.media_updated,
            software_created: r.software_created,
            software_updated: r.software_updated,
            services_created: r.services_created,
            services_updated: r.services_updated,
            relations_created: r.relations_created,
            relations_updated: r.relations_updated,
            external_refs_created: r.external_refs_created,
            external_refs_deduplicated: r.external_refs_deduplicated,
            activity_created: r.activity_created,
            tags_created: r.tags_created,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPreviewDto {
    pub valid: bool,
    pub source_dir: String,
    pub format: String,
    pub version: i64,
    pub app_version: String,
    pub created_at: String,
    pub record_counts: std::collections::BTreeMap<String, usize>,
    pub modules: Vec<String>,
    pub dispositions: ImportReportDto,
    pub fingerprint: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReceiptDto {
    pub success: bool,
    pub source_dir: String,
    pub applied_at: String,
    pub report: ImportReportDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStatusDto {
    pub name: String,
    pub display_name: String,
    pub available: bool,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettingsDto {
    pub db_path: Option<String>,
    pub db_status: String,
    pub app_version: String,
    pub providers: Vec<ProviderStatusDto>,
    pub capabilities: assetmesh_core::application::library_service::AppCapabilities,
}
