//! The SaaS check in the app: the browser check the person turns on, the
//! company allowlist, and the Sanka migration page of a SaaS.
//!
//! - The browser check is off until the person ticks it. Then each scan
//!   reads this computer's browser history and keeps per SaaS only the last
//!   day used and the days used in the last 30 (`monitor_saas::history`).
//!   Turning it off waits for a scan in flight, then drops those results from
//!   the scan the app holds and from a report about to be uploaded, and
//!   forgets what the regular checks learned from them. By default they stay
//!   on this computer: they leave it inside a report the person saves or
//!   uploads, or, on a connected PC whose person turns on SaaS sharing, as the
//!   list of SaaS used in the last 30 days and the last day each was used
//!   (`agent::sync_saas`). Turning sharing off, or the browser check off,
//!   deletes what the workspace received.
//! - The allowlist is the machine-wide file device management places, else
//!   the workspace's list from its console (connected PCs), else the copy of
//!   a file the person chose here (kept in the app's data folder, so moving
//!   the original changes nothing).

use std::path::PathBuf;

use monitor_report::copy;
use monitor_report::{Lang, ScanReport};
use monitor_saas::allowlist::{self, ALLOWLIST_FILE_NAME};
use monitor_saas::history::BrowserScan;
use monitor_saas::{Allowlist, AllowlistOrigin, SaasCatalog};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::agent::{save_config, AgentState};
use crate::checks::{self, CheckSchedule, LocalNudge};

fn chosen_allowlist_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(ALLOWLIST_FILE_NAME))
}

/// The allowlist in force: the machine-wide file, else the workspace's list
/// (a connected PC hears it with each check-in), else a file the person chose.
/// `Ok(None)` when there is none, `Err` when a file that wins exists but is
/// broken (then nothing is judged, and the app says why).
pub async fn current_allowlist(app: &AppHandle) -> Result<Option<Allowlist>, String> {
    let catalog = SaasCatalog::bundled();
    let machine: Vec<(PathBuf, AllowlistOrigin)> = allowlist::machine_path()
        .map(|path| (path, AllowlistOrigin::Machine))
        .into_iter()
        .collect();
    if let Some(found) = allowlist::find(&machine, catalog) {
        return found.map(Some);
    }
    let workspace = app
        .state::<AgentState>()
        .config
        .lock()
        .await
        .workspace_allowlist
        .clone();
    if let Some(list) = workspace {
        return Ok(Some(Allowlist::from_keys(
            list.name,
            &list.allowed,
            catalog,
            AllowlistOrigin::Workspace,
        )));
    }
    let chosen: Vec<(PathBuf, AllowlistOrigin)> = chosen_allowlist_path(app)
        .map(|path| (path, AllowlistOrigin::App))
        .into_iter()
        .collect();
    allowlist::find(&chosen, catalog).transpose()
}

/// Reads `report`'s SaaS against the browser check's results (`None` while
/// it is off) and the allowlist in force. A broken allowlist judges nothing,
/// and the report says it could not be read.
pub fn apply_saas(
    report: &mut ScanReport,
    browser: Option<&BrowserScan>,
    allowlist: &Result<Option<Allowlist>, String>,
) {
    let list = allowlist.as_ref().ok().and_then(Option::as_ref);
    report.set_saas(SaasCatalog::bundled(), browser, list);
    report.saas_check.allowlist_unreadable = allowlist.is_err();
}

/// Runs the browser check now (blocking: it reads the history files).
pub fn browser_scan(now: chrono::DateTime<chrono::Utc>) -> BrowserScan {
    use chrono::{Local, TimeZone};
    let today = now.with_timezone(&Local).date_naive();
    let local_day = |unix: i64| {
        Local
            .timestamp_opt(unix, 0)
            .single()
            .map(|at| at.date_naive())
    };
    monitor_saas::history::scan(
        SaasCatalog::bundled(),
        &monitor_saas::history::sources(),
        now.timestamp(),
        today,
        &local_day,
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowlistView {
    pub origin: Option<AllowlistOrigin>,
    pub name: Option<String>,
    pub allowed: usize,
    pub unknown: Vec<String>,
    /// A file exists but could not be read as an allowlist.
    pub error: Option<String>,
    /// Where device management places the company's file on this OS.
    pub machine_path: Option<String>,
    /// The person chose a file here (it is used when there is no machine file).
    pub chosen: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasSettingsView {
    pub browser_check: bool,
    /// The person shares the SaaS this PC used with its workspace (connected only).
    pub saas_sharing: bool,
    /// Sharing is on but nothing is sent: the workspace paused Sanka Monitor, or
    /// this app needs an update.
    pub saas_sharing_held: bool,
    pub allowlist: AllowlistView,
    pub schedule: Option<CheckSchedule>,
    pub last_scheduled_check_at: Option<chrono::DateTime<chrono::Utc>>,
    pub nudges: Vec<LocalNudge>,
    pub catalog_verified_on: chrono::NaiveDate,
}

pub async fn allowlist_view(app: &AppHandle, lang: Lang) -> AllowlistView {
    let (list, error) = match current_allowlist(app).await {
        Ok(list) => (list, None),
        Err(err) => (None, Some(err)),
    };
    let summary = list.as_ref().map(monitor_report::AllowlistSummary::from);
    AllowlistView {
        origin: list.as_ref().map(|l| l.origin),
        name: list.as_ref().and_then(|l| l.name.clone()),
        allowed: list.as_ref().map_or(0, |l| l.allowed.len()),
        unknown: list.as_ref().map(|l| l.unknown.clone()).unwrap_or_default(),
        error,
        machine_path: allowlist::machine_path().map(|p| p.display().to_string()),
        chosen: chosen_allowlist_path(app).is_some_and(|p| p.exists()),
        summary: copy::allowlist_summary(summary.as_ref(), lang),
    }
}

pub async fn settings_view(app: &AppHandle, lang: Lang) -> SaasSettingsView {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    SaasSettingsView {
        browser_check: config.browser_saas_check,
        saas_sharing: config.saas_sharing,
        saas_sharing_held: config.saas_sharing && !config.saas_uploads_allowed(),
        allowlist: allowlist_view(app, lang).await,
        schedule: config.check_schedule,
        last_scheduled_check_at: config.last_scheduled_check_at,
        nudges: checks::nudges(app).await,
        catalog_verified_on: SaasCatalog::bundled().verified_on(),
    }
}

#[tauri::command]
pub async fn get_saas_settings(app: AppHandle, lang: Lang) -> SaasSettingsView {
    settings_view(&app, lang).await
}

/// Re-marks the scan the app holds against the allowlist now in force,
/// after any scan in flight.
pub(crate) async fn remark_last_scan(app: &AppHandle) {
    let state: State<'_, AgentState> = app.state();
    let _one_scan = state.scan_lock.lock().await;
    let allowlist = current_allowlist(app).await;
    let mut last_scan = state.last_scan.lock().await;
    if let Some(report) = last_scan.as_mut() {
        let list = allowlist.as_ref().ok().and_then(Option::as_ref);
        report.set_allowlist(SaasCatalog::bundled(), list);
        report.saas_check.allowlist_unreadable = allowlist.is_err();
    }
}

/// Turns the browser check on or off. Off drops its results from the scan
/// the app holds, forgets what the regular checks learned from them, and
/// turns SaaS sharing off, which deletes what the workspace received.
#[tauri::command]
pub async fn set_browser_check(
    app: AppHandle,
    enabled: bool,
    lang: Lang,
) -> Result<SaasSettingsView, String> {
    let state: State<'_, AgentState> = app.state();
    // A scan in flight read the old setting: let it finish first, so its
    // results are dropped below rather than stored after.
    let one_scan = state.scan_lock.lock().await;
    let was_sharing = {
        let mut config = state.config.lock().await;
        let mut next = config.clone();
        let was_sharing = next.saas_sharing;
        next.browser_saas_check = enabled;
        if !enabled {
            next.saas_sharing = false;
        }
        save_config(&app, &next)?;
        *config = next;
        was_sharing
    };
    if !enabled && was_sharing {
        stop_sharing(&app);
    }
    if !enabled {
        let allowlist = current_allowlist(&app).await;
        let mut last_scan = state.last_scan.lock().await;
        if let Some(report) = last_scan.as_mut() {
            apply_saas(report, None, &allowlist);
        }
        drop(last_scan);
        *state.pending_upload.lock().await = None;
        checks::forget(&app).await;
    }
    drop(one_scan);
    Ok(settings_view(&app, lang).await)
}

/// Tells the workspace to delete what this PC shared, in the background. A
/// PC that is offline tells it with its next check-in (`saasSharing: false`).
fn stop_sharing(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::agent::send_saas_sharing_off(&app).await;
    });
}

/// Turns sharing the SaaS this PC used with the workspace on or off. On
/// needs a connected PC and the browser check; it sends the summary now. Off
/// deletes what the workspace received.
#[tauri::command]
pub async fn set_saas_sharing(
    app: AppHandle,
    enabled: bool,
    lang: Lang,
) -> Result<SaasSettingsView, String> {
    let state: State<'_, AgentState> = app.state();
    let was_sharing = {
        let mut config = state.config.lock().await;
        if enabled && !(config.enrolled() && config.browser_saas_check) {
            return Err("turn on the browser check on a connected computer first".to_string());
        }
        let mut next = config.clone();
        let was_sharing = next.saas_sharing;
        next.saas_sharing = enabled;
        save_config(&app, &next)?;
        *config = next;
        was_sharing
    };
    if enabled {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::agent::sync_saas(&app).await;
        });
    } else if was_sharing {
        stop_sharing(&app);
    }
    Ok(settings_view(&app, lang).await)
}

/// Uses a file the person picks as the allowlist (validated, then copied
/// into the app's data folder). `None` when they cancel.
#[tauri::command]
pub async fn choose_allowlist(app: AppHandle, lang: Lang) -> Result<SaasSettingsView, String> {
    let dialog = app.dialog().file().add_filter(lang_label(lang), &["json"]);
    let Some(picked) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(|err| err.to_string())?
    else {
        return Ok(settings_view(&app, lang).await);
    };
    let source = picked
        .into_path()
        .map_err(|err| format!("cannot open that file: {err}"))?;
    Allowlist::read(&source, SaasCatalog::bundled(), AllowlistOrigin::App)?;
    let bytes = std::fs::read(&source).map_err(|err| err.to_string())?;
    let target = chosen_allowlist_path(&app).ok_or("app data folder unavailable")?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    crate::cli::write_whole(&target, &bytes).map_err(|err| err.to_string())?;
    remark_last_scan(&app).await;
    Ok(settings_view(&app, lang).await)
}

fn lang_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Sanka Monitor allowlist",
        Lang::Ja => "Sanka Monitor の許可リスト",
    }
}

/// Stops using the file the person chose. A machine-wide file stays in force.
#[tauri::command]
pub async fn clear_allowlist(app: AppHandle, lang: Lang) -> Result<SaasSettingsView, String> {
    if let Some(path) = chosen_allowlist_path(&app).filter(|p| p.exists()) {
        std::fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    remark_last_scan(&app).await;
    Ok(settings_view(&app, lang).await)
}

/// Opens a SaaS's Sanka migration page in the browser.
#[tauri::command]
pub async fn open_migration_page(app: AppHandle, key: String, lang: Lang) -> Result<(), String> {
    let url = SaasCatalog::bundled()
        .get(&key)
        .and_then(|entry| entry.migrate_url(lang.code()))
        .ok_or("Sanka has no migration page for this SaaS")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| err.to_string())
}
