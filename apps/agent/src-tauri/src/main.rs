#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod acks;
mod agent;
mod browser_context;
mod checks;
mod cli;
mod context_history;
mod enrollment;
mod locale;
mod reports;
mod saas;
mod scan;
mod tray;
mod updates;
mod usage;

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;

/// The window opens on Settings and checks for a newer version.
pub(crate) const CHECK_FOR_UPDATES_EVENT: &str = "check-for-updates";

/// The app menu item that opens Settings and checks for an update.
pub(crate) const CHECK_FOR_UPDATES_MENU_ID: &str = "check-for-updates";

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
    if let Some(code) = browser_context::run_if_host(&args) {
        std::process::exit(code);
    }
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
        .manage(context_history::ContextState::default())
        .on_menu_event(|app, event| {
            if event.id().as_ref() == CHECK_FOR_UPDATES_MENU_ID {
                show_main_window(app);
                let _ = tauri::Emitter::emit(app, CHECK_FOR_UPDATES_EVENT, ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            locale::set_ui_language,
            browser_context::register_context_browser_host,
            context_history::cancel_context_preview,
            context_history::invalidate_context_selection,
            context_history::get_context_history,
            context_history::dismiss_context_candidate,
            context_history::export_context_history,
            context_history::set_context_settings,
            context_history::delete_context_history,
            context_history::save_context_workflow,
            context_history::prepare_context_package,
            context_history::send_context_package,
            context_history::delete_shared_context,
            context_history::enable_context_summary,
            context_history::open_context_result,
            acks::acknowledge_check,
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
            agent::finish_first_run,
            enrollment::preview_invite,
            enrollment::enroll_with_invite,
            enrollment::enroll_signed_in,
            enrollment::take_pending_invite,
            updates::install_update,
            updates::check_for_update,
            updates::dismiss_update_notes,
            reports::scan_device,
            reports::get_scan,
            reports::save_report,
            reports::get_saved_reports,
            reports::reveal_report,
            reports::open_report,
            reports::prepare_send,
            reports::prepare_edited_excel,
            reports::send_report,
            reports::open_report_help,
            reports::open_report_result,
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
            tray::build(app.handle())?;

            let handle = app.handle().clone();
            let mut config = agent::load_config(&handle);
            agent::enable_autostart_once(&handle, &mut config);
            let (first_run, just_updated) = agent::note_this_version(&mut config);
            let _ = agent::save_config(&handle, &config);
            // Enrolled, still owing the workspace a revoke after Disconnect, or
            // running regular checks: the login item starts it with work to do.
            let has_work = config.enrolled()
                || !config.pending_revokes.is_empty()
                || config.check_schedule.is_some();
            let collecting = config.collecting();
            tauri::async_runtime::block_on(async {
                let state: tauri::State<'_, agent::AgentState> = handle.state();
                *state.config.lock().await = config;
                *state.first_run.lock().await = first_run;
                *state.just_updated.lock().await = just_updated;
            });
            // Native menus in the language the window last chose; the window
            // reports it again once it has loaded.
            #[cfg(target_os = "macos")]
            {
                let lang = tauri::async_runtime::block_on(locale::current(&handle));
                app.set_menu(locale::app_menu(&handle, lang)?)?;
            }
            tauri::async_runtime::block_on(tray::refresh(&handle));

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

            context_history::start(handle.clone());
            browser_context::start(handle.clone());
            agent::spawn_checkin_loop(handle.clone());
            agent::spawn_revoke_loop(handle.clone());
            checks::spawn_schedule_loop(handle.clone());
            updates::spawn_update_loop(handle.clone());
            tray::spawn_refresh_loop(handle.clone());
            // Device management: a deployment token connects the computer.
            enrollment::spawn_managed_enrollment(handle.clone());
            // The browser hands a sign-in back through sanka-monitor://.
            account::register_deep_links(&handle);

            // The login item starts the agent in the tray; any other launch,
            // or a device with no connected work to do, opens the window.
            let background = std::env::args().any(|arg| arg == agent::BACKGROUND_ARG);
            // An update installed while the window was closed relaunches here,
            // and stays in the tray like the app it replaced.
            let relaunched = updates::take_background_relaunch(&handle);
            if !relaunched && (!has_work || !background) {
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
    if matches!(event, RunEvent::Exit) {
        browser_context::shutdown(app);
    }
    if let RunEvent::Reopen { .. } = event {
        show_main_window(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn on_run_event(app: &AppHandle, event: RunEvent) {
    if matches!(event, RunEvent::Exit) {
        browser_context::shutdown(app);
    }
}
