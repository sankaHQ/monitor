//! Installed-software inventory.
//!
//! What the device has installed, as the workspace's IT team would see it in
//! Finder's Applications folder or in Windows' "Installed apps": a source, an
//! identifier, a name, a version and (on Windows) a publisher. Nothing else:
//! [`InstalledSoftware`] has no fields for install locations, file names,
//! file contents, user names or usage, and none may be added.
//!
//! Collection is user-level like the posture checks — no admin rights and no
//! macOS privacy prompt. The pure parts (bundle scanning over given folders,
//! the Windows probe parser, normalization, the fingerprint) are testable on
//! every platform; [`collect`] picks the platform collector.
//!
//! The serialized shape is the sanka-api `/monitor/agent/software` item:
//! camelCase keys and snake_case sources.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod macos;
pub mod windows;

/// Contract limits (characters, as the API counts them).
pub const MAX_ITEMS: usize = 5000;
pub const MAX_IDENTIFIER_CHARS: usize = 300;
pub const MAX_NAME_CHARS: usize = 300;
pub const MAX_VERSION_CHARS: usize = 100;
pub const MAX_PUBLISHER_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareSource {
    /// An `.app` bundle in an Applications folder (identifier: bundle id).
    MacosApp,
    /// A Windows "Uninstall" registry entry (identifier: its key name).
    WindowsRegistry,
    /// A Microsoft Store package (identifier: package family name).
    WindowsStore,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSoftware {
    pub source: SoftwareSource,
    pub identifier: String,
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
}

impl InstalledSoftware {
    /// Builds an item within the contract limits: fields are trimmed and
    /// truncated, empty optional fields become `None`, and an item without an
    /// identifier or a name is no item.
    pub fn new(
        source: SoftwareSource,
        identifier: &str,
        name: &str,
        version: Option<&str>,
        publisher: Option<&str>,
    ) -> Option<Self> {
        Some(Self {
            source,
            identifier: clean(identifier, MAX_IDENTIFIER_CHARS)?,
            name: clean(name, MAX_NAME_CHARS)?,
            version: version.and_then(|v| clean(v, MAX_VERSION_CHARS)),
            publisher: publisher.and_then(|p| clean(p, MAX_PUBLISHER_CHARS)),
        })
    }
}

/// Trims, then keeps at most `max` characters; `None` when nothing is left.
fn clean(value: &str, max: usize) -> Option<String> {
    let kept: String = value.trim().chars().take(max).collect();
    let kept = kept.trim_end();
    (!kept.is_empty()).then(|| kept.to_string())
}

/// One snapshot as it is uploaded: the first entry per (source, identifier)
/// wins — collectors list the preferred location first — then the snapshot
/// is sorted, so it reads the same in the Transparency pane every time, and
/// capped at the contract's item limit.
pub fn normalize(items: impl IntoIterator<Item = InstalledSoftware>) -> Vec<InstalledSoftware> {
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<InstalledSoftware> = items
        .into_iter()
        .filter(|item| seen.insert((item.source, item.identifier.clone())))
        .collect();
    unique.sort();
    unique.truncate(MAX_ITEMS);
    unique
}

/// SHA-256 (hex) over the sorted snapshot: the agent re-uploads when it
/// changes, whatever order the collector found the items in.
pub fn fingerprint(items: &[InstalledSoftware]) -> String {
    let mut sorted: Vec<&InstalledSoftware> = items.iter().collect();
    sorted.sort();
    let bytes = serde_json::to_vec(&sorted).unwrap_or_default();
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// This device's installed software. Empty when the platform has no
/// collector or its probe failed — an empty list is never a snapshot (it
/// would mark everything the workspace knows as removed).
pub fn collect() -> Vec<InstalledSoftware> {
    #[cfg(target_os = "macos")]
    {
        macos::collect()
    }
    #[cfg(target_os = "windows")]
    {
        windows::collect()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(identifier: &str, name: &str, version: Option<&str>) -> InstalledSoftware {
        InstalledSoftware::new(SoftwareSource::MacosApp, identifier, name, version, None).unwrap()
    }

    #[test]
    fn items_are_trimmed_to_the_contract_limits() {
        let long = "x".repeat(400);
        let item = InstalledSoftware::new(
            SoftwareSource::WindowsRegistry,
            &format!("  {long}"),
            "日本語のアプリ ",
            Some(&"9".repeat(150)),
            Some("   "),
        )
        .unwrap();
        assert_eq!(item.identifier.chars().count(), MAX_IDENTIFIER_CHARS);
        assert_eq!(item.name, "日本語のアプリ");
        assert_eq!(item.version.unwrap().len(), MAX_VERSION_CHARS);
        assert_eq!(item.publisher, None);
        for (identifier, name) in [("", "Name"), ("id", " \t"), ("  ", "")] {
            assert_eq!(
                InstalledSoftware::new(SoftwareSource::MacosApp, identifier, name, None, None),
                None
            );
        }
    }

    #[test]
    fn normalize_keeps_the_first_entry_per_identifier_and_sorts() {
        let items = normalize([
            app("com.b", "B (Applications)", Some("2")),
            app("com.a", "A", None),
            app("com.b", "B (~/Applications)", Some("1")),
        ]);
        let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, ["A", "B (Applications)"]);
        let many =
            normalize((0..MAX_ITEMS + 10).map(|n| app(&format!("com.app{n:05}"), "App", None)));
        assert_eq!(many.len(), MAX_ITEMS);
    }

    #[test]
    fn fingerprint_follows_content_not_order() {
        let a = app("com.a", "A", Some("1.0"));
        let b = app("com.b", "B", None);
        let base = fingerprint(&[a.clone(), b.clone()]);
        assert_eq!(base.len(), 64);
        assert_eq!(fingerprint(&[b.clone(), a.clone()]), base);
        let upgraded = app("com.a", "A", Some("1.1"));
        assert_ne!(fingerprint(&[upgraded, b.clone()]), base);
        assert_ne!(fingerprint(&[a]), base);
    }
}
