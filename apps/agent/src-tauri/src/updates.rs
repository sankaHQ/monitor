//! Self-updates from the Monitor release channel (`latest.json` on
//! release.sanka.com, then on this repository's latest GitHub release;
//! signed with Monitor's own updater key). Checks after launch and hourly.
//!
//! A newer build is downloaded in the background and staged: the window, the
//! tray and the sidebar offer "Restart to update", and it installs when the
//! person restarts or quits the app, or by itself once the window has been
//! closed for half an hour with no scan running. A build the workspace
//! requires (a check-in raised the minimum version above this one) installs at
//! once, as before. The release workflow refuses a tag that differs from the
//! agent's version, so an update can never offer the running version again.
//!
//! The check is the app's only network request when it is not enrolled. It
//! downloads `latest.json` and sends nothing about the device. The person can
//! turn it off (`update_checks_off`), and so can device management
//! (`DisableUpdateChecks`); the Update button still works.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::agent::{AgentConfig, AgentState, AGENT_VERSION, STATUS_EVENT};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(3600);
/// A staged update installs by itself after the window has been closed this
/// long (checked every minute).
const IDLE_INSTALL_MINUTES: u32 = 30;
/// Left by an idle install: the relaunch stays in the tray instead of opening
/// the window the person had closed.
const BACKGROUND_RELAUNCH_MARKER: &str = "relaunch-in-background";

/// A version and its release notes, as the window shows them.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// A downloaded, verified update waiting for a restart.
pub struct Staged {
    update: Update,
    bytes: Vec<u8>,
}

impl Staged {
    fn info(&self) -> UpdateInfo {
        UpdateInfo {
            version: self.update.version.clone(),
            notes: self
                .update
                .body
                .clone()
                .filter(|notes| !notes.trim().is_empty()),
        }
    }
}

pub fn spawn_update_loop(app: AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        let state: State<'_, AgentState> = app.state();
        loop {
            let (checks_on, required) = {
                let config = state.config.lock().await;
                (
                    !config.update_checks_off,
                    config.enrolled() && config.update_required(),
                )
            };
            let managed_off =
                tauri::async_runtime::spawn_blocking(crate::enrollment::managed_prefs)
                    .await
                    .unwrap_or_default()
                    .disable_update_checks
                    .unwrap_or(false);
            if (checks_on && !managed_off) || required {
                let result = stage_newer_version(&app).await;
                *state.last_update_check_at.lock().await = Some(chrono::Utc::now());
                *state.update_check_error.lock().await = result.err();
                if required && state.staged_update.lock().await.is_some() {
                    let _ = install_and_restart(&app, false).await;
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(CHECK_INTERVAL) => {}
                _ = state.update_wake.notified() => {}
            }
        }
    });
    spawn_idle_install(handle);
}

/// Installs a staged update once the window has stayed closed for
/// [`IDLE_INSTALL_MINUTES`] and no scan is running.
fn spawn_idle_install(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut closed_minutes = 0u32;
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let visible = app
                .get_webview_window("main")
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false);
            closed_minutes = if visible { 0 } else { closed_minutes + 1 };
            let state: State<'_, AgentState> = app.state();
            if closed_minutes < IDLE_INSTALL_MINUTES || state.staged_update.lock().await.is_none() {
                continue;
            }
            let Ok(_no_scan) = state.scan_lock.try_lock() else {
                continue;
            };
            drop(_no_scan);
            let _ = install_and_restart(&app, true).await;
        }
    });
}

/// Downloads and stages the newer version when the release channel has one.
/// `Ok(())` also when this is already the newest version or one is staged.
async fn stage_newer_version(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let _one_at_a_time = state.update_lock.lock().await;
    let updater = app.updater().map_err(|err| err.to_string())?;
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(()),
        Err(tauri_plugin_updater::Error::TargetNotFound(_)) => {
            return Err("no build for this computer has been published yet".to_string())
        }
        Err(err) => return Err(err.to_string()),
    };
    if let Some(staged) = state.staged_update.lock().await.as_ref() {
        if staged.update.version == update.version {
            return Ok(());
        }
    }
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|err| err.to_string())?;
    let staged = Staged { update, bytes };
    let notes = staged.info().notes;
    *state.staged_update.lock().await = Some(staged);
    // The notes are shown once after the restart, by the new version.
    {
        let mut config = state.config.lock().await;
        config.update_notes = notes;
        let _ = crate::agent::save_config(app, &config);
    }
    let _ = app.emit(STATUS_EVENT, ());
    crate::tray::refresh(app).await;
    Ok(())
}

/// Installs the staged update and restarts into it; never returns when it
/// works. `background`: the relaunch stays in the tray.
async fn install_and_restart(app: &AppHandle, background: bool) -> Result<(), String> {
    install_staged(app).await?;
    if background {
        if let Some(path) = marker_path(app) {
            let _ = std::fs::write(path, AGENT_VERSION);
        }
    }
    app.restart();
}

/// Installs the staged update without restarting (quitting).
pub async fn install_staged(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let _one_at_a_time = state.update_lock.lock().await;
    let staged = state
        .staged_update
        .lock()
        .await
        .take()
        .ok_or_else(|| "no update is ready yet".to_string())?;
    staged
        .update
        .install(&staged.bytes)
        .map_err(|err| format!("the update could not be installed: {err}"))
}

fn marker_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(BACKGROUND_RELAUNCH_MARKER))
}

/// An idle install relaunched the app: start in the tray. Consumes the marker.
pub fn take_background_relaunch(app: &AppHandle) -> bool {
    marker_path(app).is_some_and(|path| std::fs::remove_file(path).is_ok())
}

/// The staged update, if any.
pub async fn ready(app: &AppHandle) -> Option<UpdateInfo> {
    let state: State<'_, AgentState> = app.state();
    let staged = state.staged_update.lock().await;
    staged.as_ref().map(Staged::info)
}

/// The version this launch updated to, with the notes staged before the
/// restart, until the person dismisses them.
pub async fn just_updated(app: &AppHandle, config: &AgentConfig) -> Option<UpdateInfo> {
    let state: State<'_, AgentState> = app.state();
    let updated = *state.just_updated.lock().await;
    updated.then(|| UpdateInfo {
        version: AGENT_VERSION.to_string(),
        notes: config.update_notes.clone(),
    })
}

/// Restart to update: installs the staged update now, or checks, downloads
/// and installs, or says why not.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if app
        .state::<AgentState>()
        .staged_update
        .lock()
        .await
        .is_none()
    {
        stage_newer_version(&app)
            .await
            .map_err(|err| format!("the update could not be downloaded: {err}"))?;
    }
    if app
        .state::<AgentState>()
        .staged_update
        .lock()
        .await
        .is_none()
    {
        return Err("no newer version is available yet; ask your IT admin".to_string());
    }
    install_and_restart(&app, false).await
}

/// Check now (Settings): looks for a newer version and stages it.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let result = stage_newer_version(&app).await;
    let state: State<'_, AgentState> = app.state();
    *state.last_update_check_at.lock().await = Some(chrono::Utc::now());
    *state.update_check_error.lock().await = result.clone().err();
    result?;
    Ok(ready(&app).await)
}

#[tauri::command]
pub async fn dismiss_update_notes(app: AppHandle) {
    *app.state::<AgentState>().just_updated.lock().await = false;
}
