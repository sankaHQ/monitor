//! IT-risk findings computed on the device.
//!
//! The local scan reads the device's security checks and installed software
//! against a lifecycle catalog bundled in the app, so a finding never needs a
//! server. The catalog and the rules are the ones sanka-api applies to
//! connected devices (`app/model/domain/monitor/findings.py`), so a device
//! scanned locally and the same device in the console report the same risks;
//! only `device_silent`, which needs a server to notice the silence, is
//! console-only.
//!
//! The catalog is two files in `catalog/`:
//! - `rules.json`: per product release, the patterns that recognise it on a
//!   device and the date its vendor stops security updates (ported from the
//!   sanka-api lifecycle seed);
//! - `eol-catalog.json`: a verbatim snapshot of the public
//!   `https://sanka.com/lab/eol-catalog.json`. Rules that name an
//!   `eolProduct` and `eolEvent` take their date, source and successor from
//!   it, as the server joins `data_ferry_eol_event`.
//!
//! Detection is deterministic: the same scan on the same day gives the same
//! findings in the same order.

use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use monitor_inventory::InstalledSoftware;
use monitor_posture::{PostureCheckId, PostureCheckResult};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

const RULES_JSON: &str = include_str!("../catalog/rules.json");
const EOL_CATALOG_JSON: &str = include_str!("../catalog/eol-catalog.json");

/// The schema versions this build reads; a newer file needs a newer app.
const RULES_SCHEMA_VERSION: u32 = 1;
const EOL_CATALOG_SCHEMA_VERSION: u32 = 1;

/// Support that ends within this many days is flagged before it ends.
pub const SUPPORT_ENDING_WINDOW_DAYS: i64 = 180;
/// Matching programs listed in one software finding.
pub const MAX_EVIDENCE_PROGRAMS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    Os,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleKind {
    /// The vendor stops security updates for this release.
    EndOfSupport,
    /// A newer version replaced this one (browsers): only "ended" counts.
    Outdated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Ended,
    Ending,
    Supported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatePrecision {
    Day,
    /// The vendor published only a month: support lasts to its last day.
    Month,
}

/// A rule's lifecycle as one device sees it on one day: the evidence a
/// finding and a software row carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lifecycle {
    pub rule_key: String,
    pub kind: LifecycleKind,
    pub status: LifecycleStatus,
    pub vendor: String,
    pub product: String,
    pub product_ja: String,
    /// `YYYY-MM-DD`, or `YYYY-MM` when the vendor published only a month.
    pub support_ends_on: String,
    pub date_precision: DatePrecision,
    pub successor: String,
    pub successor_ja: String,
    pub source_url: String,
    pub verified_on: NaiveDate,
}

/// One release in the catalog, with its patterns compiled. A rule matches
/// when every pattern it sets matches the corresponding field (a
/// case-insensitive search) and no exclude pattern does. A missing field
/// never matches a pattern.
#[derive(Debug, Clone)]
pub struct Rule {
    pub rule_key: String,
    pub subject: Subject,
    pub platform: Option<String>,
    pub kind: LifecycleKind,
    pub vendor: String,
    pub product: String,
    pub product_ja: String,
    pub release: String,
    name_pattern: Regex,
    name_exclude: Option<Regex>,
    identifier_pattern: Option<Regex>,
    publisher_pattern: Option<Regex>,
    version_pattern: Option<Regex>,
    version_exclude: Option<Regex>,
    edition_pattern: Option<Regex>,
    /// `None` while the linked catalog event is missing: the rule then
    /// recognises the program but raises nothing.
    pub support_ends_on: Option<String>,
    pub date_precision: Option<DatePrecision>,
    pub successor: String,
    pub successor_ja: String,
    pub source_url: String,
    pub verified_on: NaiveDate,
    pub priority: i64,
}

impl Rule {
    pub fn status(&self, today: NaiveDate) -> Option<LifecycleStatus> {
        let ends = support_end_date(self.support_ends_on.as_deref()?, self.date_precision?)?;
        Some(if today > ends {
            LifecycleStatus::Ended
        } else if (ends - today).num_days() <= SUPPORT_ENDING_WINDOW_DAYS {
            LifecycleStatus::Ending
        } else {
            LifecycleStatus::Supported
        })
    }

    pub fn lifecycle(&self, today: NaiveDate) -> Option<Lifecycle> {
        Some(Lifecycle {
            rule_key: self.rule_key.clone(),
            kind: self.kind,
            status: self.status(today)?,
            vendor: self.vendor.clone(),
            product: self.product.clone(),
            product_ja: if self.product_ja.is_empty() {
                self.product.clone()
            } else {
                self.product_ja.clone()
            },
            support_ends_on: self.support_ends_on.clone()?,
            date_precision: self.date_precision?,
            successor: self.successor.clone(),
            successor_ja: if self.successor_ja.is_empty() {
                self.successor.clone()
            } else {
                self.successor_ja.clone()
            },
            source_url: self.source_url.clone(),
            verified_on: self.verified_on,
        })
    }

    fn matches_software(&self, platform: &str, item: &InstalledSoftware) -> bool {
        self.subject == Subject::Software
            && self.platform.as_deref().is_none_or(|p| p == platform)
            && search(Some(&self.name_pattern), Some(&item.name))
            && !excluded(self.name_exclude.as_ref(), Some(&item.name))
            && search(self.identifier_pattern.as_ref(), Some(&item.identifier))
            && search(self.publisher_pattern.as_ref(), item.publisher.as_deref())
            && search(self.version_pattern.as_ref(), item.version.as_deref())
            && !excluded(self.version_exclude.as_ref(), item.version.as_deref())
    }

    fn matches_os(&self, platform: &str, os_version: &str, os_edition: Option<&str>) -> bool {
        self.subject == Subject::Os
            && self.platform.as_deref() == Some(platform)
            && search(Some(&self.name_pattern), Some(os_version))
            && search(self.edition_pattern.as_ref(), os_edition)
    }
}

fn search(pattern: Option<&Regex>, value: Option<&str>) -> bool {
    match pattern {
        None => true,
        Some(pattern) => value.is_some_and(|value| pattern.is_match(value)),
    }
}

fn excluded(pattern: Option<&Regex>, value: Option<&str>) -> bool {
    pattern.is_some_and(|pattern| value.is_some_and(|value| pattern.is_match(value)))
}

/// The last day with support: a month-precision date covers the whole month.
pub fn support_end_date(ends_on: &str, precision: DatePrecision) -> Option<NaiveDate> {
    match precision {
        DatePrecision::Day => NaiveDate::parse_from_str(ends_on, "%Y-%m-%d").ok(),
        DatePrecision::Month => {
            let first = NaiveDate::parse_from_str(&format!("{ends_on}-01"), "%Y-%m-%d").ok()?;
            let next = if first.month() == 12 {
                NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
            }?;
            next.pred_opt()
        }
    }
}

#[derive(Debug)]
pub struct CatalogError(String);

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lifecycle catalog: {}", self.0)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RulesFile {
    schema_version: u32,
    rules: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    rule_key: String,
    subject: Subject,
    platform: Option<String>,
    kind: LifecycleKind,
    vendor: String,
    product: String,
    #[serde(default)]
    product_ja: String,
    #[serde(default)]
    release: String,
    name_pattern: String,
    name_exclude: Option<String>,
    identifier_pattern: Option<String>,
    publisher_pattern: Option<String>,
    version_pattern: Option<String>,
    version_exclude: Option<String>,
    edition_pattern: Option<String>,
    support_ends_on: Option<String>,
    date_precision: Option<DatePrecision>,
    eol_product: Option<String>,
    eol_event: Option<String>,
    #[serde(default)]
    successor: String,
    #[serde(default)]
    successor_ja: String,
    source_url: String,
    verified_on: NaiveDate,
    priority: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EolCatalogFile {
    schema_version: u32,
    products: Vec<EolProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EolProduct {
    slug: String,
    successor: Option<EolSuccessor>,
    #[serde(default)]
    events: Vec<EolEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EolSuccessor {
    #[serde(default)]
    vendor_directed: Vec<Localized>,
}

#[derive(Deserialize)]
struct Localized {
    en: String,
    ja: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EolEvent {
    event_key: String,
    date: String,
    date_precision: DatePrecision,
    source: EolSource,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EolSource {
    url: String,
    last_verified: NaiveDate,
}

fn compile(pattern: &str, rule_key: &str) -> Result<Regex, CatalogError> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|err| CatalogError(format!("rule {rule_key}: {err}")))
}

fn compile_opt(pattern: Option<&str>, rule_key: &str) -> Result<Option<Regex>, CatalogError> {
    pattern
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| compile(pattern, rule_key))
        .transpose()
}

/// The lifecycle catalog: rules by priority, with catalog-linked dates,
/// sources and successors resolved.
#[derive(Debug, Clone)]
pub struct Catalog {
    rules: Vec<Rule>,
}

impl Catalog {
    /// The catalog bundled in this build.
    pub fn bundled() -> &'static Catalog {
        static BUNDLED: OnceLock<Catalog> = OnceLock::new();
        BUNDLED.get_or_init(|| {
            Catalog::from_json(RULES_JSON, EOL_CATALOG_JSON)
                .expect("the bundled lifecycle catalog is valid")
        })
    }

    pub fn from_json(rules_json: &str, eol_catalog_json: &str) -> Result<Catalog, CatalogError> {
        let rules: RulesFile = serde_json::from_str(rules_json)
            .map_err(|err| CatalogError(format!("rules.json: {err}")))?;
        let eol: EolCatalogFile = serde_json::from_str(eol_catalog_json)
            .map_err(|err| CatalogError(format!("eol-catalog.json: {err}")))?;
        if rules.schema_version != RULES_SCHEMA_VERSION {
            return Err(CatalogError(format!(
                "rules.json schema {} is not {RULES_SCHEMA_VERSION}",
                rules.schema_version
            )));
        }
        if eol.schema_version != EOL_CATALOG_SCHEMA_VERSION {
            return Err(CatalogError(format!(
                "eol-catalog.json schema {} is not {EOL_CATALOG_SCHEMA_VERSION}",
                eol.schema_version
            )));
        }
        let mut seen = std::collections::HashSet::new();
        let mut compiled = Vec::with_capacity(rules.rules.len());
        for raw in rules.rules {
            if !seen.insert(raw.rule_key.clone()) {
                return Err(CatalogError(format!("duplicate rule {}", raw.rule_key)));
            }
            compiled.push(resolve(raw, &eol)?);
        }
        compiled.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.rule_key.cmp(&b.rule_key))
        });
        Ok(Catalog { rules: compiled })
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The newest date a source in the catalog was checked: how current the
    /// bundled lifecycle data is.
    pub fn verified_on(&self) -> Option<NaiveDate> {
        self.rules.iter().map(|rule| rule.verified_on).max()
    }

    /// The first rule (by priority) for this operating system release and edition.
    pub fn match_os(
        &self,
        platform: &str,
        os_version: &str,
        os_edition: Option<&str>,
    ) -> Option<&Rule> {
        if os_version.is_empty() || os_version.eq_ignore_ascii_case("unknown") {
            return None;
        }
        self.rules
            .iter()
            .find(|rule| rule.matches_os(platform, os_version, os_edition))
    }

    /// The first rule (by priority) that recognises this installed program.
    pub fn match_software(&self, platform: &str, item: &InstalledSoftware) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| rule.matches_software(platform, item))
    }
}

/// A rule with its catalog link resolved: the event's date, precision,
/// source and verification win, and the product's first vendor-directed
/// successor fills an empty successor. A link to an event the catalog lacks
/// leaves the rule without a date.
fn resolve(raw: RawRule, eol: &EolCatalogFile) -> Result<Rule, CatalogError> {
    let key = raw.rule_key.as_str();
    let product = raw
        .eol_product
        .as_deref()
        .and_then(|slug| eol.products.iter().find(|product| product.slug == slug));
    let event = product.and_then(|product| {
        raw.eol_event
            .as_deref()
            .and_then(|key| product.events.iter().find(|event| event.event_key == key))
    });
    let successor = product
        .and_then(|product| product.successor.as_ref())
        .and_then(|successor| successor.vendor_directed.first());
    let (support_ends_on, date_precision, source_url, verified_on) = match event {
        Some(event) => (
            Some(event.date.clone()),
            Some(event.date_precision),
            event.source.url.clone(),
            event.source.last_verified,
        ),
        None if raw.eol_product.is_some() => (None, None, raw.source_url, raw.verified_on),
        None => (
            raw.support_ends_on,
            raw.date_precision,
            raw.source_url,
            raw.verified_on,
        ),
    };
    if let (Some(ends_on), Some(precision)) = (&support_ends_on, date_precision) {
        if support_end_date(ends_on, precision).is_none() {
            return Err(CatalogError(format!("rule {key}: bad date {ends_on}")));
        }
    }
    Ok(Rule {
        name_pattern: compile(&raw.name_pattern, key)?,
        name_exclude: compile_opt(raw.name_exclude.as_deref(), key)?,
        identifier_pattern: compile_opt(raw.identifier_pattern.as_deref(), key)?,
        publisher_pattern: compile_opt(raw.publisher_pattern.as_deref(), key)?,
        version_pattern: compile_opt(raw.version_pattern.as_deref(), key)?,
        version_exclude: compile_opt(raw.version_exclude.as_deref(), key)?,
        edition_pattern: compile_opt(raw.edition_pattern.as_deref(), key)?,
        successor: if raw.successor.is_empty() {
            successor.map(|s| s.en.clone()).unwrap_or_default()
        } else {
            raw.successor
        },
        successor_ja: if raw.successor_ja.is_empty() {
            successor.map(|s| s.ja.clone()).unwrap_or_default()
        } else {
            raw.successor_ja
        },
        rule_key: raw.rule_key,
        subject: raw.subject,
        platform: raw.platform,
        kind: raw.kind,
        vendor: raw.vendor,
        product: raw.product,
        product_ja: raw.product_ja,
        release: raw.release,
        support_ends_on,
        date_precision,
        source_url,
        verified_on,
        priority: raw.priority,
    })
}

/// The finding types a device can raise on its own. The names are the
/// console's (`MonitorFindingType`), so a report and the console agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingType {
    DeviceDiskUnencrypted,
    DeviceFirewallOff,
    DeviceAppProtectionOff,
    DeviceSystemIntegrityOff,
    DeviceOsEndOfSupport,
    DeviceOsUpdatesPending,
    DeviceAutomaticUpdatesOff,
    DeviceAntivirusOff,
    DeviceLocalAdmin,
    DeviceScreenLockOff,
    DeviceUnmanaged,
    DeviceSecureBootOff,
    SoftwareEndOfSupport,
    SoftwareOutdated,
}

impl FindingType {
    /// The wire name (`device_disk_unencrypted`).
    pub fn as_str(self) -> &'static str {
        match self {
            FindingType::DeviceDiskUnencrypted => "device_disk_unencrypted",
            FindingType::DeviceFirewallOff => "device_firewall_off",
            FindingType::DeviceAppProtectionOff => "device_app_protection_off",
            FindingType::DeviceSystemIntegrityOff => "device_system_integrity_off",
            FindingType::DeviceOsEndOfSupport => "device_os_end_of_support",
            FindingType::DeviceOsUpdatesPending => "device_os_updates_pending",
            FindingType::DeviceAutomaticUpdatesOff => "device_automatic_updates_off",
            FindingType::DeviceAntivirusOff => "device_antivirus_off",
            FindingType::DeviceLocalAdmin => "device_local_admin",
            FindingType::DeviceScreenLockOff => "device_screen_lock_off",
            FindingType::DeviceUnmanaged => "device_unmanaged",
            FindingType::DeviceSecureBootOff => "device_secure_boot_off",
            FindingType::SoftwareEndOfSupport => "software_end_of_support",
            FindingType::SoftwareOutdated => "software_outdated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    Device,
    Software,
}

/// Ordered most severe first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
        }
    }
}

/// A failed check and the finding it raises. `pass: null` (could not
/// determine) raises none.
pub fn posture_finding(check: PostureCheckId) -> (FindingType, Severity) {
    match check {
        PostureCheckId::DiskEncryption => (FindingType::DeviceDiskUnencrypted, Severity::High),
        PostureCheckId::Firewall => (FindingType::DeviceFirewallOff, Severity::Medium),
        PostureCheckId::Gatekeeper => (FindingType::DeviceAppProtectionOff, Severity::Medium),
        PostureCheckId::SystemIntegrity => (FindingType::DeviceSystemIntegrityOff, Severity::High),
        PostureCheckId::OsUpdate => (FindingType::DeviceOsUpdatesPending, Severity::Medium),
        PostureCheckId::AutomaticUpdates => {
            (FindingType::DeviceAutomaticUpdatesOff, Severity::Medium)
        }
        PostureCheckId::Antivirus => (FindingType::DeviceAntivirusOff, Severity::High),
        PostureCheckId::LocalAdmin => (FindingType::DeviceLocalAdmin, Severity::Medium),
        PostureCheckId::ScreenLock => (FindingType::DeviceScreenLockOff, Severity::Medium),
        PostureCheckId::DeviceManagement => (FindingType::DeviceUnmanaged, Severity::Low),
        PostureCheckId::SecureBoot => (FindingType::DeviceSecureBootOff, Severity::Medium),
    }
}

/// An installed program named in a software finding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramRef {
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
}

/// One risk on this device, with its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    #[serde(rename = "type")]
    pub finding_type: FindingType,
    pub area: Area,
    pub severity: Severity,
    /// Tells findings of one type apart: the lifecycle rule for software.
    pub subject_key: String,
    /// The failed check behind a device finding, and what it reported.
    pub check: Option<PostureCheckId>,
    pub detail: Option<String>,
    pub lifecycle: Option<Lifecycle>,
    /// Matching programs (at most [`MAX_EVIDENCE_PROGRAMS`]) and how many matched.
    pub programs: Vec<ProgramRef>,
    pub program_count: usize,
}

/// What the device says about its operating system.
#[derive(Debug, Clone, Copy)]
pub struct DeviceFacts<'a> {
    /// `macos` or `windows`.
    pub platform: &'a str,
    /// macOS `14.6.1`; Windows `Windows 11 24H2 (26100.4351)`.
    pub os_version: &'a str,
    /// Windows `EditionID`; `None` on macOS.
    pub os_edition: Option<&'a str>,
}

/// Every finding the scan supports, in the console's order: failed checks as
/// the device listed them, the operating system, then software by rule.
pub fn detect(
    device: DeviceFacts<'_>,
    posture: &[PostureCheckResult],
    software: &[InstalledSoftware],
    catalog: &Catalog,
    today: NaiveDate,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for check in posture {
        if check.pass != Some(false) {
            continue;
        }
        let (finding_type, severity) = posture_finding(check.id);
        findings.push(Finding {
            finding_type,
            area: Area::Device,
            severity,
            subject_key: String::new(),
            check: Some(check.id),
            detail: check.detail.clone(),
            lifecycle: None,
            programs: Vec::new(),
            program_count: 0,
        });
    }

    let os_lifecycle = catalog
        .match_os(device.platform, device.os_version, device.os_edition)
        .and_then(|rule| rule.lifecycle(today));
    if let Some(lifecycle) = os_lifecycle.filter(|l| l.status != LifecycleStatus::Supported) {
        findings.push(Finding {
            finding_type: FindingType::DeviceOsEndOfSupport,
            area: Area::Device,
            severity: if lifecycle.status == LifecycleStatus::Ended {
                Severity::High
            } else {
                Severity::Medium
            },
            subject_key: String::new(),
            check: None,
            detail: None,
            lifecycle: Some(lifecycle),
            programs: Vec::new(),
            program_count: 0,
        });
    }

    findings.extend(software_findings(device.platform, software, catalog, today));
    findings
}

fn software_findings(
    platform: &str,
    software: &[InstalledSoftware],
    catalog: &Catalog,
    today: NaiveDate,
) -> Vec<Finding> {
    let mut matched: std::collections::BTreeMap<&str, (&Rule, Vec<ProgramRef>)> =
        std::collections::BTreeMap::new();
    for item in software {
        if let Some(rule) = catalog.match_software(platform, item) {
            matched
                .entry(rule.rule_key.as_str())
                .or_insert_with(|| (rule, Vec::new()))
                .1
                .push(ProgramRef {
                    name: item.name.clone(),
                    version: item.version.clone(),
                    publisher: item.publisher.clone(),
                });
        }
    }
    let mut findings = Vec::new();
    for (rule_key, (rule, mut programs)) in matched {
        let Some(lifecycle) = rule.lifecycle(today) else {
            continue;
        };
        let (finding_type, severity) = match (rule.kind, lifecycle.status) {
            (_, LifecycleStatus::Supported) => continue,
            (LifecycleKind::Outdated, LifecycleStatus::Ended) => {
                (FindingType::SoftwareOutdated, Severity::Medium)
            }
            (LifecycleKind::Outdated, _) => continue,
            (LifecycleKind::EndOfSupport, LifecycleStatus::Ended) => {
                (FindingType::SoftwareEndOfSupport, Severity::High)
            }
            (LifecycleKind::EndOfSupport, _) => {
                (FindingType::SoftwareEndOfSupport, Severity::Medium)
            }
        };
        programs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));
        let program_count = programs.len();
        programs.truncate(MAX_EVIDENCE_PROGRAMS);
        findings.push(Finding {
            finding_type,
            area: Area::Software,
            severity,
            subject_key: rule_key.to_string(),
            check: None,
            detail: None,
            lifecycle: Some(lifecycle),
            programs,
            program_count,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use monitor_inventory::SoftwareSource;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn program(source: SoftwareSource, name: &str, version: Option<&str>) -> InstalledSoftware {
        let publisher = (source != SoftwareSource::MacosApp).then_some("Microsoft Corporation");
        InstalledSoftware::new(source, &format!("id:{name}"), name, version, publisher).unwrap()
    }

    fn check(id: PostureCheckId, pass: Option<bool>) -> PostureCheckResult {
        PostureCheckResult {
            id,
            pass,
            detail: None,
            collected_at: Utc.with_ymd_and_hms(2026, 10, 19, 0, 0, 0).unwrap(),
        }
    }

    fn types(findings: &[Finding]) -> Vec<(&'static str, Severity, &str)> {
        findings
            .iter()
            .map(|f| (f.finding_type.as_str(), f.severity, f.subject_key.as_str()))
            .collect()
    }

    /// The server's catalog-linked rules resolve against the bundled public
    /// catalog snapshot; a link the snapshot cannot satisfy would silently
    /// drop a finding, so it fails here first.
    #[test]
    fn every_catalog_link_in_the_bundled_rules_resolves() {
        let catalog = Catalog::bundled();
        let rules: serde_json::Value = serde_json::from_str(RULES_JSON).unwrap();
        for raw in rules["rules"].as_array().unwrap() {
            let key = raw["ruleKey"].as_str().unwrap();
            let rule = catalog
                .rules()
                .iter()
                .find(|rule| rule.rule_key == key)
                .unwrap();
            assert!(
                rule.support_ends_on.is_some() && rule.date_precision.is_some(),
                "{key} has no support end date"
            );
        }
    }

    #[test]
    fn month_precision_lasts_to_the_end_of_the_month() {
        for (ends_on, precision, last_day) in [
            ("2027-04", DatePrecision::Month, day(2027, 4, 30)),
            ("2028-02", DatePrecision::Month, day(2028, 2, 29)),
            ("2026-12", DatePrecision::Month, day(2026, 12, 31)),
            ("2025-10-14", DatePrecision::Day, day(2025, 10, 14)),
        ] {
            assert_eq!(support_end_date(ends_on, precision), Some(last_day));
        }
        assert_eq!(support_end_date("2027-13", DatePrecision::Month), None);
    }

    /// The local scan raises what the console raises for the same device on
    /// the same day (sanka-api `detect_device_findings`, minus the
    /// server-only `device_silent`).
    #[test]
    fn a_windows_scan_raises_the_consoles_findings() {
        let today = day(2026, 10, 19);
        let device = DeviceFacts {
            platform: "windows",
            os_version: "Windows 10 22H2 (19045.6456)",
            os_edition: Some("Professional"),
        };
        let posture = [
            check(PostureCheckId::DiskEncryption, Some(false)),
            check(PostureCheckId::Firewall, Some(true)),
            check(PostureCheckId::Antivirus, None),
            check(PostureCheckId::DeviceManagement, Some(false)),
        ];
        let software = [
            program(
                SoftwareSource::WindowsRegistry,
                "Microsoft Office Professional Plus 2016",
                Some("16.0.4266.1001"),
            ),
            program(
                SoftwareSource::WindowsRegistry,
                "Microsoft Visio Professional 2016",
                Some("16.0.4266.1001"),
            ),
            program(
                SoftwareSource::WindowsRegistry,
                "Google Chrome",
                Some("140.0.7339.81"),
            ),
            program(
                SoftwareSource::WindowsRegistry,
                "Mozilla Firefox ESR",
                Some("115.16.0"),
            ),
            program(
                SoftwareSource::WindowsRegistry,
                "Microsoft SharePoint Server 2019",
                Some("16.0.10337.12109"),
            ),
            // Recognised, but support lasts beyond the 180-day window.
            program(SoftwareSource::WindowsRegistry, "弥生給与 26", Some("31.0")),
            program(SoftwareSource::WindowsRegistry, "Microsoft Edge", None),
        ];
        let findings = detect(device, &posture, &software, Catalog::bundled(), today);
        assert_eq!(
            types(&findings),
            [
                ("device_disk_unencrypted", Severity::High, ""),
                ("device_unmanaged", Severity::Low, ""),
                ("device_os_end_of_support", Severity::High, ""),
                ("software_outdated", Severity::Medium, "chrome-151"),
                ("software_end_of_support", Severity::High, "office-2016"),
                (
                    "software_end_of_support",
                    Severity::High,
                    "sharepoint-server-2019"
                ),
            ]
        );
        let office = &findings[4];
        assert_eq!(office.program_count, 2);
        let os = findings[2].lifecycle.as_ref().unwrap();
        assert_eq!(
            (os.rule_key.as_str(), os.status),
            ("windows-10", LifecycleStatus::Ended)
        );
        // Linked to the public catalog: date and successor come from it.
        let sharepoint = findings[5].lifecycle.as_ref().unwrap();
        assert_eq!(sharepoint.support_ends_on, "2026-07-14");
        assert_eq!(
            sharepoint.successor,
            "SharePoint Server Subscription Edition"
        );
        // An exclude pattern stands in for the server's lookahead.
        let catalog = Catalog::bundled();
        for (name, publisher, rule_key) in [
            ("弥生給与 26", "弥生株式会社", Some("yayoi-payroll")),
            ("弥生給与 Next", "弥生株式会社", None),
            ("PCA商魂DX", "ピー・シー・エー株式会社", Some("pca-package")),
            ("PCAクラウド 会計", "ピー・シー・エー株式会社", None),
        ] {
            let item = InstalledSoftware::new(
                SoftwareSource::WindowsRegistry,
                name,
                name,
                Some("1.0"),
                Some(publisher),
            )
            .unwrap();
            assert_eq!(
                catalog
                    .match_software("windows", &item)
                    .map(|rule| rule.rule_key.as_str()),
                rule_key,
                "{name}"
            );
        }
    }

    #[test]
    fn windows_editions_pick_their_own_lifecycle() {
        let catalog = Catalog::bundled();
        let today = day(2026, 10, 19);
        // (os version, edition, rule, status)
        for (os_version, edition, rule_key, status) in [
            (
                "Windows 11 24H2 (26100.4351)",
                Some("Professional"),
                "windows-11-24h2",
                LifecycleStatus::Ended,
            ),
            (
                "Windows 11 24H2 (26100.4351)",
                Some("Enterprise"),
                "windows-11-24h2-enterprise",
                LifecycleStatus::Supported,
            ),
            (
                "Windows 11 24H2 (26100.4351)",
                Some("EnterpriseS"),
                "windows-11-ltsc-2024",
                LifecycleStatus::Supported,
            ),
            (
                "Windows 10 21H2 (19044.5011)",
                Some("IoTEnterpriseS"),
                "windows-10-iot-ltsc-2021",
                LifecycleStatus::Supported,
            ),
            (
                "Windows 11 23H2 (22631.4317)",
                Some("Education"),
                "windows-11-23h2-enterprise",
                LifecycleStatus::Ending,
            ),
        ] {
            let rule = catalog.match_os("windows", os_version, edition).unwrap();
            assert_eq!(rule.rule_key, rule_key, "{os_version} {edition:?}");
            assert_eq!(rule.status(today), Some(status), "{rule_key}");
        }
        assert!(catalog.match_os("macos", "15.1", None).is_none());
        assert!(catalog.match_os("windows", "unknown", None).is_none());
    }

    #[test]
    fn mac_office_and_browsers_match_on_version_ranges() {
        let catalog = Catalog::bundled();
        let word = |version: &str| {
            InstalledSoftware::new(
                SoftwareSource::MacosApp,
                "com.microsoft.Word",
                "Microsoft Word",
                Some(version),
                None,
            )
            .unwrap()
        };
        // (program, rule it should hit)
        for (item, rule_key) in [
            (word("16.78"), Some("office-mac-16-78")),
            (word("16.78.3"), Some("office-mac-16-78")),
            (word("15.41"), Some("office-mac-16-78")),
            (word("16.79"), None),
            (word("16.90"), None),
            (
                program(SoftwareSource::MacosApp, "Firefox", Some("115.16.0")),
                None,
            ),
            (
                program(SoftwareSource::MacosApp, "Firefox", Some("128.3.1")),
                Some("firefox-152"),
            ),
            (
                program(SoftwareSource::MacosApp, "Firefox", Some("153.0")),
                None,
            ),
            (
                program(SoftwareSource::MacosApp, "Node.js", Some("18.20.4")),
                Some("nodejs-18"),
            ),
        ] {
            assert_eq!(
                catalog
                    .match_software("macos", &item)
                    .map(|rule| rule.rule_key.as_str()),
                rule_key,
                "{} {:?}",
                item.name,
                item.version
            );
        }
        // Windows-only rules never match on a Mac.
        let office = program(
            SoftwareSource::MacosApp,
            "Microsoft Office 2016",
            Some("16.0"),
        );
        assert!(catalog.match_software("macos", &office).is_none());
    }

    #[test]
    fn support_ending_within_the_window_is_flagged_before_it_ends() {
        let catalog = Catalog::bundled();
        let device = DeviceFacts {
            platform: "macos",
            os_version: "14.7.1",
            os_edition: None,
        };
        // macOS 14 support ends 2026-09-14.
        for (today, expected) in [
            (day(2026, 3, 1), Vec::<(&str, Severity, &str)>::new()),
            (
                day(2026, 3, 18),
                vec![("device_os_end_of_support", Severity::Medium, "")],
            ),
            (
                day(2026, 9, 15),
                vec![("device_os_end_of_support", Severity::High, "")],
            ),
        ] {
            let findings = detect(device, &[], &[], catalog, today);
            assert_eq!(types(&findings), expected, "{today}");
        }
    }

    #[test]
    fn a_broken_catalog_is_refused() {
        let eol = r#"{"schemaVersion":1,"products":[]}"#;
        let rule = |extra: &str| {
            format!(
                r#"{{"schemaVersion":1,"rules":[{{"ruleKey":"a","subject":"software","kind":"end_of_support","vendor":"V","product":"P","namePattern":"^P","sourceUrl":"https://example.com","verifiedOn":"2026-10-17","priority":1{extra}}}]}}"#
            )
        };
        assert!(Catalog::from_json(&rule(""), eol).is_ok());
        for broken in [
            rule(r#","versionPattern":"(?!1)""#),
            rule(r#","supportEndsOn":"2026-02-30","datePrecision":"day""#),
            r#"{"schemaVersion":2,"rules":[]}"#.to_string(),
        ] {
            assert!(Catalog::from_json(&broken, eol).is_err(), "{broken}");
        }
        // A link to an event the catalog lacks keeps the rule, dateless.
        let linked = Catalog::from_json(&rule(r#","eolProduct":"x","eolEvent":"y""#), eol).unwrap();
        assert_eq!(linked.rules()[0].status(day(2030, 1, 1)), None);
    }
}
