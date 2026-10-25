//! The IT資産台帳 (IT asset ledger) workbook.
//!
//! Sheets, in the order an IT team works through them:
//! 1. Summary: what was scanned, the risk counts, and how the file was made.
//! 2. Devices: one row per device, as an asset ledger: identity, OS and its
//!    support, each security check, and blank columns for the asset tag,
//!    user, department, location and notes that only IT knows.
//! 3. Software: each installed program and version per device, with its
//!    support status, last day of support and the vendor's next step.
//! 4. SaaS: each SaaS per device, its category and AI flag, where it was
//!    found (desktop app, browser), when it was last used in the browser,
//!    whether the company allows it, and whether Sanka can migrate it.
//! 5. Risks: the risk list, most severe first, with evidence and the fix,
//!    plus a status (list) and an owner for IT to work through.
//! 6. Security checks: every check per device, as the device reported it.
//!
//! Every cell is written as a value, never a formula, so text from the
//! device cannot run in Excel. Several reports (a fleet) write into the same
//! sheets.

use monitor_findings::LifecycleStatus;
use monitor_posture::PostureCheckId;
use rust_xlsxwriter::{
    Color, DataValidation, DocProperties, Format, FormatAlign, FormatBorder, Workbook, Worksheet,
    XlsxError,
};

use crate::copy::{self, Lang};
use crate::ScanReport;

/// The checks as Devices columns, in the order IT reads them.
const CHECK_COLUMNS: [PostureCheckId; 11] = [
    PostureCheckId::DiskEncryption,
    PostureCheckId::Firewall,
    PostureCheckId::Antivirus,
    PostureCheckId::ScreenLock,
    PostureCheckId::OsUpdate,
    PostureCheckId::AutomaticUpdates,
    PostureCheckId::LocalAdmin,
    PostureCheckId::DeviceManagement,
    PostureCheckId::Gatekeeper,
    PostureCheckId::SecureBoot,
    PostureCheckId::SystemIntegrity,
];

struct Formats {
    title: Format,
    header: Format,
    text: Format,
    wrap: Format,
    label: Format,
    high: Format,
    medium: Format,
    low: Format,
    fail: Format,
    pass: Format,
}

impl Formats {
    fn new() -> Formats {
        let font = |format: Format| format.set_font_name("Meiryo UI").set_font_size(10);
        Formats {
            title: font(Format::new()).set_bold().set_font_size(16),
            header: font(Format::new())
                .set_bold()
                .set_background_color(Color::RGB(0x1F2A44))
                .set_font_color(Color::White)
                .set_border(FormatBorder::Thin)
                .set_text_wrap()
                .set_align(FormatAlign::VerticalCenter),
            text: font(Format::new())
                .set_border(FormatBorder::Thin)
                .set_align(FormatAlign::Top),
            wrap: font(Format::new())
                .set_border(FormatBorder::Thin)
                .set_text_wrap()
                .set_align(FormatAlign::Top),
            label: font(Format::new()).set_bold(),
            high: font(Format::new())
                .set_bold()
                .set_border(FormatBorder::Thin)
                .set_font_color(Color::RGB(0xB42318))
                .set_align(FormatAlign::Top),
            medium: font(Format::new())
                .set_bold()
                .set_border(FormatBorder::Thin)
                .set_font_color(Color::RGB(0xB54708))
                .set_align(FormatAlign::Top),
            low: font(Format::new())
                .set_border(FormatBorder::Thin)
                .set_font_color(Color::RGB(0x475467))
                .set_align(FormatAlign::Top),
            fail: font(Format::new())
                .set_border(FormatBorder::Thin)
                .set_font_color(Color::RGB(0xB42318))
                .set_align(FormatAlign::Top),
            pass: font(Format::new())
                .set_border(FormatBorder::Thin)
                .set_font_color(Color::RGB(0x067647))
                .set_align(FormatAlign::Top),
        }
    }

    fn severity(&self, severity: monitor_findings::Severity) -> &Format {
        match severity {
            monitor_findings::Severity::High => &self.high,
            monitor_findings::Severity::Medium => &self.medium,
            monitor_findings::Severity::Low => &self.low,
        }
    }

    fn check(&self, pass: Option<bool>) -> &Format {
        match pass {
            Some(true) => &self.pass,
            Some(false) => &self.fail,
            None => &self.text,
        }
    }
}

/// A header row, frozen, with a filter over the data below it.
fn table_header(
    sheet: &mut Worksheet,
    formats: &Formats,
    headers: &[(&str, f64)],
    rows: u32,
) -> Result<(), XlsxError> {
    for (col, (header, width)) in headers.iter().enumerate() {
        let col = col as u16;
        sheet.write_string_with_format(0, col, *header, &formats.header)?;
        sheet.set_column_width(col, *width)?;
    }
    sheet.set_row_height(0, 30)?;
    sheet.set_freeze_panes(1, 0)?;
    sheet.autofilter(0, 0, rows.max(1), headers.len() as u16 - 1)?;
    Ok(())
}

/// Writes `reports` as one workbook in `lang`.
pub fn write(reports: &[ScanReport], lang: Lang) -> Result<Vec<u8>, XlsxError> {
    let l = copy::labels(lang);
    let f = Formats::new();
    let mut workbook = Workbook::new();
    workbook.set_properties(
        &DocProperties::new()
            .set_title(l.report_title)
            .set_author("Sanka Monitor")
            .set_comment(l.made_locally),
    );

    // 1. Summary
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name(l.summary)?;
        sheet.set_column_width(0, 34)?;
        sheet.set_column_width(1, 70)?;
        sheet.write_string_with_format(0, 0, l.report_title, &f.title)?;
        let mut row = 2;
        let mut line = |label: &str, value: String| -> Result<(), XlsxError> {
            sheet.write_string_with_format(row, 0, label, &f.label)?;
            sheet.write_string(row, 1, value)?;
            row += 1;
            Ok(())
        };
        let (high, medium, low, programs, past) =
            reports
                .iter()
                .map(ScanReport::counts)
                .fold((0, 0, 0, 0, 0), |acc, c| {
                    (
                        acc.0 + c.high,
                        acc.1 + c.medium,
                        acc.2 + c.low,
                        acc.3 + c.programs,
                        acc.4 + c.programs_past_support,
                    )
                });
        match reports {
            [one] => {
                line(l.hostname, one.device.hostname.clone())?;
                line(l.os, copy::os_name(&one.device))?;
                line(l.scanned_at, local_time(one))?;
            }
            many => line(l.devices, many.len().to_string())?,
        }
        // Distinct SaaS across the devices, and those not allowed on any.
        let saas: std::collections::BTreeSet<&str> = reports
            .iter()
            .flat_map(|r| r.saas.iter().map(|s| s.key.as_str()))
            .collect();
        let not_allowed: std::collections::BTreeSet<&str> = reports
            .iter()
            .flat_map(|r| r.saas.iter())
            .filter(|s| s.allowed == Some(false))
            .map(|s| s.key.as_str())
            .collect();
        line(
            l.open_findings,
            format!(
                "{} {high} · {} {medium} · {} {low}",
                l.high, l.medium, l.low
            ),
        )?;
        line(l.programs, programs.to_string())?;
        line(l.programs_past_support, past.to_string())?;
        line(l.saas_found, saas.len().to_string())?;
        line(l.saas_not_allowed, not_allowed.len().to_string())?;
        if let [one] = reports {
            line(
                l.browser_check,
                copy::browser_check(one.saas_check.browsers.as_deref(), lang),
            )?;
            line(l.allowlist, copy::allowlist_line(&one.saas_check, lang))?;
        }
        if let Some(first) = reports.first() {
            line(l.agent_version, first.agent_version.clone())?;
            line(
                l.catalog_verified,
                first
                    .catalog_verified_on
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            )?;
            line(
                l.saas_catalog_verified,
                first
                    .saas_check
                    .catalog_verified_on
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            )?;
        }
        row += 1;
        sheet.merge_range(row, 0, row, 1, l.made_locally, &f.wrap)?;
        sheet.set_row_height(row, 32)?;
        sheet.merge_range(row + 1, 0, row + 1, 1, l.no_findings_caveat, &f.wrap)?;
        sheet.set_row_height(row + 1, 32)?;
    }

    // 2. Devices (the asset ledger)
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name(l.devices)?;
        let platform = reports
            .first()
            .map(|r| r.device.platform.as_str())
            .unwrap_or("macos");
        let check_labels: Vec<&str> = CHECK_COLUMNS
            .iter()
            .map(|id| copy::check_label(*id, platform, lang))
            .collect();
        let mut headers: Vec<(&str, f64)> = vec![
            (l.asset_tag, 12.0),
            (l.hostname, 22.0),
            (l.model, 22.0),
            (l.serial, 18.0),
            (l.os, 30.0),
            (l.os_support, 16.0),
            (l.os_support_ends, 14.0),
        ];
        headers.extend(check_labels.iter().map(|label| (*label, 13.0)));
        headers.extend([
            (l.high, 6.0),
            (l.medium, 6.0),
            (l.low, 6.0),
            (l.user, 14.0),
            (l.department, 14.0),
            (l.location, 14.0),
            (l.scanned_at, 18.0),
            (l.notes, 24.0),
        ]);
        table_header(sheet, &f, &headers, reports.len() as u32)?;
        for (index, report) in reports.iter().enumerate() {
            let row = index as u32 + 1;
            let device = &report.device;
            let os = &report.os_lifecycle;
            let mut col = 0u16;
            let mut cell = |value: &str, format: &Format| -> Result<(), XlsxError> {
                sheet.write_string_with_format(row, col, value, format)?;
                col += 1;
                Ok(())
            };
            cell("", &f.text)?;
            cell(&device.hostname, &f.text)?;
            cell(device.device_model.as_deref().unwrap_or_default(), &f.text)?;
            cell(device.serial_number.as_deref().unwrap_or_default(), &f.text)?;
            cell(&copy::os_name(device), &f.text)?;
            match &os {
                Some(os) => {
                    let format = if os.status == LifecycleStatus::Supported {
                        &f.pass
                    } else {
                        &f.fail
                    };
                    cell(copy::lifecycle_status(os.status, lang), format)?;
                    cell(&os.support_ends_on, &f.text)?;
                }
                None => {
                    cell(copy::software_status(None, lang), &f.text)?;
                    cell("", &f.text)?;
                }
            }
            for id in CHECK_COLUMNS {
                match report.checks.iter().find(|check| check.id == id) {
                    Some(check) => cell(copy::check_result(check.pass, lang), f.check(check.pass))?,
                    None => cell("—", &f.text)?,
                }
            }
            let counts = report.counts();
            for n in [counts.high, counts.medium, counts.low] {
                sheet.write_number_with_format(row, col, n as f64, &f.text)?;
                col += 1;
            }
            for _ in 0..3 {
                sheet.write_string_with_format(row, col, "", &f.text)?;
                col += 1;
            }
            sheet.write_string_with_format(row, col, local_time(report), &f.text)?;
            sheet.write_string_with_format(row, col + 1, "", &f.text)?;
        }
    }

    // 3. Software
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name(l.software)?;
        let rows: u32 = reports.iter().map(|r| r.software.len() as u32).sum();
        table_header(
            sheet,
            &f,
            &[
                (l.hostname, 20.0),
                (l.name, 40.0),
                (l.version, 16.0),
                (l.publisher, 26.0),
                (l.support_status, 16.0),
                (l.support_ends, 14.0),
                (l.recommended, 36.0),
                (l.source, 14.0),
                (l.identifier, 30.0),
                (l.vendor_source, 30.0),
            ],
            rows,
        )?;
        let mut row = 1u32;
        for report in reports {
            for record in &report.software {
                let item = &record.item;
                let lifecycle = record.lifecycle.as_ref();
                let status_format = match lifecycle.map(|l| l.status) {
                    Some(LifecycleStatus::Ended) | Some(LifecycleStatus::Ending) => &f.fail,
                    Some(LifecycleStatus::Supported) => &f.pass,
                    None => &f.text,
                };
                let cells: [(&str, &Format); 10] = [
                    (&report.device.hostname, &f.text),
                    (&item.name, &f.text),
                    (item.version.as_deref().unwrap_or_default(), &f.text),
                    (item.publisher.as_deref().unwrap_or_default(), &f.text),
                    (copy::software_status(lifecycle, lang), status_format),
                    (
                        lifecycle
                            .map(|l| l.support_ends_on.as_str())
                            .unwrap_or_default(),
                        &f.text,
                    ),
                    (
                        lifecycle
                            .filter(|l| l.status != LifecycleStatus::Supported)
                            .map(|l| copy::successor(l, lang))
                            .unwrap_or_default(),
                        &f.text,
                    ),
                    (source_label(item.source), &f.text),
                    (&item.identifier, &f.text),
                    (
                        lifecycle.map(|l| l.source_url.as_str()).unwrap_or_default(),
                        &f.text,
                    ),
                ];
                for (col, (value, format)) in cells.iter().enumerate() {
                    sheet.write_string_with_format(row, col as u16, *value, format)?;
                }
                row += 1;
            }
        }
    }

    // 4. SaaS
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name(l.saas)?;
        let rows: u32 = reports.iter().map(|r| r.saas.len() as u32).sum();
        table_header(
            sheet,
            &f,
            &[
                (l.hostname, 20.0),
                (l.saas_name, 30.0),
                (l.vendor, 18.0),
                (l.category, 20.0),
                (l.ai, 6.0),
                (l.found_from, 18.0),
                (l.desktop_app, 26.0),
                (l.last_used, 14.0),
                (l.days_used, 12.0),
                (l.allowed, 14.0),
                (l.sanka_migration, 18.0),
                (l.migration_page, 40.0),
            ],
            rows,
        )?;
        let mut row = 1u32;
        for report in reports {
            for record in &report.saas {
                let allowed_format = match record.allowed {
                    Some(true) => &f.pass,
                    Some(false) => &f.fail,
                    None => &f.text,
                };
                let apps = record.desktop_app_names();
                let cells: [(&str, &Format); 8] = [
                    (&report.device.hostname, &f.text),
                    (&record.product, &f.text),
                    (&record.vendor, &f.text),
                    (copy::saas_category(record.category, lang), &f.text),
                    (if record.ai { "AI" } else { "" }, &f.text),
                    (copy::saas_found_from(record, lang), &f.text),
                    (&apps.join(", "), &f.text),
                    (
                        &record
                            .browser
                            .map(|b| b.last_used_on.to_string())
                            .unwrap_or_default(),
                        &f.text,
                    ),
                ];
                for (col, (value, format)) in cells.iter().enumerate() {
                    sheet.write_string_with_format(row, col as u16, *value, format)?;
                }
                match record.browser {
                    Some(used) => {
                        sheet.write_number_with_format(row, 8, used.days_used30 as f64, &f.text)?
                    }
                    None => sheet.write_string_with_format(row, 8, "", &f.text)?,
                };
                sheet.write_string_with_format(
                    row,
                    9,
                    copy::saas_allowed(record.allowed, lang),
                    allowed_format,
                )?;
                sheet.write_string_with_format(
                    row,
                    10,
                    copy::saas_migration(record, lang),
                    &f.text,
                )?;
                sheet.write_string_with_format(
                    row,
                    11,
                    copy::saas_migration_url(record, lang).unwrap_or_default(),
                    &f.text,
                )?;
                row += 1;
            }
        }
    }

    // 5. Risks
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name(l.findings)?;
        let rows: u32 = reports.iter().map(|r| r.findings.len() as u32).sum();
        table_header(
            sheet,
            &f,
            &[
                (l.number, 6.0),
                (l.severity, 9.0),
                (l.area, 12.0),
                (l.risk, 44.0),
                (l.hostname, 20.0),
                (l.evidence, 44.0),
                (l.fix, 56.0),
                (l.status, 12.0),
                (l.owner, 14.0),
                (l.support_ends, 14.0),
                (l.vendor_source, 30.0),
            ],
            rows,
        )?;
        let mut row = 1u32;
        for report in reports {
            for finding in &report.findings {
                let platform = report.device.platform.as_str();
                sheet.write_number_with_format(row, 0, row as f64, &f.text)?;
                sheet.write_string_with_format(
                    row,
                    1,
                    copy::severity(finding.severity, lang),
                    f.severity(finding.severity),
                )?;
                sheet.write_string_with_format(row, 2, copy::area(finding.area, lang), &f.text)?;
                sheet.write_string_with_format(row, 3, copy::headline(finding, lang), &f.wrap)?;
                sheet.write_string_with_format(row, 4, &report.device.hostname, &f.text)?;
                sheet.write_string_with_format(row, 5, copy::evidence(finding, lang), &f.wrap)?;
                sheet.write_string_with_format(
                    row,
                    6,
                    copy::fix(finding, platform, lang),
                    &f.wrap,
                )?;
                sheet.write_string_with_format(row, 7, l.statuses[0], &f.text)?;
                sheet.write_string_with_format(row, 8, "", &f.text)?;
                let lifecycle = finding.lifecycle.as_ref();
                sheet.write_string_with_format(
                    row,
                    9,
                    lifecycle
                        .map(|l| l.support_ends_on.as_str())
                        .unwrap_or_default(),
                    &f.text,
                )?;
                sheet.write_string_with_format(
                    row,
                    10,
                    lifecycle.map(|l| l.source_url.as_str()).unwrap_or_default(),
                    &f.text,
                )?;
                row += 1;
            }
        }
        if rows > 0 {
            let statuses = DataValidation::new().allow_list_strings(&l.statuses)?;
            sheet.add_data_validation(1, 7, rows, 7, &statuses)?;
        }
    }

    // 6. Security checks
    {
        let sheet = workbook.add_worksheet();
        sheet.set_name(l.checks)?;
        let rows: u32 = reports.iter().map(|r| r.checks.len() as u32).sum();
        table_header(
            sheet,
            &f,
            &[
                (l.hostname, 20.0),
                (l.check, 30.0),
                (l.result, 10.0),
                (l.detail, 50.0),
                (l.scanned_at, 18.0),
            ],
            rows,
        )?;
        let mut row = 1u32;
        for report in reports {
            let platform = report.device.platform.as_str();
            for check in &report.checks {
                sheet.write_string_with_format(row, 0, &report.device.hostname, &f.text)?;
                sheet.write_string_with_format(
                    row,
                    1,
                    copy::check_label(check.id, platform, lang),
                    &f.text,
                )?;
                sheet.write_string_with_format(
                    row,
                    2,
                    copy::check_result(check.pass, lang),
                    f.check(check.pass),
                )?;
                sheet.write_string_with_format(
                    row,
                    3,
                    copy::check_detail(check.detail.as_deref().unwrap_or_default(), lang),
                    &f.wrap,
                )?;
                sheet.write_string_with_format(row, 4, local_time(report), &f.text)?;
                row += 1;
            }
        }
    }

    workbook.save_to_buffer()
}

fn local_time(report: &ScanReport) -> String {
    report
        .scanned_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// Whether `bytes` is a Sanka Monitor workbook, as the app saved it or after
/// someone filled it in: a spreadsheet package that Sanka Monitor created, or
/// one that still has the ledger's device and software sheets (in either
/// language), which a spreadsheet program keeps when it saves.
pub fn is_monitor_workbook(bytes: &[u8]) -> bool {
    use std::io::Read as _;
    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return false;
    };
    let mut part = |name: &str| -> Option<String> {
        let file = archive.by_name(name).ok()?;
        let mut text = String::new();
        // These parts are small; a larger one is not a ledger's.
        file.take(1 << 20).read_to_string(&mut text).ok()?;
        Some(text)
    };
    if part("docProps/core.xml")
        .is_some_and(|core| core.contains("<dc:creator>Sanka Monitor</dc:creator>"))
    {
        return true;
    }
    let Some(workbook) = part("xl/workbook.xml") else {
        return false;
    };
    [Lang::En, Lang::Ja].into_iter().any(|lang| {
        let labels = copy::labels(lang);
        [labels.devices, labels.software]
            .iter()
            .all(|sheet| workbook.contains(&format!("name=\"{sheet}\"")))
    })
}

fn source_label(source: monitor_inventory::SoftwareSource) -> &'static str {
    match source {
        monitor_inventory::SoftwareSource::MacosApp => "macOS app",
        monitor_inventory::SoftwareSource::WindowsRegistry => "Windows program",
        monitor_inventory::SoftwareSource::WindowsStore => "Microsoft Store",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    /// Reads one sheet's shared-string cells back out of the workbook, the
    /// way a spreadsheet would: enough to see which values landed where.
    fn sheet_strings(bytes: &[u8], sheet: usize) -> Vec<String> {
        let files = unzip(bytes);
        let shared = files
            .iter()
            .find(|(name, _)| name == "xl/sharedStrings.xml")
            .map(|(_, body)| body.clone())
            .unwrap_or_default();
        let strings: Vec<String> = shared
            .split("<t")
            .skip(1)
            .filter_map(|part| part.split_once('>').map(|(_, rest)| rest))
            .filter_map(|rest| rest.split_once("</t>").map(|(text, _)| unescape(text)))
            .collect();
        let name = format!("xl/worksheets/sheet{sheet}.xml");
        let body = &files.iter().find(|(file, _)| *file == name).unwrap().1;
        body.split("t=\"s\"")
            .skip(1)
            .filter_map(|part| {
                let start = part.find("<v>")? + 3;
                let end = part.find("</v>")?;
                strings
                    .get(part[start..end].parse::<usize>().ok()?)
                    .cloned()
            })
            .collect()
    }

    fn unescape(text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
    }

    /// The workbook's parts, as (name, text).
    fn unzip(bytes: &[u8]) -> Vec<(String, String)> {
        use std::io::Read;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut file = archive.by_index(i).unwrap();
                let mut text = String::new();
                let _ = file.read_to_string(&mut text);
                (file.name().unwrap().to_string(), text)
            })
            .collect()
    }

    /// "Send an edited Excel" takes the ledger, whether the app saved it or
    /// someone filled it in, and refuses any other workbook or file.
    #[test]
    fn only_a_sanka_monitor_ledger_is_a_monitor_workbook() {
        let report = fixtures::windows_report();
        for lang in [Lang::En, Lang::Ja] {
            let saved = write(std::slice::from_ref(&report), lang).unwrap();
            assert!(is_monitor_workbook(&saved), "{lang:?}");
        }
        // Saved again by someone else: another creator, the ledger's sheets kept.
        let labels = copy::labels(Lang::Ja);
        let mut edited = Workbook::new();
        edited.set_properties(&DocProperties::new().set_author("Aiko Tanaka"));
        edited.add_worksheet().set_name(labels.summary).unwrap();
        edited.add_worksheet().set_name(labels.devices).unwrap();
        edited.add_worksheet().set_name(labels.software).unwrap();
        assert!(is_monitor_workbook(&edited.save_to_buffer().unwrap()));

        let mut other = Workbook::new();
        other.add_worksheet().set_name(labels.devices).unwrap();
        assert!(!is_monitor_workbook(&other.save_to_buffer().unwrap()));
        assert!(!is_monitor_workbook(b"PK\x03\x04 not really a zip"));
        assert!(!is_monitor_workbook(b"<html>a report</html>"));
    }

    #[test]
    fn the_ledger_lists_each_risk_with_its_fix_in_the_chosen_language() {
        let report = fixtures::windows_report();
        for lang in [Lang::En, Lang::Ja] {
            let bytes = write(std::slice::from_ref(&report), lang).unwrap();
            assert_eq!(&bytes[..2], b"PK");
            let risks = sheet_strings(&bytes, 5);
            for finding in &report.findings {
                assert!(risks.contains(&copy::headline(finding, lang)), "{lang:?}");
                assert!(
                    risks.contains(&copy::fix(finding, "windows", lang)),
                    "{lang:?}"
                );
            }
            // The program named like a formula is plain text in the ledger.
            let software = sheet_strings(&bytes, 3);
            assert!(software.contains(&"=HYPERLINK(\"https://example.com\")".to_string()));
        }
    }

    #[test]
    fn the_saas_sheet_lists_each_saas_with_the_companys_verdict_and_sankas_path() {
        let report = fixtures::windows_report_with_saas();
        for lang in [Lang::En, Lang::Ja] {
            let bytes = write(std::slice::from_ref(&report), lang).unwrap();
            let saas = sheet_strings(&bytes, 4);
            for product in ["ChatGPT", "Dropbox", "Salesforce", "Slack"] {
                assert!(saas.contains(&product.to_string()), "{lang:?} {product}");
            }
            assert!(saas.contains(&copy::saas_allowed(Some(false), lang).to_string()));
            assert!(saas.contains(&copy::saas_migration_url(&report.saas[2], lang).unwrap()));
            assert!(saas.contains(&"2026-10-18".to_string()));
            let summary = sheet_strings(&bytes, 1);
            assert!(
                summary.iter().any(|s| s.contains("Example K.K.")),
                "{lang:?}"
            );
            assert!(summary.iter().any(|s| s.contains("Chrome")), "{lang:?}");
        }
    }

    #[test]
    fn a_fleet_writes_one_ledger_row_per_device() {
        let mut second = fixtures::windows_report();
        second.device.hostname = "SALES-PC-08".into();
        let reports = [fixtures::windows_report(), second];
        let bytes = write(&reports, Lang::En).unwrap();
        let devices = sheet_strings(&bytes, 2);
        assert!(devices.contains(&"SALES-PC-07".to_string()));
        assert!(devices.contains(&"SALES-PC-08".to_string()));
        let risks = sheet_strings(&bytes, 5);
        let per_device = reports[0].findings.len();
        assert_eq!(
            risks.iter().filter(|s| s.as_str() == "SALES-PC-08").count(),
            per_device
        );
    }
}
