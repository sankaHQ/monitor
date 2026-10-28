//! Acknowledged risks. Two checks can fail for reasons the person may choose
//! to live with, and they can say so in the app:
//!
//! - **Standard account.** An administrator account for daily work: the check
//!   needs attention (yellow) for a week, then fails (red). Acknowledging it
//!   passes for a week; then it needs attention again for a week, and fails if
//!   it isn't acknowledged again. A standard account passes.
//! - **Device management.** MDM is optional: not enrolled fails until the
//!   person acknowledges it, then it needs attention. Enrolled passes.
//!
//! The acknowledgement only changes what this app shows. The scan, the saved
//! reports and the workspace's check-in keep the check's own result.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use monitor_posture::PostureCheckId;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::agent::{save_config, AgentState, STATUS_EVENT};

/// How long an acknowledgement lasts, and the grace before an unacknowledged
/// administrator account fails.
pub const ACK_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Pass,
    Attention,
    Fail,
    Unknown,
}

/// A check's acknowledgement as the window shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AckView {
    /// The check failed and the person can acknowledge it now.
    pub can_acknowledge: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// Passing because it was acknowledged, until then.
    pub valid_until: Option<DateTime<Utc>>,
    /// Needs attention now; fails at this time unless acknowledged.
    pub due_at: Option<DateTime<Utc>>,
}

/// What the app knows about acknowledgements when it shows a scan.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub acknowledged: BTreeMap<String, DateTime<Utc>>,
    pub failing_since: BTreeMap<String, DateTime<Utc>>,
    pub now: DateTime<Utc>,
}

pub fn acknowledgeable(id: PostureCheckId) -> bool {
    matches!(
        id,
        PostureCheckId::LocalAdmin | PostureCheckId::DeviceManagement
    )
}

/// The check id as config and the window spell it (`local_admin`).
pub fn key(id: PostureCheckId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn parse_key(id: &str) -> Option<PostureCheckId> {
    serde_json::from_value(serde_json::Value::String(id.to_string())).ok()
}

/// A check's state after its acknowledgement.
pub fn evaluate(id: PostureCheckId, pass: Option<bool>, policy: &Policy) -> (CheckState, AckView) {
    let plain = match pass {
        Some(true) => CheckState::Pass,
        Some(false) => CheckState::Fail,
        None => CheckState::Unknown,
    };
    if pass != Some(false) || !acknowledgeable(id) {
        return (plain, AckView::default());
    }
    let acknowledged = policy.acknowledged.get(&key(id)).copied();
    let period = Duration::days(ACK_DAYS);
    match id {
        PostureCheckId::DeviceManagement => match acknowledged {
            Some(at) => (
                CheckState::Attention,
                AckView {
                    acknowledged_at: Some(at),
                    ..AckView::default()
                },
            ),
            None => (
                CheckState::Fail,
                AckView {
                    can_acknowledge: true,
                    ..AckView::default()
                },
            ),
        },
        _ => {
            if let Some(at) = acknowledged.filter(|at| policy.now < *at + period) {
                return (
                    CheckState::Pass,
                    AckView {
                        acknowledged_at: Some(at),
                        valid_until: Some(at + period),
                        ..AckView::default()
                    },
                );
            }
            // The week of attention starts when the last acknowledgement ran
            // out, or when the check first failed.
            let since = acknowledged
                .map(|at| at + period)
                .or_else(|| policy.failing_since.get(&key(id)).copied())
                .unwrap_or(policy.now);
            let due = since + period;
            let state = if policy.now < due {
                CheckState::Attention
            } else {
                CheckState::Fail
            };
            (
                state,
                AckView {
                    can_acknowledge: true,
                    acknowledged_at: acknowledged,
                    due_at: (state == CheckState::Attention).then_some(due),
                    ..AckView::default()
                },
            )
        }
    }
}

/// Checks that fail but pass because the person acknowledged them: their
/// risks leave the window's Risks and the menu bar's count.
pub fn accepted(report: &monitor_report::ScanReport, policy: &Policy) -> Vec<PostureCheckId> {
    report
        .checks
        .iter()
        .filter(|check| {
            check.pass == Some(false)
                && evaluate(check.id, check.pass, policy).0 == CheckState::Pass
        })
        .map(|check| check.id)
        .collect()
}

/// The risks still to review after acknowledgements, as the window counts them.
pub fn open_risks(report: &monitor_report::ScanReport, policy: &Policy) -> usize {
    let accepted = accepted(report, policy);
    report
        .findings
        .iter()
        .filter(|finding| finding.check.is_none_or(|id| !accepted.contains(&id)))
        .count()
}

/// After a scan: starts or clears each acknowledgeable check's failing-since
/// time, and drops an acknowledgement once its check passes.
pub async fn note_scan(app: &AppHandle, report: &monitor_report::ScanReport) {
    let state: State<'_, AgentState> = app.state();
    let mut config = state.config.lock().await;
    let before = (
        config.failing_since.clone(),
        config.acknowledged_checks.clone(),
    );
    let now = Utc::now();
    for check in report.checks.iter().filter(|c| acknowledgeable(c.id)) {
        let id = key(check.id);
        match check.pass {
            Some(false) => {
                config.failing_since.entry(id).or_insert(now);
            }
            Some(true) => {
                config.failing_since.remove(&id);
                config.acknowledged_checks.remove(&id);
            }
            None => {}
        }
    }
    if (
        config.failing_since.clone(),
        config.acknowledged_checks.clone(),
    ) != before
    {
        let _ = save_config(app, &config);
    }
}

pub async fn policy(app: &AppHandle) -> Policy {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await;
    Policy {
        acknowledged: config.acknowledged_checks.clone(),
        failing_since: config.failing_since.clone(),
        now: Utc::now(),
    }
}

/// Acknowledge (the check page): accepts the check's risk from now on.
#[tauri::command]
pub async fn acknowledge_check(app: AppHandle, id: String) -> Result<(), String> {
    let check = parse_key(&id)
        .filter(|check| acknowledgeable(*check))
        .ok_or_else(|| format!("{id} can't be acknowledged"))?;
    {
        let state: State<'_, AgentState> = app.state();
        let mut config = state.config.lock().await;
        config.acknowledged_checks.insert(key(check), Utc::now());
        save_config(&app, &config)?;
    }
    let _ = tauri::Emitter::emit(&app, STATUS_EVENT, ());
    crate::tray::refresh(&app).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_790_000_000, 0).unwrap() + Duration::days(day)
    }

    fn policy(now: i64, acked: Option<i64>, since: Option<i64>, id: PostureCheckId) -> Policy {
        let mut p = Policy {
            now: at(now),
            ..Policy::default()
        };
        if let Some(day) = acked {
            p.acknowledged.insert(key(id), at(day));
        }
        if let Some(day) = since {
            p.failing_since.insert(key(id), at(day));
        }
        p
    }

    #[test]
    fn admin_account_week_by_week() {
        let id = PostureCheckId::LocalAdmin;
        let state = |p: &Policy| evaluate(id, Some(false), p).0;
        // First week without acknowledging: attention, then failing.
        assert_eq!(state(&policy(3, None, Some(0), id)), CheckState::Attention);
        assert_eq!(state(&policy(7, None, Some(0), id)), CheckState::Fail);
        // Acknowledged: passes for a week, then a week of attention, then fails.
        assert_eq!(state(&policy(10, Some(8), Some(0), id)), CheckState::Pass);
        assert_eq!(
            state(&policy(16, Some(8), Some(0), id)),
            CheckState::Attention
        );
        assert_eq!(state(&policy(22, Some(8), Some(0), id)), CheckState::Fail);
        // A standard account passes whatever was acknowledged.
        assert_eq!(
            evaluate(id, Some(true), &policy(30, Some(8), None, id)).0,
            CheckState::Pass
        );
    }

    #[test]
    fn device_management_is_optional_once_acknowledged() {
        let id = PostureCheckId::DeviceManagement;
        assert_eq!(
            evaluate(id, Some(false), &policy(0, None, None, id)).0,
            CheckState::Fail
        );
        assert_eq!(
            evaluate(id, Some(false), &policy(90, Some(0), None, id)).0,
            CheckState::Attention
        );
        assert_eq!(
            evaluate(id, Some(true), &policy(0, None, None, id)).0,
            CheckState::Pass
        );
    }
}
