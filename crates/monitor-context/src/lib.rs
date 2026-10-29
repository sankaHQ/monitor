//! Local-only, opt-in normalized context. No input values, titles, URLs or network clients.
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};
use uuid::Uuid;

pub mod collector;
#[cfg(target_os = "macos")]
mod macos;
pub mod store;
#[cfg(target_os = "windows")]
mod windows;
pub use collector::{Collector, SessionGate};
pub use store::{AppendOutcome, ContextCandidate, ContextStore};

pub const MAX_PACKAGE_BYTES: usize = 1_048_576;
pub const MAX_EVENT_BYTES: usize = 4096;
pub const MAX_STORE_BYTES: usize = 250 * 1024 * 1024;
pub const CAPTURE_DEADLINE_MS: u64 = 250;
pub const MAX_PENDING_EVENTS: usize = 1000;

#[derive(Debug)]
pub enum ContextError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(&'static str),
    Encryption,
    Busy,
    Capacity,
}
impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Errors never format captured or user-authored content.
        f.write_str(match self {
            Self::Io(_) => "local context storage unavailable",
            Self::Json(_) => "invalid context encoding",
            Self::Invalid(reason) => reason,
            Self::Encryption => "context encryption or authentication failed",
            Self::Busy => "local context store already open",
            Self::Capacity => "local context capacity reached",
        })
    }
}
impl std::error::Error for ContextError {}
impl From<std::io::Error> for ContextError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for ContextError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, ContextError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    PermissionDenied,
    Excluded,
    PrivateOrUnknownBrowser,
    Locked,
    Capacity,
    Unsupported,
    Offline,
    InsufficientEvidence,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Navigate,
    Read,
    Search,
    Create,
    Edit,
    Export,
    Import,
    Approve,
    Send,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectCategory {
    CrmRecord,
    Spreadsheet,
    Invoice,
    Message,
    File,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Observed,
    Inferred,
    UserConfirmed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Personal,
    Company,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextSettings {
    pub enabled: bool,
    pub paused: bool,
    pub allowed_app_ids: Vec<String>,
    pub allowed_domains: Vec<String>,
}
impl ContextSettings {
    pub fn allows_app(&self, app_id: &str) -> bool {
        self.enabled && !self.paused && self.allowed_app_ids.iter().any(|id| id == app_id)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextEvent {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub app_id: String,
    pub app_name: String,
    pub action: Action,
    pub object_category: ObjectCategory,
    pub provenance: Provenance,
    pub gap: Option<Gap>,
}
impl ContextEvent {
    pub fn coverage_gap(at: DateTime<Utc>, gap: Gap) -> Self {
        Self {
            id: Uuid::new_v4(),
            at,
            app_id: String::new(),
            app_name: String::new(),
            action: Action::Unknown,
            object_category: ObjectCategory::Unknown,
            provenance: Provenance::Observed,
            gap: Some(gap),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.gap.is_some() {
            ensure(
                self.app_id.is_empty()
                    && self.app_name.is_empty()
                    && self.action == Action::Unknown
                    && self.object_category == ObjectCategory::Unknown,
                "gap must contain no app content",
            )?;
        } else {
            text(&self.app_id, 1, 200)?;
            text(&self.app_name, 1, 200)?;
        }
        ensure(
            serde_json::to_vec(self)?.len() <= MAX_EVENT_BYTES,
            "event too large",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextCoverage {
    pub observed_days: u8,
    pub gaps: Vec<Gap>,
}
impl ContextCoverage {
    fn validate(&self) -> Result<()> {
        ensure(self.observed_days <= 30, "invalid observed days")?;
        gaps(&self.gaps)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextApp {
    pub app_id: String,
    pub name: String,
    pub service_key: Option<String>,
    pub observed_days: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextStep {
    pub ordinal: u8,
    pub app_id: String,
    pub action: Action,
    pub object_category: ObjectCategory,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextEpisode {
    pub id: Uuid,
    pub day: NaiveDate,
    pub repeat_count: u16,
    pub extraction_version: u8,
    pub steps: Vec<ContextStep>,
    pub gaps: Vec<Gap>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextWorkflow {
    pub id: Uuid,
    pub name: String,
    pub purpose: String,
    pub app_ids: Vec<String>,
    pub steps: Vec<String>,
    pub exceptions: String,
    pub required_features: String,
    pub frequency_per_week: Option<f64>,
    pub minutes_per_run: Option<f64>,
    pub confirmed: bool,
    pub owns_custom_code: Option<bool>,
    pub episode_ids: Vec<Uuid>,
}
impl ContextWorkflow {
    pub fn validate(&self) -> Result<()> {
        text(&self.name, 1, 200)?;
        for s in [&self.purpose, &self.exceptions, &self.required_features] {
            text(s, 0, 1000)?;
        }
        ensure(
            self.app_ids.len() <= 30 && unique(self.app_ids.iter()),
            "invalid workflow apps",
        )?;
        for id in &self.app_ids {
            text(id, 1, 200)?;
        }
        ensure(
            !self.steps.is_empty() && self.steps.len() <= 30,
            "invalid workflow steps",
        )?;
        for s in &self.steps {
            text(s, 1, 500)?;
        }
        for (value, max) in [
            (self.frequency_per_week, 10000.0),
            (self.minutes_per_run, 10080.0),
        ] {
            ensure(
                value.is_none_or(|n| n.is_finite() && (0.0..=max).contains(&n)),
                "invalid workflow estimate",
            )?;
        }
        ensure(
            self.episode_ids.len() <= 50 && unique(self.episode_ids.iter()),
            "invalid workflow episodes",
        )
    }
}

// The existing v1 scan shape, closed here without changing the legacy report types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportEvidence {
    pub schema_version: u8,
    pub report_id: String,
    pub scanned_at: DateTime<Utc>,
    pub agent_version: String,
    pub catalog_verified_on: Option<NaiveDate>,
    pub platform: ScanPlatform,
    pub os_version: String,
    pub os_edition: Option<String>,
    pub checks: Vec<ReportCheck>,
    pub software: Vec<ReportSoftware>,
    pub saas: Vec<ReportSaas>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPlatform {
    Macos,
    Windows,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    DiskEncryption,
    Firewall,
    Gatekeeper,
    ScreenLock,
    OsUpdate,
    AutomaticUpdates,
    Antivirus,
    LocalAdmin,
    DeviceManagement,
    SystemIntegrity,
    SecureBoot,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportCheck {
    pub id: CheckId,
    pub pass: Option<bool>,
    #[serde(default)]
    pub accepted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareSource {
    MacosApp,
    WindowsRegistry,
    WindowsStore,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSoftware {
    pub source: SoftwareSource,
    pub identifier: String,
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSaas {
    pub key: String,
    pub allowed: Option<bool>,
}
impl ReportEvidence {
    fn validate(&self) -> Result<()> {
        ensure(self.schema_version == 1, "invalid scan version")?;
        text(&self.report_id, 1, 100)?;
        text(&self.agent_version, 1, 40)?;
        text(&self.os_version, 0, 200)?;
        if let Some(s) = &self.os_edition {
            text(s, 0, 100)?;
        }
        ensure(
            self.checks.len() <= 20 && unique(self.checks.iter().map(|c| c.id)),
            "invalid scan checks",
        )?;
        ensure(
            self.software.len() <= 5000
                && unique(self.software.iter().map(|s| (s.source, &s.identifier))),
            "invalid scan software",
        )?;
        ensure(
            self.saas.len() <= 500 && unique(self.saas.iter().map(|s| &s.key)),
            "invalid scan SaaS",
        )?;
        for c in &self.checks {
            ensure(
                !c.accepted || matches!(c.id, CheckId::LocalAdmin | CheckId::DeviceManagement),
                "invalid accepted check",
            )?;
        }
        for s in &self.software {
            text(&s.identifier, 1, 300)?;
            text(&s.name, 1, 300)?;
            if let Some(v) = &s.version {
                text(v, 0, 100)?;
            }
            if let Some(p) = &s.publisher {
                text(p, 0, 300)?;
            }
        }
        for s in &self.saas {
            text(&s.key, 1, 100)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextPackage {
    pub schema_version: u8,
    pub package_id: Uuid,
    pub workspace_id: Uuid,
    pub account_device_id: Uuid,
    pub audience: Audience,
    pub policy_version: u8,
    pub captured_from: DateTime<Utc>,
    pub captured_to: DateTime<Utc>,
    pub consented_at: DateTime<Utc>,
    pub coverage: ContextCoverage,
    pub apps: Vec<ContextApp>,
    pub episodes: Vec<ContextEpisode>,
    pub workflows: Vec<ContextWorkflow>,
    pub scan: Option<ReportEvidence>,
}
impl ContextPackage {
    /// The only wire parsing entry point: bounded bytes, closed fields and semantic checks.
    pub fn from_json(json: &str, now: DateTime<Utc>) -> Result<Self> {
        ensure(json.len() <= MAX_PACKAGE_BYTES, "package too large")?;
        let package: Self = serde_json::from_str(json)?;
        package.validate(now)?;
        Ok(package)
    }
    pub fn to_json(&self, now: DateTime<Utc>) -> Result<String> {
        self.validate(now)?;
        let json = serde_json::to_string(self)?;
        ensure(json.len() <= MAX_PACKAGE_BYTES, "package too large")?;
        Ok(json)
    }
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        ensure(
            self.schema_version == 1 && self.policy_version == 1,
            "unsupported context version",
        )?;
        ensure(
            self.captured_from <= self.captured_to
                && self.captured_to <= self.consented_at
                && self.consented_at <= now + Duration::minutes(5)
                && self.consented_at >= now - Duration::hours(24),
            "invalid context time window",
        )?;
        self.coverage.validate()?;
        ensure(
            self.apps.len() <= 500 && unique(self.apps.iter().map(|a| &a.app_id)),
            "invalid context apps",
        )?;
        for app in &self.apps {
            text(&app.app_id, 1, 200)?;
            text(&app.name, 1, 200)?;
            ensure(app.observed_days <= 30, "invalid app observed days")?;
            if let Some(key) = &app.service_key {
                text(key, 1, 100)?;
            }
        }
        let app_ids: HashSet<_> = self.apps.iter().map(|a| &a.app_id).collect();
        ensure(
            self.episodes.len() <= 50 && unique(self.episodes.iter().map(|e| e.id)),
            "invalid context episodes",
        )?;
        for episode in &self.episodes {
            ensure(
                episode.day >= self.captured_from.date_naive()
                    && episode.day <= self.captured_to.date_naive(),
                "episode outside capture window",
            )?;
            ensure(
                episode.extraction_version == 1 && (1..=10000).contains(&episode.repeat_count),
                "invalid episode version or repetition",
            )?;
            ensure(
                !episode.steps.is_empty() && episode.steps.len() <= 30,
                "invalid episode steps",
            )?;
            for (i, step) in episode.steps.iter().enumerate() {
                ensure(
                    step.ordinal as usize == i && app_ids.contains(&step.app_id),
                    "invalid step ordinal or app",
                )?;
            }
            gaps(&episode.gaps)?;
        }
        ensure(
            self.workflows.len() <= 50 && unique(self.workflows.iter().map(|w| w.id)),
            "invalid context workflows",
        )?;
        let episode_ids: HashSet<_> = self.episodes.iter().map(|e| e.id).collect();
        for workflow in &self.workflows {
            workflow.validate()?;
            ensure(
                workflow.app_ids.iter().all(|id| app_ids.contains(id))
                    && workflow
                        .episode_ids
                        .iter()
                        .all(|id| episode_ids.contains(id)),
                "invalid workflow reference",
            )?;
        }
        if let Some(scan) = &self.scan {
            scan.validate()?;
        }
        Ok(())
    }
}
fn ensure(ok: bool, reason: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(ContextError::Invalid(reason))
    }
}
fn text(s: &str, min: usize, max: usize) -> Result<()> {
    ensure(
        s.chars().count() <= max
            && s.trim().chars().count() >= min
            && !s.chars().any(|c| c.is_control()),
        "invalid context text",
    )
}
fn unique<T: Eq + std::hash::Hash>(values: impl IntoIterator<Item = T>) -> bool {
    let mut seen = HashSet::new();
    values.into_iter().all(|v| seen.insert(v))
}
fn gaps(values: &[Gap]) -> Result<()> {
    ensure(
        values.len() <= 8 && unique(values.iter()),
        "invalid coverage gaps",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn wire_boundary_rejects_unreviewed_fields_and_unresolved_evidence() {
        let now = Utc.with_ymd_and_hms(2026, 10, 29, 10, 0, 0).unwrap();
        let package = ContextPackage {
            schema_version: 1,
            package_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            account_device_id: Uuid::new_v4(),
            audience: Audience::Personal,
            policy_version: 1,
            captured_from: now,
            captured_to: now,
            consented_at: now,
            coverage: ContextCoverage {
                observed_days: 1,
                gaps: vec![],
            },
            apps: vec![ContextApp {
                app_id: "safe".into(),
                name: "Synthetic app".into(),
                service_key: None,
                observed_days: 1,
            }],
            episodes: vec![ContextEpisode {
                id: Uuid::new_v4(),
                day: now.date_naive(),
                repeat_count: 1,
                extraction_version: 1,
                steps: vec![ContextStep {
                    ordinal: 0,
                    app_id: "safe".into(),
                    action: Action::Unknown,
                    object_category: ObjectCategory::Unknown,
                    provenance: Provenance::Observed,
                }],
                gaps: vec![],
            }],
            workflows: vec![],
            scan: None,
        };
        let json = package.to_json(now).unwrap();
        assert_eq!(ContextPackage::from_json(&json, now).unwrap(), package);
        let original: serde_json::Value = serde_json::from_str(&json).unwrap();
        for pointer in [
            "",
            "/coverage",
            "/apps/0",
            "/episodes/0",
            "/episodes/0/steps/0",
        ] {
            let mut extra = original.clone();
            extra
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("rawContext".into(), serde_json::json!("unreviewed content"));
            assert!(ContextPackage::from_json(&extra.to_string(), now).is_err());
        }
        let mut broken = package.clone();
        broken.episodes[0].steps[0].app_id = "unselected".into();
        assert!(broken.to_json(now).is_err());
        broken = package.clone();
        broken.episodes[0].steps[0].ordinal = 1;
        assert!(broken.to_json(now).is_err());
        broken = package.clone();
        broken.apps.push(broken.apps[0].clone());
        assert!(broken.to_json(now).is_err());
        assert!(package.to_json(now + Duration::hours(25)).is_err());
    }
}
