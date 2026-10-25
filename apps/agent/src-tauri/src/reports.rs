//! The local scan in the app: scan this device, show the risks and the SaaS
//! in use, and save the Excel ledger, the CSV or the one-page report where
//! the person chooses.
//! Works without enrollment and sends nothing: saving writes one file that
//! the person picked in the system's save dialog (which also spares macOS's
//! folder-access prompt).
//!
//! "Send to Sanka" is the one exception, and only on the person's word:
//! [`prepare_send`] builds the Excel report from the latest scan (or
//! [`prepare_edited_excel`] reads a ledger the person filled in) and shows
//! exactly what will be sent: the device, the counts, the file name, size and
//! SHA-256. [`send_report`] sends those bytes as the Sanka account the person
//! signed in with (`account.rs`) and compares Sanka's receipt with the hash.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use monitor_findings::{LifecycleStatus, Severity};
use monitor_report::copy;
use monitor_report::{DeviceRecord, Lang, ReportCounts, ScanReport};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::account;
use crate::agent::{AgentState, AGENT_VERSION};
use crate::cli::{self, Format};
use monitor_sync::{AgentClient, DesktopEndpoint, ReportReceipt, ReportSendFields, ReportSummary};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingView {
    pub severity: Severity,
    pub severity_label: &'static str,
    pub area_label: &'static str,
    pub title: String,
    pub evidence: String,
    pub fix: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckView {
    pub label: &'static str,
    pub pass: Option<bool>,
    pub result_label: &'static str,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareView {
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub status: Option<LifecycleStatus>,
    pub status_label: &'static str,
    /// Past or near the end of support, or outdated.
    pub flagged: bool,
    pub support_ends_on: Option<String>,
    pub successor: Option<String>,
}

/// A SaaS as the app shows it. No web addresses: the record has none.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasView {
    pub key: String,
    pub product: String,
    pub vendor: String,
    pub category_label: &'static str,
    pub ai: bool,
    pub found_from: &'static str,
    pub sources: String,
    pub allowed: Option<bool>,
    pub allowed_label: &'static str,
    /// Sanka can migrate it: a migration page, and whether a connector exists.
    pub migration_label: &'static str,
    pub migratable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasCheckView {
    /// The browser check ran with this scan.
    pub browser_check: bool,
    pub browsers: Vec<monitor_saas::history::BrowserResult>,
    pub browser_summary: String,
    pub allowlist_summary: String,
    pub catalog_verified_on: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsSupportView {
    pub status: LifecycleStatus,
    pub label: &'static str,
    pub support_ends_on: String,
}

/// A scan as the app shows it, worded in the report language.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanView {
    pub lang: Lang,
    pub report_id: String,
    pub scanned_at: DateTime<Utc>,
    pub catalog_verified_on: Option<NaiveDate>,
    pub device: DeviceRecord,
    pub os_name: String,
    pub os_support: Option<OsSupportView>,
    pub counts: ReportCounts,
    pub findings: Vec<FindingView>,
    pub checks: Vec<CheckView>,
    pub software: Vec<SoftwareView>,
    pub saas: Vec<SaasView>,
    pub saas_check: SaasCheckView,
}

pub fn scan_view(report: &ScanReport, lang: Lang) -> ScanView {
    let platform = report.device.platform.as_str();
    ScanView {
        lang,
        report_id: report.report_id.clone(),
        scanned_at: report.scanned_at,
        catalog_verified_on: report.catalog_verified_on,
        device: report.device.clone(),
        os_name: copy::os_name(&report.device),
        os_support: report.os_lifecycle.as_ref().map(|os| OsSupportView {
            status: os.status,
            label: copy::lifecycle_status(os.status, lang),
            support_ends_on: os.support_ends_on.clone(),
        }),
        counts: report.counts(),
        findings: report
            .findings
            .iter()
            .map(|finding| FindingView {
                severity: finding.severity,
                severity_label: copy::severity(finding.severity, lang),
                area_label: copy::area(finding.area, lang),
                title: copy::headline(finding, lang),
                evidence: copy::evidence(finding, lang),
                fix: copy::fix(finding, platform, lang),
            })
            .collect(),
        checks: report
            .checks
            .iter()
            .map(|check| CheckView {
                label: copy::check_label(check.id, platform, lang),
                pass: check.pass,
                result_label: copy::check_result(check.pass, lang),
                detail: check
                    .detail
                    .as_deref()
                    .map(|detail| copy::check_detail(detail, lang)),
            })
            .collect(),
        software: report
            .software
            .iter()
            .map(|record| {
                let lifecycle = record.lifecycle.as_ref();
                let flagged = lifecycle.is_some_and(|l| l.status != LifecycleStatus::Supported);
                SoftwareView {
                    name: record.item.name.clone(),
                    version: record.item.version.clone(),
                    publisher: record.item.publisher.clone(),
                    status: lifecycle.map(|l| l.status),
                    status_label: copy::software_status(lifecycle, lang),
                    flagged,
                    support_ends_on: lifecycle.map(|l| l.support_ends_on.clone()),
                    successor: lifecycle
                        .filter(|_| flagged)
                        .map(|l| copy::successor(l, lang).to_string())
                        .filter(|s| !s.is_empty()),
                }
            })
            .collect(),
        saas: report
            .saas
            .iter()
            .map(|record| SaasView {
                key: record.key.clone(),
                product: record.product.clone(),
                vendor: record.vendor.clone(),
                category_label: copy::saas_category(record.category, lang),
                ai: record.ai,
                found_from: copy::saas_found_from(record, lang),
                sources: copy::saas_sources(&record.desktop_apps, record.browser.as_ref(), lang),
                allowed: record.allowed,
                allowed_label: copy::saas_allowed(record.allowed, lang),
                migration_label: copy::saas_migration(record, lang),
                migratable: record.migrate.is_some(),
            })
            .collect(),
        saas_check: SaasCheckView {
            browser_check: report.saas_check.browsers.is_some(),
            browsers: report.saas_check.browsers.clone().unwrap_or_default(),
            browser_summary: copy::browser_check(report.saas_check.browsers.as_deref(), lang),
            allowlist_summary: copy::allowlist_line(&report.saas_check, lang),
            catalog_verified_on: report.saas_check.catalog_verified_on,
        },
    }
}

/// A report file the person saved from this app in this session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedReport {
    pub path: PathBuf,
    pub file_name: String,
    pub format: &'static str,
    pub size_bytes: u64,
    pub saved_at: DateTime<Utc>,
    pub report_id: String,
}

/// Scans this device now (a few seconds; longer with the browser check on)
/// and shows it in `lang`.
#[tauri::command]
pub async fn scan_device(app: AppHandle, lang: Lang) -> Result<ScanView, String> {
    let state: State<'_, AgentState> = app.state();
    let _one_scan = state.scan_lock.lock().await;
    let report = crate::checks::scan_now(&app).await?;
    crate::checks::record_scan(&app, &report).await;
    let view = scan_view(&report, lang);
    *state.last_scan.lock().await = Some(report);
    Ok(view)
}

/// The latest scan of this session in `lang`, if there is one.
#[tauri::command]
pub async fn get_scan(app: AppHandle, lang: Lang) -> Option<ScanView> {
    let state: State<'_, AgentState> = app.state();
    let report = state.last_scan.lock().await;
    report.as_ref().map(|report| scan_view(report, lang))
}

fn parse_format(format: &str) -> Result<Format, String> {
    match format {
        "xlsx" => Ok(Format::Xlsx),
        "csv" => Ok(Format::Csv),
        "html" => Ok(Format::Html),
        other => Err(format!("unknown report format {other:?}")),
    }
}

fn filter_label(format: Format, lang: Lang) -> &'static str {
    match (format, lang) {
        (Format::Xlsx, Lang::En) => "Excel workbook",
        (Format::Xlsx, Lang::Ja) => "Excel ブック",
        (Format::Csv, _) => "CSV",
        (Format::Html, Lang::En) => "Web page (print to PDF)",
        (Format::Html, Lang::Ja) => "Web ページ（PDF に印刷できます）",
    }
}

/// Saves the latest scan as `format` where the person chooses. `None` when
/// they cancel the dialog.
#[tauri::command]
pub async fn save_report(
    app: AppHandle,
    format: String,
    lang: Lang,
) -> Result<Option<SavedReport>, String> {
    let format = parse_format(&format)?;
    let state: State<'_, AgentState> = app.state();
    let report = state
        .last_scan
        .lock()
        .await
        .clone()
        .ok_or("scan this computer first")?;
    let bytes = cli::encode(&report, format, lang)?;
    let suggested = format!("{}.{}", report.file_stem(), format.extension());
    let mut dialog = app
        .dialog()
        .file()
        .set_file_name(&suggested)
        .add_filter(filter_label(format, lang), &[format.extension()]);
    if let Ok(documents) = app.path().document_dir() {
        dialog = dialog.set_directory(documents);
    }
    let Some(chosen) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|err| format!("cannot save there: {err}"))?;
    let path = with_extension(path, format.extension());
    let write_path = path.clone();
    let size_bytes = bytes.len() as u64;
    tauri::async_runtime::spawn_blocking(move || cli::write_whole(&write_path, &bytes))
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| format!("cannot save {}: {err}", path.display()))?;
    let saved = SavedReport {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path,
        format: format.extension(),
        size_bytes,
        saved_at: Utc::now(),
        report_id: report.report_id.clone(),
    };
    let mut saved_reports = state.saved_reports.lock().await;
    saved_reports.retain(|existing| existing.path != saved.path);
    saved_reports.insert(0, saved.clone());
    Ok(Some(saved))
}

/// A name typed without the extension still gets it.
fn with_extension(path: PathBuf, extension: &str) -> PathBuf {
    let has = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
    if has {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".");
        name.push(extension);
        PathBuf::from(name)
    }
}

#[tauri::command]
pub async fn get_saved_reports(app: AppHandle) -> Vec<SavedReport> {
    app.state::<AgentState>().saved_reports.lock().await.clone()
}

/// Only files this app saved can be shown or opened from the window.
async fn saved_path(app: &AppHandle, path: &Path) -> Result<PathBuf, String> {
    app.state::<AgentState>()
        .saved_reports
        .lock()
        .await
        .iter()
        .find(|saved| saved.path == path)
        .map(|saved| saved.path.clone())
        .ok_or_else(|| "this report is no longer in the list".to_string())
}

#[tauri::command]
pub async fn reveal_report(app: AppHandle, path: PathBuf) -> Result<(), String> {
    let path = saved_path(&app, &path).await?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn open_report(app: AppHandle, path: PathBuf) -> Result<(), String> {
    let path = saved_path(&app, &path).await?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|err| err.to_string())
}

/// The largest report Sanka accepts.
const MAX_UPLOAD_BYTES: u64 = 10 * 1024 * 1024;
const XLSX_CONTENT_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// A report about to be sent: built or read once and kept as it is, so the
/// bytes sent are the bytes the person was shown.
pub struct PendingUpload {
    token: String,
    file_name: String,
    content: Vec<u8>,
    sha256: String,
    report_id: Option<String>,
    summary: Option<ReportSummary>,
}

/// The scan a report was built from, as the preview shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendScan {
    pub hostname: String,
    pub device_model: Option<String>,
    pub os_name: String,
    pub scanned_at: DateTime<Utc>,
    pub summary: ReportSummary,
    /// The workbook's SaaS sheet, as the preview names it: shown, never sent
    /// as separate fields.
    pub saas: usize,
    pub saas_not_allowed: usize,
    /// The sheet then includes each SaaS's last day used and days used.
    pub saas_browser_check: bool,
}

/// Exactly what Send will send.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPreview {
    pub token: String,
    pub file_name: String,
    pub format: &'static str,
    pub size_bytes: u64,
    pub sha256: String,
    /// `None` for an edited workbook: its contents are the person's.
    pub scan: Option<SendScan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub receipt: ReportReceipt,
    /// Sanka's hash and size match the file that was shown.
    pub matches: bool,
}

fn sha256_hex(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The counts the preview shows and that go with the report, as shown.
pub fn report_summary(report: &ScanReport) -> ReportSummary {
    let counts = report.counts();
    ReportSummary {
        high: counts.high,
        medium: counts.medium,
        low: counts.low,
        programs: counts.programs,
        programs_past_support: counts.programs_past_support,
    }
}

/// The name of a report the app sends: a local time (the scan's, or when an
/// edited ledger was picked), and no computer name or serial number (those
/// stay inside the file).
pub fn sent_file_name(at: DateTime<Utc>) -> String {
    format!(
        "sanka-monitor-report_{}.xlsx",
        at.with_timezone(&chrono::Local).format("%Y%m%d-%H%M")
    )
}

async fn keep_pending(
    state: &State<'_, AgentState>,
    pending: PendingUpload,
    scan: Option<SendScan>,
) -> SendPreview {
    let preview = SendPreview {
        token: pending.token.clone(),
        file_name: pending.file_name.clone(),
        format: Format::Xlsx.extension(),
        size_bytes: pending.content.len() as u64,
        sha256: pending.sha256.clone(),
        scan,
    };
    *state.pending_upload.lock().await = Some(pending);
    preview
}

/// Builds the Excel report (IT資産台帳) from the latest scan in `lang` and
/// shows exactly what Send will send.
#[tauri::command]
pub async fn prepare_send(app: AppHandle, lang: Lang) -> Result<SendPreview, String> {
    let state: State<'_, AgentState> = app.state();
    let report = state
        .last_scan
        .lock()
        .await
        .clone()
        .ok_or("scan this computer first")?;
    let content = cli::encode(&report, Format::Xlsx, lang)?;
    let summary = report_summary(&report);
    let pending = PendingUpload {
        token: uuid::Uuid::new_v4().to_string(),
        file_name: sent_file_name(report.scanned_at),
        sha256: sha256_hex(&content),
        content,
        report_id: Some(report.report_id.clone()),
        summary: Some(summary),
    };
    let counts = report.counts();
    let scan = SendScan {
        hostname: report.device.hostname.clone(),
        device_model: report.device.device_model.clone(),
        os_name: copy::os_name(&report.device),
        scanned_at: report.scanned_at,
        summary,
        saas: counts.saas,
        saas_not_allowed: counts.saas_not_allowed,
        saas_browser_check: report.saas_check.browsers.is_some(),
    };
    Ok(keep_pending(&state, pending, Some(scan)).await)
}

/// An Excel ledger the person filled in (users, locations), picked in the
/// open dialog: only a Sanka Monitor workbook. `None` when they cancel.
#[tauri::command]
pub async fn prepare_edited_excel(app: AppHandle) -> Result<Option<SendPreview>, String> {
    let state: State<'_, AgentState> = app.state();
    let dialog = app
        .dialog()
        .file()
        .add_filter("Excel (Sanka Monitor)", &[Format::Xlsx.extension()]);
    let Some(chosen) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|err| format!("cannot open that file: {err}"))?;
    let read_path = path.clone();
    let content = tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&read_path)?.len();
        if size > MAX_UPLOAD_BYTES {
            return Err(std::io::Error::other("the workbook is larger than 10 MB"));
        }
        std::fs::read(&read_path)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    if !monitor_report::xlsx::is_monitor_workbook(&content) {
        return Err(
            "choose an Excel workbook saved from Sanka Monitor (the IT asset ledger)".to_string(),
        );
    }
    let pending = PendingUpload {
        token: uuid::Uuid::new_v4().to_string(),
        // Sent under the app's own name: a saved ledger is named after its computer.
        file_name: sent_file_name(Utc::now()),
        sha256: sha256_hex(&content),
        content,
        report_id: None,
        summary: None,
    };
    Ok(Some(keep_pending(&state, pending, None).await))
}

/// Sends the prepared report to Sanka as the signed-in account, exactly as
/// shown, and lists the request (without the file's contents) in the
/// Privacy pane.
#[tauri::command]
pub async fn send_report(
    app: AppHandle,
    token: String,
    note: Option<String>,
) -> Result<UploadResult, String> {
    let state: State<'_, AgentState> = app.state();
    let pending = {
        let mut slot = state.pending_upload.lock().await;
        match slot.take() {
            Some(pending) if pending.token == token => pending,
            other => {
                *slot = other;
                return Err("prepare the report again; this one is no longer ready".to_string());
            }
        }
    };
    let fields = ReportSendFields {
        note: note
            .map(|note| note.trim().to_string())
            .filter(|note| !note.is_empty()),
        report_id: pending.report_id.clone(),
        agent_version: Some(AGENT_VERSION.to_string()),
        summary: pending.summary,
    };
    let sent = send_as_account(&app, &pending, &fields).await;
    let receipt = match sent {
        Ok(receipt) => receipt,
        Err(err) => {
            // Kept for "Try again": the same bytes, still as shown, unless a newer
            // report was prepared while this one was on its way.
            state.pending_upload.lock().await.get_or_insert(pending);
            if err.signed_out() {
                account::forget_account(&app).await?;
                return Err("You are signed out of Sanka. Sign in again to send.".to_string());
            }
            return Err(format!("Sanka did not receive the report: {err}"));
        }
    };
    let matches = receipt.sha256.eq_ignore_ascii_case(&pending.sha256)
        && receipt.size_bytes == pending.content.len() as u64;
    Ok(UploadResult { receipt, matches })
}

/// One send with the account's token; a token Sanka no longer accepts is
/// replaced once.
async fn send_as_account(
    app: &AppHandle,
    pending: &PendingUpload,
    fields: &ReportSendFields,
) -> Result<ReportReceipt, account::TokenError> {
    let state: State<'_, AgentState> = app.state();
    let mut token = account::access_token(app).await?;
    let api_base = state.config.lock().await.api_base();
    let client = AgentClient::new(api_base, AGENT_VERSION)?;
    let mut shown = serde_json::json!({
        "file": {
            "name": pending.file_name,
            "sizeBytes": pending.content.len(),
            "sha256": pending.sha256,
        },
    });
    for (name, value) in fields.form() {
        shown[name] = match name {
            "summary" => serde_json::to_value(fields.summary).unwrap_or_default(),
            _ => serde_json::Value::String(value),
        };
    }
    let request_id = state
        .record_payload(&DesktopEndpoint::Reports.path(), shown)
        .await;
    let mut retried = false;
    loop {
        let sent = client
            .send_report(
                &token,
                &pending.file_name,
                XLSX_CONTENT_TYPE,
                &pending.content,
                fields,
            )
            .await;
        match sent {
            Err(err) if err.token_rejected() && !retried => {
                account::forget_token(app).await;
                token = account::access_token(app).await?;
                retried = true;
            }
            Ok(receipt) => {
                state.mark_payload_delivered(request_id).await;
                return Ok(receipt);
            }
            Err(err) => return Err(err.into()),
        }
    }
}

#[tauri::command]
pub async fn cancel_send(app: AppHandle) {
    *app.state::<AgentState>().pending_upload.lock().await = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_name_gets_the_formats_extension() {
        for (typed, extension, saved) in [
            ("/r/PC_1.xlsx", "xlsx", "/r/PC_1.xlsx"),
            ("/r/PC_1.XLSX", "xlsx", "/r/PC_1.XLSX"),
            ("/r/PC_1", "xlsx", "/r/PC_1.xlsx"),
            ("/r/PC_1.csv", "html", "/r/PC_1.csv.html"),
        ] {
            assert_eq!(
                with_extension(PathBuf::from(typed), extension),
                PathBuf::from(saved)
            );
        }
    }

    /// Sanka keeps exactly this name, so the receipt names the file the
    /// preview showed; a saved ledger's name (its computer and serial) it
    /// would replace.
    #[test]
    fn a_sent_report_is_named_as_sanka_keeps_it() {
        let name = sent_file_name(Utc::now());
        let stamp = name
            .strip_prefix("sanka-monitor-report_")
            .and_then(|rest| rest.strip_suffix(".xlsx"))
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!(stamp.len(), 13, "{name}");
        assert!(
            stamp.chars().enumerate().all(|(at, c)| if at == 8 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }),
            "{name}"
        );
    }
}
