//! HTTP client for the sanka-api `/api/v2/monitor/agent` surface, and for
//! `/api/v2/monitor/desktop`, where the app signed in with a Sanka account
//! sends Sanka one report. The agent also reads one list back: who in the
//! workspace opened this person's app usage, and why (`/agent/person-views`).
//!
//! Auth model (mirrors the Chrome-extension surface): a one-time enrollment
//! code becomes a `device_id` + `device_secret`; the secret is exchanged for a
//! short-lived Bearer runtime token before each check-in. Signing in to send
//! works the same way, with a one-time code from the browser plus its PKCE
//! verifier instead of an enrollment code. Every response is the standard
//! `{success, data, meta}` envelope.
//!
//! The SaaS a connected PC used leaves only after its person turns on
//! sharing (`/agent/saas`): each SaaS's catalog key and the last day it was
//! used, nothing else ([`SaasUploadRequest`]).

use chrono::{DateTime, Duration, NaiveDate, Utc};
use monitor_core::{AGENT_API_BASE_PATH, DESKTOP_API_BASE_PATH};
use monitor_inventory::InstalledSoftware;
use monitor_posture::PostureCheckResult;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEndpoint {
    /// Enrollment code → `device_id` + `device_secret`.
    Enrollments,
    /// Device secret → short-lived runtime bearer token.
    Token,
    /// Posture snapshot + versions → nudges, config, kill switch, min version.
    Checkins,
    /// Batched usage-event ingest (P2; only after the employee opted in).
    Events,
    /// The agent's Disconnect: revokes the device and deletes its usage
    /// records on the server.
    Revoke,
    /// Installed-software snapshot. A full snapshot: whatever the device
    /// reported before and is missing now is marked removed.
    Software,
    /// Read-only: each time someone in the workspace opened this person's
    /// daily app presence, with the reason they gave. Sends nothing.
    PersonViews,
    /// Read-only, no auth: who an invite link is from, before connecting.
    Invites,
    /// The SaaS this PC used in the last 30 days, after its person turned
    /// on sharing; `sharing: false` deletes what it shared.
    Saas,
}

impl AgentEndpoint {
    pub fn path(self) -> String {
        let leaf = match self {
            AgentEndpoint::Enrollments => "enrollments",
            AgentEndpoint::Token => "token",
            AgentEndpoint::Checkins => "checkins",
            AgentEndpoint::Events => "events",
            AgentEndpoint::Revoke => "revoke",
            AgentEndpoint::Software => "software",
            AgentEndpoint::PersonViews => "person-views",
            AgentEndpoint::Invites => "invites",
            AgentEndpoint::Saas => "saas",
        };
        format!("{AGENT_API_BASE_PATH}/{leaf}")
    }
}

/// The signed-in app's routes (`/api/v2/monitor/desktop/*`): sign-in, tokens,
/// sending a report as the account, and signing out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEndpoint {
    /// One-time code + PKCE verifier → `device_id` + `device_secret`.
    Devices,
    /// Device secret → one-hour runtime bearer token.
    Token,
    /// Sign out: the credential and every token it holds stop working.
    Revoke,
    /// One report, sent as the signed-in account.
    Reports,
    /// Connects this computer to the workspace the person signed in to.
    Enrollments,
}

impl DesktopEndpoint {
    pub fn path(self) -> String {
        let leaf = match self {
            DesktopEndpoint::Devices => "devices",
            DesktopEndpoint::Token => "token",
            DesktopEndpoint::Revoke => "revoke",
            DesktopEndpoint::Reports => "reports",
            DesktopEndpoint::Enrollments => "enrollments",
        };
        format!("{DESKTOP_API_BASE_PATH}/{leaf}")
    }
}

/// `/agent/token` answers this once the device was revoked: in the console,
/// by its own Disconnect, or replaced by a newer enrollment of the machine.
pub const DEVICE_REVOKED: &str = "MONITOR_DEVICE_REVOKED";
/// `/agent/token` answers this when no device has this id and secret.
pub const DEVICE_INVALID: &str = "MONITOR_DEVICE_INVALID";
/// `/agent/token` answers this while an admin has not approved the device yet.
pub const DEVICE_PENDING: &str = "MONITOR_DEVICE_PENDING";
/// `/agent/enrollments` answers this for a code, token or invite that is
/// unknown, expired, revoked, used up, or for another email domain.
pub const ENROLLMENT_INVALID: &str = "MONITOR_ENROLLMENT_INVALID";
/// `/agent/invites/{token}` answers this for an unknown or expired invite.
pub const INVITE_INVALID: &str = "MONITOR_INVITE_INVALID";
/// `/desktop/devices` answers this for an unknown, expired, used or
/// mismatched sign-in code.
pub const DESKTOP_INVALID_GRANT: &str = "MONITOR_DESKTOP_INVALID_GRANT";
/// The signed-in app's credential no longer works: signed out (here or by
/// Sanka), or the member left the workspace. Sign in again.
pub const DESKTOP_DEVICE_REVOKED: &str = "MONITOR_DESKTOP_DEVICE_REVOKED";
pub const DESKTOP_DEVICE_INVALID: &str = "MONITOR_DESKTOP_DEVICE_INVALID";

#[derive(Debug)]
pub enum SyncError {
    Http(reqwest::Error),
    /// The API refused the request. `code` is the envelope's `error.code`
    /// when the body carried one.
    Api {
        status: u16,
        code: Option<String>,
        message: String,
    },
    Envelope(String),
}

impl SyncError {
    pub fn code(&self) -> Option<&str> {
        match self {
            SyncError::Api { code, .. } => code.as_deref(),
            _ => None,
        }
    }

    /// The workspace revoked this device or no longer knows it. Neither
    /// credential works again, so retrying is pointless.
    pub fn device_gone(&self) -> bool {
        matches!(self.code(), Some(DEVICE_REVOKED | DEVICE_INVALID))
    }

    /// An admin has not approved this device yet: keep the credential and
    /// try again later.
    pub fn device_pending(&self) -> bool {
        self.code() == Some(DEVICE_PENDING)
    }

    /// The signed-in app's credential no longer works: sign in again.
    pub fn signed_out(&self) -> bool {
        matches!(
            self.code(),
            Some(DESKTOP_DEVICE_REVOKED | DESKTOP_DEVICE_INVALID)
        )
    }

    /// A runtime token the API did not accept (expired or unknown) while the
    /// credential itself may still work: ask for a new token and try again.
    pub fn token_rejected(&self) -> bool {
        matches!(self, SyncError::Api { status: 401, .. }) && !self.signed_out()
    }
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Http(err) => write!(f, "network error: {err}"),
            SyncError::Api {
                status,
                code: Some(code),
                message,
            } => write!(f, "api error ({status} {code}): {message}"),
            SyncError::Api {
                status,
                code: None,
                message,
            } => write!(f, "api error ({status}): {message}"),
            SyncError::Envelope(message) => write!(f, "unexpected response: {message}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<reqwest::Error> for SyncError {
    fn from(err: reqwest::Error) -> Self {
        SyncError::Http(err)
    }
}

pub type SyncResult<T> = Result<T, SyncError>;

/// One credential (`code`, `deployment_token` or `invite_token`) and what the
/// workspace needs to tell this computer apart: a hash of its hardware id,
/// never the id itself. `{"code": …}` alone is the 0.3 request.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_token: Option<String>,
    /// Device management's owner of the computer (an MDM variable); only
    /// with a deployment token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_email: Option<String>,
    /// The credential came from managed preferences.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub managed: bool,
    #[serde(flatten)]
    pub machine: Option<MachineFields>,
}

/// What every new enrollment sends about the computer itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineFields {
    /// sha256("sanka-monitor:" + IOPlatformUUID / MachineGuid), hex.
    pub hardware_id_hash: String,
    pub platform: String,
    pub arch: String,
    pub agent_version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentData {
    pub device_id: String,
    pub device_secret: String,
    pub workspace_id: String,
    /// The rest arrived with 0.4; an older API sends none of it.
    #[serde(default)]
    pub workspace_name: Option<String>,
    /// `active`, or `pending_approval` until an admin approves the device.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub managed: bool,
    #[serde(default)]
    pub owner_email: Option<String>,
    #[serde(default = "default_true")]
    pub allow_disconnect: bool,
}

fn default_true() -> bool {
    true
}

impl EnrollmentData {
    pub fn pending_approval(&self) -> bool {
        self.status.as_deref() == Some("pending_approval")
    }
}

/// Who an invite link is from, shown before the person connects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitePreview {
    pub workspace_name: String,
    #[serde(default)]
    pub invited_by: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRequest {
    pub device_id: String,
    pub device_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenData {
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinRequest {
    pub agent_version: String,
    pub platform: String,
    pub os_version: String,
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_model: Option<String>,
    /// Windows `EditionID` (`Professional`, `Enterprise`, `EnterpriseS`, …):
    /// the workspace picks Windows lifecycle dates by it. macOS has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_edition: Option<String>,
    pub sharing_consent: String,
    /// Whether the person shares the SaaS this PC used; `false` makes the
    /// workspace delete what was shared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saas_sharing: Option<bool>,
    /// `arm64` or `x86_64`: how many Intel Macs still check in decides when
    /// the universal build retires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    pub posture: Vec<PostureCheckResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBatchRequest {
    pub events: Vec<UsageDay>,
}

/// Only this daily fact can leave the device; focus intervals stay in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    pub app_id: String,
    pub app_name: String,
    pub date: NaiveDate,
}

pub fn daily_presence(events: &[monitor_core::UsageEvent]) -> Vec<UsageDay> {
    let mut days = BTreeMap::new();
    for event in events {
        if event.ended_at <= event.started_at {
            continue;
        }
        let mut day = event.started_at.date_naive();
        let last = (event.ended_at - Duration::microseconds(1)).date_naive();
        while day <= last {
            days.entry((event.app_id.clone(), day))
                .or_insert_with(|| UsageDay {
                    app_id: event.app_id.clone(),
                    app_name: event.app_name.clone(),
                    date: day,
                });
            let Some(next) = day.succ_opt() else { break };
            day = next;
        }
    }
    days.into_values().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBatchData {
    pub accepted: u32,
    pub duplicates: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NudgeData {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinData {
    pub kill_switch: bool,
    pub min_agent_version: String,
    pub checkin_interval_seconds: u64,
    pub idle_threshold_seconds: u64,
    #[serde(default)]
    pub nudges: Vec<NudgeData>,
    /// The workspace's SaaS allowlist, set in its console.
    #[serde(default)]
    pub saas_allowlist: Option<WorkspaceAllowlist>,
}

/// The SaaS the workspace allows, by catalog key, as its console sets them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAllowlist {
    #[serde(default)]
    pub name: Option<String>,
    pub allowed: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

/// One SaaS this PC used: its catalog key and the last day it was used.
/// Nothing else, and no room for more (see the wire-shape test).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasUseItem {
    pub key: String,
    pub last_used_on: NaiveDate,
}

/// The PC's whole SaaS summary for the workspace. `sharing: false` carries
/// no items and deletes what the PC shared before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasUploadRequest {
    pub sharing: bool,
    /// The PC's local date: the 30 days end on it.
    pub collected_on: NaiveDate,
    pub items: Vec<SaasUseItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasUploadData {
    pub stored: u64,
    pub ignored: u64,
    pub deleted: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUploadRequest {
    pub collected_at: DateTime<Utc>,
    pub items: Vec<InstalledSoftware>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUploadData {
    pub received: u64,
    pub added: u64,
    pub changed: u64,
    pub removed: u64,
    /// Re-upload an unchanged snapshot after this long.
    pub upload_interval_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeData {
    pub revoked_at: DateTime<Utc>,
    pub deleted_usage_events: u64,
}

/// The most views the agent asks for at once: the latest ones.
pub const PERSON_VIEWS_LIMIT: u32 = 50;

/// One time someone in the workspace opened this person's daily app
/// presence: when, which days, who (`None` when the workspace does not name
/// them) and the reason they gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonView {
    pub viewed_at: DateTime<Utc>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(default)]
    pub viewer_name: Option<String>,
    pub reason: String,
}

/// The latest views, newest first, and how many there are in all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonViewsData {
    pub views: Vec<PersonView>,
    pub total: u64,
}

/// `/desktop/devices`: the one-time code the browser handed back and the
/// verifier only this app knows. Nothing about the computer goes with it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSignInRequest {
    pub code: String,
    pub code_verifier: String,
    pub platform: String,
    pub app_version: String,
}

/// The workspace the app sends as: the sender's company.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspace {
    pub id: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopUser {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSignInData {
    pub device_id: String,
    pub device_secret: String,
    pub workspace: DesktopWorkspace,
    pub user: DesktopUser,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTokenRequest {
    pub device_id: String,
    pub device_secret: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTokenData {
    pub access_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRevokeData {
    pub revoked_at: DateTime<Utc>,
}

/// The counts the app shows beside a report it built from a scan, sent as
/// they were shown. Company level: no computer names, people or software.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub programs: usize,
    pub programs_past_support: usize,
}

/// What goes with a report besides the file; every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReportSendFields {
    pub note: Option<String>,
    /// The id the app gave the scan, for a report it built.
    pub report_id: Option<String>,
    pub agent_version: Option<String>,
    /// Left out for a workbook the person edited: the app cannot vouch for it.
    pub summary: Option<ReportSummary>,
    pub evidence: Option<String>,
    pub purpose_version: Option<String>,
}

impl ReportSendFields {
    /// The form fields as sent, in order, without the empty ones. The counts
    /// travel as one JSON field.
    pub fn form(&self) -> Vec<(&'static str, String)> {
        let text = [
            ("note", &self.note),
            ("reportId", &self.report_id),
            ("agentVersion", &self.agent_version),
            ("evidence", &self.evidence),
            ("purposeVersion", &self.purpose_version),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| (name, v.to_string()))
        });
        let summary = self.summary.map(|summary| {
            (
                "summary",
                serde_json::to_string(&summary).expect("counts serialize"),
            )
        });
        text.chain(summary).collect()
    }
}

/// Sanka's receipt: its own SHA-256 of the bytes it stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportReceipt {
    pub id: String,
    pub received_at: DateTime<Utc>,
    pub file_name: String,
    pub format: String,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub evidence_sha256: Option<String>,
}

/// A `multipart/form-data` body: the text fields, then the file, byte for
/// byte. Field values and the file name never contain the boundary's line
/// breaks or quotes (they are replaced), so a value cannot open a new part.
pub fn multipart_body(
    boundary: &str,
    fields: &[(&str, String)],
    file_name: &str,
    content_type: &str,
    content: &[u8],
) -> Vec<u8> {
    let quoted = |value: &str| value.replace(['"', '\\', '\r', '\n'], "_");
    let mut body = Vec::with_capacity(content.len() + 512);
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(value.replace('\r', "").as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {content_type}\r\n\r\n",
            quoted(file_name)
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    success: bool,
    // Option fields deserialize to None when missing — no `default` attr,
    // which would force a `T: Default` bound on the generic.
    data: Option<T>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct AgentClient {
    base_url: String,
    http: reqwest::Client,
}

impl AgentClient {
    pub fn new(base_url: impl Into<String>, agent_version: &str) -> SyncResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(format!("sanka-monitor-agent/{agent_version}"))
            .build()?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn enroll(&self, request: &EnrollRequest) -> SyncResult<EnrollmentData> {
        self.post_json(AgentEndpoint::Enrollments, request, None)
            .await
    }

    /// Who an invite is from. No auth and nothing about this computer.
    pub async fn invite_preview(&self, invite_token: &str) -> SyncResult<InvitePreview> {
        let mut url = reqwest::Url::parse(&format!(
            "{}{}/",
            self.base_url,
            AgentEndpoint::Invites.path()
        ))
        .map_err(|err| SyncError::Envelope(err.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| SyncError::Envelope("bad base URL".to_string()))?
            .pop_if_empty()
            .push(invite_token);
        let response = self.http.get(url).send().await?;
        self.parse_envelope(response).await
    }

    /// Connects this computer to the workspace the signed-in account chose,
    /// with the account's runtime token; the account is its owner.
    pub async fn desktop_enroll(
        &self,
        runtime_token: &str,
        machine: &MachineFields,
    ) -> SyncResult<EnrollmentData> {
        let response = self
            .http
            .post(format!(
                "{}{}",
                self.base_url,
                DesktopEndpoint::Enrollments.path()
            ))
            .bearer_auth(runtime_token)
            .json(machine)
            .send()
            .await?;
        self.parse_envelope(response).await
    }

    pub async fn issue_token(&self, device_id: &str, device_secret: &str) -> SyncResult<TokenData> {
        self.post_json(
            AgentEndpoint::Token,
            &TokenRequest {
                device_id: device_id.to_string(),
                device_secret: device_secret.to_string(),
            },
            None,
        )
        .await
    }

    pub async fn checkin(
        &self,
        runtime_token: &str,
        payload: &CheckinRequest,
    ) -> SyncResult<CheckinData> {
        self.post_json(AgentEndpoint::Checkins, payload, Some(runtime_token))
            .await
    }

    pub async fn ingest_events(
        &self,
        runtime_token: &str,
        payload: &EventBatchRequest,
    ) -> SyncResult<EventBatchData> {
        self.post_json(AgentEndpoint::Events, payload, Some(runtime_token))
            .await
    }

    /// Sends the full installed-software snapshot. 403 while the workspace
    /// has paused the agent.
    pub async fn upload_software(
        &self,
        runtime_token: &str,
        payload: &SoftwareUploadRequest,
    ) -> SyncResult<SoftwareUploadData> {
        self.post_json(AgentEndpoint::Software, payload, Some(runtime_token))
            .await
    }

    pub async fn upload_saas(
        &self,
        runtime_token: &str,
        payload: &SaasUploadRequest,
    ) -> SyncResult<SaasUploadData> {
        self.post_json(AgentEndpoint::Saas, payload, Some(runtime_token))
            .await
    }

    /// Revokes this device on the server and deletes its usage records there.
    /// The runtime token dies with the device, so a repeat answers 403 and
    /// `/agent/token` then answers [`DEVICE_REVOKED`].
    pub async fn revoke(&self, runtime_token: &str) -> SyncResult<RevokeData> {
        let url = format!("{}{}", self.base_url, AgentEndpoint::Revoke.path());
        let response = self
            .http
            .post(url)
            .bearer_auth(runtime_token)
            .send()
            .await?;
        self.parse_envelope(response).await
    }

    /// Who opened this person's app usage: the latest `limit` views. A GET
    /// with the runtime token and no body; it works whether or not usage
    /// sharing is on, so someone who stopped sharing still sees past views.
    pub async fn person_views(
        &self,
        runtime_token: &str,
        limit: u32,
    ) -> SyncResult<PersonViewsData> {
        let response = self
            .http
            .get(format!(
                "{}{}?limit={limit}",
                self.base_url,
                AgentEndpoint::PersonViews.path()
            ))
            .bearer_auth(runtime_token)
            .send()
            .await?;
        self.parse_envelope(response).await
    }

    /// Signs the app in: the browser's one-time code and this app's PKCE
    /// verifier become a device credential for sending reports.
    pub async fn desktop_sign_in(
        &self,
        request: &DesktopSignInRequest,
    ) -> SyncResult<DesktopSignInData> {
        self.post_desktop(DesktopEndpoint::Devices, request).await
    }

    pub async fn desktop_token(
        &self,
        request: &DesktopTokenRequest,
    ) -> SyncResult<DesktopTokenData> {
        self.post_desktop(DesktopEndpoint::Token, request).await
    }

    /// Signs the app out on the server: its credential stops working.
    pub async fn desktop_revoke(&self, runtime_token: &str) -> SyncResult<DesktopRevokeData> {
        let response = self
            .http
            .post(format!(
                "{}{}",
                self.base_url,
                DesktopEndpoint::Revoke.path()
            ))
            .bearer_auth(runtime_token)
            .send()
            .await?;
        self.parse_envelope(response).await
    }

    /// Sends one report, exactly these bytes, to Sanka as the signed-in
    /// account.
    pub async fn send_report(
        &self,
        runtime_token: &str,
        file_name: &str,
        content_type: &str,
        content: &[u8],
        fields: &ReportSendFields,
    ) -> SyncResult<ReportReceipt> {
        let boundary = format!("sanka-monitor-{}", uuid::Uuid::new_v4().simple());
        let body = multipart_body(&boundary, &fields.form(), file_name, content_type, content);
        let response = self
            .http
            .post(format!(
                "{}{}",
                self.base_url,
                DesktopEndpoint::Reports.path()
            ))
            .bearer_auth(runtime_token)
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await?;
        self.parse_envelope(response).await
    }

    async fn post_desktop<TReq: Serialize + ?Sized, TRes: DeserializeOwned>(
        &self,
        endpoint: DesktopEndpoint,
        body: &TReq,
    ) -> SyncResult<TRes> {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, endpoint.path()))
            .json(body)
            .send()
            .await?;
        self.parse_envelope(response).await
    }

    async fn post_json<TReq: Serialize + ?Sized, TRes: DeserializeOwned>(
        &self,
        endpoint: AgentEndpoint,
        body: &TReq,
        bearer: Option<&str>,
    ) -> SyncResult<TRes> {
        let url = format!("{}{}", self.base_url, endpoint.path());
        let mut request = self.http.post(url).json(body);
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        self.parse_envelope(response).await
    }

    async fn parse_envelope<TRes: DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> SyncResult<TRes> {
        let status = response.status().as_u16();
        let text = response.text().await?;
        decode(status, &text)
    }
}

/// Turns a response into its envelope's `data`, or the API's error with the
/// envelope's `error.code` and `error.message` when it sent them.
fn decode<TRes: DeserializeOwned>(status: u16, text: &str) -> SyncResult<TRes> {
    if !(200..300).contains(&status) {
        let body = serde_json::from_str::<serde_json::Value>(text).ok();
        let error = body
            .as_ref()
            .map(|value| value.get("error").unwrap_or(value));
        return Err(SyncError::Api {
            status,
            code: error.and_then(extract_error_code),
            message: error
                .and_then(extract_error_message)
                .or_else(|| body.as_ref().and_then(extract_error_message))
                .unwrap_or_else(|| truncate(text, 300)),
        });
    }
    let envelope: Envelope<TRes> = serde_json::from_str(text)
        .map_err(|err| SyncError::Envelope(format!("{err}: {}", truncate(text, 200))))?;
    if !envelope.success {
        let error = envelope.error.as_ref();
        return Err(SyncError::Api {
            status,
            code: error.and_then(extract_error_code),
            message: error
                .and_then(extract_error_message)
                .unwrap_or_else(|| "request was not successful".to_string()),
        });
    }
    envelope
        .data
        .ok_or_else(|| SyncError::Envelope("response envelope had no data".to_string()))
}

fn extract_error_code(value: &serde_json::Value) -> Option<String> {
    value
        .get("code")
        .and_then(|v| v.as_str())
        .filter(|code| !code.is_empty())
        .map(|code| code.to_string())
}

fn extract_error_message(value: &serde_json::Value) -> Option<String> {
    for key in ["message", "detail", "error"] {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }
    }
    value.as_str().map(|s| s.to_string())
}

fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max {
        trimmed.to_string()
    } else {
        let mut end = max;
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &trimmed[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use monitor_inventory::SoftwareSource;
    use monitor_posture::{PostureCheckId, PostureCheckResult};

    #[test]
    fn upload_contains_only_deduplicated_utc_days() {
        use chrono::TimeZone;
        let at = |day, hour, minute| {
            Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
                .unwrap()
        };
        let event = |start, end| monitor_core::UsageEvent {
            event_id: uuid::Uuid::new_v4(),
            app_id: "test.app".into(),
            app_name: "Test".into(),
            started_at: start,
            ended_at: end,
            ended_by_idle: true,
        };
        let intervals = vec![
            event(at(27, 9, 0), at(27, 9, 5)),
            event(at(27, 16, 0), at(27, 19, 0)),
            event(at(27, 23, 59), at(28, 0, 1)),
        ];
        let days = daily_presence(&intervals);
        assert_eq!(
            days.iter().map(|d| d.date.to_string()).collect::<Vec<_>>(),
            ["2026-10-27", "2026-10-28"]
        );
        let wire = serde_json::to_value(EventBatchRequest { events: days }).unwrap();
        assert_eq!(
            wire["events"][0],
            serde_json::json!({"appId": "test.app", "appName": "Test", "date": "2026-10-27"})
        );
        assert_eq!(
            daily_presence(&[event(at(27, 23, 59), at(28, 0, 0))]).len(),
            1
        );
        assert!(daily_presence(&[event(at(27, 9, 0), at(27, 9, 0))]).is_empty());
    }

    #[test]
    fn endpoints_live_under_the_monitor_agent_namespace() {
        assert_eq!(
            AgentEndpoint::Enrollments.path(),
            "/api/v2/monitor/agent/enrollments"
        );
        assert_eq!(AgentEndpoint::Token.path(), "/api/v2/monitor/agent/token");
        assert_eq!(
            AgentEndpoint::Checkins.path(),
            "/api/v2/monitor/agent/checkins"
        );
        assert_eq!(AgentEndpoint::Events.path(), "/api/v2/monitor/agent/events");
        assert_eq!(AgentEndpoint::Revoke.path(), "/api/v2/monitor/agent/revoke");
        assert_eq!(
            AgentEndpoint::Software.path(),
            "/api/v2/monitor/agent/software"
        );
        assert_eq!(
            AgentEndpoint::PersonViews.path(),
            "/api/v2/monitor/agent/person-views"
        );
        assert_eq!(AgentEndpoint::Saas.path(), "/api/v2/monitor/agent/saas");
    }

    /// The privacy invariant as a test: a SaaS upload carries exactly these
    /// fields, and each SaaS only its key and last day used.
    #[test]
    fn a_saas_upload_has_no_room_for_more_than_key_and_day() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let upload = SaasUploadRequest {
            sharing: true,
            collected_on: day,
            items: vec![SaasUseItem {
                key: "slack".into(),
                last_used_on: day,
            }],
        };
        assert_eq!(
            serde_json::to_value(&upload).unwrap(),
            serde_json::json!({
                "sharing": true,
                "collectedOn": "2026-10-26",
                "items": [{"key": "slack", "lastUsedOn": "2026-10-26"}],
            })
        );
        let allowlist: CheckinData = serde_json::from_value(serde_json::json!({
            "killSwitch": false,
            "minAgentVersion": "0.1.1",
            "checkinIntervalSeconds": 3600,
            "idleThresholdSeconds": 300,
            "nudges": [],
            "saasAllowlist": {"name": "Example K.K.", "allowed": ["slack"], "updatedAt": "2026-10-26T00:00:00Z"},
        }))
        .unwrap();
        assert_eq!(
            allowlist.saas_allowlist.map(|list| list.allowed),
            Some(vec!["slack".to_string()])
        );
    }

    #[test]
    fn requests_serialize_to_the_api_contract() {
        let at = Utc.with_ymd_and_hms(2026, 9, 1, 3, 0, 0).unwrap();
        let mac = CheckinRequest {
            agent_version: "0.1.0".into(),
            platform: "macos".into(),
            os_version: "14.6.1".into(),
            hostname: "adas-macbook-pro".into(),
            serial_number: Some("C02ZX1ABCD12".into()),
            device_model: None,
            os_edition: None,
            sharing_consent: "opted_out".into(),
            saas_sharing: None,
            arch: None,
            posture: vec![PostureCheckResult {
                id: PostureCheckId::Gatekeeper,
                pass: Some(true),
                detail: None,
                collected_at: at,
            }],
        };
        let windows = CheckinRequest {
            platform: "windows".into(),
            os_version: "Windows 11 24H2 (26100.4351)".into(),
            serial_number: None,
            os_edition: Some("Professional".into()),
            posture: vec![PostureCheckResult {
                id: PostureCheckId::LocalAdmin,
                pass: Some(false),
                detail: Some("The signed-in user is a local administrator".into()),
                collected_at: at,
            }],
            ..mac.clone()
        };
        let item = |source, identifier, name, version, publisher| {
            InstalledSoftware::new(source, identifier, name, version, publisher).unwrap()
        };
        let software = SoftwareUploadRequest {
            collected_at: at,
            items: vec![
                item(
                    SoftwareSource::MacosApp,
                    "com.microsoft.Word",
                    "Microsoft Word",
                    Some("16.90"),
                    None,
                ),
                item(
                    SoftwareSource::WindowsRegistry,
                    "{90160000-0011-0000-1000-0000000FF1CE}",
                    "Microsoft Office Professional Plus 2016",
                    None,
                    Some("Microsoft Corporation"),
                ),
                item(
                    SoftwareSource::WindowsStore,
                    "Microsoft.WindowsTerminal_8wekyb3d8bbwe",
                    "Microsoft.WindowsTerminal",
                    Some("1.21.2911.0"),
                    Some("Microsoft Corporation"),
                ),
            ],
        };
        // (request, wire body)
        for (request, wire) in [
            (
                serde_json::to_value(&mac).unwrap(),
                serde_json::json!({
                    "agentVersion": "0.1.0",
                    "platform": "macos",
                    "osVersion": "14.6.1",
                    "hostname": "adas-macbook-pro",
                    "serialNumber": "C02ZX1ABCD12",
                    "sharingConsent": "opted_out",
                    "posture": [{
                        "id": "gatekeeper",
                        "pass": true,
                        "collectedAt": "2026-09-01T03:00:00Z"
                    }]
                }),
            ),
            (
                serde_json::to_value(&windows).unwrap(),
                serde_json::json!({
                    "agentVersion": "0.1.0",
                    "platform": "windows",
                    "osVersion": "Windows 11 24H2 (26100.4351)",
                    "osEdition": "Professional",
                    "hostname": "adas-macbook-pro",
                    "sharingConsent": "opted_out",
                    "posture": [{
                        "id": "local_admin",
                        "pass": false,
                        "detail": "The signed-in user is a local administrator",
                        "collectedAt": "2026-09-01T03:00:00Z"
                    }]
                }),
            ),
            (
                serde_json::to_value(&software).unwrap(),
                serde_json::json!({
                    "collectedAt": "2026-09-01T03:00:00Z",
                    "items": [
                        {"source": "macos_app", "identifier": "com.microsoft.Word", "name": "Microsoft Word", "version": "16.90", "publisher": null},
                        {"source": "windows_registry", "identifier": "{90160000-0011-0000-1000-0000000FF1CE}", "name": "Microsoft Office Professional Plus 2016", "version": null, "publisher": "Microsoft Corporation"},
                        {"source": "windows_store", "identifier": "Microsoft.WindowsTerminal_8wekyb3d8bbwe", "name": "Microsoft.WindowsTerminal", "version": "1.21.2911.0", "publisher": "Microsoft Corporation"}
                    ]
                }),
            ),
        ] {
            assert_eq!(request, wire);
        }
    }

    #[test]
    fn api_errors_keep_the_envelope_code() {
        let envelope = |code: &str, message: &str| {
            serde_json::json!({
                "success": false,
                "data": null,
                "error": {"code": code, "message": message, "details": null},
            })
            .to_string()
        };
        // (status, body, code, message, device gone)
        for (status, body, code, message, gone) in [
            (
                401,
                envelope(DEVICE_REVOKED, "This device was revoked."),
                Some(DEVICE_REVOKED),
                "This device was revoked.",
                true,
            ),
            (
                401,
                envelope(DEVICE_INVALID, "The agent device credential is invalid."),
                Some(DEVICE_INVALID),
                "The agent device credential is invalid.",
                true,
            ),
            (
                200,
                envelope(DEVICE_REVOKED, "This device was revoked."),
                Some(DEVICE_REVOKED),
                "This device was revoked.",
                true,
            ),
            (
                403,
                envelope("MONITOR_AGENT_UPDATE_REQUIRED", "Update the agent."),
                Some("MONITOR_AGENT_UPDATE_REQUIRED"),
                "Update the agent.",
                false,
            ),
            (
                403,
                r#"{"detail": "Forbidden"}"#.to_string(),
                None,
                "Forbidden",
                false,
            ),
            (
                502,
                "<html>Bad gateway</html>".to_string(),
                None,
                "<html>Bad gateway</html>",
                false,
            ),
        ] {
            let err = decode::<TokenData>(status, &body).unwrap_err();
            assert_eq!(err.code(), code, "{body}");
            assert_eq!(err.device_gone(), gone, "{body}");
            match err {
                SyncError::Api {
                    status: got,
                    message: got_message,
                    ..
                } => assert_eq!((got, got_message.as_str()), (status, message)),
                other => panic!("not an api error: {other}"),
            }
        }
    }

    #[test]
    fn answers_decode() {
        let data: RevokeData = decode(
            200,
            r#"{"success": true, "data": {"revokedAt": "2026-10-16T01:00:00Z", "deletedUsageEvents": 42}, "error": null}"#,
        )
        .unwrap();
        assert_eq!(
            data.revoked_at,
            Utc.with_ymd_and_hms(2026, 10, 16, 1, 0, 0).unwrap()
        );
        assert_eq!(data.deleted_usage_events, 42);

        let software: SoftwareUploadData = decode(
            200,
            r#"{"success": true, "data": {"received": 312, "added": 3, "changed": 1, "removed": 2, "uploadIntervalSeconds": 86400}, "error": null}"#,
        )
        .unwrap();
        assert_eq!(
            (
                software.received,
                software.added,
                software.changed,
                software.removed,
                software.upload_interval_seconds
            ),
            (312, 3, 1, 2, 86400)
        );
    }

    #[test]
    fn person_views_decode() {
        let data: PersonViewsData = decode(
            200,
            r#"{"success": true, "data": {"views": [
                {"viewedAt": "2026-10-19T05:30:00Z", "from": "2026-10-13", "to": "2026-10-19", "viewerName": "Aiko Tanaka", "reason": "Laptop replacement: checking which apps you use"},
                {"viewedAt": "2026-10-02T09:15:42.123456+09:00", "from": "2026-10-01", "to": "2026-10-01", "viewerName": null, "reason": "Security review"},
                {"viewedAt": "2026-09-30T00:00:00+00:00", "from": "2026-09-01", "to": "2026-09-30", "reason": "Audit"}
            ], "total": 57}, "error": null}"#,
        )
        .unwrap();
        assert_eq!(data.total, 57);
        assert_eq!(data.views.len(), 3);
        let first = &data.views[0];
        assert_eq!(
            first.viewed_at,
            Utc.with_ymd_and_hms(2026, 10, 19, 5, 30, 0).unwrap()
        );
        assert_eq!(first.from, NaiveDate::from_ymd_opt(2026, 10, 13).unwrap());
        assert_eq!(first.to, NaiveDate::from_ymd_opt(2026, 10, 19).unwrap());
        assert_eq!(first.viewer_name.as_deref(), Some("Aiko Tanaka"));
        assert_eq!(
            first.reason,
            "Laptop replacement: checking which apps you use"
        );
        // An offset is converted to UTC; a null or missing viewer stays unnamed.
        assert_eq!(
            data.views[1].viewed_at.to_rfc3339(),
            "2026-10-02T00:15:42.123456+00:00"
        );
        assert_eq!(data.views[1].viewer_name, None);
        assert_eq!(data.views[2].viewer_name, None);

        // The window gets the same shape back (camelCase, dates as days).
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::json!({
                "viewedAt": "2026-10-19T05:30:00Z",
                "from": "2026-10-13",
                "to": "2026-10-19",
                "viewerName": "Aiko Tanaka",
                "reason": "Laptop replacement: checking which apps you use",
            })
        );

        let none: PersonViewsData = decode(
            200,
            r#"{"success": true, "data": {"views": [], "total": 0}, "error": null}"#,
        )
        .unwrap();
        assert!(none.views.is_empty());

        // A device the workspace revoked or no longer knows answers here too,
        // and wipes itself as after any other agent call.
        for code in [DEVICE_REVOKED, DEVICE_INVALID] {
            let body = serde_json::json!({
                "success": false,
                "data": null,
                "error": {"code": code, "message": "gone", "details": null},
            })
            .to_string();
            assert!(decode::<PersonViewsData>(401, &body)
                .unwrap_err()
                .device_gone());
        }
        // A day that is not a date is an unexpected response, not a guess.
        let bad = r#"{"success": true, "data": {"views": [{"viewedAt": "2026-10-19T05:30:00Z", "from": "last week", "to": "2026-10-19", "viewerName": null, "reason": "x"}], "total": 1}, "error": null}"#;
        assert!(matches!(
            decode::<PersonViewsData>(200, bad),
            Err(SyncError::Envelope(_))
        ));
    }

    /// The request on the wire: a GET for the latest views with the runtime
    /// token, and no body.
    #[tokio::test]
    async fn person_views_is_a_bearer_get_without_a_body() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"success":true,"data":{"views":[],"total":0},"error":null}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });

        let client = AgentClient::new(format!("{base}/"), "0.3.1").unwrap();
        let data = client
            .person_views("runtime-token", PERSON_VIEWS_LIMIT)
            .await
            .unwrap();
        assert_eq!(data.total, 0);

        let request = server.join().unwrap();
        let lines: Vec<&str> = request.split("\r\n").collect();
        assert_eq!(
            lines[0],
            "GET /api/v2/monitor/agent/person-views?limit=50 HTTP/1.1"
        );
        let headers: Vec<String> = lines[1..].iter().map(|l| l.to_ascii_lowercase()).collect();
        assert!(headers.contains(&"authorization: bearer runtime-token".to_string()));
        assert!(headers.contains(&"user-agent: sanka-monitor-agent/0.3.1".to_string()));
        assert!(!headers
            .iter()
            .any(|h| h.starts_with("content-length:") || h.starts_with("content-type:")));
        assert!(request.ends_with("\r\n\r\n"), "no body: {request}");
    }

    /// What a multipart parser sees: the fields and the file, unchanged,
    /// even when a value tries to start a part of its own.
    #[test]
    fn a_report_body_carries_the_file_byte_for_byte() {
        let content: Vec<u8> = (0..=255u8).collect();
        let fields = ReportSendFields {
            note: Some("hi\r\n--b\r\nContent-Disposition: form-data; name=\"x\"".into()),
            agent_version: Some("   ".into()),
            summary: Some(ReportSummary {
                high: 1,
                medium: 2,
                low: 3,
                programs: 40,
                programs_past_support: 4,
            }),
            ..ReportSendFields::default()
        };
        assert_eq!(
            fields
                .form()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            ["note", "summary"]
        );
        let body = multipart_body(
            "b",
            &fields.form(),
            "PC\"1\r\n.xlsx",
            "application/x",
            &content,
        );
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains(
            "name=\"summary\"\r\n\r\n{\"high\":1,\"medium\":2,\"low\":3,\"programs\":40,\"programsPastSupport\":4}\r\n"
        ));
        assert!(text.contains("filename=\"PC_1__.xlsx\""));
        // The note's line breaks lose their CR, so no field line equals a boundary.
        assert_eq!(text.matches("\r\n--b\r\n").count(), 2);
        let header = b"Content-Type: application/x\r\n\r\n";
        let start = body
            .windows(header.len())
            .position(|w| w == header)
            .unwrap()
            + header.len();
        let end = body.len() - b"\r\n--b--\r\n".len();
        assert_eq!(&body[start..end], content.as_slice());
    }

    /// Signing in sends the code, the verifier, the platform and the app
    /// version: nothing about the computer or its scan.
    #[test]
    fn signing_in_sends_nothing_about_the_computer() {
        let request = DesktopSignInRequest {
            code: "one-time-code".into(),
            code_verifier: "verifier".into(),
            platform: "macos".into(),
            app_version: "0.4.0".into(),
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({
                "code": "one-time-code",
                "codeVerifier": "verifier",
                "platform": "macos",
                "appVersion": "0.4.0",
            })
        );
        let signed_in: DesktopSignInData = decode(
            200,
            r#"{"success": true, "data": {"deviceId": "d-1", "deviceSecret": "s", "workspace": {"id": "w-1", "code": "12345678", "name": "Acme"}, "user": {"name": "Aiko Tanaka", "email": "aiko@acme.example"}}, "error": null}"#,
        )
        .unwrap();
        assert_eq!(signed_in.workspace.name.as_deref(), Some("Acme"));
        assert_eq!(signed_in.user.email.as_deref(), Some("aiko@acme.example"));
    }

    #[test]
    fn a_rejected_token_is_retried_and_a_revoked_app_signs_in_again() {
        let error = |status: u16, code: &str| {
            decode::<DesktopTokenData>(
                status,
                &serde_json::json!({
                    "success": false,
                    "data": null,
                    "error": {"code": code, "message": "x", "details": null},
                })
                .to_string(),
            )
            .unwrap_err()
        };
        // (error, sign in again, new token and retry)
        for (err, signed_out, retry) in [
            (error(401, DESKTOP_DEVICE_REVOKED), true, false),
            (error(401, DESKTOP_DEVICE_INVALID), true, false),
            (error(401, "MONITOR_DESKTOP_TOKEN_EXPIRED"), false, true),
            (error(401, "MONITOR_DESKTOP_TOKEN_INVALID"), false, true),
            (error(429, "MONITOR_REPORT_RATE_LIMITED"), false, false),
            (error(400, DESKTOP_INVALID_GRANT), false, false),
        ] {
            assert_eq!(err.signed_out(), signed_out, "{err}");
            assert_eq!(err.token_rejected(), retry, "{err}");
        }
    }

    /// The report on the wire: a multipart POST with the runtime token, the
    /// file as shown, and Sanka's receipt decoded back.
    #[tokio::test]
    async fn a_report_is_a_bearer_multipart_post_with_the_file_as_shown() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            let head_end = loop {
                let n = stream.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
                if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let head = String::from_utf8_lossy(&request[..head_end]).to_ascii_lowercase();
            let length: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            while request.len() < head_end + length {
                let n = stream.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"success":true,"data":{"id":"r-1","receivedAt":"2026-10-26T01:00:00Z","fileName":"report.xlsx","format":"xlsx","sizeBytes":3,"sha256":"abc"},"error":null}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });

        let client = AgentClient::new(base, "0.4.0").unwrap();
        let receipt = client
            .send_report(
                "runtime-token",
                "report.xlsx",
                "application/x",
                b"PK!",
                &ReportSendFields {
                    note: Some("Please call".into()),
                    evidence: Some(r#"{"schemaVersion":1}"#.into()),
                    purpose_version: Some("monitor-report-review-v1".into()),
                    ..ReportSendFields::default()
                },
            )
            .await
            .unwrap();
        assert_eq!((receipt.id.as_str(), receipt.size_bytes), ("r-1", 3));
        assert_eq!(receipt.evidence_sha256, None);

        let request = server.join().unwrap();
        let text = String::from_utf8_lossy(&request);
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert_eq!(lines[0], "POST /api/v2/monitor/desktop/reports HTTP/1.1");
        let lower = text.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer runtime-token\r\n"));
        assert!(lower.contains("content-type: multipart/form-data; boundary=sanka-monitor-"));
        assert!(text.contains("name=\"note\"\r\n\r\nPlease call\r\n"));
        assert!(text.contains("name=\"evidence\"\r\n\r\n{\"schemaVersion\":1}\r\n"));
        assert!(text.contains("name=\"purposeVersion\"\r\n\r\nmonitor-report-review-v1\r\n"));
        assert!(
            text.contains("filename=\"report.xlsx\"\r\nContent-Type: application/x\r\n\r\nPK!\r\n")
        );
    }
}
