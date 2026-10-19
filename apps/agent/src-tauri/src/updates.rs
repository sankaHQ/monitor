//! Self-updates from the Monitor release channel (the latest release of
//! `sankaHQ/monitor-releases`, signed with Monitor's own updater key). Ported
//! from Sanka Recorder: check after launch and hourly, install a newer build in
//! the background and restart into it. A check-in that raises the minimum
//! version above this build wakes the loop at once. The release workflow
//! refuses a tag that differs from the agent's version, so an update can
//! never offer the running version again.
//!
//! The check is the app's only network request when it is not enrolled. It
//! downloads `latest.json` and sends nothing about the device. The person can
//! turn it off (`update_checks_off`); the Update button still works.

use std::time::Duration;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::agent::AgentState;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(3600);

pub fn spawn_update_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        let state: State<'_, AgentState> = app.state();
        loop {
            let enabled = !state.config.lock().await.update_checks_off;
            if enabled {
                let result = install_newer_version(&app).await;
                *state.last_update_check_at.lock().await = Some(chrono::Utc::now());
                *state.update_check_error.lock().await = result.err();
            }
            tokio::select! {
                _ = tokio::time::sleep(CHECK_INTERVAL) => {}
                _ = state.update_wake.notified() => {}
            }
        }
    });
}

/// Installs a newer version when the release channel has one; the app then
/// restarts into it and this never returns. `Ok(())` means this is already
/// the newest version.
async fn install_newer_version(app: &AppHandle) -> Result<(), String> {
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
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|err| err.to_string())?;
    app.restart();
}

/// The Update button: install now, or say why not.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    install_newer_version(&app)
        .await
        .map_err(|err| format!("the update could not be installed: {err}"))?;
    Err("no newer version is available yet; ask your IT admin".to_string())
}
