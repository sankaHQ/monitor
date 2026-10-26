//! The company allowlist: which SaaS the company allows.
//!
//! A small JSON file the company's IT team writes once and hands out:
//!
//! ```json
//! { "schemaVersion": 1, "name": "Example K.K.", "allowed": ["slack", "zoom", "Microsoft 365"] }
//! ```
//!
//! Each `allowed` entry is a catalog key (`sanka-monitor-agent saas-catalog`
//! lists them) or a product name, in any case. Entries the catalog does not
//! know are kept aside and shown, so a typo is visible rather than silent.
//! SaaS in use that are not on the list are flagged.
//!
//! Where a scan finds it, first match wins: the command line's
//! `--allowlist <file>`, then the machine-wide file that device management
//! can place ([`machine_path`]), then, on a connected PC, the list the
//! workspace sets in its console (sent with each check-in), then a file the
//! person chose in the app.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SaasCatalog;

pub const ALLOWLIST_SCHEMA_VERSION: u32 = 1;
/// An allowlist is a few kilobytes; anything far bigger is not one.
pub const MAX_ALLOWLIST_BYTES: u64 = 1024 * 1024;
pub const ALLOWLIST_FILE_NAME: &str = "allowlist.json";

/// Where the allowlist came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowlistOrigin {
    /// `--allowlist <file>` on the command line.
    CommandLine,
    /// The machine-wide file device management places.
    Machine,
    /// The workspace's list, set in its console and sent with each check-in.
    Workspace,
    /// A file the person chose in the app.
    App,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowlist {
    pub name: Option<String>,
    /// Catalog keys.
    pub allowed: BTreeSet<String>,
    /// Entries the catalog does not know, as written.
    pub unknown: Vec<String>,
    pub origin: AllowlistOrigin,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AllowlistFile {
    schema_version: u32,
    name: Option<String>,
    allowed: Vec<String>,
}

impl Allowlist {
    pub fn parse(
        json: &str,
        catalog: &SaasCatalog,
        origin: AllowlistOrigin,
    ) -> Result<Allowlist, String> {
        let file: AllowlistFile = serde_json::from_str(json.trim_start_matches('\u{feff}'))
            .map_err(|err| format!("not a Sanka Monitor allowlist: {err}"))?;
        if file.schema_version != ALLOWLIST_SCHEMA_VERSION {
            return Err(format!(
                "allowlist schema {} is not {ALLOWLIST_SCHEMA_VERSION}",
                file.schema_version
            ));
        }
        let mut allowed = BTreeSet::new();
        let mut unknown = Vec::new();
        for written in file.allowed {
            let wanted = written.trim();
            if wanted.is_empty() {
                continue;
            }
            let entry = catalog.entries().iter().find(|entry| {
                entry.key.eq_ignore_ascii_case(wanted)
                    || entry.product.to_lowercase() == wanted.to_lowercase()
            });
            match entry {
                Some(entry) => {
                    allowed.insert(entry.key.clone());
                }
                None if !unknown.iter().any(|u: &String| u == wanted) => {
                    unknown.push(wanted.to_string())
                }
                None => {}
            }
        }
        Ok(Allowlist {
            name: file
                .name
                .map(|name| name.trim().chars().take(200).collect::<String>())
                .filter(|name| !name.is_empty()),
            allowed,
            unknown,
            origin,
        })
    }

    /// A list given as catalog keys (the workspace console's). Keys the
    /// catalog does not know are kept aside, as in a file.
    pub fn from_keys(
        name: Option<String>,
        keys: &[String],
        catalog: &SaasCatalog,
        origin: AllowlistOrigin,
    ) -> Allowlist {
        let mut allowed = BTreeSet::new();
        let mut unknown = Vec::new();
        for key in keys {
            match catalog.get(key.trim()) {
                Some(entry) => {
                    allowed.insert(entry.key.clone());
                }
                None => unknown.push(key.clone()),
            }
        }
        Allowlist {
            name: name
                .map(|name| name.trim().chars().take(200).collect::<String>())
                .filter(|name| !name.is_empty()),
            allowed,
            unknown,
            origin,
        }
    }

    pub fn read(
        path: &Path,
        catalog: &SaasCatalog,
        origin: AllowlistOrigin,
    ) -> Result<Allowlist, String> {
        let size = std::fs::metadata(path)
            .map_err(|err| format!("cannot read {}: {err}", path.display()))?
            .len();
        if size > MAX_ALLOWLIST_BYTES {
            return Err(format!("{} is too large for an allowlist", path.display()));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
        Allowlist::parse(&text, catalog, origin)
    }

    pub fn allows(&self, key: &str) -> bool {
        self.allowed.contains(key)
    }
}

/// The machine-wide allowlist that device management can place:
/// `/Library/Application Support/Sanka Monitor/allowlist.json` on macOS,
/// `%ProgramData%\Sanka Monitor\allowlist.json` on Windows.
pub fn machine_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from("/Library/Application Support/Sanka Monitor").join(ALLOWLIST_FILE_NAME))
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        Some(base.join("Sanka Monitor").join(ALLOWLIST_FILE_NAME))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// The allowlist a scan uses: the first of `candidates` that exists. A file
/// that exists but does not parse is an error, not a fallback: a broken
/// company list must not silently become "no list".
pub fn find(
    candidates: &[(PathBuf, AllowlistOrigin)],
    catalog: &SaasCatalog,
) -> Option<Result<Allowlist, String>> {
    candidates
        .iter()
        .find(|(path, _)| path.exists())
        .map(|(path, origin)| Allowlist::read(path, catalog, *origin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_keys_or_product_names_and_typos_are_kept_visible() {
        let catalog = SaasCatalog::bundled();
        let list = Allowlist::parse(
            "\u{feff}{\"schemaVersion\": 1, \"name\": \" Example K.K. \", \"allowed\": [\"SLACK\", \"Microsoft 365\", \"zooom\", \"zooom\", \" \"]}",
            catalog,
            AllowlistOrigin::Machine,
        )
        .unwrap();
        assert_eq!(list.name.as_deref(), Some("Example K.K."));
        assert_eq!(
            list.allowed.iter().map(String::as_str).collect::<Vec<_>>(),
            ["microsoft-365", "slack"]
        );
        assert_eq!(list.unknown, ["zooom"]);
        assert!(list.allows("slack") && !list.allows("chatgpt"));
    }

    #[test]
    fn a_broken_file_is_an_error_never_an_empty_list() {
        let catalog = SaasCatalog::bundled();
        for bad in [
            "",
            "[\"slack\"]",
            "{\"allowed\": [\"slack\"]}",
            "{\"schemaVersion\": 2, \"allowed\": []}",
            "{\"schemaVersion\": 1, \"allowed\": [], \"blocked\": []}",
        ] {
            assert!(
                Allowlist::parse(bad, catalog, AllowlistOrigin::App).is_err(),
                "{bad}"
            );
        }
        let dir = std::env::temp_dir().join(format!("monitor-allowlist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.json");
        let good = dir.join("good.json");
        std::fs::write(&broken, "{").unwrap();
        std::fs::write(&good, r#"{"schemaVersion": 1, "allowed": ["zoom"]}"#).unwrap();
        let missing = dir.join("missing.json");
        let found = find(
            &[
                (missing.clone(), AllowlistOrigin::CommandLine),
                (good.clone(), AllowlistOrigin::Machine),
            ],
            catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.origin, AllowlistOrigin::Machine);
        assert!(find(
            &[
                (broken, AllowlistOrigin::Machine),
                (good, AllowlistOrigin::App)
            ],
            catalog
        )
        .unwrap()
        .is_err());
        assert!(find(&[(missing, AllowlistOrigin::App)], catalog).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
