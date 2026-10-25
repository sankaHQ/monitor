//! The browser check: the browsers' history files on this PC, read in place
//! and matched against the SaaS catalog by host name. It runs only when the
//! person has turned it on in the app; the command-line scan never calls it,
//! because device management runs that without the person.
//!
//! What it reads, and what it keeps:
//! - Chrome and Edge (`History`, every profile), Firefox (`places.sqlite`,
//!   every profile) and, on a Mac, Safari (`History.db`) when macOS lets
//!   the app read it. macOS allows that only with Full Disk Access, which
//!   the app never asks for; without it Safari is reported as not read;
//! - the visits of the last [`LOOKBACK_DAYS`] days made on this computer
//!   (Chrome and Safari mark the ones synced from other devices, which are
//!   left out), one row at a time and in memory only: the host name and the
//!   day of each visit. Firefox's file has the host already; the others' web
//!   address is cut down to its host as it is read, and the rest of it is
//!   dropped;
//! - per SaaS, the last day used and the number of days used in the last
//!   [`WINDOW_DAYS`] ([`BrowserUse`]). Hosts that are not in the catalog,
//!   and everything else, are gone when the scan returns.
//!
//! The files are opened read-only in SQLite's `immutable` mode and never
//! copied: the read takes no lock, so it cannot disturb a running browser,
//! and writes nothing, not even the `-wal` and `-shm` side files a normal
//! read-only open creates next to a closed browser's history. The cost: a
//! running browser's visits that are still in its write-ahead log (Firefox,
//! Safari) are read only after the browser moves them into the file, so the
//! last day used can trail by a day.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::{normalize_host, BrowserUse, SaasCatalog};

/// How far back the check reads: "last used" is within this many days.
pub const LOOKBACK_DAYS: i64 = 90;
/// "Days used" counts the days in this window, today included.
pub const WINDOW_DAYS: i64 = 30;

/// Seconds from 1601-01-01 (Chromium's epoch) to 1970-01-01.
const CHROMIUM_EPOCH_OFFSET_SECS: i64 = 11_644_473_600;
/// Seconds from 1970-01-01 to 2001-01-01 (Safari's epoch).
const SAFARI_EPOCH_OFFSET_SECS: i64 = 978_307_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Browser {
    Chrome,
    Edge,
    Firefox,
    Safari,
}

impl Browser {
    pub fn name(self) -> &'static str {
        match self {
            Browser::Chrome => "Chrome",
            Browser::Edge => "Edge",
            Browser::Firefox => "Firefox",
            Browser::Safari => "Safari",
        }
    }
}

/// What the check could do with one browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserStatus {
    /// Read, in `profiles` profiles.
    Read,
    /// No history of it on this PC.
    NotFound,
    /// Safari on a Mac without Full Disk Access for the app.
    NeedsFullDiskAccess,
    /// Found but could not be read.
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserResult {
    pub browser: Browser,
    pub status: BrowserStatus,
    /// Profiles read.
    pub profiles: u32,
}

/// One run of the browser check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserScan {
    pub browsers: Vec<BrowserResult>,
    /// By SaaS key.
    pub uses: BTreeMap<String, BrowserUse>,
}

/// The layout of a browser's history file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFormat {
    Chromium,
    Firefox,
    Safari,
}

/// One browser's history files on this PC.
#[derive(Debug, Clone)]
pub struct HistorySource {
    pub browser: Browser,
    pub format: HistoryFormat,
    pub files: Vec<PathBuf>,
    /// The OS refuses the app access (Safari without Full Disk Access).
    pub blocked: bool,
}

/// The real (not symlinked) directories in `dir`, sorted.
fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    dirs.sort();
    dirs
}

fn regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
}

/// A Chromium browser's profiles (`Default`, `Profile 1`, …) under its user
/// data folder; not the guest or system profiles.
pub fn chromium_source(browser: Browser, user_data: &Path) -> HistorySource {
    let files = subdirectories(user_data)
        .into_iter()
        .filter(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "Default" || name.starts_with("Profile "))
        })
        .map(|dir| dir.join("History"))
        .filter(|file| regular_file(file))
        .collect();
    HistorySource {
        browser,
        format: HistoryFormat::Chromium,
        files,
        blocked: false,
    }
}

/// Firefox's profiles under its `Profiles` folder.
pub fn firefox_source(profiles: &Path) -> HistorySource {
    let files = subdirectories(profiles)
        .into_iter()
        .map(|dir| dir.join("places.sqlite"))
        .filter(|file| regular_file(file))
        .collect();
    HistorySource {
        browser: Browser::Firefox,
        format: HistoryFormat::Firefox,
        files,
        blocked: false,
    }
}

/// Safari's history. macOS protects it: without Full Disk Access, opening
/// it fails with "operation not permitted" and no prompt appears.
pub fn safari_source(history_db: &Path) -> HistorySource {
    let (files, blocked) = match std::fs::File::open(history_db) {
        Ok(_) if regular_file(history_db) => (vec![history_db.to_path_buf()], false),
        Ok(_) => (Vec::new(), false),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => (Vec::new(), true),
        Err(_) => (Vec::new(), false),
    };
    HistorySource {
        browser: Browser::Safari,
        format: HistoryFormat::Safari,
        files,
        blocked,
    }
}

/// This PC's browsers, for the signed-in person.
pub fn sources() -> Vec<HistorySource> {
    #[cfg(target_os = "macos")]
    {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return Vec::new();
        };
        let support = home.join("Library").join("Application Support");
        vec![
            chromium_source(Browser::Chrome, &support.join("Google").join("Chrome")),
            chromium_source(Browser::Edge, &support.join("Microsoft Edge")),
            firefox_source(&support.join("Firefox").join("Profiles")),
            safari_source(&home.join("Library").join("Safari").join("History.db")),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let mut found = Vec::new();
        if let Some(local) = env("LOCALAPPDATA") {
            found.push(chromium_source(
                Browser::Chrome,
                &local.join("Google").join("Chrome").join("User Data"),
            ));
            found.push(chromium_source(
                Browser::Edge,
                &local.join("Microsoft").join("Edge").join("User Data"),
            ));
        }
        if let Some(roaming) = env("APPDATA") {
            found.push(firefox_source(
                &roaming.join("Mozilla").join("Firefox").join("Profiles"),
            ));
        }
        found
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// `path` as an SQLite URI (`file:///…`), percent-encoded.
fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// Opens a history file read-only in `immutable` mode: no lock, no side
/// files, nothing written.
fn open(path: &Path) -> rusqlite::Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn =
        Connection::open_with_flags(format!("{}?mode=ro&immutable=1", file_uri(path)), flags)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
    Ok(conn)
}

/// Whether `table` has `column` (browser versions differ).
fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")
        .and_then(|mut stmt| stmt.exists([table, column]))
        .unwrap_or(false)
}

/// The host of an `http` or `https` address, as written (the caller
/// normalizes it); `None` for any other kind of address.
pub fn host_of_url(url: &str) -> Option<&str> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")) {
        return None;
    }
    let rest = &url[scheme_end + 3..];
    let authority = &rest[..rest.find(['/', '?', '#', '\\']).unwrap_or(rest.len())];
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host_port.starts_with('[') {
        return None;
    }
    let host = host_port.split(':').next().unwrap_or_default();
    (!host.is_empty()).then_some(host)
}

/// Firefox's `rev_host` (`moc.kcals.emca.`) the right way round.
fn unreverse_host(rev_host: &str) -> String {
    rev_host
        .chars()
        .rev()
        .collect::<String>()
        .trim_start_matches('.')
        .to_string()
}

/// Calls `visit(host, unix_seconds)` for each visit made on this computer
/// since `since_unix`. Only the host and the time leave this function; a row
/// that cannot be read is skipped.
pub fn read_visits(
    format: HistoryFormat,
    path: &Path,
    since_unix: i64,
    mut visit: impl FnMut(&str, i64),
) -> rusqlite::Result<()> {
    let conn = open(path)?;
    match format {
        HistoryFormat::Chromium => {
            // Visits synced from another device name that device.
            let local_only = if has_column(&conn, "visits", "originator_cache_guid") {
                " AND (v.originator_cache_guid IS NULL OR v.originator_cache_guid = '')"
            } else {
                ""
            };
            let mut stmt = conn.prepare(&format!(
                "SELECT u.url, v.visit_time FROM visits v JOIN urls u ON u.id = v.url \
                 WHERE v.visit_time >= ?1{local_only}"
            ))?;
            let since = (since_unix + CHROMIUM_EPOCH_OFFSET_SECS) * 1_000_000;
            let mut rows = stmt.query([since])?;
            while let Some(row) = rows.next()? {
                let (Ok(url), Ok(micros)) = (row.get::<_, String>(0), row.get::<_, i64>(1)) else {
                    continue;
                };
                if let Some(host) = host_of_url(&url) {
                    visit(host, micros / 1_000_000 - CHROMIUM_EPOCH_OFFSET_SECS);
                }
            }
        }
        HistoryFormat::Firefox => {
            let mut stmt = conn.prepare(
                "SELECT p.rev_host, v.visit_date FROM moz_historyvisits v \
                 JOIN moz_places p ON p.id = v.place_id WHERE v.visit_date >= ?1",
            )?;
            let mut rows = stmt.query([since_unix * 1_000_000])?;
            while let Some(row) = rows.next()? {
                let (Ok(rev_host), Ok(micros)) = (row.get::<_, String>(0), row.get::<_, i64>(1))
                else {
                    continue;
                };
                visit(&unreverse_host(&rev_host), micros / 1_000_000);
            }
        }
        HistoryFormat::Safari => {
            // Visits synced through iCloud have a non-zero origin.
            let local_only = if has_column(&conn, "history_visits", "origin") {
                " AND v.origin = 0"
            } else {
                ""
            };
            let mut stmt = conn.prepare(&format!(
                "SELECT i.url, v.visit_time FROM history_visits v \
                 JOIN history_items i ON i.id = v.history_item WHERE v.visit_time >= ?1{local_only}"
            ))?;
            let since = (since_unix - SAFARI_EPOCH_OFFSET_SECS) as f64;
            let mut rows = stmt.query([since])?;
            while let Some(row) = rows.next()? {
                let (Ok(url), Ok(seconds)) = (row.get::<_, String>(0), row.get::<_, f64>(1)) else {
                    continue;
                };
                if let Some(host) = host_of_url(&url) {
                    visit(host, seconds as i64 + SAFARI_EPOCH_OFFSET_SECS);
                }
            }
        }
    }
    Ok(())
}

/// Per catalog entry: the days it was used in the lookback window.
struct Tally<'c> {
    catalog: &'c SaasCatalog,
    /// Host → catalog entry, so each distinct host is matched once.
    hosts: HashMap<String, Option<usize>>,
    days: BTreeMap<usize, BTreeSet<NaiveDate>>,
}

impl Tally<'_> {
    fn add(&mut self, host: &str, day: NaiveDate) {
        let Some(host) = normalize_host(host) else {
            return;
        };
        let catalog = self.catalog;
        let index = *self.hosts.entry(host).or_insert_with_key(|host| {
            catalog
                .match_host(host)
                .and_then(|entry| catalog.entries().iter().position(|e| e.key == entry.key))
        });
        if let Some(index) = index {
            self.days.entry(index).or_default().insert(day);
        }
    }
}

/// Runs the check over `sources`. `now_unix` and `today` are the scan's
/// time and the PC's local date; `local_day` turns a visit's time into the
/// PC's local date.
pub fn scan(
    catalog: &SaasCatalog,
    sources: &[HistorySource],
    now_unix: i64,
    today: NaiveDate,
    local_day: &dyn Fn(i64) -> Option<NaiveDate>,
) -> BrowserScan {
    let since_unix = now_unix - LOOKBACK_DAYS * 86_400;
    let window_start = today - chrono::Duration::days(WINDOW_DAYS - 1);
    let mut tally = Tally {
        catalog,
        hosts: HashMap::new(),
        days: BTreeMap::new(),
    };
    let mut browsers = Vec::new();
    for source in sources {
        let mut read = 0u32;
        for file in &source.files {
            let ok = read_visits(source.format, file, since_unix, |host, at| {
                if let Some(day) = local_day(at).filter(|day| *day <= today) {
                    tally.add(host, day);
                }
            })
            .is_ok();
            read += u32::from(ok);
        }
        let status = if source.blocked {
            BrowserStatus::NeedsFullDiskAccess
        } else if source.files.is_empty() {
            BrowserStatus::NotFound
        } else if read == 0 {
            BrowserStatus::Unreadable
        } else {
            BrowserStatus::Read
        };
        browsers.push(BrowserResult {
            browser: source.browser,
            status,
            profiles: read,
        });
    }
    let uses = tally
        .days
        .into_iter()
        .filter_map(|(index, days)| {
            let last_used_on = *days.last()?;
            let days_used30 = days.range(window_start..=today).count() as u32;
            Some((
                catalog.entries()[index].key.clone(),
                BrowserUse {
                    last_used_on,
                    days_used30,
                },
            ))
        })
        .collect();
    BrowserScan { browsers, uses }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(day: NaiveDate, hour: u32) -> i64 {
        Utc.from_utc_datetime(&day.and_hms_opt(hour, 0, 0).unwrap())
            .timestamp()
    }

    fn utc_day(unix: i64) -> Option<NaiveDate> {
        DateTime::<Utc>::from_timestamp(unix, 0).map(|t| t.date_naive())
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "monitor-history-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A history file in `format`, with these (address, unix time) visits.
    fn history_file(path: &Path, format: HistoryFormat, visits: &[(&str, i64)]) {
        let conn = Connection::open(path).unwrap();
        match format {
            HistoryFormat::Chromium => {
                conn.execute_batch(
                    "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT);
                     CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER,
                       originator_cache_guid TEXT NOT NULL DEFAULT '');",
                )
                .unwrap();
                for (n, (url, unix)) in visits.iter().enumerate() {
                    conn.execute(
                        "INSERT INTO urls (id, url, title) VALUES (?1, ?2, 'secret title')",
                        rusqlite::params![n as i64 + 1, url],
                    )
                    .unwrap();
                    conn.execute(
                        "INSERT INTO visits (url, visit_time) VALUES (?1, ?2)",
                        rusqlite::params![
                            n as i64 + 1,
                            (unix + CHROMIUM_EPOCH_OFFSET_SECS) * 1_000_000
                        ],
                    )
                    .unwrap();
                }
            }
            HistoryFormat::Firefox => {
                conn.execute_batch(
                    "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, rev_host TEXT);
                     CREATE TABLE moz_historyvisits (id INTEGER PRIMARY KEY, place_id INTEGER, visit_date INTEGER);
                     PRAGMA journal_mode=WAL;",
                )
                .unwrap();
                for (n, (url, unix)) in visits.iter().enumerate() {
                    let host = host_of_url(url).unwrap_or_default().to_ascii_lowercase();
                    let rev: String = format!(".{host}").chars().rev().collect();
                    conn.execute(
                        "INSERT INTO moz_places (id, url, title, rev_host) VALUES (?1, ?2, 'secret', ?3)",
                        rusqlite::params![n as i64 + 1, url, rev],
                    )
                    .unwrap();
                    conn.execute(
                        "INSERT INTO moz_historyvisits (place_id, visit_date) VALUES (?1, ?2)",
                        rusqlite::params![n as i64 + 1, unix * 1_000_000],
                    )
                    .unwrap();
                }
            }
            HistoryFormat::Safari => {
                conn.execute_batch(
                    "CREATE TABLE history_items (id INTEGER PRIMARY KEY, url TEXT);
                     CREATE TABLE history_visits (id INTEGER PRIMARY KEY, history_item INTEGER, visit_time REAL, title TEXT,
                       origin INTEGER NOT NULL DEFAULT 0);",
                )
                .unwrap();
                for (n, (url, unix)) in visits.iter().enumerate() {
                    conn.execute(
                        "INSERT INTO history_items (id, url) VALUES (?1, ?2)",
                        rusqlite::params![n as i64 + 1, url],
                    )
                    .unwrap();
                    conn.execute(
                        "INSERT INTO history_visits (history_item, visit_time, title) VALUES (?1, ?2, 'secret')",
                        rusqlite::params![n as i64 + 1, (unix - SAFARI_EPOCH_OFFSET_SECS) as f64],
                    )
                    .unwrap();
                }
            }
        }
    }

    #[test]
    fn addresses_are_cut_to_their_host() {
        for (url, host) in [
            (
                "https://app.slack.com/client/T1/C2?x=1",
                Some("app.slack.com"),
            ),
            (
                "HTTP://User:pw@Acme.Zendesk.com:8443/agent",
                Some("Acme.Zendesk.com"),
            ),
            ("https://chatgpt.com", Some("chatgpt.com")),
            ("https://chatgpt.com#frag", Some("chatgpt.com")),
            ("https://[2001:db8::1]/", None),
            ("file:///Users/taro/secret.pdf", None),
            ("chrome://settings", None),
            ("about:blank", None),
            ("https:///nohost", None),
        ] {
            assert_eq!(host_of_url(url), host, "{url}");
        }
        assert_eq!(unreverse_host("moc.kcals.emca."), "acme.slack.com");
    }

    #[test]
    fn uris_survive_spaces_and_unicode() {
        let uri = file_uri(Path::new("/Users/金 太郎/Library/A#b?.db"));
        assert!(uri.starts_with("file:///Users/"));
        assert!(!uri.contains(' ') && !uri.contains('#') && !uri.contains('?'));
        assert!(file_uri(Path::new(
            r"C:\Users\Taro\AppData\Local\Google\Chrome\User Data\Default\History"
        ))
        .starts_with("file:///C:/Users/Taro/"));
    }

    #[test]
    fn every_browser_format_yields_the_last_day_and_the_days_used() {
        let catalog = SaasCatalog::bundled();
        let today = day(2026, 10, 26);
        let now = at(today, 12);
        let dir = temp_dir("formats");
        // Chrome: Slack on three days (one of them twice), one 40 days ago,
        // and a site that is not SaaS.
        let chrome = dir.join("Chrome");
        std::fs::create_dir_all(chrome.join("Default")).unwrap();
        std::fs::create_dir_all(chrome.join("Profile 2")).unwrap();
        std::fs::create_dir_all(chrome.join("Guest Profile")).unwrap();
        history_file(
            &chrome.join("Default").join("History"),
            HistoryFormat::Chromium,
            &[
                ("https://app.slack.com/client/T1", at(day(2026, 10, 25), 9)),
                (
                    "https://app.slack.com/client/T1/C9",
                    at(day(2026, 10, 25), 15),
                ),
                (
                    "https://acme.slack.com/archives/C1",
                    at(day(2026, 10, 1), 9),
                ),
                ("https://app.slack.com/", at(day(2026, 9, 16), 9)),
                ("https://www.example.com/private", at(day(2026, 10, 25), 9)),
                // Outside the lookback: never read.
                ("https://chatgpt.com/c/1", at(day(2026, 6, 1), 9)),
            ],
        );
        history_file(
            &chrome.join("Profile 2").join("History"),
            HistoryFormat::Chromium,
            &[("https://chatgpt.com/c/2", at(day(2026, 10, 20), 9))],
        );
        // A guest profile is never read.
        history_file(
            &chrome.join("Guest Profile").join("History"),
            HistoryFormat::Chromium,
            &[("https://claude.ai/", at(day(2026, 10, 20), 9))],
        );
        // Firefox (WAL mode): Slack on another day.
        let firefox = dir.join("Profiles");
        std::fs::create_dir_all(firefox.join("abcd.default-release")).unwrap();
        history_file(
            &firefox.join("abcd.default-release").join("places.sqlite"),
            HistoryFormat::Firefox,
            &[("https://acme.slack.com/x", at(day(2026, 10, 26), 8))],
        );
        // Safari: HubSpot.
        let safari_db = dir.join("History.db");
        history_file(
            &safari_db,
            HistoryFormat::Safari,
            &[(
                "https://app.hubspot.com/contacts/1",
                at(day(2026, 10, 24), 10),
            )],
        );
        // Visits synced from other devices are not this computer's.
        let synced = Connection::open(chrome.join("Default").join("History")).unwrap();
        synced
            .execute_batch(&format!(
                "INSERT INTO urls (id, url) VALUES (100, 'https://claude.ai/new');
                 INSERT INTO visits (url, visit_time, originator_cache_guid) VALUES (100, {}, 'phone-guid');",
                (at(day(2026, 10, 25), 9) + CHROMIUM_EPOCH_OFFSET_SECS) * 1_000_000
            ))
            .unwrap();
        drop(synced);
        let synced = Connection::open(&safari_db).unwrap();
        synced
            .execute_batch(&format!(
                "INSERT INTO history_items (id, url) VALUES (100, 'https://app.asana.com/0/home');
                 INSERT INTO history_visits (history_item, visit_time, origin) VALUES (100, {}, 1);",
                (at(day(2026, 10, 25), 9) - SAFARI_EPOCH_OFFSET_SECS) as f64
            ))
            .unwrap();
        drop(synced);
        // Edge: present but broken.
        let edge = dir.join("Edge");
        std::fs::create_dir_all(edge.join("Default")).unwrap();
        std::fs::write(edge.join("Default").join("History"), b"not sqlite").unwrap();

        let sources = [
            chromium_source(Browser::Chrome, &chrome),
            chromium_source(Browser::Edge, &edge),
            firefox_source(&firefox),
            safari_source(&safari_db),
            chromium_source(Browser::Chrome, &dir.join("missing")),
        ];
        let result = scan(catalog, &sources, now, today, &utc_day);
        assert_eq!(
            result.uses,
            BTreeMap::from([
                (
                    "chatgpt".to_string(),
                    BrowserUse {
                        last_used_on: day(2026, 10, 20),
                        days_used30: 1
                    }
                ),
                (
                    "slack".to_string(),
                    BrowserUse {
                        last_used_on: day(2026, 10, 26),
                        // 10-26, 10-25 and 10-01; 09-16 is outside the 30 days.
                        days_used30: 3
                    }
                ),
                (
                    "hubspot".to_string(),
                    BrowserUse {
                        last_used_on: day(2026, 10, 24),
                        days_used30: 1
                    }
                ),
            ])
        );
        let statuses: Vec<(Browser, BrowserStatus, u32)> = result
            .browsers
            .iter()
            .map(|b| (b.browser, b.status, b.profiles))
            .collect();
        assert_eq!(
            statuses,
            [
                (Browser::Chrome, BrowserStatus::Read, 2),
                (Browser::Edge, BrowserStatus::Unreadable, 0),
                (Browser::Firefox, BrowserStatus::Read, 1),
                (Browser::Safari, BrowserStatus::Read, 1),
                (Browser::Chrome, BrowserStatus::NotFound, 0),
            ]
        );
        // Nothing was written next to the history files, not even the side
        // files a read-only open creates beside a WAL-mode file.
        let listing = |dir: PathBuf| -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        assert_eq!(listing(chrome.join("Default")), ["History"]);
        assert_eq!(
            listing(firefox.join("abcd.default-release")),
            ["places.sqlite"]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_locked_file_is_still_read_without_disturbing_the_browser() {
        let catalog = SaasCatalog::bundled();
        let today = day(2026, 10, 26);
        let dir = temp_dir("locked");
        let path = dir.join("History");
        history_file(
            &path,
            HistoryFormat::Chromium,
            &[("https://app.hubspot.com/contacts", at(today, 9))],
        );
        // The browser holds an exclusive lock, as Chrome does.
        let browser = Connection::open(&path).unwrap();
        browser
            .execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN EXCLUSIVE;")
            .unwrap();
        let source = HistorySource {
            browser: Browser::Chrome,
            format: HistoryFormat::Chromium,
            files: vec![path.clone()],
            blocked: false,
        };
        let result = scan(catalog, &[source], at(today, 12), today, &utc_day);
        assert_eq!(result.uses.get("hubspot").map(|u| u.days_used30), Some(1));
        browser.execute_batch("COMMIT;").unwrap();
        drop(browser);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_protected_safari_history_is_reported_not_read() {
        let source = HistorySource {
            browser: Browser::Safari,
            format: HistoryFormat::Safari,
            files: Vec::new(),
            blocked: true,
        };
        let result = scan(
            SaasCatalog::bundled(),
            &[source],
            at(day(2026, 10, 26), 12),
            day(2026, 10, 26),
            &utc_day,
        );
        assert_eq!(
            result.browsers[0].status,
            BrowserStatus::NeedsFullDiskAccess
        );
        assert!(result.uses.is_empty());
    }
}
