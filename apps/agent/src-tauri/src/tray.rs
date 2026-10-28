//! The menu bar (macOS) or notification-area (Windows) icon and its menu.
//!
//! On macOS the icon is a template image (black and alpha only), so the
//! system tints it for light and dark menu bars like its own icons. A
//! template has no color, so the state is in the shape: the Lens alone when
//! all is well, a badge in the ring's gap when something needs attention (a
//! risk, an update ready, a device waiting for approval), a hollow dot when
//! the workspace paused or removed this computer. Windows shows the app icon.

use std::sync::Mutex as StdMutex;
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, State, Wry};

use crate::agent::AgentState;

const TRAY_ID: &str = "main-tray";
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Ok,
    Attention,
    Paused,
}

/// What the menu shows; rebuilt only when it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct View {
    look: Look,
    status: String,
    connected: bool,
    update: Option<String>,
}

static SHOWN: StdMutex<Option<View>> = StdMutex::new(None);

fn icon(look: Look) -> Image<'static> {
    let bytes: &'static [u8] = match look {
        Look::Ok => include_bytes!("../icons/tray/trayTemplate@2x.png"),
        Look::Attention => include_bytes!("../icons/tray/trayAttentionTemplate@2x.png"),
        Look::Paused => include_bytes!("../icons/tray/trayPausedTemplate@2x.png"),
    };
    Image::from_bytes(bytes).expect("bundled tray icon")
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let view = View {
        look: Look::Ok,
        status: "Sanka Monitor".to_string(),
        connected: false,
        update: None,
    };
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Sanka Monitor")
        .menu(&menu(app, &view)?)
        .on_menu_event(on_menu_event);
    let builder = if cfg!(target_os = "macos") {
        builder.icon(icon(Look::Ok)).icon_as_template(true)
    } else {
        builder.icon(
            app.default_window_icon()
                .expect("bundled window icon")
                .clone(),
        )
    };
    builder.build(app)?;
    *SHOWN.lock().unwrap() = Some(view);
    Ok(())
}

fn menu(app: &AppHandle, view: &View) -> tauri::Result<Menu<Wry>> {
    let status = MenuItem::with_id(app, "status", &view.status, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Sanka Monitor", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Scan Now", true, None::<&str>)?;
    let checkin = MenuItem::with_id(app, "checkin", "Check In Now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Sanka Monitor", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[&status, &PredefinedMenuItem::separator(app)?, &open, &scan],
    )?;
    if view.connected {
        menu.append(&checkin)?;
    }
    if let Some(version) = &view.update {
        let restart = MenuItem::with_id(
            app,
            "restart",
            format!("Restart to Update ({version})"),
            true,
            None::<&str>,
        )?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&restart)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let app = app.clone();
    match event.id.as_ref() {
        "open" => crate::show_main_window(&app),
        "scan" => {
            tauri::async_runtime::spawn(async move {
                let _ = crate::reports::scan_in_background(&app).await;
            });
        }
        "checkin" => {
            tauri::async_runtime::spawn(async move {
                let _ = crate::agent::run_checkin(&app).await;
                refresh(&app).await;
            });
        }
        "restart" => {
            tauri::async_runtime::spawn(async move {
                let _ = crate::updates::install_update(app).await;
            });
        }
        // Quitting installs an update that's ready, so the next launch runs it.
        "quit" => {
            tauri::async_runtime::spawn(async move {
                let _ = crate::updates::install_staged(&app).await;
                app.exit(0);
            });
        }
        _ => {}
    }
}

async fn current(app: &AppHandle) -> View {
    let state: State<'_, AgentState> = app.state();
    let (connected, paused, removed, pending) = {
        let config = state.config.lock().await;
        (
            config.enrolled(),
            config.enrolled() && config.kill_switch,
            config.removed_by_workspace_at.is_some(),
            config.enrolled() && config.pending_approval,
        )
    };
    // The same count as the window: acknowledged risks are not to review.
    let policy = crate::acks::policy(app).await;
    let risks = state
        .last_scan
        .lock()
        .await
        .as_ref()
        .map(|scan| crate::acks::open_risks(scan, &policy));
    let update = crate::updates::ready(app).await.map(|info| info.version);
    let look = if paused || removed {
        Look::Paused
    } else if risks.unwrap_or(0) > 0 || update.is_some() || pending {
        Look::Attention
    } else {
        Look::Ok
    };
    let status = if paused {
        "Paused by your workspace".to_string()
    } else if removed {
        "Removed by your workspace".to_string()
    } else if pending {
        "Waiting for your workspace to approve".to_string()
    } else {
        match risks {
            None => "Not scanned yet".to_string(),
            Some(0) => "No risks found".to_string(),
            Some(1) => "1 risk to review".to_string(),
            Some(n) => format!("{n} risks to review"),
        }
    };
    View {
        look,
        status,
        connected,
        update,
    }
}

/// Brings the icon and the menu in line with the agent's state.
pub async fn refresh(app: &AppHandle) {
    let view = current(app).await;
    if SHOWN.lock().unwrap().as_ref() == Some(&view) {
        return;
    }
    let Some(tray): Option<TrayIcon> = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = menu(app, &view) {
        let _ = tray.set_menu(Some(menu));
    }
    if cfg!(target_os = "macos") {
        let _ = tray.set_icon(Some(icon(view.look)));
        let _ = tray.set_icon_as_template(true);
    }
    let _ = tray.set_tooltip(Some(format!("Sanka Monitor · {}", view.status)));
    *SHOWN.lock().unwrap() = Some(view);
}

/// Keeps the tray current with changes no command reports (a check-in in
/// the background, the workspace's kill switch).
pub fn spawn_refresh_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&app).await;
            tokio::time::sleep(REFRESH_INTERVAL).await;
        }
    });
}
