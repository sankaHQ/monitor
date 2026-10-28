//! Shared types for the Sanka Monitor desktop agent.
//!
//! Privacy invariant (structural, not policy): local usage records carry *which
//! app* had focus and *when*. Only aggregated UTC dates leave the device. [`UsageEvent`] has no fields for
//! window titles, URLs, document names, input contents, or captures, and none
//! may be added. The wire-shape test below pins this.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API namespace the agent talks to in sanka-api.
pub const AGENT_API_BASE_PATH: &str = "/api/v2/monitor/agent";

/// API namespace of the app signed in with a Sanka account to send reports.
pub const DESKTOP_API_BASE_PATH: &str = "/api/v2/monitor/desktop";

/// Deep-link scheme registered by the desktop app (`sanka-monitor://`).
pub const DEEP_LINK_SCHEME: &str = "sanka-monitor";

/// A device is enrolled into exactly one workspace. Both ids are pinned
/// together at enrollment and echoed on every call (workspace-safety rule:
/// never depend on a mutable "current workspace").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub workspace_id: String,
}

/// Employee consent for sharing usage records with the workspace.
///
/// Defaults to [`SharingConsent::OptedOut`]: records never leave the machine
/// until the employee explicitly opts in. Posture booleans are consented by
/// enrollment itself and are not gated by this flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SharingConsent {
    #[default]
    OptedOut,
    OptedIn,
}

/// One local foreground-focus interval. Never use this type in an upload payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvent {
    /// Local queue id; never uploaded.
    pub event_id: Uuid,
    /// macOS bundle id (e.g. `com.tinyspeck.slackmacgap`) or Windows
    /// executable name. Classification to app slugs happens server-side.
    pub app_id: String,
    /// Localized display name as reported by the OS.
    pub app_name: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    /// True when the interval was closed because the user went idle.
    pub ended_by_idle: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> UsageEvent {
        UsageEvent {
            event_id: Uuid::nil(),
            app_id: "com.example.app".into(),
            app_name: "Example".into(),
            started_at: Utc.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(),
            ended_at: Utc.with_ymd_and_hms(2026, 9, 1, 9, 5, 0).unwrap(),
            ended_by_idle: false,
        }
    }

    /// Even local intervals carry no title, URL or content.
    #[test]
    fn local_usage_record_has_no_content() {
        let value = serde_json::to_value(sample()).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        // serde_json's default map is sorted by key.
        assert_eq!(
            keys,
            vec![
                "appId",
                "appName",
                "endedAt",
                "endedByIdle",
                "eventId",
                "startedAt"
            ]
        );
    }

    #[test]
    fn sharing_consent_defaults_to_opted_out() {
        assert_eq!(SharingConsent::default(), SharingConsent::OptedOut);
    }

    #[test]
    fn sharing_consent_wire_values_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&SharingConsent::OptedIn).unwrap(),
            "\"opted_in\""
        );
        assert_eq!(
            serde_json::to_string(&SharingConsent::OptedOut).unwrap(),
            "\"opted_out\""
        );
    }
}
