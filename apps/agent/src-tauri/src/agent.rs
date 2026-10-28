//! Agent state, enrollment, the check-in loop and the local data policy.
//!
//! The app works without enrollment: the local scan (`reports.rs`) needs no
//! account and sends nothing. Enrolling a device connects it to a workspace
//! (connected mode), and only then does anything below run.
//!
//! Privacy invariants enforced here:
//! - the device secret lives only in the OS keychain, never in the config file;
//! - every body sent to the workspace is recorded verbatim for the
//!   Transparency pane before it is sent — the employee sees exactly what the
//!   workspace sees — except the token request before each call, which
//!   carries only the device's id and secret;
//! - `sharing_consent` defaults to opted-out and is reported on every check-in,
//!   and only usage that started after the latest opt-in can be uploaded;
//! - usage is collected only while the device is enrolled and the person has
//!   opted in to sharing it; history recorded while sharing was on stays on
//!   the device 14 days after it is turned off;
//! - the workspace's kill switch stops collection and uploads, and a version
//!   below the workspace's minimum stops uploads until the agent updates;
//! - Disconnect revokes the device on the server, which deletes the usage it
//!   received, and a device the workspace revoked wipes itself the same way;
//! - installed software is device-tier data like posture: it follows a
//!   check-in (not while paused or outdated), carries names and versions
//!   only, and is not gated by the usage-sharing consent;
//! - "Who opened your data" is read from the workspace only while connected,
//!   when the Privacy pane asks for it: a device that is not connected sends
//!   nothing but the update check;
//! - the browser check (`saas.rs`) is the person's choice, off by default.
//!   What it finds stays on the computer, in reports the person saves or
//!   uploads, unless the person of a connected PC also turns on SaaS sharing:
//!   then `sync_saas` sends the workspace which SaaS this PC used in the last
//!   30 days and the last day each was used, recorded first in the
//!   Transparency pane like every request. Turning sharing (or the browser
//!   check) off, Disconnect and a revoke delete what the workspace received.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use monitor_posture::PostureCheckResult;
use monitor_store::sqlite::StoreResult;
use monitor_store::{PolicyOutcome, SqliteQueue};
use monitor_sync::{
    AgentClient, AgentEndpoint, CheckinData, CheckinRequest, EnrollRequest, EventBatchRequest,
    PersonViewsData, RevokeData, SaasUploadRequest, SaasUseItem, SoftwareUploadRequest, SyncError,
    WorkspaceAllowlist, PERSON_VIEWS_LIMIT,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter as _, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::scan::{collect_device_info, collect_posture, DeviceInfo};
use crate::usage::{self, UsageRuntime};

// The Cloudflare-fronted public API host — the same canonical origin the
// Chrome extension's store guardrail pins. Direct-to-Fly hostnames are
// rejected by sanka-api's edge-origin auth (EDGE_ORIGIN_AUTH_REQUIRED) —
// only the proxied hostname carries the required X-Sanka-Origin-Verify
// header, which Cloudflare injects. Overridable via `apiBase` in config.json.
pub const DEFAULT_API_BASE: &str = "https://api-v2.sanka.com";
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Passed by the login item: start in the tray without opening the window.
pub const BACKGROUND_ARG: &str = "--background";
pub(crate) const KEYRING_SERVICE: &str = "com.sanka.monitor";
const TRANSPARENCY_LIMIT: usize = 20;
const DEFAULT_CHECKIN_INTERVAL_SECONDS: u64 = 3600;
/// An unchanged installed-software snapshot is sent again after this long,
/// until the workspace says otherwise (`uploadIntervalSeconds`).
const DEFAULT_SOFTWARE_UPLOAD_INTERVAL_SECONDS: u64 = 86_400;
/// The local data policy (retention + consent window) runs this often.
const POLICY_INTERVAL_SECS: u64 = 3600;
/// An unconfirmed server-side revoke is retried this often.
const REVOKE_RETRY_SECS: u64 = 3600;
/// Disconnect waits this long for the workspace before wiping anyway.
const REVOKE_TIMEOUT: Duration = Duration::from_secs(20);
/// "Who opened your data" waits this long for the workspace.
const PERSON_VIEWS_TIMEOUT: Duration = Duration::from_secs(20);

/// The Sanka account the app sends reports as (`account.rs`). The device
/// secret lives only in the OS keychain; these are the names it shows.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountConfig {
    pub device_id: String,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    pub workspace_name: Option<String>,
    pub workspace_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentConfig {
    pub api_base: Option<String>,
    /// Where the sign-in page lives (the Monitor host); a developer override.
    pub app_base: Option<String>,
    pub device_id: Option<String>,
    pub workspace_id: Option<String>,
    pub sharing_consent: bool,
    /// When the employee last turned sharing on. Only usage that started at
    /// or after it may leave the device; `None` while sharing is off.
    pub sharing_consent_at: Option<DateTime<Utc>>,
    pub checkin_interval_seconds: Option<u64>,
    pub last_checkin_at: Option<DateTime<Utc>>,
    /// The workspace's kill switch as of the last check-in, kept across
    /// restarts so a paused agent never collects before its first check-in.
    pub kill_switch: bool,
    /// The oldest agent version the workspace accepts, as of the last check-in.
    pub min_agent_version: Option<String>,
    /// Launch at login was set up once after enrollment; later the
    /// employee's choice stands.
    pub autostart_configured: bool,
    /// Devices this computer disconnected whose revoke the workspace has not
    /// confirmed yet. Each keeps its keychain credential, and nothing else,
    /// until the workspace confirms or no longer knows the device.
    pub pending_revokes: Vec<String>,
    /// When the workspace turned out to have revoked this device, or to no
    /// longer know it, and the agent disconnected itself.
    pub removed_by_workspace_at: Option<DateTime<Utc>>,
    /// The installed-software snapshot the workspace last accepted: its
    /// fingerprint, when, how many programs, and the workspace's interval
    /// for sending it again unchanged.
    pub software_fingerprint: Option<String>,
    pub software_sent_at: Option<DateTime<Utc>>,
    pub software_count: Option<u64>,
    pub software_upload_interval_seconds: Option<u64>,
    /// The person turned off the automatic update check, the app's only
    /// network request when it is not enrolled. A device setting: it
    /// survives Disconnect.
    pub update_checks_off: bool,
    /// Signed in with a Sanka account to send reports. Not an enrollment: it
    /// survives Disconnect, and connecting does not need it.
    pub account: Option<AccountConfig>,
    /// The person turned on the browser check: scans read this computer's
    /// browser history to find the SaaS in use. Off by default; the
    /// person's setting, so it survives Disconnect.
    pub browser_saas_check: bool,
    /// Regular checks: rescan on this schedule and nudge about anything new.
    pub check_schedule: Option<crate::checks::CheckSchedule>,
    pub last_scheduled_check_at: Option<DateTime<Utc>>,
    /// The person of this connected PC shares which SaaS it used in the
    /// last 30 days with the workspace. Off by default; forgotten on
    /// Disconnect.
    pub saas_sharing: bool,
    /// The workspace's SaaS allowlist, from the latest check-in.
    pub workspace_allowlist: Option<WorkspaceAllowlist>,
    /// The workspace's name, from enrollment (0.4 APIs send it).
    pub workspace_name: Option<String>,
    /// Device management connected this computer (a deployment token in
    /// managed preferences).
    pub managed: bool,
    /// The workspace said a managed computer may not disconnect itself;
    /// managed preferences' `AllowDisconnect` overrides it.
    pub disconnect_locked: bool,
    /// The computer's owner as the workspace knows it, when it said.
    pub owner_email: Option<String>,
    /// How this computer connected: `code`, `invite`, `deployment_token`
    /// or `sign_in`.
    pub enrollment_method: Option<String>,
    /// Connected, but an admin has not approved the device yet: the token
    /// request is refused until then, and the check-in loop keeps asking.
    pub pending_approval: bool,
    /// The hash of the deployment token device management last connected
    /// with. Kept across removal, so a removed computer doesn't connect
    /// itself again with the same token.
    pub managed_token_hash: Option<String>,
    /// The person disconnected this computer themselves: device management's
    /// token doesn't connect it again until a different token is placed.
    pub disconnected_by_person: bool,
    /// The version that last ran here: a different one means the app was
    /// just updated, and says so once.
    pub last_run_version: Option<String>,
    /// The release notes of the update staged for the next launch.
    pub update_notes: Option<String>,
    /// When the person accepted a check's risk for themselves, by check id
    /// (`local_admin`, `device_management`; see acks.rs).
    pub acknowledged_checks: BTreeMap<String, DateTime<Utc>>,
    /// When an acknowledgeable check first failed in a row of scans, by check
    /// id; it starts the week before an unacknowledged admin account fails.
    pub failing_since: BTreeMap<String, DateTime<Utc>>,
}

impl AgentConfig {
    pub fn api_base(&self) -> String {
        self.api_base
            .clone()
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
    }

    pub fn app_base(&self) -> String {
        self.app_base
            .clone()
            .unwrap_or_else(|| crate::account::DEFAULT_APP_BASE.to_string())
    }

    pub fn enrolled(&self) -> bool {
        self.device_id.is_some()
    }

    /// Turns usage sharing on or off. Each opt-in opens a consent window at
    /// `now` and opting out closes it, so usage recorded while sharing was off
    /// never becomes uploadable later. A config saved before consent
    /// timestamps existed, with sharing on, gets a window starting `now`.
    pub fn set_sharing(&mut self, enabled: bool, now: DateTime<Utc>) {
        self.sharing_consent_at = if !enabled {
            None
        } else if self.sharing_consent {
            self.sharing_consent_at.or(Some(now))
        } else {
            Some(now)
        };
        self.sharing_consent = enabled;
    }

    /// Start of the consent window: usage that started earlier never leaves
    /// the device.
    pub fn upload_since(&self) -> Option<DateTime<Utc>> {
        if self.enrolled() && self.sharing_consent {
            self.sharing_consent_at
        } else {
            None
        }
    }

    pub fn update_required(&self) -> bool {
        version_below(AGENT_VERSION, self.min_agent_version.as_deref())
    }

    /// Where uploads may start right now: the consent window, while the
    /// workspace has not paused the agent and still accepts this version.
    pub fn uploads_from(&self) -> Option<DateTime<Utc>> {
        if self.kill_switch || self.update_required() {
            None
        } else {
            self.upload_since()
        }
    }

    /// Usage is collected only while enrolled and opted in to sharing, unless
    /// the workspace has paused the agent. A device that is not connected, or
    /// whose person has not opted in, records no usage at all.
    pub fn collecting(&self) -> bool {
        self.enrolled() && self.sharing_consent && !self.kill_switch
    }

    /// Installed software is collected and sent like posture: while enrolled,
    /// not while the workspace has paused the agent, and not while it needs a
    /// newer version. The usage-sharing consent does not apply.
    pub fn software_uploads_allowed(&self) -> bool {
        self.enrolled() && !self.kill_switch && !self.update_required()
    }

    /// The SaaS this PC used goes to the workspace only while it is
    /// connected, its person shares it and the browser check is on, and not
    /// while the workspace has paused the agent or needs a newer version.
    pub fn saas_uploads_allowed(&self) -> bool {
        self.enrolled()
            && self.saas_sharing
            && self.browser_saas_check
            && !self.kill_switch
            && !self.update_required()
    }

    /// A snapshot with `fingerprint` is sent when it differs from the last
    /// one the workspace accepted, or when the workspace's interval for
    /// sending it again has passed (or the clock went back).
    pub fn software_upload_due(&self, fingerprint: &str, now: DateTime<Utc>) -> bool {
        if !self.software_uploads_allowed() {
            return false;
        }
        if self.software_fingerprint.as_deref() != Some(fingerprint) {
            return true;
        }
        let interval = self
            .software_upload_interval_seconds
            .unwrap_or(DEFAULT_SOFTWARE_UPLOAD_INTERVAL_SECONDS);
        self.software_sent_at.is_none_or(|sent| {
            let elapsed = (now - sent).num_seconds();
            elapsed < 0 || elapsed.unsigned_abs() >= interval
        })
    }

    /// Forgets the enrollment and everything learned under it, keeping only
    /// the developer overrides, the revokes still owed to the server, the
    /// person's own settings (update checks, the browser check and regular
    /// checks) and the account they send reports as.
    pub fn unenrolled(&self) -> AgentConfig {
        AgentConfig {
            api_base: self.api_base.clone(),
            app_base: self.app_base.clone(),
            pending_revokes: self.pending_revokes.clone(),
            update_checks_off: self.update_checks_off,
            account: self.account.clone(),
            browser_saas_check: self.browser_saas_check,
            check_schedule: self.check_schedule,
            last_scheduled_check_at: self.last_scheduled_check_at,
            managed_token_hash: self.managed_token_hash.clone(),
            last_run_version: self.last_run_version.clone(),
            update_notes: self.update_notes.clone(),
            ..AgentConfig::default()
        }
    }

    /// Enrolled as `device_id` in `workspace_id`. Revokes still owed for
    /// earlier enrollments carry over.
    pub fn enrolled_as(
        &self,
        device_id: &str,
        workspace_id: &str,
        sharing: bool,
        now: DateTime<Utc>,
    ) -> AgentConfig {
        let mut next = AgentConfig {
            device_id: Some(device_id.to_string()),
            workspace_id: Some(workspace_id.to_string()),
            removed_by_workspace_at: None,
            disconnected_by_person: false,
            ..self.clone()
        };
        next.set_sharing(sharing, now);
        next
    }

    /// Disconnect: forgets the enrollment and owes the server a revoke of the
    /// device, which deletes the usage records it received.
    pub fn disconnected(&self) -> AgentConfig {
        let mut next = self.unenrolled();
        next.disconnected_by_person = true;
        if let Some(device_id) = &self.device_id {
            if !next.pending_revokes.contains(device_id) {
                next.pending_revokes.push(device_id.clone());
            }
        }
        next
    }

    /// The config after an agent call for `device_id` failed with `err`. A
    /// device the workspace revoked, or no longer knows, leaves it as on
    /// Disconnect, with no revoke owed: its credential no longer works.
    /// `None` keeps everything, including when this computer has disconnected
    /// or re-enrolled since the call started.
    pub fn after_call_error(
        &self,
        device_id: &str,
        err: &SyncError,
        now: DateTime<Utc>,
    ) -> Option<AgentConfig> {
        if !err.device_gone() || self.device_id.as_deref() != Some(device_id) {
            return None;
        }
        Some(AgentConfig {
            removed_by_workspace_at: Some(now),
            ..self.unenrolled()
        })
    }
}

/// True when `current` is older than the workspace's `minimum` (dotted
/// numeric versions). No minimum, or one that does not parse, never blocks.
pub fn version_below(current: &str, minimum: Option<&str>) -> bool {
    let Some(minimum) = minimum else {
        return false;
    };
    let parse = |version: &str| -> Vec<u64> {
        let mut parts: Vec<u64> = version
            .trim()
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect();
        while parts.last() == Some(&0) {
            parts.pop();
        }
        parts
    };
    parse(current) < parse(minimum)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentPayload {
    #[serde(skip)]
    request_id: Uuid,
    pub sent_at: DateTime<Utc>,
    pub endpoint: String,
    pub body: serde_json::Value,
    /// False until the server acknowledged the request; a recorded-but-failed
    /// attempt shows as "delivery unconfirmed" in the Transparency pane.
    pub delivered: bool,
}

/// A check as the Checks pane shows it: the check-in's result plus its name
/// in this OS's wording.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostureView {
    #[serde(flatten)]
    pub check: PostureCheckResult,
    pub label: &'static str,
}

/// An app used today on this device: presence only, never how long.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUsageView {
    pub app_id: String,
    pub app_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pub enrolled: bool,
    pub agent_version: String,
    pub api_base: String,
    pub device_id: Option<String>,
    pub workspace_id: Option<String>,
    pub sharing_consent: bool,
    pub sharing_consent_at: Option<DateTime<Utc>>,
    pub last_checkin_at: Option<DateTime<Utc>>,
    pub checkin_interval_seconds: u64,
    pub last_checkin: Option<CheckinData>,
    pub last_error: Option<String>,
    pub device: DeviceInfo,
    pub posture: Vec<PostureView>,
    /// Programs in the installed-software snapshot the workspace last
    /// accepted, and when.
    pub software_count: Option<u64>,
    pub software_sent_at: Option<DateTime<Utc>>,
    pub collecting: bool,
    /// The workspace's kill switch is on: nothing is collected or uploaded.
    pub paused: bool,
    /// The workspace no longer accepts this version: uploads wait for an update.
    pub update_required: bool,
    pub min_agent_version: Option<String>,
    pub launch_at_login: bool,
    pub pending_upload_count: u64,
    pub today_usage: Vec<LocalUsageView>,
    /// A Disconnect the workspace has not confirmed yet: retried hourly.
    pub revoke_pending: bool,
    /// Why the last attempt to reach the workspace for it failed.
    pub revoke_error: Option<String>,
    pub removed_by_workspace_at: Option<DateTime<Utc>>,
    /// The automatic update check is on (the default).
    pub update_checks: bool,
    pub last_update_check_at: Option<DateTime<Utc>>,
    pub update_check_error: Option<String>,
    pub workspace_name: Option<String>,
    pub managed: bool,
    /// Disconnect is offered: false for a managed computer whose device
    /// management or workspace doesn't allow it.
    pub can_disconnect: bool,
    pub owner_email: Option<String>,
    pub enrollment_method: Option<String>,
    pub pending_approval: bool,
    /// A newer version is downloaded and installs when the app restarts.
    pub update_ready: Option<crate::updates::UpdateInfo>,
    /// The app was just updated to this version: its notes, shown once.
    pub just_updated: Option<crate::updates::UpdateInfo>,
    pub first_run: bool,
}

/// The window reloads the status when this fires (an enrollment it didn't
/// start, an update that finished downloading).
pub const STATUS_EVENT: &str = "status-changed";

/// Lock order: `config` before `usage`. Collection is switched while the
/// config lock is held, so it always matches the config that was just saved.
/// `scan_lock` comes before `config`, `last_scan`, `pending_upload` and
/// `checks_lock`: whatever changes what a scan reads (the browser check, the
/// allowlist) waits for the scan in flight, so it cannot bring old results back.
/// `saas_sharing_lock` comes before `config` and `sent_payloads`, and nothing
/// holding it waits on `scan_lock`.
#[derive(Default)]
pub struct AgentState {
    pub config: Mutex<AgentConfig>,
    pub last_posture: Mutex<Vec<PostureCheckResult>>,
    pub last_checkin: Mutex<Option<CheckinData>>,
    pub last_error: Mutex<Option<String>>,
    pub sent_payloads: Mutex<VecDeque<SentPayload>>,
    /// Wakes the check-in loop when enrollment completes or the server hands
    /// back a different interval, so the loop never sleeps on stale timing.
    pub checkin_wake: Notify,
    /// Wakes the update loop when a check-in reports a newer minimum version.
    pub update_wake: Notify,
    /// One update check or install at a time (the loop and the Update button).
    pub update_lock: Mutex<()>,
    /// Local usage collection runtime (SQLite queue + collector switches).
    pub usage: Mutex<Option<UsageRuntime>>,
    /// One pass over the owed server-side revokes at a time (the retry loop
    /// and enrollment).
    pub revoke_lock: Mutex<()>,
    pub revoke_error: Mutex<Option<String>>,
    /// One installed-software pass at a time (check-ins can overlap: the
    /// loop, the tray, the window).
    pub software_lock: Mutex<()>,
    /// The latest local scan, and the reports saved from it this session.
    pub last_scan: Mutex<Option<monitor_report::ScanReport>>,
    pub saved_reports: Mutex<Vec<crate::reports::SavedReport>>,
    /// One local scan at a time.
    pub scan_lock: Mutex<()>,
    /// The report the person is about to send to Sanka: built or read once,
    /// shown, then sent as these exact bytes.
    pub pending_upload: Mutex<Option<crate::reports::PendingUpload>>,
    /// A sign-in the person started that the browser has not answered yet.
    pub pending_sign_in: Mutex<Option<crate::account::PendingSignIn>>,
    /// The signed-in app's runtime token and when it expires; refreshed under
    /// this lock.
    pub account_token: Mutex<Option<(String, std::time::Instant)>>,
    /// Why the last sign-in from the browser failed.
    pub account_error: Mutex<Option<String>>,
    /// When the update check last ran, and why it last failed.
    pub last_update_check_at: Mutex<Option<DateTime<Utc>>>,
    pub update_check_error: Mutex<Option<String>>,
    /// Wakes the regular-check loop when its schedule changes.
    pub schedule_wake: Notify,
    /// One request that carries the person's SaaS-sharing choice at a time (a
    /// check-in or a SaaS upload), from reading the choice to the workspace's
    /// answer: the workspace applies the choices in the order they were made, so
    /// an upload already under way can't bring back what turning sharing off
    /// deleted.
    pub saas_sharing_lock: Mutex<()>,
    /// One read-modify-write of the regular checks' state at a time.
    pub checks_lock: Mutex<()>,
    /// A downloaded update waiting for the app to restart.
    pub staged_update: Mutex<Option<crate::updates::Staged>>,
    /// This launch is the first after an update; its notes show until dismissed.
    pub just_updated: Mutex<bool>,
    /// No version of the app ran here before this launch: the window opens on
    /// the welcome screen.
    pub first_run: Mutex<bool>,
    /// An invite link opened before the window could show it.
    pub pending_invite: Mutex<Option<String>>,
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("app data dir unavailable: {err}"))?;
    Ok(dir.join("config.json"))
}

pub fn load_config(app: &AppHandle) -> AgentConfig {
    let Ok(path) = config_path(app) else {
        return AgentConfig::default();
    };
    let saved: AgentConfig = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut config = saved.clone();
    config.set_sharing(saved.sharing_consent, Utc::now());
    if config != saved {
        let _ = save_config(app, &config);
    }
    config
}

/// Turns launch at login on the first time this device runs enrolled from an
/// installed copy (at enrollment, or on the first launch of an already
/// enrolled device); later the employee's choice stands. Returns whether
/// `config` changed.
pub fn enable_autostart_once(app: &AppHandle, config: &mut AgentConfig) -> bool {
    if !config.enrolled() || config.autostart_configured || !running_installed() {
        return false;
    }
    if app.autolaunch().enable().is_err() {
        return false;
    }
    config.autostart_configured = true;
    true
}

/// A login item must point at an installed copy, not at the mounted disk
/// image or a translocated quarantine copy whose path changes every launch.
pub(crate) fn running_installed() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        let exe = exe.to_string_lossy();
        !(exe.starts_with("/Volumes/") || exe.contains("/AppTranslocation/"))
    })
}

/// Writes through a temporary file and a rename, so a crash never leaves a
/// torn config.
pub(crate) fn save_config(app: &AppHandle, config: &AgentConfig) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|err| err.to_string())?;
    std::fs::rename(&tmp, path).map_err(|err| err.to_string())
}

fn keyring_entry(device_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, device_id).map_err(|err| err.to_string())
}

async fn store_device_secret(device_id: String, secret: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        keyring_entry(&device_id)?
            .set_password(&secret)
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

pub(crate) async fn read_device_secret(device_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        keyring_entry(&device_id)?
            .get_password()
            .map_err(|err| format!("device credential unavailable: {err}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

/// `None` when the keychain has no credential for the device.
async fn read_device_secret_if_any(device_id: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || match keyring_entry(&device_id)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format!("device credential unavailable: {err}")),
    })
    .await
    .map_err(|err| err.to_string())?
}

async fn delete_device_secret(device_id: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        keyring_entry(&device_id)
            .and_then(|entry| entry.delete_credential().map_err(|err| err.to_string()))
    })
    .await;
}

/// Runs one check-in. While the workspace has paused the agent, the check-in
/// is only a heartbeat that learns when the pause lifts: no posture is
/// collected or sent. A check-in that lifts the pause is followed at once by
/// a full one. The installed software follows on its own task, so a slow
/// inventory probe never holds up the check-in (or the window waiting on it);
/// its failure shows as the last error.
pub async fn run_checkin(app: &AppHandle) -> Result<CheckinData, String> {
    let paused = app.state::<AgentState>().config.lock().await.kill_switch;
    let mut data = checkin_once(app, !paused).await?;
    if paused && !data.kill_switch {
        data = checkin_once(app, true).await?;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = sync_software(&app).await;
    });
    Ok(data)
}

/// Collects posture (unless told not to), exchanges the device secret for a
/// runtime token, sends, records everything for the Transparency pane, and
/// applies the workspace's answer.
async fn checkin_once(app: &AppHandle, with_posture: bool) -> Result<CheckinData, String> {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let Some(device_id) = config.device_id.clone() else {
        return Err("not enrolled".to_string());
    };
    let secret = read_device_secret(device_id.clone()).await?;

    let (posture, device) = tauri::async_runtime::spawn_blocking(move || {
        let posture = if with_posture {
            collect_posture()
        } else {
            Vec::new()
        };
        (posture, collect_device_info())
    })
    .await
    .map_err(|err| err.to_string())?;

    let client =
        AgentClient::new(config.api_base(), AGENT_VERSION).map_err(|err| err.to_string())?;
    let mut payload = CheckinRequest {
        agent_version: AGENT_VERSION.to_string(),
        platform: device.platform.clone(),
        os_version: device.os_version.clone(),
        hostname: device.hostname.clone(),
        serial_number: device.serial_number.clone(),
        device_model: device.device_model.clone(),
        os_edition: device.os_edition.clone(),
        sharing_consent: if config.sharing_consent {
            "opted_in".to_string()
        } else {
            "opted_out".to_string()
        },
        // Read just before sending (below), not with the rest of the config.
        saas_sharing: None,
        arch: Some(crate::enrollment::arch().to_string()),
        posture: posture.clone(),
    };

    if with_posture {
        *state.last_posture.lock().await = posture;
    }

    let token = match client.issue_token(&device_id, &secret).await {
        Ok(token) => {
            let mut config = state.config.lock().await;
            if config.pending_approval && config.device_id.as_deref() == Some(device_id.as_str()) {
                // Approved: this is the first token the workspace issued.
                config.pending_approval = false;
                let _ = save_config(app, &config);
            }
            token
        }
        // The check-in was never attempted, so nothing is recorded in the
        // Transparency history — it lists only requests actually made.
        Err(err) => return Err(call_failed(app, &device_id, err).await),
    };

    let (request_id, answer) = {
        let _in_order = state.saas_sharing_lock.lock().await;
        payload.saas_sharing = Some(state.config.lock().await.saas_sharing);
        // The check-in is about to be attempted — record the exact outbound body
        // first so even a mid-flight failure still shows in the Transparency pane.
        let request_id = state
            .record_payload(
                "/api/v2/monitor/agent/checkins",
                serde_json::to_value(&payload).unwrap_or_default(),
            )
            .await;
        (
            request_id,
            client.checkin(&token.access_token, &payload).await,
        )
    };

    match answer {
        Ok(data) => {
            state.mark_payload_delivered(request_id).await;
            let (newer_minimum, allowlist_changed) = {
                let mut config = state.config.lock().await;
                if config.device_id.as_deref() != Some(device_id.as_str()) {
                    // Unenrolled while the request was in flight.
                    return Err("not enrolled".to_string());
                }
                let allowlist_changed = config.workspace_allowlist != data.saas_allowlist;
                config.workspace_allowlist = data.saas_allowlist.clone();
                let minimum_changed =
                    config.min_agent_version.as_deref() != Some(data.min_agent_version.as_str());
                config.last_checkin_at = Some(Utc::now());
                config.checkin_interval_seconds = Some(data.checkin_interval_seconds);
                config.kill_switch = data.kill_switch;
                config.min_agent_version = Some(data.min_agent_version.clone());
                // The workspace's answer applies even if it cannot be saved.
                let _ = save_config(app, &config);
                if let Some(runtime) = state.usage.lock().await.as_ref() {
                    runtime.set_idle_threshold(data.idle_threshold_seconds);
                    runtime.set_collecting(config.collecting());
                }
                (
                    minimum_changed && config.update_required(),
                    allowlist_changed,
                )
            };
            if allowlist_changed {
                // The scan shown judges its SaaS by the workspace's new list,
                // in the window too.
                crate::saas::remark_last_scan(app).await;
                let _ = app.emit(crate::saas::SCAN_EVENT, ());
            }
            *state.last_checkin.lock().await = Some(data.clone());
            *state.last_error.lock().await = None;
            if newer_minimum {
                // Look for the update now rather than within the hour.
                state.update_wake.notify_one();
            }
            // Wake the scheduler so its next sleep uses the fresh interval.
            state.checkin_wake.notify_one();
            Ok(data)
        }
        Err(err) => Err(call_failed(app, &device_id, err).await),
    }
}

/// Sends the workspace the SaaS this PC used in the last 30 days, from the
/// scan the app holds, after a scan, in the background.
pub fn spawn_saas_sync(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = sync_saas(&app).await;
    });
}

/// The SaaS summary this PC shares: each SaaS the browser check found in the
/// 30 days ending on the scan's day, and the last day it was used. Nothing
/// else: no desktop apps (the workspace has the installed software), no day
/// counts, no web addresses.
pub fn saas_summary(report: &monitor_report::ScanReport) -> SaasUploadRequest {
    let collected_on = report.scanned_at.with_timezone(&chrono::Local).date_naive();
    let window_start = collected_on - chrono::Duration::days(29);
    let items = report
        .saas
        .iter()
        .filter_map(|record| {
            let used = record.browser?;
            (used.last_used_on >= window_start && used.last_used_on <= collected_on).then(|| {
                SaasUseItem {
                    key: record.key.clone(),
                    last_used_on: used.last_used_on,
                }
            })
        })
        .collect();
    SaasUploadRequest {
        sharing: true,
        collected_on,
        items,
    }
}

/// Sends the SaaS summary of the scan the app holds when its person shares
/// it (see [`AgentConfig::saas_uploads_allowed`]). A scan without the
/// browser check's results sends nothing: it has no summary to share.
pub async fn sync_saas(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let Some(device_id) = config.device_id.clone() else {
        return Ok(());
    };
    if !config.saas_uploads_allowed() {
        return Ok(());
    }
    let payload = match state.last_scan.lock().await.as_ref() {
        Some(report) if report.saas_check.browsers.is_some() => saas_summary(report),
        _ => return Ok(()),
    };
    send_saas(app, &config, &device_id, &payload).await
}

/// Tells the workspace to delete the SaaS this PC shared (the person turned
/// sharing or the browser check off). Without a connection the next
/// check-in, which says `saasSharing: false`, does the same.
pub async fn send_saas_sharing_off(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let Some(device_id) = config.device_id.clone() else {
        return Ok(());
    };
    let payload = SaasUploadRequest {
        sharing: false,
        collected_on: Utc::now().with_timezone(&chrono::Local).date_naive(),
        items: Vec::new(),
    };
    send_saas(app, &config, &device_id, &payload).await
}

async fn send_saas(
    app: &AppHandle,
    config: &AgentConfig,
    device_id: &str,
    payload: &SaasUploadRequest,
) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let secret = read_device_secret(device_id.to_string()).await?;
    let client =
        AgentClient::new(config.api_base(), AGENT_VERSION).map_err(|err| err.to_string())?;
    let token = match client.issue_token(device_id, &secret).await {
        Ok(token) => token,
        Err(err) => return Err(call_failed(app, device_id, err).await),
    };
    let (request_id, answer) = {
        let _in_order = state.saas_sharing_lock.lock().await;
        // The person may have changed their mind, or the PC disconnected, meanwhile.
        {
            let current = state.config.lock().await;
            let still = current.device_id.as_deref() == Some(device_id)
                && if payload.sharing {
                    current.saas_uploads_allowed()
                } else {
                    !current.saas_sharing
                };
            if !still {
                return Ok(());
            }
        }
        let request_id = state
            .record_payload(
                &AgentEndpoint::Saas.path(),
                serde_json::to_value(payload).unwrap_or_default(),
            )
            .await;
        (
            request_id,
            client.upload_saas(&token.access_token, payload).await,
        )
    };
    match answer {
        Ok(_) => {
            state.mark_payload_delivered(request_id).await;
            Ok(())
        }
        Err(err) => Err(call_failed(app, device_id, err).await),
    }
}

/// Collects the installed software and sends it when the snapshot changed or
/// the workspace's interval for sending it again passed, under the same rules
/// as posture (see [`AgentConfig::software_uploads_allowed`]). An empty
/// collection is a failed probe, never a snapshot: the workspace would mark
/// every program removed. A pass already running covers this one.
async fn sync_software(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let Ok(_one_pass) = state.software_lock.try_lock() else {
        return Ok(());
    };
    let config = state.config.lock().await.clone();
    let Some(device_id) = config.device_id.clone() else {
        return Ok(());
    };
    if !config.software_uploads_allowed() {
        return Ok(());
    }

    let collected_at = Utc::now();
    let items = tauri::async_runtime::spawn_blocking(monitor_inventory::collect)
        .await
        .map_err(|err| err.to_string())?;
    if items.is_empty() {
        return Ok(());
    }
    let fingerprint = monitor_inventory::fingerprint(&items);
    if !config.software_upload_due(&fingerprint, Utc::now()) {
        return Ok(());
    }

    let secret = read_device_secret(device_id.clone()).await?;
    let client =
        AgentClient::new(config.api_base(), AGENT_VERSION).map_err(|err| err.to_string())?;
    let token = match client.issue_token(&device_id, &secret).await {
        Ok(token) => token,
        Err(err) => return Err(call_failed(app, &device_id, err).await),
    };
    // The workspace's answer to another check-in, or a Disconnect, may have
    // arrived meanwhile.
    {
        let current = state.config.lock().await;
        if current.device_id != config.device_id || !current.software_uploads_allowed() {
            return Ok(());
        }
    }

    let count = items.len() as u64;
    let payload = SoftwareUploadRequest {
        collected_at,
        items,
    };
    let request_id = state
        .record_payload(
            &AgentEndpoint::Software.path(),
            serde_json::to_value(&payload).unwrap_or_default(),
        )
        .await;

    match client.upload_software(&token.access_token, &payload).await {
        Ok(data) => {
            state.mark_payload_delivered(request_id).await;
            let mut config = state.config.lock().await;
            if config.device_id.as_deref() == Some(device_id.as_str()) {
                config.software_fingerprint = Some(fingerprint);
                config.software_sent_at = Some(Utc::now());
                config.software_count = Some(count);
                config.software_upload_interval_seconds = Some(data.upload_interval_seconds);
                let _ = save_config(app, &config);
            }
            Ok(())
        }
        Err(err) => Err(call_failed(app, &device_id, err).await),
    }
}

/// Records a failed agent call for `device_id`. A device the workspace
/// revoked, or no longer knows, leaves it as on Disconnect: nothing it
/// collected stays on this computer.
async fn call_failed(app: &AppHandle, device_id: &str, err: SyncError) -> String {
    let message = err.to_string();
    if err.device_gone() {
        leave_removed_workspace(app, device_id, &err).await;
    } else {
        *app.state::<AgentState>().last_error.lock().await = Some(message.clone());
    }
    message
}

async fn leave_removed_workspace(app: &AppHandle, device_id: &str, err: &SyncError) {
    let state: State<'_, AgentState> = app.state();
    let saved = {
        let mut config = state.config.lock().await;
        let Some(next) = config.after_call_error(device_id, err, Utc::now()) else {
            return;
        };
        // Applies even if it cannot be saved: the next launch, still enrolled
        // on disk, asks again with the credential kept below.
        let saved = save_config(app, &next).is_ok();
        *config = next;
        if let Some(runtime) = state.usage.lock().await.as_ref() {
            runtime.set_collecting(false);
        }
        saved
    };
    // A failed wipe is finished by the hourly policy pass.
    let _ = wipe_local(app).await;
    if saved {
        delete_device_secret(device_id.to_string()).await;
    }
}

pub async fn status_view(app: &AppHandle) -> StatusView {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let last_checkin = state.last_checkin.lock().await.clone();
    let last_error = state.last_error.lock().await.clone();
    let revoke_error = state.revoke_error.lock().await.clone();
    let posture = state.last_posture.lock().await.clone();
    let runtime = state.usage.lock().await.clone();
    let upload_since = config.upload_since();
    let (collecting, pending_upload_count, today_usage) = match runtime {
        Some(runtime) => {
            let collecting = runtime
                .collecting
                .load(std::sync::atomic::Ordering::Relaxed);
            tauri::async_runtime::spawn_blocking(move || {
                let pending = upload_since
                    .and_then(|since| runtime.queue.pending_count(since).ok())
                    .unwrap_or(0);
                let today = usage::today_stats(&runtime.queue, 8)
                    .into_iter()
                    .map(|stat| LocalUsageView {
                        app_id: stat.app_id,
                        app_name: stat.app_name,
                    })
                    .collect::<Vec<_>>();
                (collecting, pending, today)
            })
            .await
            .unwrap_or((collecting, 0, Vec::new()))
        }
        None => (false, 0, Vec::new()),
    };
    let device = tauri::async_runtime::spawn_blocking(collect_device_info)
        .await
        .unwrap_or_else(|_| collect_device_info());
    let last_update_check_at = *state.last_update_check_at.lock().await;
    let update_check_error = state.update_check_error.lock().await.clone();
    let prefs = tauri::async_runtime::spawn_blocking(crate::enrollment::managed_prefs)
        .await
        .unwrap_or_default();
    let update_ready = crate::updates::ready(app).await;
    let just_updated = crate::updates::just_updated(app, &config).await;
    let first_run = *state.first_run.lock().await;
    StatusView {
        enrolled: config.enrolled(),
        agent_version: AGENT_VERSION.to_string(),
        api_base: config.api_base(),
        device_id: config.device_id.clone(),
        workspace_id: config.workspace_id.clone(),
        sharing_consent: config.sharing_consent,
        sharing_consent_at: config.sharing_consent_at,
        last_checkin_at: config.last_checkin_at,
        checkin_interval_seconds: config
            .checkin_interval_seconds
            .unwrap_or(DEFAULT_CHECKIN_INTERVAL_SECONDS),
        last_checkin,
        last_error,
        device,
        posture: posture
            .into_iter()
            .map(|check| PostureView {
                label: check.id.label(),
                check,
            })
            .collect(),
        software_count: config.software_count,
        software_sent_at: config.software_sent_at,
        collecting,
        paused: config.enrolled() && config.kill_switch,
        update_required: config.enrolled() && config.update_required(),
        min_agent_version: config.min_agent_version.clone(),
        launch_at_login: app.autolaunch().is_enabled().unwrap_or(false),
        pending_upload_count,
        today_usage,
        revoke_pending: !config.pending_revokes.is_empty(),
        revoke_error,
        removed_by_workspace_at: config.removed_by_workspace_at,
        update_checks: !config.update_checks_off,
        last_update_check_at,
        update_check_error,
        workspace_name: config.workspace_name.clone(),
        managed: config.managed,
        can_disconnect: crate::enrollment::disconnect_allowed(&config, &prefs),
        owner_email: config.owner_email.clone(),
        enrollment_method: config.enrollment_method.clone(),
        pending_approval: config.enrolled() && config.pending_approval,
        update_ready,
        just_updated,
        first_run,
    }
}

/// Remembers which version runs now: the first launch ever opens on the
/// welcome screen, and the first after an update shows its notes.
pub fn note_this_version(config: &mut AgentConfig) -> (bool, bool) {
    let first_run = config.last_run_version.is_none();
    let updated = config
        .last_run_version
        .as_deref()
        .is_some_and(|previous| previous != AGENT_VERSION);
    config.last_run_version = Some(AGENT_VERSION.to_string());
    (first_run, updated)
}

/// The welcome screen was seen.
#[tauri::command]
pub async fn finish_first_run(app: AppHandle) {
    *app.state::<AgentState>().first_run.lock().await = false;
}

#[tauri::command]
pub async fn get_status(app: AppHandle) -> StatusView {
    status_view(&app).await
}

/// Connects with a one-time code from the workspace.
#[tauri::command]
pub async fn enroll(
    app: AppHandle,
    code: String,
    sharing_consent: bool,
) -> Result<StatusView, String> {
    let request = EnrollRequest {
        code: Some(code.trim().to_string()),
        machine: Some(crate::enrollment::machine_fields()),
        ..EnrollRequest::default()
    };
    enroll_with(&app, request, sharing_consent, "code", None).await
}

pub async fn ensure_not_enrolled(app: &AppHandle) -> Result<(), String> {
    if app.state::<AgentState>().config.lock().await.enrolled() {
        return Err("this device is already enrolled".to_string());
    }
    Ok(())
}

/// Connects with one credential (a code, an invite or a deployment token).
/// `managed_token_hash` is set for device management's token.
pub async fn enroll_with(
    app: &AppHandle,
    request: EnrollRequest,
    sharing_consent: bool,
    method: &str,
    managed_token_hash: Option<String>,
) -> Result<StatusView, String> {
    ensure_not_enrolled(app).await?;
    let api_base = app.state::<AgentState>().config.lock().await.api_base();
    let client = AgentClient::new(api_base, AGENT_VERSION).map_err(|err| err.to_string())?;
    // Revokes still owed go first: the new device's first check-in revokes
    // this machine's earlier enrollments but keeps their usage records, and
    // after that no revoke from here can delete them.
    settle_pending_revokes(app).await;
    let enrollment = client
        .enroll(&request)
        .await
        .map_err(|err| err.to_string())?;
    finish_enrollment(app, enrollment, sharing_consent, method, managed_token_hash).await
}

/// Keeps the credential the workspace issued and starts checking in.
pub async fn finish_enrollment(
    app: &AppHandle,
    enrollment: monitor_sync::EnrollmentData,
    sharing_consent: bool,
    method: &str,
    managed_token_hash: Option<String>,
) -> Result<StatusView, String> {
    let app = app.clone();
    let state: State<'_, AgentState> = app.state();
    store_device_secret(
        enrollment.device_id.clone(),
        enrollment.device_secret.clone(),
    )
    .await?;
    {
        let mut config = state.config.lock().await;
        let mut next = config.enrolled_as(
            &enrollment.device_id,
            &enrollment.workspace_id,
            sharing_consent,
            Utc::now(),
        );
        next.workspace_name = enrollment.workspace_name.clone();
        next.managed = enrollment.managed || managed_token_hash.is_some();
        next.disconnect_locked = next.managed && !enrollment.allow_disconnect;
        next.owner_email = enrollment.owner_email.clone();
        next.enrollment_method = Some(method.to_string());
        next.pending_approval = enrollment.pending_approval();
        if managed_token_hash.is_some() {
            next.managed_token_hash = managed_token_hash;
        }
        // Launch at login, so collection resumes after a restart.
        enable_autostart_once(&app, &mut next);
        save_config(&app, &next)?;
        *config = next;
        if let Some(runtime) = state.usage.lock().await.as_ref() {
            runtime.set_collecting(config.collecting());
        }
    }
    let _ = apply_local_policy(&app).await;
    // First check-in immediately reports posture; enrollment stands even if
    // the network drops here — the scheduler retries.
    let _ = run_checkin(&app).await;
    // Wake the sleeping loop so it reschedules on the (possibly server-set)
    // interval instead of finishing a pre-enrollment default sleep.
    state.checkin_wake.notify_one();
    Ok(status_view(&app).await)
}

#[tauri::command]
pub async fn set_sharing_consent(app: AppHandle, enabled: bool) -> Result<StatusView, String> {
    let state: State<'_, AgentState> = app.state();
    {
        let mut config = state.config.lock().await;
        if !config.enrolled() {
            return Err("not enrolled".to_string());
        }
        let mut next = config.clone();
        next.set_sharing(enabled, Utc::now());
        // Saved before it applies: the consent on disk never lags the one in use.
        save_config(&app, &next)?;
        *config = next;
        // Usage is recorded only while sharing is on.
        if let Some(runtime) = state.usage.lock().await.as_ref() {
            runtime.set_collecting(config.collecting());
        }
    }
    // Turning sharing on withholds everything recorded before this moment;
    // turning it off withholds whatever was not uploaded yet. The upload
    // query enforces the same window, so a failure here leaks nothing.
    let _ = apply_local_policy(&app).await;
    // Report the new consent state to the workspace right away.
    let _ = run_checkin(&app).await;
    Ok(status_view(&app).await)
}

#[tauri::command]
pub async fn checkin_now(app: AppHandle) -> Result<StatusView, String> {
    run_checkin(&app).await?;
    Ok(status_view(&app).await)
}

#[tauri::command]
pub async fn get_transparency(app: AppHandle) -> Vec<SentPayload> {
    let state: State<'_, AgentState> = app.state();
    let sent = state.sent_payloads.lock().await;
    sent.iter().cloned().collect()
}

#[tauri::command]
pub async fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<StatusView, String> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|err| err.to_string())?;
    Ok(status_view(&app).await)
}

/// Turns the automatic update check on or off. Turning it on checks now.
#[tauri::command]
pub async fn set_update_checks(app: AppHandle, enabled: bool) -> Result<StatusView, String> {
    let state: State<'_, AgentState> = app.state();
    {
        let mut config = state.config.lock().await;
        let mut next = config.clone();
        next.update_checks_off = !enabled;
        save_config(&app, &next)?;
        *config = next;
    }
    if enabled {
        state.update_wake.notify_one();
    }
    Ok(status_view(&app).await)
}

/// Opens a workspace nudge's link in the default browser (https only).
#[tauri::command]
pub async fn open_nudge(app: AppHandle, id: String) -> Result<(), String> {
    let url = app
        .state::<AgentState>()
        .last_checkin
        .lock()
        .await
        .as_ref()
        .and_then(|data| data.nudges.iter().find(|nudge| nudge.id == id))
        .and_then(|nudge| nudge.url.clone())
        .filter(|url| url.starts_with("https://"))
        .ok_or("this link is no longer available")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| err.to_string())
}

/// Why "Who opened your data" could not be loaded. `offline`: the workspace
/// could not be reached or did not answer in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonViewsError {
    pub offline: bool,
    pub message: String,
}

impl PersonViewsError {
    fn local(message: String) -> Self {
        Self {
            offline: false,
            message,
        }
    }
}

impl From<&SyncError> for PersonViewsError {
    fn from(err: &SyncError) -> Self {
        Self {
            offline: matches!(err, SyncError::Http(_)),
            message: err.to_string(),
        }
    }
}

/// Asks `fetch` (API base, device id) for the person's views only while this
/// computer is connected to a workspace. Otherwise nothing is requested and
/// the answer is `None`. Usage sharing, a pause and an outdated version do
/// not matter: the list is about views already made.
async fn ask_person_views<F, Fut>(
    config: &AgentConfig,
    fetch: F,
) -> Option<Result<PersonViewsData, PersonViewsError>>
where
    F: FnOnce(String, String) -> Fut,
    Fut: std::future::Future<Output = Result<PersonViewsData, PersonViewsError>>,
{
    let device_id = config.device_id.clone()?;
    Some(fetch(config.api_base(), device_id).await)
}

/// The same token flow as the other agent calls. A device the workspace
/// revoked, or no longer knows, leaves it as after any other call; other
/// failures are only reported to the window, not as the last check-in error.
async fn fetch_person_views(
    app: &AppHandle,
    api_base: String,
    device_id: String,
) -> Result<PersonViewsData, PersonViewsError> {
    let secret = read_device_secret(device_id.clone())
        .await
        .map_err(PersonViewsError::local)?;
    let client =
        AgentClient::new(api_base, AGENT_VERSION).map_err(|err| PersonViewsError::from(&err))?;
    let ask = async {
        let token = client.issue_token(&device_id, &secret).await?;
        client
            .person_views(&token.access_token, PERSON_VIEWS_LIMIT)
            .await
    };
    match tokio::time::timeout(PERSON_VIEWS_TIMEOUT, ask).await {
        Ok(Ok(data)) => Ok(data),
        Ok(Err(err)) => {
            if err.device_gone() {
                leave_removed_workspace(app, &device_id, &err).await;
            }
            Err(PersonViewsError::from(&err))
        }
        Err(_) => Err(PersonViewsError {
            offline: true,
            message: "the workspace did not answer in time".to_string(),
        }),
    }
}

/// "Who opened your data": each time someone in the workspace opened this
/// person's daily app presence, and why. Asked when the Privacy pane opens,
/// never in the background. Newest first. `None` when this computer is not
/// connected (no request is made) or disconnected while the request was in
/// flight.
#[tauri::command]
pub async fn person_views(app: AppHandle) -> Result<Option<PersonViewsData>, PersonViewsError> {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let answer = ask_person_views(&config, |api_base, device_id| {
        fetch_person_views(&app, api_base, device_id)
    })
    .await;
    let Some(answer) = answer else {
        return Ok(None);
    };
    let mut data = answer?;
    if state.config.lock().await.device_id != config.device_id {
        return Ok(None);
    }
    data.views
        .sort_by_key(|view| std::cmp::Reverse(view.viewed_at));
    Ok(Some(data))
}

/// Disconnects this device: the workspace revokes it and deletes the usage
/// records it received, then the saved enrollment, the local usage history,
/// the keychain credential and the login item are removed here. Without an
/// answer from the workspace, the local wipe happens anyway and only the
/// credential and the login item stay, so the revoke is retried hourly and
/// after a restart.
#[tauri::command]
pub async fn unenroll(app: AppHandle) -> Result<StatusView, String> {
    let state: State<'_, AgentState> = app.state();
    let prefs = tauri::async_runtime::spawn_blocking(crate::enrollment::managed_prefs)
        .await
        .unwrap_or_default();
    if !crate::enrollment::disconnect_allowed(&*state.config.lock().await, &prefs) {
        return Err(
            "your company manages this connection, so it can't be disconnected here".to_string(),
        );
    }
    // Forget the enrollment and stop collecting first: a check-in that
    // answers after this finds no device and leaves collection alone, so the
    // wipe below is final. The credential stays for the revoke.
    let (api_base, device_id) = {
        let mut config = state.config.lock().await;
        let next = config.disconnected();
        save_config(&app, &next)?;
        let device_id = config.device_id.take();
        *config = next;
        if let Some(runtime) = state.usage.lock().await.as_ref() {
            runtime.set_collecting(false);
        }
        (config.api_base(), device_id)
    };
    // Only this device: Disconnect waits at most REVOKE_TIMEOUT, and older
    // revokes still owed stay with the retry loop. A concurrent attempt on
    // the same device settles it too, so this needs no `revoke_lock`.
    if let Some(device_id) = device_id {
        *state.revoke_error.lock().await = settle_revoke(&app, api_base, device_id).await;
    }
    wipe_local(&app)
        .await
        .map_err(|err| format!("{err}; Sanka Monitor tries again within the hour"))?;
    Ok(status_view(&app).await)
}

/// Drops the open interval and deletes the local usage history, the login
/// item, and what this session learned and sent. If the history cannot be
/// deleted, the hourly policy pass retries: an unenrolled device keeps none.
async fn wipe_local(app: &AppHandle) -> Result<usize, String> {
    let state: State<'_, AgentState> = app.state();
    let revoke_owed = !state.config.lock().await.pending_revokes.is_empty();
    let runtime = state.usage.lock().await.clone();
    let wiped = match runtime {
        Some(runtime) => {
            match tauri::async_runtime::spawn_blocking(move || runtime.discard_and_wipe()).await {
                Ok(result) => result,
                Err(err) => Err(err.to_string()),
            }
        }
        None => Ok(0),
    };
    // A revoke still owed is retried from login too; the login item goes
    // with the last one (see `settle_revoke`).
    if !revoke_owed {
        let _ = app.autolaunch().disable();
    }
    *state.last_checkin.lock().await = None;
    *state.last_posture.lock().await = Vec::new();
    *state.last_error.lock().await = None;
    state.sent_payloads.lock().await.clear();
    wiped
}

/// How one attempt to revoke a disconnected device ended.
#[derive(Debug)]
enum RevokeAttempt {
    /// The workspace revoked the device and deleted its usage records.
    Revoked,
    /// The workspace had already revoked the device or does not know it, or
    /// the credential is gone: there is nothing left to revoke from here.
    NothingToRevoke,
    Retry(String),
}

async fn attempt_revoke(api_base: String, device_id: String) -> RevokeAttempt {
    let secret = match read_device_secret_if_any(device_id.clone()).await {
        Ok(Some(secret)) => secret,
        Ok(None) => return RevokeAttempt::NothingToRevoke,
        Err(err) => return RevokeAttempt::Retry(err),
    };
    let client = match AgentClient::new(api_base, AGENT_VERSION) {
        Ok(client) => client,
        Err(err) => return RevokeAttempt::Retry(err.to_string()),
    };
    let revoke = async {
        let token = client.issue_token(&device_id, &secret).await?;
        client.revoke(&token.access_token).await
    };
    revoke_outcome(tokio::time::timeout(REVOKE_TIMEOUT, revoke).await.ok())
}

/// What the workspace's answer, `None` when none came in time, means for an
/// owed revoke. Only an answer about the device itself settles it; a 403
/// without a code (the token died with a revoke made meanwhile) is asked
/// again, and the next `/agent/token` tells.
fn revoke_outcome(answer: Option<Result<RevokeData, SyncError>>) -> RevokeAttempt {
    match answer {
        Some(Ok(_)) => RevokeAttempt::Revoked,
        Some(Err(err)) if err.device_gone() => RevokeAttempt::NothingToRevoke,
        Some(Err(err)) => RevokeAttempt::Retry(err.to_string()),
        None => RevokeAttempt::Retry("the workspace did not answer in time".to_string()),
    }
}

/// Tries to revoke one disconnected device on the server. A settled device
/// loses its credential, the last thing kept of it, and with the last owed
/// revoke the login item that kept retrying goes too. Returns why it must be
/// retried.
async fn settle_revoke(app: &AppHandle, api_base: String, device_id: String) -> Option<String> {
    if let RevokeAttempt::Retry(err) = attempt_revoke(api_base, device_id.clone()).await {
        return Some(err);
    }
    // Credential first: a revoke still listed without one settles on the
    // next pass, while a credential nothing lists would never be deleted.
    delete_device_secret(device_id.clone()).await;
    let state: State<'_, AgentState> = app.state();
    let mut config = state.config.lock().await;
    config.pending_revokes.retain(|id| id != &device_id);
    let _ = save_config(app, &config);
    if config.pending_revokes.is_empty() && !config.enrolled() {
        let _ = app.autolaunch().disable();
    }
    None
}

/// Revokes on the server each device this computer disconnected without the
/// workspace's confirmation; the ones without an answer wait for the next pass.
async fn settle_pending_revokes(app: &AppHandle) {
    let state: State<'_, AgentState> = app.state();
    let _one_pass = state.revoke_lock.lock().await;
    let (api_base, pending) = {
        let config = state.config.lock().await;
        (config.api_base(), config.pending_revokes.clone())
    };
    let mut failure = None;
    for device_id in pending {
        if let Some(err) = settle_revoke(app, api_base.clone(), device_id).await {
            failure = Some(err);
        }
    }
    *state.revoke_error.lock().await = failure;
}

/// Background loop: shortly after launch and then hourly, retries the
/// server-side revoke of each Disconnect the workspace has not confirmed.
pub fn spawn_revoke_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            settle_pending_revokes(&app).await;
            tokio::time::sleep(Duration::from_secs(REVOKE_RETRY_SECS)).await;
        }
    });
}

/// Background loop: check in on launch, then on the server-provided interval.
/// `checkin_wake` interrupts the sleep when enrollment completes or the server
/// hands back a different interval, so a stale sleep never delays a workspace's
/// requested cadence.
pub fn spawn_checkin_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let state: State<'_, AgentState> = app.state();
        if state.config.lock().await.enrolled() {
            let _ = run_checkin(&app).await;
        }
        loop {
            let interval = {
                let config = state.config.lock().await;
                config
                    .checkin_interval_seconds
                    .unwrap_or(DEFAULT_CHECKIN_INTERVAL_SECONDS)
                    .clamp(300, 86_400)
            };
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {
                    if state.config.lock().await.enrolled() {
                        let _ = run_checkin(&app).await;
                    }
                }
                _ = state.checkin_wake.notified() => {
                    // Recompute the sleep with fresh config; the wake itself
                    // performs no check-in (enroll/run_checkin already did).
                }
            }
        }
    });
}

/// Background loop, independent of enrollment, consent and the network:
/// applies the local data policy at launch and then hourly. Usage older than
/// 14 days is deleted, and pending usage outside the consent window is
/// withheld for good.
pub fn spawn_policy_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = apply_local_policy(&app).await;
            tokio::time::sleep(Duration::from_secs(POLICY_INTERVAL_SECS)).await;
        }
    });
}

pub async fn apply_local_policy(app: &AppHandle) -> Result<PolicyOutcome, String> {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let Some(runtime) = state.usage.lock().await.clone() else {
        return Ok(PolicyOutcome::default());
    };
    tauri::async_runtime::spawn_blocking(move || policy_pass(&config, &runtime.queue, Utc::now()))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

/// One pass of the local data policy, whatever the consent state. An enrolled
/// device deletes usage older than 14 days and withholds pending usage outside
/// the consent window; an unenrolled device keeps no usage history at all,
/// which also finishes a Disconnect whose wipe failed.
fn policy_pass(
    config: &AgentConfig,
    queue: &SqliteQueue,
    now: DateTime<Utc>,
) -> StoreResult<PolicyOutcome> {
    if config.enrolled() {
        return queue.apply_policy(now, config.upload_since());
    }
    if queue.is_empty()? {
        return Ok(PolicyOutcome::default());
    }
    Ok(PolicyOutcome {
        pruned: queue.wipe()?,
        withheld: 0,
    })
}

/// Background loop: uploads pending usage events on a fixed cadence. Uploads
/// happen only for usage inside the consent window, while the workspace has
/// not paused the agent and accepts this version — and the server enforces
/// consent independently.
pub fn spawn_upload_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(usage::UPLOAD_INTERVAL_SECS)).await;
            let _ = run_upload(&app).await;
        }
    });
}

pub async fn run_upload(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await.clone();
    let runtime = state.usage.lock().await.clone();
    let Some(runtime) = runtime else {
        return Ok(());
    };
    let (Some(device_id), Some(since)) = (config.device_id.clone(), config.uploads_from()) else {
        return Ok(());
    };

    let queue = runtime.queue.clone();
    let (pending, uploaded) = tauri::async_runtime::spawn_blocking(move || {
        let pending = queue.pending(500, since)?;
        queue.uploaded_days().map(|uploaded| (pending, uploaded))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())?;
    if pending.is_empty() {
        return Ok(());
    }

    let days: Vec<_> = monitor_sync::daily_presence(&pending)
        .into_iter()
        .filter(|day| !uploaded.contains(&(day.app_id.clone(), day.date)))
        .collect();
    if !days.is_empty() {
        let secret = read_device_secret(device_id.clone()).await?;
        let client =
            AgentClient::new(config.api_base(), AGENT_VERSION).map_err(|err| err.to_string())?;
        let token = match client.issue_token(&device_id, &secret).await {
            Ok(token) => token,
            Err(err) => return Err(call_failed(app, &device_id, err).await),
        };

        for batch in days.chunks(500) {
            // Token exchange and earlier requests may outlive this opt-in.
            {
                let current = state.config.lock().await;
                if current.device_id != config.device_id || current.uploads_from() != Some(since) {
                    return Ok(());
                }
            }
            let payload = EventBatchRequest {
                events: batch.to_vec(),
            };
            let request_id = state
                .record_payload(
                    "/api/v2/monitor/agent/events",
                    serde_json::to_value(&payload).unwrap_or_default(),
                )
                .await;
            match client.ingest_events(&token.access_token, &payload).await {
                Ok(_) => {
                    // Do not recreate an old workspace's ledger after Disconnect wiped it.
                    let current = state.config.lock().await;
                    if current.device_id != config.device_id
                        || current.uploads_from() != Some(since)
                    {
                        return Ok(());
                    }
                    let queue = runtime.queue.clone();
                    let days = batch
                        .iter()
                        .map(|day| (day.app_id.clone(), day.date))
                        .collect::<Vec<_>>();
                    tauri::async_runtime::spawn_blocking(move || {
                        queue.remember_uploaded_days(&days)
                    })
                    .await
                    .map_err(|err| err.to_string())?
                    .map_err(|err| err.to_string())?;
                    drop(current);
                    state.mark_payload_delivered(request_id).await;
                }
                Err(err) => return Err(call_failed(app, &device_id, err).await),
            }
        }
    }
    let ids: Vec<uuid::Uuid> = pending.iter().map(|event| event.event_id).collect();
    let queue = runtime.queue.clone();
    tauri::async_runtime::spawn_blocking(move || queue.mark_uploaded(&ids))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())?;
    *state.last_error.lock().await = None;
    Ok(())
}

impl AgentState {
    /// Record an attempt before sending it; completion may arrive in any order.
    pub async fn record_payload(&self, endpoint: &str, body: serde_json::Value) -> Uuid {
        let request_id = Uuid::new_v4();
        let mut sent = self.sent_payloads.lock().await;
        sent.push_front(SentPayload {
            request_id,
            sent_at: Utc::now(),
            endpoint: endpoint.to_string(),
            body,
            delivered: false,
        });
        sent.truncate(TRANSPARENCY_LIMIT);
        request_id
    }

    pub async fn mark_payload_delivered(&self, request_id: Uuid) {
        let mut sent = self.sent_payloads.lock().await;
        if let Some(payload) = sent
            .iter_mut()
            .find(|payload| payload.request_id == request_id)
        {
            payload.delivered = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use monitor_sync::{DEVICE_INVALID, DEVICE_REVOKED};

    fn api_error(status: u16, code: Option<&str>) -> SyncError {
        SyncError::Api {
            status,
            code: code.map(str::to_string),
            message: String::new(),
        }
    }

    #[test]
    fn payload_delivery_tracks_out_of_order_completions() {
        tauri::async_runtime::block_on(async {
            let state = AgentState::default();
            let first = state
                .record_payload("/checkins", serde_json::json!({}))
                .await;
            let second = state.record_payload("/events", serde_json::json!({})).await;
            state.mark_payload_delivered(second).await;
            {
                let sent = state.sent_payloads.lock().await;
                assert!(sent[0].delivered);
                assert!(!sent[1].delivered);
            }

            state
                .record_payload("/guide-events", serde_json::json!({}))
                .await;
            state.mark_payload_delivered(first).await;
            let sent = state.sent_payloads.lock().await;
            assert!(!sent[0].delivered);
            assert!(sent[1].delivered);
            assert!(sent[2].delivered);
            assert!(serde_json::to_value(&sent[2])
                .unwrap()
                .get("requestId")
                .is_none());
        });
    }

    #[test]
    fn payload_delivery_ignores_evicted_attempts() {
        tauri::async_runtime::block_on(async {
            let state = AgentState::default();
            let old = state
                .record_payload("/checkins", serde_json::json!({}))
                .await;
            for _ in 0..TRANSPARENCY_LIMIT {
                state.record_payload("/events", serde_json::json!({})).await;
            }
            state.mark_payload_delivered(old).await;
            let sent = state.sent_payloads.lock().await;
            assert_eq!(sent.len(), TRANSPARENCY_LIMIT);
            assert!(sent.iter().all(|payload| !payload.delivered));
        });
    }

    #[test]
    fn the_saas_summary_goes_only_where_its_person_shares_it_and_holds_only_key_and_day() {
        let mut config = AgentConfig {
            device_id: Some("device-1".into()),
            browser_saas_check: true,
            ..AgentConfig::default()
        };
        assert!(!config.saas_uploads_allowed(), "sharing is off by default");
        config.saas_sharing = true;
        assert!(config.saas_uploads_allowed());
        for blocked in [
            AgentConfig {
                browser_saas_check: false,
                ..config.clone()
            },
            AgentConfig {
                kill_switch: true,
                ..config.clone()
            },
            AgentConfig {
                min_agent_version: Some("999.0.0".into()),
                ..config.clone()
            },
            config.disconnected(),
        ] {
            assert!(!blocked.saas_uploads_allowed(), "{blocked:?}");
        }
        assert!(
            !config.unenrolled().saas_sharing,
            "Disconnect forgets the choice"
        );

        // A scan whose browser check found Slack yesterday and ChatGPT 40
        // days ago, and that has Dropbox installed: only Slack is shared.
        use monitor_saas::{BrowserUse, SaasCatalog};
        let at = Utc.with_ymd_and_hms(2026, 10, 26, 3, 0, 0).unwrap();
        let today = at.with_timezone(&chrono::Local).date_naive();
        let mut report = monitor_report::ScanReport::build(
            monitor_report::DeviceRecord {
                hostname: "PC-1".into(),
                platform: "macos".into(),
                os_version: "26.0".into(),
                os_edition: None,
                device_model: None,
                serial_number: None,
            },
            Vec::new(),
            vec![monitor_inventory::InstalledSoftware::new(
                monitor_inventory::SoftwareSource::MacosApp,
                "com.getdropbox.dropbox",
                "Dropbox",
                None,
                None,
            )
            .unwrap()],
            monitor_findings::Catalog::bundled(),
            today,
            at,
            "0.4.0",
        );
        let used = |days_ago: i64| BrowserUse {
            last_used_on: today - chrono::Duration::days(days_ago),
            days_used30: 1,
        };
        let browser = monitor_saas::history::BrowserScan {
            browsers: Vec::new(),
            uses: std::collections::BTreeMap::from([
                ("slack".to_string(), used(1)),
                ("chatgpt".to_string(), used(40)),
            ]),
        };
        report.set_saas(SaasCatalog::bundled(), Some(&browser), None);
        assert_eq!(
            saas_summary(&report),
            SaasUploadRequest {
                sharing: true,
                collected_on: today,
                items: vec![SaasUseItem {
                    key: "slack".into(),
                    last_used_on: today - chrono::Duration::days(1),
                }],
            }
        );
    }

    #[test]
    fn uploads_start_at_the_latest_opt_in_and_stop_while_paused_or_outdated() {
        let first = Utc.with_ymd_and_hms(2026, 10, 16, 9, 0, 0).unwrap();
        let later = Utc.with_ymd_and_hms(2026, 10, 16, 17, 0, 0).unwrap();
        let mut config = AgentConfig {
            device_id: Some("device-1".into()),
            ..AgentConfig::default()
        };
        assert_eq!(config.uploads_from(), None, "sharing is off by default");
        assert!(
            !config.collecting(),
            "no usage is recorded before the opt-in"
        );

        config.set_sharing(true, first);
        assert_eq!(config.uploads_from(), Some(first));
        config.set_sharing(true, later);
        assert_eq!(
            config.uploads_from(),
            Some(first),
            "already on: same window"
        );
        config.set_sharing(false, later);
        assert_eq!(config.uploads_from(), None);
        assert!(!config.collecting(), "opting out stops recording");
        config.set_sharing(true, later);
        assert_eq!(
            config.uploads_from(),
            Some(later),
            "a new opt-in, a new window"
        );

        let mut saved_before_timestamps = AgentConfig {
            device_id: Some("device-1".into()),
            sharing_consent: true,
            ..AgentConfig::default()
        };
        saved_before_timestamps.set_sharing(true, later);
        assert_eq!(saved_before_timestamps.uploads_from(), Some(later));

        let paused = AgentConfig {
            kill_switch: true,
            ..config.clone()
        };
        assert_eq!(paused.uploads_from(), None);
        assert!(!paused.collecting());
        let outdated = AgentConfig {
            min_agent_version: Some("999.0.0".into()),
            ..config.clone()
        };
        assert_eq!(outdated.uploads_from(), None);
        assert!(
            outdated.collecting(),
            "an outdated agent still records locally"
        );
        assert_eq!(config.unenrolled().uploads_from(), None);
    }

    fn usage_at(n: u128, started: DateTime<Utc>) -> monitor_core::UsageEvent {
        monitor_core::UsageEvent {
            event_id: Uuid::from_u128(n),
            app_id: format!("com.example.app{n}"),
            app_name: format!("App {n}"),
            started_at: started,
            ended_at: started + chrono::Duration::minutes(5),
            ended_by_idle: false,
        }
    }

    #[test]
    fn local_policy_runs_whatever_the_consent_state() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let days = chrono::Duration::days;
        let sharing_off = AgentConfig {
            device_id: Some("device-1".into()),
            ..AgentConfig::default()
        };
        let mut sharing_on = sharing_off.clone();
        sharing_on.set_sharing(true, now - days(20));
        // (config, rows kept, rows still uploadable)
        for (config, kept, uploadable) in [
            (sharing_off, 1, 0),
            (sharing_on, 1, 1),
            (AgentConfig::default(), 0, 0),
        ] {
            let dir = std::env::temp_dir().join(format!("monitor-policy-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let queue = SqliteQueue::open(dir.join("usage.db")).unwrap();
            // Past the 14 days: one uploaded, one withheld, one pending.
            queue.enqueue(&usage_at(1, now - days(16))).unwrap();
            queue.mark_uploaded(&[Uuid::from_u128(1)]).unwrap();
            queue.enqueue(&usage_at(2, now - days(16))).unwrap();
            queue.apply_policy(now - days(15), None).unwrap();
            queue.enqueue(&usage_at(3, now - days(15))).unwrap();
            queue
                .enqueue(&usage_at(4, now - chrono::Duration::hours(1)))
                .unwrap();

            policy_pass(&config, &queue, now).unwrap();

            let since = now - days(30);
            assert_eq!(
                queue.app_stats_since(since, 10).unwrap().len(),
                kept,
                "{config:?}"
            );
            assert_eq!(
                queue.pending_count(since).unwrap(),
                uploadable,
                "{config:?}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_device_the_workspace_removed_leaves_it_and_keeps_no_history() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let mut enrolled = AgentConfig {
            device_id: Some("device-2".into()),
            pending_revokes: vec!["device-1".into()],
            ..AgentConfig::default()
        };
        enrolled.set_sharing(true, now - chrono::Duration::days(1));
        // (error, device the call was for, leaves the workspace)
        for (err, device_id, leaves) in [
            (api_error(401, Some(DEVICE_REVOKED)), "device-2", true),
            (api_error(401, Some(DEVICE_INVALID)), "device-2", true),
            // Disconnected and re-enrolled while a call for device-1 ran.
            (api_error(401, Some(DEVICE_REVOKED)), "device-1", false),
            (
                api_error(403, Some("MONITOR_AGENT_UPDATE_REQUIRED")),
                "device-2",
                false,
            ),
            (SyncError::Envelope("bad gateway".into()), "device-2", false),
        ] {
            let dir = std::env::temp_dir().join(format!("monitor-removed-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let queue = SqliteQueue::open(dir.join("usage.db")).unwrap();
            queue
                .enqueue(&usage_at(1, now - chrono::Duration::hours(1)))
                .unwrap();

            let next = enrolled.after_call_error(device_id, &err, now);
            policy_pass(next.as_ref().unwrap_or(&enrolled), &queue, now).unwrap();

            assert_eq!(next.is_some(), leaves, "{err}");
            let kept = queue
                .app_stats_since(now - chrono::Duration::days(1), 10)
                .unwrap();
            assert_eq!(kept.len(), usize::from(!leaves), "{err}");
            if let Some(next) = next {
                assert!(!next.enrolled());
                assert_eq!(next.removed_by_workspace_at, Some(now));
                assert_eq!(next.pending_revokes, ["device-1"], "no revoke owed");
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn disconnect_owes_a_revoke_that_survives_re_enrollment() {
        let enrolled = AgentConfig {
            api_base: Some("https://api.example".into()),
            device_id: Some("device-1".into()),
            sharing_consent: true,
            software_fingerprint: Some("abc".into()),
            software_sent_at: Some(Utc::now()),
            software_count: Some(120),
            software_upload_interval_seconds: Some(3600),
            account: Some(AccountConfig {
                device_id: "account-1".into(),
                ..AccountConfig::default()
            }),
            ..AgentConfig::default()
        };
        let first = enrolled.disconnected();
        assert!(!first.enrolled());
        assert_eq!(first.api_base(), "https://api.example");
        assert_eq!(first.pending_revokes, ["device-1"]);
        assert_eq!(
            first.account, enrolled.account,
            "the account the person sends reports as is not part of the enrollment"
        );
        assert!(
            first.disconnected_by_person,
            "device management's token must not connect it again by itself"
        );
        assert_eq!(
            AgentConfig {
                api_base: None,
                pending_revokes: Vec::new(),
                account: None,
                disconnected_by_person: false,
                ..first.clone()
            },
            AgentConfig::default(),
            "nothing learned under the enrollment stays"
        );

        // Re-enrolled, then disconnected again before the workspace answered.
        let second = first
            .enrolled_as("device-2", "workspace-1", false, Utc::now())
            .disconnected();
        assert_eq!(second.pending_revokes, ["device-1", "device-2"]);
        assert_eq!(
            second.disconnected().pending_revokes,
            second.pending_revokes
        );
    }

    #[test]
    fn software_is_sent_when_it_changed_or_the_workspace_interval_passed() {
        let now = Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap();
        let hours = chrono::Duration::hours;
        let sent = AgentConfig {
            device_id: Some("device-1".into()),
            software_fingerprint: Some("same".into()),
            software_sent_at: Some(now - hours(2)),
            ..AgentConfig::default()
        };
        // (config, snapshot fingerprint, due)
        for (config, fingerprint, due) in [
            (sent.clone(), "same", false),
            (sent.clone(), "changed", true),
            (
                AgentConfig {
                    software_sent_at: Some(now - hours(24)),
                    ..sent.clone()
                },
                "same",
                true,
            ),
            (
                AgentConfig {
                    software_upload_interval_seconds: Some(3600),
                    ..sent.clone()
                },
                "same",
                true,
            ),
            (
                AgentConfig {
                    software_sent_at: Some(now + hours(1)),
                    ..sent.clone()
                },
                "same",
                true,
            ),
            (
                AgentConfig {
                    device_id: Some("device-1".into()),
                    ..AgentConfig::default()
                },
                "first",
                true,
            ),
            (
                AgentConfig {
                    kill_switch: true,
                    ..sent.clone()
                },
                "changed",
                false,
            ),
            (
                AgentConfig {
                    min_agent_version: Some("999.0.0".into()),
                    ..sent.clone()
                },
                "changed",
                false,
            ),
            (sent.disconnected(), "changed", false),
        ] {
            assert_eq!(
                config.software_upload_due(fingerprint, now),
                due,
                "{config:?} {fingerprint}"
            );
        }
    }

    #[test]
    fn only_an_answer_about_the_device_settles_an_owed_revoke() {
        let revoked = RevokeData {
            revoked_at: Utc::now(),
            deleted_usage_events: 3,
        };
        // (answer, settled: the credential is deleted and nothing retries)
        for (answer, settled) in [
            (Some(Ok(revoked)), true),
            (Some(Err(api_error(401, Some(DEVICE_REVOKED)))), true),
            (Some(Err(api_error(401, Some(DEVICE_INVALID)))), true),
            // The token died with a revoke made meanwhile: ask /agent/token.
            (Some(Err(api_error(403, None))), false),
            (Some(Err(api_error(503, None))), false),
            (None, false),
        ] {
            let outcome = revoke_outcome(answer);
            assert_eq!(
                !matches!(outcome, RevokeAttempt::Retry(_)),
                settled,
                "{outcome:?}"
            );
        }
    }

    #[test]
    fn person_views_are_asked_for_only_while_connected() {
        let now = Utc.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap();
        let connected = AgentConfig {
            api_base: Some("https://api.example".into()),
            device_id: Some("device-1".into()),
            workspace_id: Some("workspace-1".into()),
            ..AgentConfig::default()
        };
        let mut sharing = connected.clone();
        sharing.set_sharing(true, now);
        let removed = connected
            .after_call_error("device-1", &api_error(401, Some(DEVICE_REVOKED)), now)
            .unwrap();
        // (config, asks the workspace)
        for (config, asks) in [
            // Local mode: nothing but the update check leaves the computer.
            (AgentConfig::default(), false),
            (
                AgentConfig {
                    update_checks_off: true,
                    ..AgentConfig::default()
                },
                false,
            ),
            // Disconnected, still owing the workspace a revoke.
            (connected.disconnected(), false),
            (removed, false),
            // Connected: whether or not usage is shared, paused or outdated.
            (connected.clone(), true),
            (sharing.clone(), true),
            (
                AgentConfig {
                    kill_switch: true,
                    ..sharing.clone()
                },
                true,
            ),
            (
                AgentConfig {
                    min_agent_version: Some("999.0.0".into()),
                    ..connected.clone()
                },
                true,
            ),
        ] {
            let asked = std::sync::atomic::AtomicBool::new(false);
            let answer =
                tauri::async_runtime::block_on(ask_person_views(&config, |api_base, device_id| {
                    asked.store(true, std::sync::atomic::Ordering::SeqCst);
                    assert_eq!(
                        (api_base.as_str(), device_id.as_str()),
                        ("https://api.example", "device-1")
                    );
                    async {
                        Ok(PersonViewsData {
                            views: Vec::new(),
                            total: 0,
                        })
                    }
                }));
            assert_eq!(
                asked.load(std::sync::atomic::Ordering::SeqCst),
                asks,
                "{config:?}"
            );
            assert_eq!(answer.is_some(), asks, "{config:?}");
        }
    }

    #[test]
    fn person_views_errors_tell_offline_from_refused() {
        // (error, offline)
        for (err, offline) in [
            (api_error(401, Some(DEVICE_REVOKED)), false),
            (api_error(500, None), false),
            (SyncError::Envelope("bad gateway".into()), false),
        ] {
            assert_eq!(PersonViewsError::from(&err).offline, offline, "{err}");
        }
        // A port nothing listens on any more: the request cannot connect.
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let unreachable = tauri::async_runtime::block_on(async move {
            let client = AgentClient::new(format!("http://{closed}"), AGENT_VERSION).unwrap();
            client.person_views("token", PERSON_VIEWS_LIMIT).await
        })
        .unwrap_err();
        let err = PersonViewsError::from(&unreachable);
        assert!(err.offline, "{err:?}");
        assert_eq!(
            serde_json::to_value(&err).unwrap()["offline"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn versions_compare_numerically() {
        for (current, minimum, below) in [
            ("0.1.1", None, false),
            ("0.1.1", Some("0.1.0"), false),
            ("0.1.1", Some("0.1.1"), false),
            ("0.1.1", Some("0.1.1.0"), false),
            ("0.1.9", Some("0.1.10"), true),
            ("0.1.1", Some("0.2"), true),
            ("0.1.1", Some("not-a-version"), false),
        ] {
            assert_eq!(
                version_below(current, minimum),
                below,
                "{current} vs {minimum:?}"
            );
        }
    }
}
