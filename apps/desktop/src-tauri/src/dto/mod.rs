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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SoftwareCommandDto {
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
}
