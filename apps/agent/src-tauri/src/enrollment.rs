//! The ways a computer connects to a workspace besides a pasted code
//! (tracking/monitor-0.4-redesign/contract.md in the umbrella repo):
//!
//! - **Device management.** A deployment token in managed preferences
//!   (`com.sanka.monitor` on macOS, `HKLM\SOFTWARE\Policies\Sanka\Monitor` on
//!   Windows) connects the computer at launch, with nothing for the person to
//!   do. Device management can never turn on app-usage sharing.
//! - **Invite links.** `sanka-monitor://enroll?invite=…` opens a sheet that
//!   shows who the invite is from ([`preview_invite`]) before connecting.
//! - **Sign in with your company.** The account the person signed in with to
//!   send reports (`account.rs`) connects this computer to its workspace.
//!
//! Every enrollment sends a hash of the hardware id, so the workspace replaces
//! this computer's earlier device instead of listing it twice. The id itself
//! never leaves the computer.

use std::process::Command;
use std::time::Duration;

use monitor_sync::{AgentClient, EnrollRequest, InvitePreview, MachineFields};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::agent::{self, AgentState, StatusView, AGENT_VERSION};

/// The window listens for this to open the "Connect to …?" sheet.
pub const INVITE_EVENT: &str = "enroll-invite";
/// Device management's token is tried again this often until it works.
const MANAGED_RETRY: Duration = Duration::from_secs(3600);

/// What device management placed for Sanka Monitor. Keys as in the contract.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase", default)]
pub struct ManagedPrefs {
    pub deployment_token: Option<String>,
    pub owner_email: Option<String>,
    pub allow_disconnect: Option<bool>,
    pub disable_update_checks: Option<bool>,
}

impl ManagedPrefs {
    pub fn token(&self) -> Option<&str> {
        self.deployment_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    }
}

/// Reads managed preferences; nothing placed reads as the default.
pub fn managed_prefs() -> ManagedPrefs {
    #[cfg(target_os = "macos")]
    {
        let path = "/Library/Managed Preferences/com.sanka.monitor.plist";
        if !std::path::Path::new(path).exists() {
            return ManagedPrefs::default();
        }
        Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-", path])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| serde_json::from_slice(&out.stdout).ok())
            .unwrap_or_default()
    }
    #[cfg(target_os = "windows")]
    {
        let key = r"HKLM\SOFTWARE\Policies\Sanka\Monitor";
        let text = |name: &str| reg_value(key, name);
        let flag = |name: &str| {
            text(name).map(|v| v == "0x1" || v.eq_ignore_ascii_case("true") || v == "1")
        };
        ManagedPrefs {
            deployment_token: text("DeploymentToken"),
            owner_email: text("OwnerEmail"),
            allow_disconnect: flag("AllowDisconnect"),
            disable_update_checks: flag("DisableUpdateChecks"),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        ManagedPrefs::default()
    }
}

/// One value from `reg query`: the last column of the line naming it.
#[cfg(target_os = "windows")]
fn reg_value(key: &str, name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = Command::new("reg")
        .args(["query", key, "/v", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            (parts.next()? == name).then(|| parts.skip(1).collect::<Vec<_>>().join(" "))
        })
}

/// The hardware id this computer reports to the OS (IOPlatformUUID on macOS,
/// MachineGuid on Windows).
fn hardware_id() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("/usr/sbin/ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|line| {
                let (_, value) = line.split_once("\"IOPlatformUUID\" = ")?;
                Some(value.trim().trim_matches('"').to_string())
            })
    }
    #[cfg(target_os = "windows")]
    {
        reg_value(r"HKLM\SOFTWARE\Microsoft\Cryptography", "MachineGuid")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

pub fn hash_hardware_id(id: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("sanka-monitor:{id}").as_bytes())
    )
}

pub fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    }
}

fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else {
        "macos"
    }
}

pub fn machine_fields() -> MachineFields {
    MachineFields {
        // A computer that won't say its id still enrolls; it just can't be
        // told apart from an earlier enrollment of itself.
        hardware_id_hash: hardware_id()
            .map(|id| hash_hardware_id(&id))
            .unwrap_or_default(),
        platform: platform().to_string(),
        arch: arch().to_string(),
        agent_version: AGENT_VERSION.to_string(),
    }
}

/// The deployment token, as remembered: only its hash, to notice when the
/// workspace removed this computer and device management still holds the
/// same token (then it must not connect again on its own).
fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

/// The invite in `sanka-monitor://enroll?invite=<token>`.
pub fn invite_from_deep_link(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "sanka-monitor" || url.host_str() != Some("enroll") {
        return None;
    }
    url.query_pairs()
        .find(|(key, _)| key == "invite")
        .map(|(_, value)| value.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// An invite link was opened: show the window and its sheet. Kept until the
/// window takes it, for a link that launched the app before the window loaded.
pub fn handle_invite_link(app: &AppHandle, invite: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        *app.state::<AgentState>().pending_invite.lock().await = Some(invite.clone());
        crate::show_main_window(&app);
        let _ = app.emit(INVITE_EVENT, invite);
    });
}

/// The invite link waiting for the window, once.
#[tauri::command]
pub async fn take_pending_invite(app: AppHandle) -> Option<String> {
    app.state::<AgentState>().pending_invite.lock().await.take()
}

async fn client(app: &AppHandle) -> Result<AgentClient, String> {
    let api_base = app.state::<AgentState>().config.lock().await.api_base();
    AgentClient::new(api_base, AGENT_VERSION).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn preview_invite(app: AppHandle, invite: String) -> Result<InvitePreview, String> {
    client(&app)
        .await?
        .invite_preview(invite.trim())
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn enroll_with_invite(
    app: AppHandle,
    invite: String,
    sharing_consent: bool,
) -> Result<StatusView, String> {
    let request = EnrollRequest {
        invite_token: Some(invite.trim().to_string()),
        machine: Some(machine_fields()),
        ..EnrollRequest::default()
    };
    agent::enroll_with(&app, request, sharing_consent, "invite", None).await
}

/// Connects with the account the person signed in with: the browser asked
/// them to pick the workspace, and they are the computer's owner.
#[tauri::command]
pub async fn enroll_signed_in(app: AppHandle, sharing_consent: bool) -> Result<StatusView, String> {
    agent::ensure_not_enrolled(&app).await?;
    let token = crate::account::access_token(&app)
        .await
        .map_err(|err| err.to_string())?;
    let client = client(&app).await?;
    let enrollment = client
        .desktop_enroll(&token, &machine_fields())
        .await
        .map_err(|err| err.to_string())?;
    agent::finish_enrollment(&app, enrollment, sharing_consent, "sign_in", None).await
}

/// Device management: connects at launch, then retries hourly, whenever
/// managed preferences hold a deployment token and the computer isn't
/// connected. Never after the workspace removed it, or its person
/// disconnected it, under the same token.
pub fn spawn_managed_enrollment(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let prefs = tauri::async_runtime::spawn_blocking(managed_prefs)
                .await
                .unwrap_or_default();
            if let Some(token) = prefs.token() {
                let _ = enroll_managed(&app, token, &prefs).await;
            }
            tokio::time::sleep(MANAGED_RETRY).await;
        }
    });
}

async fn enroll_managed(app: &AppHandle, token: &str, prefs: &ManagedPrefs) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let hash = token_hash(token);
    {
        let config = state.config.lock().await;
        if config.enrolled() || !config.pending_revokes.is_empty() {
            return Ok(());
        }
        let same_token = config.managed_token_hash.as_deref() == Some(hash.as_str());
        if same_token && (config.removed_by_workspace_at.is_some() || config.disconnected_by_person)
        {
            return Ok(());
        }
    }
    let request = EnrollRequest {
        deployment_token: Some(token.to_string()),
        owner_email: prefs
            .owner_email
            .clone()
            .filter(|email| !email.trim().is_empty()),
        managed: true,
        machine: Some(machine_fields()),
        ..EnrollRequest::default()
    };
    // Usage sharing stays off: device management can't turn it on.
    agent::enroll_with(app, request, false, "deployment_token", Some(hash)).await?;
    let _ = app.emit(agent::STATUS_EVENT, ());
    Ok(())
}

/// Whether this computer may disconnect itself: always, unless device
/// management connected it and doesn't allow it (managed preferences, else
/// the workspace's answer at enrollment).
pub fn disconnect_allowed(config: &agent::AgentConfig, prefs: &ManagedPrefs) -> bool {
    if !config.managed {
        return true;
    }
    prefs.allow_disconnect.unwrap_or(!config.disconnect_locked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_links_carry_their_token_and_nothing_else_matches() {
        assert_eq!(
            invite_from_deep_link("sanka-monitor://enroll?invite=smin_abc123").as_deref(),
            Some("smin_abc123")
        );
        assert_eq!(
            invite_from_deep_link("sanka-monitor://enroll?invite="),
            None
        );
        assert_eq!(
            invite_from_deep_link("sanka-monitor://auth/callback?code=x&state=y"),
            None
        );
        assert_eq!(
            invite_from_deep_link("https://enroll?invite=smin_abc"),
            None
        );
    }

    #[test]
    fn the_hardware_id_leaves_only_as_a_salted_hash() {
        let hash = hash_hardware_id("A1B2C3D4-0000-1111-2222-333344445555");
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains("A1B2C3D4"));
        assert_ne!(
            hash,
            format!(
                "{:x}",
                Sha256::digest(b"A1B2C3D4-0000-1111-2222-333344445555")
            )
        );
    }
}
