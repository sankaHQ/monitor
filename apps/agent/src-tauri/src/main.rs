#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod agent;
mod checks;
mod cli;
mod reports;
mod saas;
mod scan;
mod updates;
mod usage;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    // `scan`, `--help` and `--version` run without the app: no window, tray,
    // single-instance hand-off, app data or network.
    let args: Vec<String> = std::env::args().collect();
    let env_lang = std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LANG").ok());
    if let Some(command) = cli::parse(&args, env_lang.as_deref()) {
        std::process::exit(cli::run(command));
    }

    tauri::Builder::default()
        // First: a second launch (or, on Windows, a sanka-monitor:// link)
        // goes to the running app.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            for arg in args.iter().filter(|arg| account::is_deep_link(arg)) {
                account::handle_deep_link(app, arg);
            }
            show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![agent::BACKGROUND_ARG]),
        ))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(agent::AgentState::default())
        .invoke_handler(tauri::generate_handler![
            agent::get_status,
            agent::enroll,
            agent::set_sharing_consent,
            agent::checkin_now,
            agent::get_transparency,
            agent::set_launch_at_login,
            agent::set_update_checks,
            agent::open_nudge,
            agent::person_views,
            agent::unenroll,
            updates::install_update,
            reports::scan_device,
            reports::get_scan,
            reports::save_report,
            reports::get_saved_reports,
            reports::reveal_report,
            reports::open_report,
            reports::prepare_send,
            reports::prepare_edited_excel,
            reports::send_report,
            reports::cancel_send,
            account::get_account,
            account::begin_sign_in,
            account::finish_sign_in,
            account::cancel_sign_in,
            account::sign_out,
            saas::get_saas_settings,
            saas::set_browser_check,
            saas::set_saas_sharing,
            saas::choose_allowlist,
            saas::clear_allowlist,
            saas::open_migration_page,
            checks::set_check_schedule,
            checks::dismiss_nudges,
        ])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "Open Sanka Monitor", true, None::<&str>)?;
            let checkin = MenuItem::with_id(app, "checkin", "Check In Now", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Sanka Monitor", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &checkin, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(
                    app.default_window_icon()
                        .expect("bundled window icon")
                        .clone(),
                )
                .tooltip("Sanka Monitor")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "checkin" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = agent::run_checkin(&app).await;
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            let handle = app.handle().clone();
            let mut config = agent::load_config(&handle);
            if agent::enable_autostart_once(&handle, &mut config) {
                let _ = agent::save_config(&handle, &config);
            }
            // Enrolled, still owing the workspace a revoke after Disconnect, or
            // running regular checks: the login item starts it with work to do.
            let has_work = config.enrolled()
                || !config.pending_revokes.is_empty()
                || config.check_schedule.is_some();
            let collecting = config.collecting();
            tauri::async_runtime::block_on(async {
                let state: tauri::State<'_, agent::AgentState> = handle.state();
                *state.config.lock().await = config;
            });

            match usage::queue_path(&handle).and_then(|path| {
                monitor_store::SqliteQueue::open(&path).map_err(|err| err.to_string())
            }) {
                Ok(queue) => {
                    let runtime = usage::UsageRuntime::new(queue, collecting);
                    tauri::async_runtime::block_on(async {
                        let state: tauri::State<'_, agent::AgentState> = handle.state();
                        *state.usage.lock().await = Some(runtime.clone());
                    });
                    usage::spawn_collector(runtime);
                    agent::spawn_policy_loop(handle.clone());
                    agent::spawn_upload_loop(handle.clone());
                }
                Err(err) => {
                    eprintln!("usage store unavailable, collection disabled: {err}");
                }
            }

            agent::spawn_checkin_loop(handle.clone());
            agent::spawn_revoke_loop(handle.clone());
            checks::spawn_schedule_loop(handle.clone());
            updates::spawn_update_loop(handle.clone());
            // The browser hands a sign-in back through sanka-monitor://.
            account::register_deep_links(&handle);

            // The login item starts the agent in the tray; any other launch,
            // or a device with no connected work to do, opens the window.
            let background = std::env::args().any(|arg| arg == agent::BACKGROUND_ARG);
            if !has_work || !background {
                show_main_window(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Tray-resident agent: closing the window hides it; quitting lives
            // in the tray menu so collection state is always deliberate.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Sanka Monitor")
        .run(on_run_event);
}

/// Opening the running app from Finder or Spotlight (macOS sends "reopen",
/// not a second launch) shows the window: a full menu bar can hide the tray
/// icon on a MacBook with a notch.
#[cfg(target_os = "macos")]
fn on_run_event(app: &AppHandle, event: RunEvent) {
    if let RunEvent::Reopen { .. } = event {
        show_main_window(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn on_run_event(_app: &AppHandle, _event: RunEvent) {}
