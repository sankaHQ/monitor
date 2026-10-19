//! The report as one CSV: one row per record, with stable English column
//! names and code values, so many files merge into one table (the fleet
//! template's Power Query reads a folder of them). Titles and fixes are
//! given in both languages, so a file reads the same wherever it is opened.
//!
//! Encoding: UTF-8 with a byte-order mark (Excel then reads Japanese
//! correctly), CRLF line ends, RFC 4180 quoting. A text cell that Excel would
//! read as a formula is prefixed with `'`.

use crate::copy::{self, Lang};
use crate::ScanReport;

pub const COLUMNS: [&str; 31] = [
    "record",
    "report_id",
    "scanned_at",
    "hostname",
    "serial_number",
    "device_model",
    "platform",
    "os_version",
    "os_edition",
    "agent_version",
    "catalog_verified_on",
    "check_id",
    "check_pass",
    "detail",
    "software_source",
    "software_identifier",
    "software_name",
    "software_version",
    "software_publisher",
    "finding_type",
    "area",
    "severity",
    "lifecycle_rule",
    "lifecycle_status",
    "support_ends_on",
    "successor",
    "source_url",
    "title_en",
    "title_ja",
    "fix_en",
    "fix_ja",
];

/// The record kinds in the `record` column.
pub const RECORD_DEVICE: &str = "device";
pub const RECORD_CHECK: &str = "check";
pub const RECORD_SOFTWARE: &str = "software";
pub const RECORD_FINDING: &str = "finding";

#[derive(Default)]
struct Row<'a> {
    record: &'a str,
    check_id: String,
    check_pass: String,
    detail: String,
    software_source: String,
    software_identifier: String,
    software_name: String,
    software_version: String,
    software_publisher: String,
    finding_type: String,
    area: String,
    severity: String,
    lifecycle_rule: String,
    lifecycle_status: String,
    support_ends_on: String,
    successor: String,
    source_url: String,
    title_en: String,
    title_ja: String,
    fix_en: String,
    fix_ja: String,
}

fn snake<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn lifecycle_cells(row: &mut Row<'_>, lifecycle: Option<&monitor_findings::Lifecycle>) {
    if let Some(l) = lifecycle {
        row.lifecycle_rule = l.rule_key.clone();
        row.lifecycle_status = snake(&l.status);
        row.support_ends_on = l.support_ends_on.clone();
        row.successor = l.successor.clone();
        row.source_url = l.source_url.clone();
    }
}

/// A finding's evidence without words to translate: what the check reported,
/// or the matching programs (`+N` when more matched than are listed). The
/// last day of support has its own column.
fn neutral_evidence(finding: &monitor_findings::Finding) -> String {
    let mut parts = Vec::new();
    if let Some(detail) = finding.detail.as_deref().filter(|d| !d.is_empty()) {
        parts.push(detail.to_string());
    }
    let mut programs: Vec<String> = finding
        .programs
        .iter()
        .map(|p| match &p.version {
            Some(version) => format!("{} {version}", p.name),
            None => p.name.clone(),
        })
        .collect();
    let more = finding.program_count.saturating_sub(finding.programs.len());
    if more > 0 {
        programs.push(format!("+{more}"));
    }
    if !programs.is_empty() {
        parts.push(programs.join(", "));
    }
    parts.join(" · ")
}

/// Writes the report as CSV bytes.
pub fn write(report: &ScanReport) -> Vec<u8> {
    let device = &report.device;
    let platform = device.platform.as_str();
    let mut device_row = Row {
        record: RECORD_DEVICE,
        ..Row::default()
    };
    lifecycle_cells(&mut device_row, report.os_lifecycle.as_ref());
    let mut rows = vec![device_row];
    for check in &report.checks {
        rows.push(Row {
            record: RECORD_CHECK,
            check_id: snake(&check.id),
            check_pass: check.pass.map(|p| p.to_string()).unwrap_or_default(),
            detail: check.detail.clone().unwrap_or_default(),
            ..Row::default()
        });
    }
    for record in &report.software {
        let mut row = Row {
            record: RECORD_SOFTWARE,
            software_source: snake(&record.item.source),
            software_identifier: record.item.identifier.clone(),
            software_name: record.item.name.clone(),
            software_version: record.item.version.clone().unwrap_or_default(),
            software_publisher: record.item.publisher.clone().unwrap_or_default(),
            ..Row::default()
        };
        lifecycle_cells(&mut row, record.lifecycle.as_ref());
        rows.push(row);
    }
    for finding in &report.findings {
        let mut row = Row {
            record: RECORD_FINDING,
            check_id: finding.check.map(|c| snake(&c)).unwrap_or_default(),
            detail: neutral_evidence(finding),
            finding_type: finding.finding_type.as_str().to_string(),
            area: snake(&finding.area),
            severity: finding.severity.as_str().to_string(),
            title_en: copy::headline(finding, Lang::En),
            title_ja: copy::headline(finding, Lang::Ja),
            fix_en: copy::fix(finding, platform, Lang::En),
            fix_ja: copy::fix(finding, platform, Lang::Ja),
            ..Row::default()
        };
        lifecycle_cells(&mut row, finding.lifecycle.as_ref());
        rows.push(row);
    }

    let scanned_at = report.scanned_at.to_rfc3339();
    let catalog = report
        .catalog_verified_on
        .map(|d| d.to_string())
        .unwrap_or_default();
    let mut out = String::from("\u{feff}");
    push_line(&mut out, COLUMNS.iter().copied());
    for row in rows {
        let is_device = row.record == RECORD_DEVICE;
        let device_only = |value: &str| {
            if is_device {
                value.to_string()
            } else {
                String::new()
            }
        };
        let cells = [
            row.record.to_string(),
            report.report_id.clone(),
            scanned_at.clone(),
            device.hostname.clone(),
            device.serial_number.clone().unwrap_or_default(),
            device_only(device.device_model.as_deref().unwrap_or_default()),
            device_only(platform),
            device_only(&device.os_version),
            device_only(device.os_edition.as_deref().unwrap_or_default()),
            device_only(&report.agent_version),
            device_only(&catalog),
            row.check_id,
            row.check_pass,
            row.detail,
            row.software_source,
            row.software_identifier,
            row.software_name,
            row.software_version,
            row.software_publisher,
            row.finding_type,
            row.area,
            row.severity,
            row.lifecycle_rule,
            row.lifecycle_status,
            row.support_ends_on,
            row.successor,
            row.source_url,
            row.title_en,
            row.title_ja,
            row.fix_en,
            row.fix_ja,
        ];
        push_line(&mut out, cells.iter().map(String::as_str));
    }
    out.into_bytes()
}

fn push_line<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>) {
    let mut first = true;
    for cell in cells {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&quote(cell));
    }
    out.push_str("\r\n");
}

/// RFC 4180 quoting, with a leading `'` on text Excel would run as a formula.
fn quote(cell: &str) -> String {
    let guarded;
    let cell = if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        guarded = format!("'{cell}");
        guarded.as_str()
    } else {
        cell
    };
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal RFC 4180 reader: what Excel and Power Query do with the file.
    fn parse(text: &str) -> Vec<Vec<String>> {
        let mut rows = vec![];
        let mut row = vec![];
        let mut cell = String::new();
        let mut quoted = false;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match (quoted, c) {
                (true, '"') if chars.peek() == Some(&'"') => {
                    cell.push('"');
                    chars.next();
                }
                (true, '"') => quoted = false,
                (true, c) => cell.push(c),
                (false, '"') => quoted = true,
                (false, ',') => row.push(std::mem::take(&mut cell)),
                (false, '\r') => {}
                (false, '\n') => {
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                }
                (false, c) => cell.push(c),
            }
        }
        rows
    }

    #[test]
    fn every_record_is_one_row_under_stable_columns() {
        let report = crate::fixtures::windows_report();
        let bytes = write(&report);
        let text = String::from_utf8(bytes).unwrap();
        let text = text.strip_prefix('\u{feff}').expect("Excel needs the BOM");
        let rows = parse(text);
        assert_eq!(rows[0], COLUMNS);
        assert!(rows.iter().all(|row| row.len() == COLUMNS.len()));
        let count = |kind: &str| rows.iter().filter(|row| row[0] == kind).count();
        assert_eq!(
            (
                count(RECORD_DEVICE),
                count(RECORD_CHECK),
                count(RECORD_SOFTWARE),
                count(RECORD_FINDING)
            ),
            (
                1,
                report.checks.len(),
                report.software.len(),
                report.findings.len()
            )
        );
        // Every row names its device, so a folder of files merges by it.
        assert!(rows[1..].iter().all(|row| row[3] == "SALES-PC-07"));
        let office = rows
            .iter()
            .find(|row| row[22] == "office-2016" && row[0] == RECORD_FINDING)
            .unwrap();
        assert_eq!(
            (office[21].as_str(), office[23].as_str()),
            ("high", "ended")
        );
    }

    #[test]
    fn a_program_named_like_a_formula_stays_text() {
        let report = crate::fixtures::windows_report();
        let text = String::from_utf8(write(&report)).unwrap();
        let rows = parse(text.trim_start_matches('\u{feff}'));
        let names: Vec<&str> = rows
            .iter()
            .filter(|row| row[0] == RECORD_SOFTWARE)
            .map(|row| row[16].as_str())
            .collect();
        assert!(names.contains(&"'=HYPERLINK(\"https://example.com\")"));
        assert!(!names.iter().any(|name| name.starts_with('=')));
    }
}
