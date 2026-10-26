//! Regular checks: when the person turns them on, the app scans this
//! computer again daily or weekly — the same scan as the Scan button,
//! including the SaaS check — and nudges in the window and the tray when a
//! risk appears that the last scan did not have (a SaaS the company has not
//! allowed, a check that started failing, software whose support ended).
//!
//! Every scan is compared with the last one, whatever ran it: the Scan
//! button, the scan when the app opens (also at login, with the window
//! hidden) or a regular check. So a risk that appeared while the app was
//! closed is nudged at the first scan after it starts, not absorbed into
//! the baseline. While regular checks are off, scans only move the baseline.
//!
//! The app keeps, in `checks.json` in its data folder, only the keys of the
//! last scan's findings (`saas_not_allowed:chatgpt`) and the nudges not yet
//! dismissed. Nothing is sent. Turning regular checks on also starts the app
//! at login, so the checks run; the loop sleeps in between.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use monitor_findings::{Area, Finding, Severity};
use monitor_report::{copy, Lang, ScanReport};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;

use crate::agent::{running_installed, save_config, AgentState};
use crate::saas::{self, SaasSettingsView};

/// How often the loop looks whether a check is due (a computer that slept
/// through one catches up within this long of waking).
const SCHEDULE_POLL: Duration = Duration::from_secs(15 * 60);
/// Nudges kept until the person dismisses them; the oldest go first.
const MAX_NUDGES: usize = 20;
const STATE_FILE: &str = "checks.json";
const TRAY_ID: &str = "main-tray";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSchedule {
    Daily,
    Weekly,
}

impl CheckSchedule {
    pub fn interval(self) -> chrono::Duration {
        match self {
            CheckSchedule::Daily => chrono::Duration::days(1),
            CheckSchedule::Weekly => chrono::Duration::days(7),
        }
    }

    /// A check is due when none ran yet, the interval has passed, or the
    /// clock went back.
    pub fn due(self, last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        last.is_none_or(|last| now < last || now - last >= self.interval())
    }
}

/// A risk a scan found that the scan before it did not have, worded in both
/// languages when it was found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalNudge {
    pub id: String,
    pub at: DateTime<Utc>,
    pub severity: Severity,
    pub saas: bool,
    pub title_en: String,
    pub title_ja: String,
    pub fix_en: String,
    pub fix_ja: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ChecksState {
    /// The keys of the last scan's findings; `None` before the first scan.
    known: Option<BTreeSet<String>>,
    nudges: Vec<LocalNudge>,
}

/// Compares `report` with the baseline and makes it the new baseline. With
/// `nudge` (regular checks are on), each finding the baseline lacks becomes a
/// nudge; the first scan only sets the baseline.
fn remember(checks: &mut ChecksState, report: &ScanReport, nudge: bool) {
    if let (true, Some(known)) = (nudge, &checks.known) {
        let fresh = new_nudges(known, report, report.scanned_at);
        checks.nudges.extend(fresh);
        let excess = checks.nudges.len().saturating_sub(MAX_NUDGES);
        checks.nudges.drain(..excess);
    }
    checks.known = Some(finding_keys(report));
}

fn finding_key(finding: &Finding) -> String {
    format!("{}:{}", finding.finding_type.as_str(), finding.subject_key)
}

pub fn finding_keys(report: &ScanReport) -> BTreeSet<String> {
    report.findings.iter().map(finding_key).collect()
}

/// The findings in `report` that `known` does not have, as nudges.
pub fn new_nudges(
    known: &BTreeSet<String>,
    report: &ScanReport,
    at: DateTime<Utc>,
) -> Vec<LocalNudge> {
    let platform = report.device.platform.as_str();
    report
        .findings
        .iter()
        .filter(|finding| !known.contains(&finding_key(finding)))
        .map(|finding| LocalNudge {
            id: format!("{}@{}", finding_key(finding), at.timestamp()),
            at,
            severity: finding.severity,
            saas: finding.area == Area::Saas,
            title_en: copy::headline(finding, Lang::En),
            title_ja: copy::headline(finding, Lang::Ja),
            fix_en: copy::fix(finding, platform, Lang::En),
            fix_ja: copy::fix(finding, platform, Lang::Ja),
        })
        .collect()
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(STATE_FILE))
}

fn load(app: &AppHandle) -> ChecksState {
    state_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, checks: &ChecksState) -> Result<(), String> {
    let path = state_path(app).ok_or("app data folder unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_vec_pretty(checks).map_err(|err| err.to_string())?;
    crate::cli::write_whole(&path, &text).map_err(|err| err.to_string())
}

/// The tray says how many nudges wait: in its tooltip, and next to the icon
/// in the macOS menu bar.
fn show_on_tray(app: &AppHandle, waiting: usize) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tooltip = match waiting {
        0 => "Sanka Monitor".to_string(),
        1 => "Sanka Monitor: 1 new risk".to_string(),
        n => format!("Sanka Monitor: {n} new risks"),
    };
    let _ = tray.set_tooltip(Some(tooltip));
    #[cfg(target_os = "macos")]
    {
        let _ = tray.set_title((waiting > 0).then(|| waiting.to_string()));
    }
}

/// Scans this computer now, as the Scan button and the regular checks do:
/// the device scan, the browser check if the person turned it on, and the
/// allowlist in force. Hold `scan_lock` while calling it.
pub async fn scan_now(app: &AppHandle) -> Result<ScanReport, String> {
    let browser_check = app
        .state::<AgentState>()
        .config
        .lock()
        .await
        .browser_saas_check;
    let allowlist = saas::current_allowlist(app).await;
    tauri::async_runtime::spawn_blocking(move || {
        let now = Utc::now();
        let mut report = crate::scan::run_scan(now);
        let browser = browser_check.then(|| saas::browser_scan(now));
        saas::apply_saas(&mut report, browser.as_ref(), &allowlist);
        report
    })
    .await
    .map_err(|err| format!("the scan stopped: {err}"))
}

/// Compares any scan with the last one and remembers it; while regular
/// checks are on, what is new becomes a nudge (see the module docs).
pub async fn record_scan(app: &AppHandle, report: &ScanReport) {
    let state: State<'_, AgentState> = app.state();
    let nudge = state.config.lock().await.check_schedule.is_some();
    let _one = state.checks_lock.lock().await;
    let mut checks = load(app);
    remember(&mut checks, report, nudge);
    let _ = save(app, &checks);
    show_on_tray(app, checks.nudges.len());
}

/// After the browser check is turned off: the known findings become the
/// scan's as it is now, and SaaS nudges go.
pub async fn forget(app: &AppHandle) {
    let state: State<'_, AgentState> = app.state();
    let known = state.last_scan.lock().await.as_ref().map(finding_keys);
    let _one = state.checks_lock.lock().await;
    let mut checks = load(app);
    match (known, checks.known.as_mut()) {
        (Some(known), _) => checks.known = Some(known),
        (None, Some(saved)) => saved.retain(|key| {
            !(key.starts_with("saas_not_allowed:") || key.starts_with("ai_not_allowed:"))
        }),
        (None, None) => {}
    }
    checks.nudges.retain(|nudge| !nudge.saas);
    let _ = save(app, &checks);
    show_on_tray(app, checks.nudges.len());
}

pub async fn nudges(app: &AppHandle) -> Vec<LocalNudge> {
    let state: State<'_, AgentState> = app.state();
    let _one = state.checks_lock.lock().await;
    let mut nudges = load(app).nudges;
    nudges.reverse();
    nudges
}

/// One regular check: scan, compare, remember.
async fn run_regular_check(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let _one_scan = state.scan_lock.lock().await;
    let report = scan_now(app).await?;
    record_scan(app, &report).await;
    let at = report.scanned_at;
    *state.last_scan.lock().await = Some(report);
    crate::agent::spawn_saas_sync(app);
    let mut config = state.config.lock().await;
    let mut next = config.clone();
    next.last_scheduled_check_at = Some(at);
    save_config(app, &next)?;
    *config = next;
    Ok(())
}

/// Background loop, independent of enrollment and the network: runs a
/// regular check when one is due.
pub fn spawn_schedule_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        {
            let state: State<'_, AgentState> = app.state();
            let _one = state.checks_lock.lock().await;
            show_on_tray(&app, load(&app).nudges.len());
        }
        loop {
            let state: State<'_, AgentState> = app.state();
            let due = {
                let config = state.config.lock().await;
                config.check_schedule.is_some_and(|schedule| {
                    schedule.due(config.last_scheduled_check_at, Utc::now())
                })
            };
            if due {
                let _ = run_regular_check(&app).await;
            }
            tokio::select! {
                _ = tokio::time::sleep(SCHEDULE_POLL) => {}
                _ = state.schedule_wake.notified() => {}
            }
        }
    });
}

/// Turns regular checks on (daily or weekly) or off. On also starts the app
/// at login from an installed copy, so the checks run.
#[tauri::command]
pub async fn set_check_schedule(
    app: AppHandle,
    schedule: Option<CheckSchedule>,
    lang: Lang,
) -> Result<SaasSettingsView, String> {
    let state: State<'_, AgentState> = app.state();
    {
        let mut config = state.config.lock().await;
        let mut next = config.clone();
        next.check_schedule = schedule;
        save_config(&app, &next)?;
        *config = next;
    }
    if schedule.is_some() && running_installed() {
        let _ = app.autolaunch().enable();
    }
    state.schedule_wake.notify_one();
    Ok(saas::settings_view(&app, lang).await)
}

#[tauri::command]
pub async fn dismiss_nudges(app: AppHandle, lang: Lang) -> Result<SaasSettingsView, String> {
    {
        let state: State<'_, AgentState> = app.state();
        let _one = state.checks_lock.lock().await;
        let mut checks = load(&app);
        checks.nudges.clear();
        save(&app, &checks)?;
        show_on_tray(&app, 0);
    }
    Ok(saas::settings_view(&app, lang).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use monitor_saas::SaasCatalog;

    #[test]
    fn a_check_is_due_after_its_interval_or_when_the_clock_went_back() {
        let at = |day: u32, hour: u32| Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap();
        for (schedule, last, now, due) in [
            (CheckSchedule::Daily, None, at(26, 9), true),
            (CheckSchedule::Daily, Some(at(25, 9)), at(26, 8), false),
            (CheckSchedule::Daily, Some(at(25, 9)), at(26, 9), true),
            (CheckSchedule::Weekly, Some(at(20, 9)), at(26, 9), false),
            (CheckSchedule::Weekly, Some(at(19, 9)), at(26, 9), true),
            (CheckSchedule::Daily, Some(at(26, 9)), at(25, 9), true),
        ] {
            assert_eq!(schedule.due(last, now), due, "{schedule:?} {last:?} {now}");
        }
    }

    #[test]
    fn only_findings_the_last_scan_did_not_have_become_nudges() {
        let report = sample_report();
        let all = finding_keys(&report);
        assert!(new_nudges(&all, &report, report.scanned_at).is_empty());
        let mut known = all.clone();
        let saas_key = all
            .iter()
            .find(|key| key.starts_with("ai_not_allowed:"))
            .cloned()
            .unwrap();
        known.remove(&saas_key);
        let nudges = new_nudges(&known, &report, report.scanned_at);
        assert_eq!(nudges.len(), 1);
        assert!(nudges[0].saas);
        assert!(nudges[0].title_en.contains("ChatGPT"));
        assert!(nudges[0].title_ja.contains("ChatGPT"));
    }

    /// What `checks.json` holds across a restart.
    fn restart(checks: &ChecksState) -> ChecksState {
        serde_json::from_slice(&serde_json::to_vec(checks).unwrap()).unwrap()
    }

    #[test]
    fn a_risk_that_appeared_while_the_app_was_closed_is_nudged_by_the_next_scan() {
        let risky = sample_report();
        let mut clean = risky.clone();
        clean.set_allowlist(SaasCatalog::bundled(), None);
        let mut checks = ChecksState::default();

        // The first scan only sets the baseline.
        remember(&mut checks, &clean, true);
        assert!(checks.nudges.is_empty());

        // The app quits; ChatGPT appears; the scan when it opens again (at
        // login, window hidden) nudges about it rather than absorbing it.
        let mut checks = restart(&checks);
        remember(&mut checks, &risky, true);
        assert_eq!(checks.nudges.len(), 1);
        assert!(checks.nudges[0].title_en.contains("ChatGPT"));

        // The regular check a minute later finds nothing new.
        let mut checks = restart(&checks);
        remember(&mut checks, &risky, true);
        assert_eq!(checks.nudges.len(), 1);

        // With regular checks off, scans only move the baseline.
        let mut quiet = ChecksState::default();
        remember(&mut quiet, &clean, false);
        remember(&mut quiet, &risky, false);
        assert!(quiet.nudges.is_empty());
        assert_eq!(quiet.known, Some(finding_keys(&risky)));
    }

    /// A Mac with ChatGPT installed and an allowlist that does not allow it.
    fn sample_report() -> ScanReport {
        use monitor_inventory::{InstalledSoftware, SoftwareSource};
        let at = Utc.with_ymd_and_hms(2026, 10, 26, 0, 0, 0).unwrap();
        let mut report = ScanReport::build(
            monitor_report::DeviceRecord {
                hostname: "MAC-01".into(),
                platform: "macos".into(),
                os_version: "26.0".into(),
                os_edition: None,
                device_model: None,
                serial_number: None,
            },
            Vec::new(),
            vec![InstalledSoftware::new(
                SoftwareSource::MacosApp,
                "com.openai.chat",
                "ChatGPT",
                Some("1.2025.300"),
                None,
            )
            .unwrap()],
            monitor_findings::Catalog::bundled(),
            at.date_naive(),
            at,
            "0.4.0",
        );
        let list = monitor_saas::Allowlist::parse(
            r#"{"schemaVersion": 1, "allowed": ["slack"]}"#,
            SaasCatalog::bundled(),
            monitor_saas::AllowlistOrigin::App,
        )
        .unwrap();
        report.set_allowlist(SaasCatalog::bundled(), Some(&list));
        report
    }
}
