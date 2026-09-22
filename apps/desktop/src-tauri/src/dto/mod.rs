//! Transport Data Transfer Objects for Tauri commands.

use assetmesh_core::application::library_service::{
    AssetSummary, LibraryModule, LibraryQuery, LibrarySort, Page, PageRequest,
};
use assetmesh_core::domain::asset::AssetKind;
use assetmesh_core::ports::repos::LifecycleFilter;
use serde::{Deserialize, Serialize};

use crate::error::DesktopError;

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

impl TryFrom<LibraryQueryDto> for LibraryQuery {
    type Error = DesktopError;

    fn try_from(dto: LibraryQueryDto) -> Result<Self, Self::Error> {
        let lifecycle = match dto.lifecycle.as_deref() {
            None | Some("active") => LifecycleFilter::Active,
            Some("archived") | Some("active_or_archived") => LifecycleFilter::ActiveOrArchived,
            Some("all") => LifecycleFilter::All,
            Some(other) => {
                return Err(DesktopError::invalid_input(format!(
                    "Unknown lifecycle filter: {}",
                    other
                )))
            }
        };

        let mut modules = Vec::new();
        if let Some(mods) = dto.modules {
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

        let mut kinds = Vec::new();
        if let Some(k_list) = dto.kinds {
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
