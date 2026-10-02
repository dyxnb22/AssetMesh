//! macOS owns the menu and its refresh loop; hiding the WebView does not pause it.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Mutex,
};
use std::time::{Duration, Instant};

use tauri::{image::Image, menu::PredefinedMenuItem, tray::TrayIconBuilder, AppHandle, Manager};

use super::{services_hash, MenuBarService, MenuBarSnapshot, MenuLanguage, ServiceMenuAction};
use crate::{error::DesktopError, state::DesktopState};

type Menu = tauri::menu::Menu<tauri::Wry>;
type MenuItem = tauri::menu::MenuItem<tauri::Wry>;
type Submenu = tauri::menu::Submenu<tauri::Wry>;

const TRAY_ID: &str = "assetmesh-menu-bar";
const OPEN_WINDOW: &str = "assetmesh-open-window";
const MANAGE_SERVICES: &str = "assetmesh-manage-services";
const QUIT: &str = "assetmesh-quit";
const MAX_VISIBLE_SERVICES: usize = 12;
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);

struct ServiceItems {
    asset_id: String,
    submenu: Submenu,
    start: MenuItem,
    stop: MenuItem,
    restart: MenuItem,
    open: MenuItem,
    show: MenuItem,
}

struct NativeMenu {
    menu: Menu,
    summary: MenuItem,
    message: Option<MenuItem>,
    empty: Option<MenuItem>,
    services: Vec<ServiceItems>,
    open_window: MenuItem,
    manage_services: Option<MenuItem>,
    quit: MenuItem,
}

struct MenuView {
    snapshot: Option<MenuBarSnapshot>,
    read_failed: bool,
    action_error: Option<String>,
    observed_at: Instant,
    native: NativeMenu,
}

struct MenuBarControl {
    view: Mutex<MenuView>,
    busy: AtomicBool,
    stopped: AtomicBool,
    refresh: mpsc::SyncSender<()>,
}

fn item(app: &AppHandle, id: &str, text: &str, enabled: bool) -> tauri::Result<MenuItem> {
    MenuItem::with_id(app, id, text, enabled, None::<&str>)
}

impl NativeMenu {
    fn new(
        app: &AppHandle,
        snapshot: Option<&MenuBarSnapshot>,
        show_message: bool,
    ) -> tauri::Result<Self> {
        let menu = Menu::new(app)?;
        let summary = item(app, "assetmesh-summary", "正在读取服务状态…", false)?;
        menu.append(&summary)?;
        let message = if show_message {
            let message = item(app, "assetmesh-message", "", false)?;
            menu.append(&message)?;
            Some(message)
        } else {
            None
        };
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let mut services = Vec::new();
        if let Some(snapshot) = snapshot {
            for service in snapshot.services.iter().take(MAX_VISIBLE_SERVICES) {
                let submenu = Submenu::new(app, &service.name, true)?;
                let action_item = |action: ServiceMenuAction, text| {
                    item(app, &action.menu_id(&service.asset_id), text, false)
                };
                let open = action_item(ServiceMenuAction::OpenPage, "打开页面")?;
                let start = action_item(ServiceMenuAction::Start, "启动")?;
                let stop = action_item(ServiceMenuAction::Stop, "停止")?;
                let restart = action_item(ServiceMenuAction::Restart, "重启")?;
                let show = action_item(ServiceMenuAction::Show, "查看详情…")?;
                submenu.append_items(&[
                    &open,
                    &start,
                    &stop,
                    &restart,
                    &PredefinedMenuItem::separator(app)?,
                    &show,
                ])?;
                menu.append(&submenu)?;
                services.push(ServiceItems {
                    asset_id: service.asset_id.clone(),
                    submenu,
                    start,
                    stop,
                    restart,
                    open,
                    show,
                });
            }
        }
        let empty = if services.is_empty() {
            let empty = item(app, "assetmesh-no-services", "暂无本地服务", false)?;
            menu.append(&empty)?;
            Some(empty)
        } else {
            None
        };
        let open_window = item(app, OPEN_WINDOW, "打开 AssetMesh", true)?;
        let manage_services = if snapshot.is_some_and(|s| s.services.len() > MAX_VISIBLE_SERVICES) {
            let manage_services = item(app, MANAGE_SERVICES, "查看全部服务…", true)?;
            menu.append(&manage_services)?;
            Some(manage_services)
        } else {
            None
        };
        let quit = item(app, QUIT, "退出 AssetMesh", true)?;
        menu.append_items(&[
            &PredefinedMenuItem::separator(app)?,
            &open_window,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ])?;
        Ok(Self {
            menu,
            summary,
            message,
            empty,
            services,
            open_window,
            manage_services,
            quit,
        })
    }

    fn same_structure(&self, snapshot: Option<&MenuBarSnapshot>, show_message: bool) -> bool {
        self.message.is_some() == show_message
            && self.manage_services.is_some()
                == snapshot.is_some_and(|s| s.services.len() > MAX_VISIBLE_SERVICES)
            && self
                .services
                .iter()
                .map(|items| items.asset_id.as_str())
                .eq(snapshot.into_iter().flat_map(|snapshot| {
                    snapshot
                        .services
                        .iter()
                        .take(MAX_VISIBLE_SERVICES)
                        .map(|service| service.asset_id.as_str())
                }))
    }
}

impl MenuView {
    fn render(&mut self, app: &AppHandle, busy: bool) -> tauri::Result<()> {
        let language = self
            .snapshot
            .as_ref()
            .map_or(MenuLanguage::Chinese, |s| s.language);
        let text = |zh, en| language.text(zh, en);
        let show_message = busy || self.action_error.is_some() || self.read_failed;
        let tray = app.tray_by_id(TRAY_ID).ok_or_else(|| {
            tauri::Error::Io(std::io::Error::other("menu bar icon is unavailable"))
        })?;
        // Update existing menu items in place during status changes. Replacing
        // the menu every poll would dismiss an open native menu.
        if !self
            .native
            .same_structure(self.snapshot.as_ref(), show_message)
        {
            let native = NativeMenu::new(app, self.snapshot.as_ref(), show_message)?;
            tray.set_menu(Some(native.menu.clone()))?;
            self.native = native;
        }
        let failed = self
            .snapshot
            .as_ref()
            .map_or(0, MenuBarSnapshot::failed_count);
        let summary = if self.read_failed {
            text(
                "状态更新失败，操作前请重试",
                "Status unavailable; retry before operating",
            )
            .to_string()
        } else if let Some(snapshot) = &self.snapshot {
            if language == MenuLanguage::Chinese {
                let mut summary = format!("{} 个服务运行中", snapshot.running_count());
                if failed > 0 {
                    summary.push_str(&format!("，{failed} 个失败"));
                }
                summary
            } else {
                let mut summary = format!("{} services running", snapshot.running_count());
                if failed > 0 {
                    summary.push_str(&format!(", {failed} failed"));
                }
                summary
            }
        } else {
            text("正在读取服务状态…", "Reading service status…").to_string()
        };
        let message = if busy {
            text("正在操作服务…", "Updating service…")
        } else if let Some(error) = self.action_error.as_deref() {
            error
        } else if self.read_failed {
            text(
                "正在自动重试；可打开主窗口检查",
                "Retrying automatically; open the app to inspect",
            )
        } else {
            ""
        };
        self.native.summary.set_text(&summary)?;
        if let Some(item) = &self.native.message {
            item.set_text(compact_text(message, 140))?;
        }
        self.native
            .open_window
            .set_text(text("打开 AssetMesh", "Open AssetMesh"))?;
        let total = self.snapshot.as_ref().map_or(0, |s| s.services.len());
        if let Some(item) = &self.native.manage_services {
            item.set_text(if language == MenuLanguage::Chinese {
                format!("查看全部 {total} 个服务…")
            } else {
                format!("View all {total} services…")
            })?;
        }
        self.native
            .quit
            .set_text(text("退出 AssetMesh", "Quit AssetMesh"))?;
        if let Some(empty) = &self.native.empty {
            empty.set_text(text("暂无本地服务", "No local services"))?;
        }
        if let Some(snapshot) = &self.snapshot {
            for (items, service) in self.native.services.iter().zip(&snapshot.services) {
                update_service(items, service, language, !self.read_failed && !busy)?;
            }
        }
        let attention = self.read_failed || self.action_error.is_some() || failed > 0;
        tray.set_title(Some(if attention { "!" } else { "" }))?;
        tray.set_tooltip(Some(format!("AssetMesh: {summary}")))?;
        Ok(())
    }
}

fn update_service(
    items: &ServiceItems,
    service: &MenuBarService,
    language: MenuLanguage,
    enabled: bool,
) -> tauri::Result<()> {
    items.submenu.set_text(format!(
        "{} ({})",
        compact_text(&service.name, 40),
        language.state_label(service.state)
    ))?;
    for (item, chinese, english, available) in [
        (
            &items.open,
            "打开页面",
            "Open page",
            enabled && service.can_open,
        ),
        (&items.start, "启动", "Start", enabled && service.can_start),
        (&items.stop, "停止", "Stop", enabled && service.can_stop),
        (
            &items.restart,
            "重启",
            "Restart",
            enabled && service.can_restart,
        ),
        (&items.show, "查看详情…", "View details…", true),
    ] {
        item.set_text(language.text(chinese, english))?;
        item.set_enabled(available)?;
    }
    Ok(())
}

fn compact_text(text: &str, limit: usize) -> String {
    let text = text.replace(['\n', '\r', '\t'], " ");
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        result.push('…');
    }
    // Native menus treat single ampersands as mnemonic markers.
    result.replace('&', "&&")
}

pub fn install(app: &mut tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let native = NativeMenu::new(handle, None, false)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(include_bytes!(
            "../../icons/menu-bar.png"
        ))?)
        .icon_as_template(true)
        .menu(&native.menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
        .build(app)?;
    let (refresh, receiver) = mpsc::sync_channel(1);
    app.manage(MenuBarControl {
        view: Mutex::new(MenuView {
            snapshot: None,
            read_failed: false,
            action_error: None,
            observed_at: Instant::now(),
            native,
        }),
        busy: AtomicBool::new(false),
        stopped: AtomicBool::new(false),
        refresh,
    });
    let app = app.handle().clone();
    std::thread::spawn(move || loop {
        let control = app.state::<MenuBarControl>();
        if control.stopped.load(Ordering::Acquire) {
            break;
        }
        if !control.busy.load(Ordering::Acquire) {
            let observed_at = Instant::now();
            let snapshot = MenuBarSnapshot::read(&app.state::<DesktopState>());
            let delay = if snapshot
                .as_ref()
                .is_ok_and(|snapshot| snapshot.running_count() > 0)
            {
                REFRESH_INTERVAL
            } else {
                Duration::from_secs(30)
            };
            publish(&app, observed_at, snapshot, None);
            if receiver.recv_timeout(delay) == Err(mpsc::RecvTimeoutError::Disconnected) {
                break;
            }
            continue;
        }
        if receiver.recv_timeout(REFRESH_INTERVAL) == Err(mpsc::RecvTimeoutError::Disconnected) {
            break;
        }
    });
    Ok(())
}

fn publish(
    app: &AppHandle,
    observed_at: Instant,
    snapshot: Result<MenuBarSnapshot, DesktopError>,
    action: Option<Result<(), String>>,
) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let control = handle.state::<MenuBarControl>();
        if control.stopped.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut view) = control.view.lock() else {
            return;
        };
        if let Some(result) = action {
            view.action_error = result.err();
            control.busy.store(false, Ordering::Release);
        }
        // A slow read started before an action must not replace its newer result.
        if observed_at >= view.observed_at {
            view.observed_at = observed_at;
            match snapshot {
                Ok(snapshot) => {
                    view.snapshot = Some(snapshot);
                    view.read_failed = false;
                }
                Err(_) => {
                    view.read_failed = true;
                }
            }
        }
        if let Err(error) = view.render(&handle, control.busy.load(Ordering::Acquire)) {
            eprintln!("[assetmesh] menu bar update failed: {error}");
        }
    }) {
        eprintln!("[assetmesh] menu bar dispatch failed: {error}");
    }
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    let Some(control) = app.try_state::<MenuBarControl>() else {
        return;
    };
    if control.stopped.load(Ordering::Acquire) {
        return;
    }
    match id {
        OPEN_WINDOW => {
            reopen(app);
            return;
        }
        MANAGE_SERVICES => {
            show_services(app, None);
            return;
        }
        QUIT => {
            app.exit(0);
            return;
        }
        _ => {}
    }
    let Some((action, asset_id)) = ServiceMenuAction::parse_menu_id(id) else {
        return;
    };
    if action == ServiceMenuAction::Show {
        show_services(app, Some(&asset_id));
        return;
    }
    if control
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let (language, name) = if let Ok(mut view) = control.view.lock() {
        let language = view
            .snapshot
            .as_ref()
            .map_or(MenuLanguage::Chinese, |s| s.language);
        let name = view
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .services
                    .iter()
                    .find(|service| service.asset_id == asset_id)
            })
            .map(|service| service.name.clone())
            .unwrap_or_else(|| language.text("服务", "Service").to_string());
        if let Err(error) = view.render(app, true) {
            eprintln!("[assetmesh] menu bar action display failed: {error}");
        }
        (language, name)
    } else {
        control.busy.store(false, Ordering::Release);
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let result = action
            .perform(&asset_id, &app.state::<DesktopState>())
            .map_err(|error| {
                if language == MenuLanguage::Chinese {
                    format!("{name} 操作失败：{}", error.message)
                } else {
                    format!("{name}: {}", error.message)
                }
            });
        let observed_at = Instant::now();
        let snapshot = MenuBarSnapshot::read(&app.state::<DesktopState>());
        publish(&app, observed_at, snapshot, Some(result));
        let _ = app.state::<MenuBarControl>().refresh.try_send(());
    });
}

pub fn reopen<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let result = window
            .show()
            .and_then(|_| window.unminimize())
            .and_then(|_| window.set_focus());
        if let Err(error) = result {
            eprintln!("[assetmesh] could not reopen window: {error}");
        }
    }
}

fn show_services(app: &AppHandle, asset_id: Option<&str>) {
    reopen(app);
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(hash) = services_hash(asset_id) else {
        return;
    };
    let hash = serde_json::to_string(&hash).expect("string serializes");
    let mut script = format!("window.location.hash = {hash};");
    if let Some(id) = asset_id {
        let id = serde_json::to_string(id).expect("string serializes");
        script.push_str(&format!(
            "window.dispatchEvent(new CustomEvent('assetmesh-open-service', {{detail: {id}}}));"
        ));
    } else {
        script.push_str("window.dispatchEvent(new Event('assetmesh-manage-services'));");
    }
    if let Err(error) = window.eval(&script) {
        eprintln!("[assetmesh] could not open service view: {error}");
    }
}

pub fn window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let Some(control) = window.app_handle().try_state::<MenuBarControl>() else {
        return;
    };
    if control.stopped.load(Ordering::Acquire) {
        return;
    }
    api.prevent_close();
    if let Err(error) = window.hide() {
        eprintln!("[assetmesh] could not hide window: {error}");
    }
}

pub fn stop_refreshing<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(control) = app.try_state::<MenuBarControl>() {
        control.stopped.store(true, Ordering::Release);
        let _ = control.refresh.try_send(());
    }
}
