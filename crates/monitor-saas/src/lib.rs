//! The SaaS a company uses, found on the PC.
//!
//! A catalog bundled in the app (`catalog/saas.json`) names each SaaS with
//! its vendor, category, an AI flag, the hosts its web app runs on, the
//! identifiers of its desktop client, and the Sanka migration page and
//! connector that move its data. Three things read a PC against it:
//! - the installed desktop clients ([`SaasCatalog::match_program`]), from
//!   the installed-software scan, which needs no permission;
//! - the browser check ([`history`]), only when the person turns it on: the
//!   browsers' history files on the PC, matched by host name. Per SaaS it
//!   keeps the last day used and the number of days used in the last 30
//!   ([`BrowserUse`]), and nothing else;
//! - the company allowlist ([`allowlist`]), which marks each SaaS allowed or
//!   not.
//!
//! Privacy invariant (structural, like the usage events): a [`SaasRecord`]
//! has no fields for web addresses, page titles, visit times or history
//! entries, and none may be added. The wire-shape tests below pin this.
//!
//! Matching is deterministic: the same PC on the same day gives the same
//! records in the same order.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use chrono::NaiveDate;
use monitor_inventory::{InstalledSoftware, SoftwareSource};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

pub mod allowlist;
pub mod history;

pub use allowlist::{Allowlist, AllowlistOrigin};

const CATALOG_JSON: &str = include_str!("../catalog/saas.json");

/// The schema version this build reads; a newer file needs a newer app.
const CATALOG_SCHEMA_VERSION: u32 = 1;

/// The Sanka migration pages, `https://sanka.com/migrate/<slug>/`.
pub const MIGRATE_BASE_URL: &str = "https://sanka.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Ai,
    Chat,
    Meetings,
    Suite,
    Files,
    Crm,
    Marketing,
    Design,
    Support,
    Itsm,
    Work,
    Knowledge,
    Dev,
    Cloud,
    Analytics,
    Automation,
    Hr,
    Finance,
    Erp,
    Esign,
    Commerce,
    Identity,
    Security,
    Remote,
}

/// One host pattern: `app.example.com` matches that host and its
/// subdomains, `*.example.com` its subdomains only.
#[derive(Debug, Clone)]
struct HostPattern {
    suffix: String,
    subdomains_only: bool,
}

impl HostPattern {
    fn parse(pattern: &str) -> Option<HostPattern> {
        let pattern = pattern.trim().to_ascii_lowercase();
        let (suffix, subdomains_only) = match pattern.strip_prefix("*.") {
            Some(rest) => (rest.to_string(), true),
            None => (pattern, false),
        };
        let valid = suffix.contains('.')
            && !suffix.starts_with('.')
            && !suffix.ends_with('.')
            && suffix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        valid.then_some(HostPattern {
            suffix,
            subdomains_only,
        })
    }

    fn matches(&self, host: &str) -> bool {
        if host == self.suffix {
            return !self.subdomains_only;
        }
        host.len() > self.suffix.len()
            && host.ends_with(&self.suffix)
            && host.as_bytes()[host.len() - self.suffix.len() - 1] == b'.'
    }

    /// Longer patterns win; a host pattern beats a wildcard of the same host.
    fn specificity(&self) -> usize {
        self.suffix.len() * 2 + usize::from(!self.subdomains_only)
    }
}

/// One SaaS in the catalog, with its patterns compiled.
#[derive(Debug, Clone)]
pub struct SaasEntry {
    pub key: String,
    pub product: String,
    pub vendor: String,
    pub category: Category,
    pub ai: bool,
    web: Vec<HostPattern>,
    web_exclude: BTreeSet<String>,
    macos: BTreeSet<String>,
    windows: Option<Regex>,
    windows_store: Option<Regex>,
    /// The Sanka Research DNS rule family for the same product.
    pub research: Option<String>,
    /// The Sanka migration page slug (`/migrate/<slug>/`).
    pub migrate: Option<String>,
    /// The sanka-api migration connector that moves its data.
    pub connector: Option<String>,
}

impl SaasEntry {
    /// How specifically this entry claims `host` (a normalized host name),
    /// or `None` when it does not.
    fn host_match(&self, host: &str) -> Option<usize> {
        if self.web_exclude.contains(host) {
            return None;
        }
        self.web
            .iter()
            .filter(|pattern| pattern.matches(host))
            .map(HostPattern::specificity)
            .max()
    }

    fn matches_program(&self, item: &InstalledSoftware) -> bool {
        match item.source {
            SoftwareSource::MacosApp => self.macos.contains(&item.identifier),
            SoftwareSource::WindowsRegistry => self
                .windows
                .as_ref()
                .is_some_and(|pattern| pattern.is_match(&item.name)),
            SoftwareSource::WindowsStore => self
                .windows_store
                .as_ref()
                .is_some_and(|pattern| pattern.is_match(&item.identifier)),
        }
    }

    /// Whether the browser check can find this SaaS at all.
    pub fn has_web(&self) -> bool {
        !self.web.is_empty()
    }

    /// The Sanka migration page in `lang` (`en` or `ja`).
    pub fn migrate_url(&self, lang: &str) -> Option<String> {
        self.migrate.as_deref().map(|slug| migrate_url(slug, lang))
    }
}

/// `https://sanka.com/migrate/<slug>/`, or `/ja/migrate/<slug>/` in Japanese.
pub fn migrate_url(slug: &str, lang: &str) -> String {
    let prefix = if lang == "ja" { "/ja" } else { "" };
    format!("{MIGRATE_BASE_URL}{prefix}/migrate/{slug}/")
}

#[derive(Debug)]
pub struct CatalogError(String);

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SaaS catalog: {}", self.0)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFile {
    schema_version: u32,
    verified_on: NaiveDate,
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    key: String,
    product: String,
    vendor: String,
    category: Category,
    ai: bool,
    #[serde(default)]
    web: Vec<String>,
    #[serde(default)]
    web_exclude: Vec<String>,
    #[serde(default)]
    macos: Vec<String>,
    windows: Option<String>,
    windows_store: Option<String>,
    research: Option<String>,
    migrate: Option<String>,
    connector: Option<String>,
}

fn compile(pattern: Option<&str>, key: &str) -> Result<Option<Regex>, CatalogError> {
    pattern
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| {
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|err| CatalogError(format!("{key}: {err}")))
        })
        .transpose()
}

/// The SaaS catalog, in its file's order.
#[derive(Debug, Clone)]
pub struct SaasCatalog {
    entries: Vec<SaasEntry>,
    verified_on: NaiveDate,
}

impl SaasCatalog {
    /// The catalog bundled in this build.
    pub fn bundled() -> &'static SaasCatalog {
        static BUNDLED: OnceLock<SaasCatalog> = OnceLock::new();
        BUNDLED.get_or_init(|| {
            SaasCatalog::from_json(CATALOG_JSON).expect("the bundled SaaS catalog is valid")
        })
    }

    pub fn from_json(json: &str) -> Result<SaasCatalog, CatalogError> {
        let file: CatalogFile =
            serde_json::from_str(json).map_err(|err| CatalogError(err.to_string()))?;
        if file.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError(format!(
                "schema {} is not {CATALOG_SCHEMA_VERSION}",
                file.schema_version
            )));
        }
        let mut seen = BTreeSet::new();
        let mut entries = Vec::with_capacity(file.entries.len());
        for raw in file.entries {
            let key = raw.key.clone();
            let valid_key = !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !valid_key || !seen.insert(key.clone()) {
                return Err(CatalogError(format!("bad or duplicate key {key:?}")));
            }
            let web = raw
                .web
                .iter()
                .map(|pattern| {
                    HostPattern::parse(pattern)
                        .ok_or_else(|| CatalogError(format!("{key}: bad host pattern {pattern:?}")))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.push(SaasEntry {
                windows: compile(raw.windows.as_deref(), &key)?,
                windows_store: compile(raw.windows_store.as_deref(), &key)?,
                key,
                product: raw.product,
                vendor: raw.vendor,
                category: raw.category,
                ai: raw.ai,
                web,
                web_exclude: raw
                    .web_exclude
                    .iter()
                    .map(|host| host.trim().to_ascii_lowercase())
                    .collect(),
                macos: raw.macos.into_iter().collect(),
                research: raw.research,
                migrate: raw.migrate,
                connector: raw.connector,
            });
        }
        Ok(SaasCatalog {
            entries,
            verified_on: file.verified_on,
        })
    }

    pub fn entries(&self) -> &[SaasEntry] {
        &self.entries
    }

    pub fn get(&self, key: &str) -> Option<&SaasEntry> {
        self.entries.iter().find(|entry| entry.key == key)
    }

    /// When the catalog was last checked.
    pub fn verified_on(&self) -> NaiveDate {
        self.verified_on
    }

    /// The SaaS whose web app runs on `host` (any case, with or without a
    /// trailing dot): the most specific pattern wins, then catalog order.
    pub fn match_host(&self, host: &str) -> Option<&SaasEntry> {
        let host = normalize_host(host)?;
        let mut best: Option<(usize, &SaasEntry)> = None;
        for entry in &self.entries {
            if let Some(score) = entry.host_match(&host) {
                if best.is_none_or(|(top, _)| score > top) {
                    best = Some((score, entry));
                }
            }
        }
        best.map(|(_, entry)| entry)
    }

    /// The SaaS this installed program is the desktop client of.
    pub fn match_program(&self, item: &InstalledSoftware) -> Option<&SaasEntry> {
        self.entries
            .iter()
            .find(|entry| entry.matches_program(item))
    }
}

/// Lowercase, without a trailing dot; `None` for something that is not a
/// DNS host name (an IP address, an empty string).
pub fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let is_ip = host.parse::<std::net::Ipv4Addr>().is_ok() || host.contains(':');
    (host.contains('.') && !is_ip).then_some(host)
}

/// What the browser check keeps about one SaaS: the last day it was used
/// and on how many of the last 30 days (today included). Nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserUse {
    pub last_used_on: NaiveDate,
    pub days_used30: u32,
}

/// An installed desktop client of a SaaS, as the installed-software scan
/// names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApp {
    pub name: String,
    pub version: Option<String>,
}

/// One SaaS found on this PC: where it was found, whether the company
/// allows it, and how Sanka can migrate it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaasRecord {
    pub key: String,
    pub product: String,
    pub vendor: String,
    pub category: Category,
    pub ai: bool,
    /// Its desktop clients installed here.
    pub desktop_apps: Vec<DesktopApp>,
    /// From the browser check, when it is on and found the SaaS.
    pub browser: Option<BrowserUse>,
    /// `None` without a company allowlist.
    pub allowed: Option<bool>,
    pub migrate: Option<String>,
    pub connector: Option<String>,
}

/// Every SaaS found: from the installed desktop clients, and from the
/// browser check's results when there are any, marked against the
/// allowlist when there is one. Sorted by product name.
pub fn records(
    catalog: &SaasCatalog,
    software: &[InstalledSoftware],
    browser: Option<&BTreeMap<String, BrowserUse>>,
    allowlist: Option<&Allowlist>,
) -> Vec<SaasRecord> {
    let mut desktop: BTreeMap<&str, BTreeSet<DesktopApp>> = BTreeMap::new();
    for item in software {
        if let Some(entry) = catalog.match_program(item) {
            desktop
                .entry(entry.key.as_str())
                .or_default()
                .insert(DesktopApp {
                    name: item.name.clone(),
                    version: item.version.clone(),
                });
        }
    }
    let mut records: Vec<SaasRecord> = catalog
        .entries()
        .iter()
        .filter_map(|entry| {
            let apps = desktop.remove(entry.key.as_str()).unwrap_or_default();
            let browser = browser.and_then(|uses| uses.get(&entry.key)).copied();
            if apps.is_empty() && browser.is_none() {
                return None;
            }
            Some(SaasRecord {
                key: entry.key.clone(),
                product: entry.product.clone(),
                vendor: entry.vendor.clone(),
                category: entry.category,
                ai: entry.ai,
                desktop_apps: apps.into_iter().collect(),
                browser,
                allowed: allowlist.map(|list| list.allows(&entry.key)),
                migrate: entry.migrate.clone(),
                connector: entry.connector.clone(),
            })
        })
        .collect();
    records.sort_by(|a, b| {
        a.product
            .to_lowercase()
            .cmp(&b.product.to_lowercase())
            .then_with(|| a.key.cmp(&b.key))
    });
    records
}

impl SaasRecord {
    /// Its desktop apps' names, each once (two versions of one app are one
    /// name).
    pub fn desktop_app_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for app in &self.desktop_apps {
            if !names.contains(&app.name.as_str()) {
                names.push(&app.name);
            }
        }
        names
    }
}

/// The browser results the records carry, by SaaS key: what a later
/// re-marking against another allowlist starts from.
pub fn browser_uses(records: &[SaasRecord]) -> BTreeMap<String, BrowserUse> {
    records
        .iter()
        .filter_map(|record| record.browser.map(|used| (record.key.clone(), used)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn the_bundled_catalog_loads_and_every_entry_can_be_found() {
        let catalog = SaasCatalog::bundled();
        assert!(catalog.entries().len() > 100);
        for entry in catalog.entries() {
            assert!(
                entry.has_web()
                    || !entry.macos.is_empty()
                    || entry.windows.is_some()
                    || entry.windows_store.is_some(),
                "{} cannot be found",
                entry.key
            );
            if let Some(slug) = &entry.migrate {
                assert!(
                    slug.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-.".contains(c)),
                    "{slug}"
                );
            }
        }
    }

    #[test]
    fn hosts_match_the_most_specific_entry_and_never_the_vendors_own_site() {
        let catalog = SaasCatalog::bundled();
        let key = |host: &str| catalog.match_host(host).map(|entry| entry.key.as_str());
        for (host, expected) in [
            ("app.slack.com", Some("slack")),
            ("acme.slack.com", Some("slack")),
            ("ACME.Slack.com.", Some("slack")),
            ("slack.com", None),
            ("www.slack.com", None),
            ("chatgpt.com", Some("chatgpt")),
            ("platform.openai.com", Some("openai-platform")),
            ("gemini.google.com", Some("gemini")),
            ("mail.google.com", Some("google-workspace")),
            // Opened from other companies' links (a shared file, a form, a
            // meeting): never counted as this company's use.
            ("docs.google.com", None),
            ("meet.google.com", None),
            ("acme-my.sharepoint.com", None),
            ("teams.microsoft.com", None),
            ("vendor.zendesk.com", None),
            ("www.google.com", None),
            ("crm.zoho.jp", Some("zoho-crm")),
            ("mail.zoho.jp", Some("zoho")),
            ("acme.my.salesforce.com", Some("salesforce")),
            ("acme.lightning.force.com", Some("salesforce")),
            ("partner.my.site.com", None),
            ("help.salesforce.com", None),
            ("acme.cybozu.com", Some("cybozu-cloud")),
            ("outlook.office.com", Some("microsoft-365")),
            ("notslack.com", None),
            ("192.168.0.1", None),
            ("", None),
        ] {
            assert_eq!(key(host), expected, "{host}");
        }
    }

    #[test]
    fn desktop_clients_name_their_saas() {
        let catalog = SaasCatalog::bundled();
        let program = |source, identifier: &str, name: &str| {
            InstalledSoftware::new(source, identifier, name, Some("1.0"), None).unwrap()
        };
        for (item, expected) in [
            (
                program(
                    SoftwareSource::MacosApp,
                    "com.tinyspeck.slackmacgap",
                    "Slack",
                ),
                Some("slack"),
            ),
            (
                program(SoftwareSource::MacosApp, "us.zoom.xos", "zoom.us"),
                Some("zoom"),
            ),
            (
                program(
                    SoftwareSource::WindowsRegistry,
                    "{ABC}",
                    "Zoom Workplace (64-bit)",
                ),
                Some("zoom"),
            ),
            (
                program(
                    SoftwareSource::WindowsStore,
                    "MSTeams_8wekyb3d8bbwe",
                    "MSTeams",
                ),
                Some("microsoft-teams"),
            ),
            (
                program(SoftwareSource::WindowsRegistry, "Box", "Boxcryptor"),
                None,
            ),
            (
                program(SoftwareSource::WindowsRegistry, "{SLK}", "Slack Deployment"),
                Some("slack"),
            ),
            // Preinstalled on Windows 11: no sign that anyone uses it.
            (
                program(
                    SoftwareSource::WindowsStore,
                    "Microsoft.Copilot_8wekyb3d8bbwe",
                    "Microsoft.Copilot",
                ),
                None,
            ),
            (
                program(SoftwareSource::MacosApp, "com.example.slack", "Slack"),
                None,
            ),
        ] {
            assert_eq!(
                catalog.match_program(&item).map(|entry| entry.key.as_str()),
                expected,
                "{item:?}"
            );
        }
    }

    #[test]
    fn records_join_desktop_and_browser_and_mark_the_allowlist() {
        let catalog = SaasCatalog::bundled();
        let software = [
            InstalledSoftware::new(
                SoftwareSource::MacosApp,
                "com.tinyspeck.slackmacgap",
                "Slack",
                Some("4.41"),
                None,
            )
            .unwrap(),
            InstalledSoftware::new(
                SoftwareSource::MacosApp,
                "com.apple.Safari",
                "Safari",
                None,
                None,
            )
            .unwrap(),
        ];
        let browser = BTreeMap::from([
            (
                "slack".to_string(),
                BrowserUse {
                    last_used_on: day(2026, 10, 25),
                    days_used30: 12,
                },
            ),
            (
                "chatgpt".to_string(),
                BrowserUse {
                    last_used_on: day(2026, 10, 20),
                    days_used30: 3,
                },
            ),
        ]);
        let allowlist = Allowlist::parse(
            r#"{"schemaVersion": 1, "allowed": ["slack"]}"#,
            catalog,
            AllowlistOrigin::App,
        )
        .unwrap();
        let found = records(catalog, &software, Some(&browser), Some(&allowlist));
        let summary: Vec<(&str, bool, Option<u32>, Option<bool>)> = found
            .iter()
            .map(|r| {
                (
                    r.key.as_str(),
                    !r.desktop_apps.is_empty(),
                    r.browser.map(|b| b.days_used30),
                    r.allowed,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("chatgpt", false, Some(3), Some(false)),
                ("slack", true, Some(12), Some(true)),
            ]
        );
        // Without the browser check or an allowlist: desktop clients only, unmarked.
        let plain = records(catalog, &software, None, None);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].allowed, None);
        assert_eq!(browser_uses(&found), browser);
    }

    /// The privacy invariant as a test: what the browser check keeps, and a
    /// SaaS record, carry exactly these fields. A url, title or visit field
    /// must fail here first.
    #[test]
    fn records_have_no_room_for_web_addresses() {
        let keys = |value: serde_json::Value| -> Vec<String> {
            value.as_object().unwrap().keys().cloned().collect()
        };
        let used = BrowserUse {
            last_used_on: day(2026, 10, 25),
            days_used30: 1,
        };
        assert_eq!(
            keys(serde_json::to_value(used).unwrap()),
            ["daysUsed30", "lastUsedOn"]
        );
        let record = SaasRecord {
            key: "slack".into(),
            product: "Slack".into(),
            vendor: "Salesforce".into(),
            category: Category::Chat,
            ai: false,
            desktop_apps: vec![],
            browser: Some(used),
            allowed: None,
            migrate: None,
            connector: None,
        };
        assert_eq!(
            keys(serde_json::to_value(record).unwrap()),
            [
                "ai",
                "allowed",
                "browser",
                "category",
                "connector",
                "desktopApps",
                "key",
                "migrate",
                "product",
                "vendor"
            ]
        );
    }

    #[test]
    fn migration_pages_follow_the_language() {
        let salesforce = SaasCatalog::bundled().get("salesforce").unwrap();
        assert_eq!(
            salesforce.migrate_url("en").as_deref(),
            Some("https://sanka.com/migrate/salesforce/")
        );
        assert_eq!(
            salesforce.migrate_url("ja").as_deref(),
            Some("https://sanka.com/ja/migrate/salesforce/")
        );
        assert_eq!(salesforce.connector.as_deref(), Some("salesforce"));
    }
}
