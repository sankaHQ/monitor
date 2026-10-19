//! Sanka Monitor's local reports.
//!
//! A [`ScanReport`] is one scan of one device: its facts, security checks,
//! installed software with lifecycle status, and the findings computed on the
//! device. It is written, on the device, as
//! - an Excel workbook in the IT資産台帳 (IT asset ledger) layout: devices,
//!   software and versions, the risk list with severity and fix, the checks
//!   ([`xlsx`]);
//! - a CSV, one row per record, with stable English column names, the format
//!   the command-line scan writes per PC for a fleet folder ([`csv`]);
//! - a one-page IT risk report as a self-contained HTML page that prints on
//!   one A4 sheet ([`html`]).
//!
//! Nothing here touches the network: a report leaves the device only when
//! the person sends the file.

use chrono::{DateTime, NaiveDate, Utc};
use monitor_findings::{Catalog, DeviceFacts, Finding, Lifecycle, LifecycleStatus};
use monitor_inventory::InstalledSoftware;
use monitor_posture::PostureCheckResult;
use serde::{Deserialize, Serialize};

pub mod copy;
pub mod csv;
pub mod html;
pub mod xlsx;

pub use copy::Lang;

/// Bumped when the report's shape changes.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// What the device says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub hostname: String,
    /// `macos` or `windows`.
    pub platform: String,
    pub os_version: String,
    pub os_edition: Option<String>,
    pub device_model: Option<String>,
    pub serial_number: Option<String>,
}

/// An installed program and, when the catalog recognises it, its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRecord {
    #[serde(flatten)]
    pub item: InstalledSoftware,
    pub lifecycle: Option<Lifecycle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub schema_version: u32,
    /// Random, per scan: tells two reports of one device apart.
    pub report_id: String,
    pub scanned_at: DateTime<Utc>,
    pub agent_version: String,
    /// How current the bundled lifecycle catalog is.
    pub catalog_verified_on: Option<NaiveDate>,
    pub device: DeviceRecord,
    /// The operating system's lifecycle, when the catalog has it.
    pub os_lifecycle: Option<Lifecycle>,
    pub checks: Vec<PostureCheckResult>,
    /// Sorted by name, then version.
    pub software: Vec<SoftwareRecord>,
    /// Most severe first; within a severity, in detection order.
    pub findings: Vec<Finding>,
}

/// Counts the summary sections show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCounts {
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub checks_passed: usize,
    pub checks_failed: usize,
    pub checks_unknown: usize,
    pub programs: usize,
    pub programs_past_support: usize,
}

impl ScanReport {
    /// Reads one scan against the catalog: `today` is the device's local
    /// date, which decides what has ended.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        device: DeviceRecord,
        checks: Vec<PostureCheckResult>,
        software: Vec<InstalledSoftware>,
        catalog: &Catalog,
        today: NaiveDate,
        scanned_at: DateTime<Utc>,
        agent_version: &str,
    ) -> ScanReport {
        let facts = DeviceFacts {
            platform: &device.platform,
            os_version: &device.os_version,
            os_edition: device.os_edition.as_deref(),
        };
        let mut findings = monitor_findings::detect(facts, &checks, &software, catalog, today);
        findings.sort_by_key(|finding| finding.severity);
        let mut software: Vec<SoftwareRecord> = software
            .into_iter()
            .map(|item| SoftwareRecord {
                lifecycle: catalog
                    .match_software(&device.platform, &item)
                    .and_then(|rule| rule.lifecycle(today)),
                item,
            })
            .collect();
        software.sort_by(|a, b| {
            a.item
                .name
                .to_lowercase()
                .cmp(&b.item.name.to_lowercase())
                .then_with(|| a.item.version.cmp(&b.item.version))
        });
        let os_lifecycle = catalog
            .match_os(
                &device.platform,
                &device.os_version,
                device.os_edition.as_deref(),
            )
            .and_then(|rule| rule.lifecycle(today));
        ScanReport {
            schema_version: REPORT_SCHEMA_VERSION,
            report_id: uuid::Uuid::new_v4().to_string(),
            scanned_at,
            agent_version: agent_version.to_string(),
            catalog_verified_on: catalog.verified_on(),
            device,
            os_lifecycle,
            checks,
            software,
            findings,
        }
    }

    pub fn counts(&self) -> ReportCounts {
        let mut counts = ReportCounts {
            programs: self.software.len(),
            ..ReportCounts::default()
        };
        for finding in &self.findings {
            match finding.severity {
                monitor_findings::Severity::High => counts.high += 1,
                monitor_findings::Severity::Medium => counts.medium += 1,
                monitor_findings::Severity::Low => counts.low += 1,
            }
        }
        for check in &self.checks {
            match check.pass {
                Some(true) => counts.checks_passed += 1,
                Some(false) => counts.checks_failed += 1,
                None => counts.checks_unknown += 1,
            }
        }
        counts.programs_past_support = self
            .software
            .iter()
            .filter(|record| {
                record
                    .lifecycle
                    .as_ref()
                    .is_some_and(|l| l.status == LifecycleStatus::Ended)
            })
            .count();
        counts
    }

    /// A file name for this report: the device and the scan time, safe on
    /// every file system (`MACBOOK-PRO_C02XK1ZZJG5H_20261019-0930`).
    pub fn file_stem(&self) -> String {
        format!(
            "{}_{}",
            device_file_key(&self.device),
            self.scanned_at
                .with_timezone(&chrono::Local)
                .format("%Y%m%d-%H%M")
        )
    }
}

/// One stable name per device for the fleet folder: the hostname and, when
/// known, the serial number, reduced to characters every file system and
/// share accepts. A rescan overwrites the device's previous file.
pub fn device_file_key(device: &DeviceRecord) -> String {
    let clean = |value: &str| -> String {
        let kept: String = value
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let mut collapsed = String::with_capacity(kept.len());
        for c in kept.chars() {
            if !(c == '-' && collapsed.ends_with('-')) {
                collapsed.push(c);
            }
        }
        collapsed.trim_matches('-').chars().take(60).collect()
    };
    let host = match clean(&device.hostname) {
        host if host.is_empty() => "device".to_string(),
        host => host,
    };
    match device.serial_number.as_deref().map(clean) {
        Some(serial) if !serial.is_empty() => format!("{host}_{serial}"),
        _ => host,
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use chrono::TimeZone;
    use monitor_inventory::SoftwareSource;
    use monitor_posture::PostureCheckId;

    pub fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 19).unwrap()
    }

    /// A Windows 10 PC with Office 2016, an old Chrome and a failed check.
    pub fn windows_report() -> ScanReport {
        let at = Utc.with_ymd_and_hms(2026, 10, 19, 0, 30, 0).unwrap();
        let check = |id, pass, detail: Option<&str>| PostureCheckResult {
            id,
            pass,
            detail: detail.map(str::to_string),
            collected_at: at,
        };
        let program = |name: &str, version: &str| {
            InstalledSoftware::new(
                SoftwareSource::WindowsRegistry,
                &format!("{{{name}}}"),
                name,
                Some(version),
                Some("Microsoft Corporation"),
            )
            .unwrap()
        };
        ScanReport::build(
            DeviceRecord {
                hostname: "SALES-PC-07".into(),
                platform: "windows".into(),
                os_version: "Windows 10 22H2 (19045.6456)".into(),
                os_edition: Some("Professional".into()),
                device_model: Some("LENOVO 20XW".into()),
                serial_number: Some("PF2ABC12".into()),
            },
            vec![
                check(PostureCheckId::DiskEncryption, Some(false), None),
                check(PostureCheckId::Firewall, Some(true), None),
                check(
                    PostureCheckId::Antivirus,
                    None,
                    Some("Security Center unavailable"),
                ),
            ],
            vec![
                program("Microsoft Office Professional Plus 2016", "16.0.4266.1001"),
                program("=HYPERLINK(\"https://example.com\")", "1.0"),
                program("Google Chrome", "140.0.7339.81"),
            ],
            Catalog::bundled(),
            today(),
            at,
            "0.3.0",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_most_severe_first_and_software_carries_its_lifecycle() {
        let report = fixtures::windows_report();
        let severities: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.severity.as_str())
            .collect();
        assert_eq!(severities, ["high", "high", "high", "medium"]);
        let office = report
            .software
            .iter()
            .find(|record| record.item.name.contains("Office"))
            .unwrap();
        assert_eq!(
            office.lifecycle.as_ref().map(|l| l.status),
            Some(LifecycleStatus::Ended)
        );
        let counts = report.counts();
        assert_eq!(
            (counts.high, counts.medium, counts.programs_past_support),
            (3, 1, 2)
        );
        assert_eq!(
            (
                counts.checks_passed,
                counts.checks_failed,
                counts.checks_unknown
            ),
            (1, 1, 1)
        );
    }

    #[test]
    fn device_file_keys_are_safe_on_any_share() {
        let device = |hostname: &str, serial: Option<&str>| DeviceRecord {
            hostname: hostname.into(),
            platform: "windows".into(),
            os_version: String::new(),
            os_edition: None,
            device_model: None,
            serial_number: serial.map(str::to_string),
        };
        for (hostname, serial, key) in [
            ("SALES-PC-07", Some("PF2ABC12"), "SALES-PC-07_PF2ABC12"),
            ("Taro’s MacBook Pro", None, "Taro-s-MacBook-Pro"),
            (
                "MacBook Pro (2)",
                Some("C02ZX1ABCD12"),
                "MacBook-Pro-2_C02ZX1ABCD12",
            ),
            ("../../etc", Some(" "), "etc"),
            ("営業部のPC", Some("C02/XK1"), "PC_C02-XK1"),
            ("", None, "device"),
        ] {
            assert_eq!(device_file_key(&device(hostname, serial)), key);
        }
    }
}
