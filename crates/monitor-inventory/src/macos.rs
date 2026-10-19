//! macOS: the `.app` bundles in the Applications folders.
//!
//! Reads one `Contents/Info.plist` per bundle and nothing else. No privacy
//! prompt is involved: the Applications folders are not protected, the scan
//! never follows a symlink (one could lead into Desktop, Documents or
//! Downloads, which are), and it does not call `system_profiler`, which
//! walks the whole disk.

use std::path::{Path, PathBuf};

use crate::{normalize, InstalledSoftware, SoftwareSource};

/// The item for one bundle, from its parsed `Contents/Info.plist`. A bundle
/// without a bundle id is left out: the id is how the workspace tells apps
/// apart.
pub fn app_from_info(info: &plist::Dictionary, bundle: &Path) -> Option<InstalledSoftware> {
    let text = |key: &str| {
        info.get(key)
            .and_then(plist::Value::as_string)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let identifier = text("CFBundleIdentifier")?;
    let stem = bundle
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let name = text("CFBundleDisplayName")
        .or_else(|| text("CFBundleName"))
        .unwrap_or(stem);
    let version = text("CFBundleShortVersionString").or_else(|| text("CFBundleVersion"));
    InstalledSoftware::new(SoftwareSource::MacosApp, identifier, name, version, None)
}

fn read_bundle(bundle: &Path) -> Option<InstalledSoftware> {
    let contents = bundle.join("Contents");
    let info_path = contents.join("Info.plist");
    if [&contents, &info_path].into_iter().any(|path| {
        std::fs::symlink_metadata(path).map_or(true, |meta| meta.file_type().is_symlink())
    }) {
        return None;
    }
    let info = plist::Value::from_file(info_path).ok()?.into_dictionary()?;
    app_from_info(&info, bundle)
}

/// The real (not symlinked), visible directories in `folder`, sorted, split
/// into `.app` bundles and plain folders.
fn bundles_and_folders(folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    if std::fs::symlink_metadata(folder).map_or(true, |meta| !meta.is_dir()) {
        return (Vec::new(), Vec::new());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        // `DirEntry::file_type` does not follow symlinks.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    entries.into_iter().partition(|path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
    })
}

/// The bundles directly in each folder, then those one plain subfolder down
/// (`/Applications/Microsoft Office 2011/Microsoft Word.app`,
/// `/Applications/Utilities/…`); never anything inside a bundle. Earlier
/// folders win when two hold the same bundle id.
pub fn scan_folders(folders: &[PathBuf]) -> Vec<InstalledSoftware> {
    let mut items = Vec::new();
    for folder in folders {
        let (bundles, subfolders) = bundles_and_folders(folder);
        items.extend(bundles.iter().filter_map(|bundle| read_bundle(bundle)));
        for subfolder in subfolders {
            let (inner, _) = bundles_and_folders(&subfolder);
            items.extend(inner.iter().filter_map(|bundle| read_bundle(bundle)));
        }
    }
    normalize(items)
}

/// `/Applications` (with `Utilities` and vendor folders), then
/// `~/Applications`.
#[cfg(target_os = "macos")]
pub fn collect() -> Vec<InstalledSoftware> {
    let mut folders = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(PathBuf::from(home).join("Applications"));
    }
    scan_folders(&folders)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(root: &Path, relative: &str, keys: &[(&str, &str)]) {
        let contents = root.join(relative).join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        let entries: String = keys
            .iter()
            .map(|(key, value)| format!("<key>{key}</key><string>{value}</string>"))
            .collect();
        std::fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>{entries}</dict></plist>"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn scans_applications_folders_one_level_deep_without_entering_bundles() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("monitor-inventory-{}-{nanos}", std::process::id()));
        let (system, user) = (base.join("Applications"), base.join("home/Applications"));
        let id = "CFBundleIdentifier";
        bundle(
            &system,
            "Zed.app",
            &[
                (id, "dev.zed.Zed"),
                ("CFBundleName", "Zed"),
                ("CFBundleShortVersionString", "0.150.4"),
                ("CFBundleVersion", "20240901"),
            ],
        );
        bundle(
            &system,
            "Microsoft Office 2011/Microsoft Word.app",
            &[
                (id, "com.microsoft.Word"),
                ("CFBundleDisplayName", "Microsoft Word"),
                ("CFBundleName", "Word"),
                ("CFBundleVersion", "14.7.7"),
            ],
        );
        bundle(
            &system,
            "Utilities/Helper Tool.app",
            &[(id, "com.example.helper")],
        );
        bundle(&system, "No Id.app", &[("CFBundleName", "No Id")]);
        bundle(
            &system,
            "Zed.app/Contents/Helpers/Inner.app",
            &[(id, "dev.zed.inner")],
        );
        bundle(
            &system,
            "Vendor/Suite/Too Deep.app",
            &[(id, "com.example.deep")],
        );
        bundle(&system, ".Hidden.app", &[(id, "com.example.hidden")]);
        bundle(
            &user,
            "Zed.app",
            &[(id, "dev.zed.Zed"), ("CFBundleShortVersionString", "0.140")],
        );
        bundle(
            &user,
            "Chrome Apps.localized/Docs.app",
            &[(id, "com.google.Chrome.app.docs"), ("CFBundleName", "Docs")],
        );
        #[cfg(unix)]
        {
            bundle(
                &base,
                "Downloads/Elsewhere.app",
                &[(id, "com.example.downloaded")],
            );
            let linked = system.join("Linked.app");
            std::fs::create_dir_all(&linked).unwrap();
            std::os::unix::fs::symlink(
                base.join("Downloads/Elsewhere.app/Contents"),
                linked.join("Contents"),
            )
            .unwrap();
            let linked_root = base.join("Applications-link");
            std::os::unix::fs::symlink(&system, &linked_root).unwrap();
            assert!(scan_folders(&[linked_root]).is_empty());
            std::os::unix::fs::symlink(
                base.join("Downloads/Elsewhere.app"),
                system.join("Elsewhere.app"),
            )
            .unwrap();
        }

        let items = scan_folders(&[system, user, base.join("missing")]);
        let summary: Vec<(&str, &str, Option<&str>)> = items
            .iter()
            .map(|item| {
                (
                    item.identifier.as_str(),
                    item.name.as_str(),
                    item.version.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("com.example.helper", "Helper Tool", None),
                ("com.google.Chrome.app.docs", "Docs", None),
                ("com.microsoft.Word", "Microsoft Word", Some("14.7.7")),
                ("dev.zed.Zed", "Zed", Some("0.150.4")),
            ]
        );
        assert!(items
            .iter()
            .all(|item| item.source == SoftwareSource::MacosApp && item.publisher.is_none()));
        std::fs::remove_dir_all(base).unwrap();
    }
}
