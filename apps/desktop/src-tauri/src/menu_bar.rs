//! A second desktop entry point over the same canonical service commands.
//! The native menu refreshes independently of the React window's visibility.

use std::collections::HashMap;

use assetmesh_core::domain::{ids::AssetId, service::ServiceType, LifecycleState};
use assetmesh_core::ports::repos::{ServiceFilter, ServiceSort};

use crate::{commands, error::DesktopError, runtime::RuntimeState, state::DesktopState};

#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
pub use native::{install, reopen, stop_refreshing, window_event};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuLanguage {
    Chinese,
    English,
}

#[cfg(target_os = "macos")]
impl MenuLanguage {
    fn text(self, chinese: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::Chinese => chinese,
            Self::English => english,
        }
    }

    fn state_label(self, state: RuntimeState) -> &'static str {
        match state {
            RuntimeState::Starting => self.text("启动中", "Starting"),
            RuntimeState::Running => self.text("运行中", "Running"),
            RuntimeState::Stopping => self.text("停止中", "Stopping"),
            RuntimeState::Stopped => self.text("已停止", "Stopped"),
            RuntimeState::Failed => self.text("运行失败", "Failed"),
            RuntimeState::External => self.text("外部运行", "Running externally"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuBarService {
    pub asset_id: String,
    pub name: String,
    pub state: RuntimeState,
    pub can_start: bool,
    pub can_stop: bool,
    pub can_restart: bool,
    pub can_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuBarSnapshot {
    pub language: MenuLanguage,
    pub services: Vec<MenuBarService>,
}

impl MenuBarSnapshot {
    pub fn read(state: &DesktopState) -> Result<Self, DesktopError> {
        let rows = state.with_modules(|modules| {
            Ok(modules.service().list_services(&ServiceFilter {
                service_type: Some(ServiceType::Local),
                provider: None,
                tag: None,
                sort: ServiceSort::TitleAsc,
            })?)
        })?;
        let statuses: HashMap<_, _> = commands::service_runtime_statuses_impl(state)?
            .into_iter()
            .map(|status| (status.asset_id, status.state))
            .collect();
        let mut services = Vec::new();
        for row in rows {
            let asset = row.entry.asset;
            if asset.lifecycle_state != LifecycleState::Active {
                continue;
            }
            let asset_id = asset.id.to_string();
            let runtime_state = *statuses.get(&asset_id).ok_or_else(|| {
                DesktopError::unavailable("Service records changed; refresh their status")
            })?;
            let record = row.entry.record;
            let configured = record.project_dir.as_deref().is_some_and(nonempty)
                && record.start_command.as_deref().is_some_and(nonempty);
            let running = matches!(
                runtime_state,
                RuntimeState::Running | RuntimeState::External
            );
            let can_stop = running
                && (runtime_state != RuntimeState::External
                    || (record.project_dir.as_deref().is_some_and(nonempty)
                        && record.stop_command.as_deref().is_some_and(nonempty)));
            let valid_address = record.endpoint_url.as_deref().is_some_and(|address| {
                tauri::Url::parse(address).is_ok_and(|url| {
                    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
                })
            });
            services.push(MenuBarService {
                asset_id,
                name: asset.name,
                state: runtime_state,
                can_start: configured
                    && matches!(runtime_state, RuntimeState::Stopped | RuntimeState::Failed),
                can_stop,
                can_restart: configured && can_stop,
                can_open: running && valid_address,
            });
        }
        // Keep a stable ordering, including services with equal names.
        services.sort_by(|a, b| a.name.cmp(&b.name).then(a.asset_id.cmp(&b.asset_id)));
        let preferences = state
            .backup()
            .and_then(|backup| backup.preferences().map_err(DesktopError::from))
            .unwrap_or_default();
        let language = if preferences
            .get("assetmesh-lang")
            .is_some_and(|lang| lang == "en")
        {
            MenuLanguage::English
        } else {
            MenuLanguage::Chinese
        };
        Ok(Self { language, services })
    }

    pub fn running_count(&self) -> usize {
        self.services
            .iter()
            .filter(|service| {
                matches!(
                    service.state,
                    RuntimeState::Running | RuntimeState::External
                )
            })
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.services
            .iter()
            .filter(|service| service.state == RuntimeState::Failed)
            .count()
    }
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceMenuAction {
    Start,
    Stop,
    Restart,
    OpenPage,
    Show,
}

impl ServiceMenuAction {
    pub fn menu_id(self, asset_id: &str) -> String {
        let action = match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::OpenPage => "open",
            Self::Show => "show",
        };
        format!("assetmesh-service:{action}:{asset_id}")
    }

    pub fn parse_menu_id(id: &str) -> Option<(Self, String)> {
        let (action, id) = id.strip_prefix("assetmesh-service:")?.split_once(':')?;
        let id = uuid::Uuid::parse_str(id).ok()?.to_string();
        let action = match action {
            "start" => Self::Start,
            "stop" => Self::Stop,
            "restart" => Self::Restart,
            "open" => Self::OpenPage,
            "show" => Self::Show,
            _ => return None,
        };
        Some((action, id))
    }

    /// Menu enabled states are advisory. The canonical command revalidates the
    /// configuration and lifecycle at click time, including stale menu entries.
    pub fn perform(self, asset_id: &str, state: &DesktopState) -> Result<(), DesktopError> {
        match self {
            Self::Start => {
                commands::service_runtime_start_impl(asset_id, state)?;
            }
            Self::Stop => {
                commands::service_runtime_stop_impl(asset_id, state)?;
            }
            Self::Restart => {
                commands::service_runtime_restart_impl(asset_id, state)?;
            }
            Self::OpenPage => commands::service_open_page_impl(asset_id, state)?,
            Self::Show => {
                return Err(DesktopError::invalid_input(
                    "Show requires a desktop window",
                ))
            }
        }
        Ok(())
    }
}

pub fn services_hash(asset_id: Option<&str>) -> Result<String, DesktopError> {
    let mut hash = "#/?module=services".to_string();
    if let Some(id) = asset_id {
        let id = AssetId::from_uuid(
            uuid::Uuid::parse_str(id)
                .map_err(|_| DesktopError::invalid_input("invalid service ID"))?,
        );
        hash.push_str("&asset=");
        hash.push_str(&id.to_string());
    }
    Ok(hash)
}
