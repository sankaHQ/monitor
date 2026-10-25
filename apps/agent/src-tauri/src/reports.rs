//! The local scan in the app: scan this device, show the risks and the SaaS
//! in use, and save the Excel ledger, the CSV or the one-page report where
//! the person chooses.
//! Works without enrollment and sends nothing: saving writes one file that
//! the person picked in the system's save dialog (which also spares macOS's
//! folder-access prompt).
//!
//! "Upload this report to Sanka" is the one exception, and only on the
//! person's word: [`prepare_upload`] reads one report file and shows its
//! name, size and SHA-256; [`send_upload`] sends exactly those bytes, with
//! whatever the person typed, and compares Sanka's receipt with the hash.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use monitor_findings::{LifecycleStatus, Severity};
use monitor_report::copy;
use monitor_report::{DeviceRecord, Lang, ReportCounts, ScanReport};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::agent::{AgentState, AGENT_VERSION};
use crate::cli::{self, Format};
use monitor_sync::{AgentClient, ReportReceipt, ReportUploadFields, REPORTS_PATH};
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
    crate::checks::record_scan(&app, &report, false).await;
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

/// A report read for sending: kept as read, so the bytes sent are the bytes
/// the person was shown.
pub struct PendingUpload {
    token: String,
    file_name: String,
    format: Format,
    content: Vec<u8>,
    sha256: String,
    report_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadPreview {
    pub token: String,
    pub file_name: String,
    pub format: &'static str,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub receipt: ReportReceipt,
    /// Sanka's hash and size match the file that was shown.
    pub matches: bool,
}

fn upload_format(path: &Path) -> Result<Format, String> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("xlsx") => Ok(Format::Xlsx),
        Some("csv") => Ok(Format::Csv),
        Some("html" | "htm") => Ok(Format::Html),
        _ => Err("choose an Excel (.xlsx), CSV or HTML report from Sanka Monitor".to_string()),
    }
}

fn content_type(format: Format) -> &'static str {
    match format {
        Format::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Format::Csv => "text/csv",
        Format::Html => "text/html",
    }
}

fn sha256_hex(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Reads the report to send: one this app saved (`path`), or one the person
/// picks (`None`, the open dialog). `None` back when they cancel.
#[tauri::command]
pub async fn prepare_upload(
    app: AppHandle,
    path: Option<PathBuf>,
) -> Result<Option<UploadPreview>, String> {
    let state: State<'_, AgentState> = app.state();
    let (path, report_id) = match path {
        Some(path) => {
            let path = saved_path(&app, &path).await?;
            let report_id = state
                .saved_reports
                .lock()
                .await
                .iter()
                .find(|saved| saved.path == path)
                .map(|saved| saved.report_id.clone());
            (path, report_id)
        }
        None => {
            let dialog = app
                .dialog()
                .file()
                .add_filter("Sanka Monitor report", &["xlsx", "csv", "html"]);
            let Some(chosen) =
                tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
                    .await
                    .map_err(|err| err.to_string())?
            else {
                return Ok(None);
            };
            let path = chosen
                .into_path()
                .map_err(|err| format!("cannot open that file: {err}"))?;
            (path, None)
        }
    };
    let format = upload_format(&path)?;
    let read_path = path.clone();
    let content = tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&read_path)?.len();
        if size > MAX_UPLOAD_BYTES {
            return Err(std::io::Error::other("the report is larger than 10 MB"));
        }
        std::fs::read(&read_path)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    if content.is_empty() {
        return Err("the report is empty".to_string());
    }
    let pending = PendingUpload {
        token: uuid::Uuid::new_v4().to_string(),
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("report.{}", format.extension())),
        format,
        sha256: sha256_hex(&content),
        content,
        report_id,
    };
    let preview = UploadPreview {
        token: pending.token.clone(),
        file_name: pending.file_name.clone(),
        format: format.extension(),
        size_bytes: pending.content.len() as u64,
        sha256: pending.sha256.clone(),
    };
    *state.pending_upload.lock().await = Some(pending);
    Ok(Some(preview))
}

/// Sends the prepared report to Sanka, exactly as shown, and records the
/// request (without the file's contents) in the Privacy pane.
#[tauri::command]
pub async fn send_upload(
    app: AppHandle,
    token: String,
    fields: ReportUploadFields,
) -> Result<UploadResult, String> {
    let state: State<'_, AgentState> = app.state();
    let pending = {
        let mut slot = state.pending_upload.lock().await;
        match slot.take() {
            Some(pending) if pending.token == token => pending,
            other => {
                *slot = other;
                return Err("choose the report again; this one is no longer prepared".to_string());
            }
        }
    };
    let fields = ReportUploadFields {
        report_id: pending.report_id.clone(),
        agent_version: Some(AGENT_VERSION.to_string()),
        ..fields
    };
    let api_base = state.config.lock().await.api_base();
    let client = AgentClient::new(api_base, AGENT_VERSION).map_err(|err| err.to_string())?;
    let mut shown = serde_json::json!({
        "file": {
            "name": pending.file_name,
            "sizeBytes": pending.content.len(),
            "sha256": pending.sha256,
        },
    });
    for (name, value) in fields.form() {
        shown[name] = serde_json::Value::String(value.to_string());
    }
    let request_id = state.record_payload(REPORTS_PATH, shown).await;
    let sent = client
        .upload_report(
            &pending.file_name,
            content_type(pending.format),
            &pending.content,
            &fields,
        )
        .await;
    let receipt = match sent {
        Ok(receipt) => receipt,
        Err(err) => {
            // Kept for "Try again": the same bytes, still as shown.
            *state.pending_upload.lock().await = Some(pending);
            return Err(format!("Sanka did not receive the report: {err}"));
        }
    };
    state.mark_payload_delivered(request_id).await;
    let matches = receipt.sha256.eq_ignore_ascii_case(&pending.sha256)
        && receipt.size_bytes == pending.content.len() as u64;
    Ok(UploadResult { receipt, matches })
}

#[tauri::command]
pub async fn cancel_upload(app: AppHandle) {
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

    #[test]
    fn only_report_files_can_be_sent() {
        for (name, format) in [
            ("PC_1.xlsx", Some(Format::Xlsx)),
            ("PC_1.XLSX", Some(Format::Xlsx)),
            ("fleet.csv", Some(Format::Csv)),
            ("report.htm", Some(Format::Html)),
            ("notes.docx", None),
            ("noextension", None),
        ] {
            assert_eq!(upload_format(Path::new(name)).ok(), format, "{name}");
        }
    }
}
