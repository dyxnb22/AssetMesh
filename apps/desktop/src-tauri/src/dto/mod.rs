//! Transport Data Transfer Objects for Tauri commands.

use assetmesh_core::application::library_service::{
    AssetDetailView, AssetSummary, LibraryModule, LibraryQuery, LibrarySearchQuery, LibrarySort,
    Page, PageRequest,
};
use assetmesh_core::domain::asset::{Asset, AssetKind};
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

impl AssetDetailDto {
    pub fn for_merged(
        asset: &Asset,
        tags: Vec<String>,
        external_refs: Vec<ExternalRefDto>,
    ) -> Self {
        let surviving_asset_id = asset
            .merged_into
            .map(|id| id.to_string())
            .unwrap_or_default();
        Self {
            id: asset.id.to_string(),
            kind: asset.kind.as_str().to_string(),
            name: asset.name.clone(),
            summary: asset.summary.clone(),
            lifecycle: asset.lifecycle_state.as_str().to_string(),
            revision: asset.revision,
            created_at: asset.created_at.to_rfc3339(),
            updated_at: asset.updated_at.to_rfc3339(),
            archived_at: asset.archived_at.map(|t| t.to_rfc3339()),
            merged_into: asset.merged_into.map(|id| id.to_string()),
            details: serde_json::json!({
                "module": "merged_redirect",
                "surviving_asset_id": surviving_asset_id,
            }),
            tags,
            external_refs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSummaryDto {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub lifecycle: String,
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
    },
    UpdateProgress {
        asset_id: String,
        #[serde(default)]
        unit: Option<String>,
        #[serde(default)]
        current: Option<f64>,
        #[serde(default)]
        total: Option<f64>,
    },
    Rate {
        asset_id: String,
        rating: f64,
    },
    Archive {
        asset_id: String,
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
    },
    Archive {
        asset_id: String,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationRemoveDto {
    pub relation_id: String,
}
