//! Windows: the "Installed apps" list, user-level.
//!
//! One PowerShell probe ([`INVENTORY_SCRIPT`]) reads the Uninstall registry
//! keys (machine-wide 64- and 32-bit, and the current user's) and the
//! current user's Microsoft Store packages, and prints them as JSON.
//! [`parse_windows_inventory_json`] decides what counts as installed
//! software and is pure, so the rules are unit-tested on every platform.

use serde_json::Value;

use crate::{normalize, InstalledSoftware, SoftwareSource};

/// The probe. Each section fails on its own (`$null`); the output is ASCII
/// only because `powershell.exe` writes redirected output in the OEM code
/// page, where a program name such as "ウイルスバスター" or "Intel® Graphics"
/// would arrive as invalid UTF-8 and lose the whole snapshot.
pub const INVENTORY_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$r = @{}
try {
  $paths = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
  )
  $r.registry = @(foreach ($path in $paths) {
    Get-ItemProperty -Path $path | ForEach-Object {
      [pscustomobject]@{
        key = [string]$_.PSChildName
        name = [string]$_.DisplayName
        version = [string]$_.DisplayVersion
        publisher = [string]$_.Publisher
        system = [string]$_.SystemComponent
        parent = [string]$_.ParentKeyName
        release = [string]$_.ReleaseType
      }
    }
  })
} catch { $r.registry = $null }
try {
  $r.store = @(Get-AppxPackage -ErrorAction Stop | Where-Object { -not $_.IsFramework -and $_.SignatureKind -eq 'Store' } | ForEach-Object {
    [pscustomobject]@{
      family = [string]$_.PackageFamilyName
      name = [string]$_.Name
      version = [string]$_.Version
      publisher = [string]$_.Publisher
    }
  })
} catch { $r.store = $null }
$json = $r | ConvertTo-Json -Compress -Depth 4
[regex]::Replace($json, '[^\x00-\x7F]', { param($m) '\u{0:x4}' -f [int][char]$m.Value })
"#;

/// Registry release types that are patches to another entry, not programs.
const PATCH_RELEASE_TYPES: [&str; 4] = ["update", "hotfix", "security update", "service pack"];

/// Display-name prefixes of Windows and Office patches that some installers
/// register without a release type.
const PATCH_NAME_PREFIXES: [&str; 2] = ["security update for", "update for microsoft"];

/// The section's entries: `ConvertTo-Json` writes a one-element list as a
/// bare object, and Windows PowerShell 5.1 sometimes wraps a list as
/// `{"value": [...], "Count": n}`.
fn entries(section: &Value) -> Vec<&Value> {
    match section {
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => match map.get("value") {
            Some(Value::Array(items)) => items.iter().collect(),
            _ => vec![section],
        },
        _ => Vec::new(),
    }
}

fn text<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Hidden from "Installed apps" (`SystemComponent = 1`), a child of another
/// entry (`ParentKeyName`), or a patch: none of these is a program.
fn is_listed_program(entry: &Value, name: &str) -> bool {
    let system_component = text(entry, "system").is_some_and(|value| value == "1");
    let has_parent = text(entry, "parent").is_some();
    let patch_release = text(entry, "release").is_some_and(|release| {
        PATCH_RELEASE_TYPES
            .iter()
            .any(|patch| release.eq_ignore_ascii_case(patch))
    });
    let lower = name.to_ascii_lowercase();
    let kb_article =
        lower.starts_with("kb") && lower[2..].starts_with(|c: char| c.is_ascii_digit());
    let patch_name = PATCH_NAME_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    !(system_component || has_parent || patch_release || kb_article || patch_name)
}

/// The `CN=` value of a package publisher's distinguished name
/// (`CN=Microsoft Corporation, O=Microsoft Corporation, …`); a quoted value
/// keeps its commas.
pub fn publisher_common_name(distinguished_name: &str) -> Option<String> {
    let mut parts = Vec::new();
    let (mut current, mut quoted) = (String::new(), false);
    for c in distinguished_name.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts.iter().find_map(|part| {
        let (key, value) = part.split_once('=')?;
        let value = value.trim();
        (key.trim().eq_ignore_ascii_case("cn") && !value.is_empty()).then(|| value.to_string())
    })
}

/// The probe's JSON → the snapshot. Without the registry section (the probe
/// failed or timed out) there is no snapshot at all: a partial one would
/// mark every program as removed. The Store section is optional — some
/// Windows editions have no Store, and its packages are the least of an
/// inventory.
pub fn parse_windows_inventory_json(json: &str) -> Vec<InstalledSoftware> {
    let raw: Value = serde_json::from_str(json.trim()).unwrap_or(Value::Null);
    let Some(registry) = raw.get("registry").filter(|section| !section.is_null()) else {
        return Vec::new();
    };
    let programs = entries(registry).into_iter().filter_map(|entry| {
        let name = text(entry, "name")?;
        if !is_listed_program(entry, name) {
            return None;
        }
        InstalledSoftware::new(
            SoftwareSource::WindowsRegistry,
            text(entry, "key")?,
            name,
            text(entry, "version"),
            text(entry, "publisher"),
        )
    });
    let packages = raw
        .get("store")
        .map(entries)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            InstalledSoftware::new(
                SoftwareSource::WindowsStore,
                text(entry, "family")?,
                text(entry, "name")?,
                text(entry, "version"),
                text(entry, "publisher")
                    .and_then(publisher_common_name)
                    .as_deref(),
            )
        });
    normalize(programs.chain(packages).collect::<Vec<_>>())
}

#[cfg(target_os = "windows")]
pub fn collect() -> Vec<InstalledSoftware> {
    use monitor_posture::windows::collect::run_powershell;
    run_powershell(INVENTORY_SCRIPT, std::time::Duration::from_secs(120))
        .map(|output| parse_windows_inventory_json(&output))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn program(key: &str, name: &str) -> Value {
        json!({"key": key, "name": name, "version": "", "publisher": "", "system": "", "parent": "", "release": ""})
    }

    fn with(mut entry: Value, field: &str, value: &str) -> Value {
        entry[field] = json!(value);
        entry
    }

    #[test]
    fn keeps_installed_programs_and_drops_components_and_patches() {
        let office = with(
            with(
                program(
                    "{90160000-0011-0000-1000-0000000FF1CE}",
                    " Microsoft Office Professional Plus 2016 ",
                ),
                "version",
                "16.0.4266.1001",
            ),
            "publisher",
            "Microsoft Corporation",
        );
        let registry = json!([
            office,
            program("Zoom", "Zoom Workplace"),
            // The 32-bit view of the same key: the first one stays.
            program("Zoom", "Zoom (x86)"),
            with(
                program("{runtime}", "Microsoft Visual C++ Additional Runtime"),
                "system",
                "1"
            ),
            with(
                program("KB2920678", "Update for Office 2016"),
                "parent",
                "Office16.PROPLUS"
            ),
            with(
                program("{sp1}", "Office Service Pack 1"),
                "release",
                "Service Pack"
            ),
            with(program("{hotfix}", "Some Hotfix"), "release", "hotfix"),
            program("{kb}", "KB5005565"),
            program(
                "{secupd}",
                "Security Update for Microsoft Excel 2016 (KB5002098)"
            ),
            program("{upd}", "Update for Microsoft OneDrive for Business"),
            program("{no-name}", "  "),
            program("  ", "Nameless key"),
            program("{kbd}", "Kbd Layout Tool"),
        ]);
        let store = json!({
            "family": "Microsoft.WindowsTerminal_8wekyb3d8bbwe",
            "name": "Microsoft.WindowsTerminal",
            "version": "1.21.2911.0",
            "publisher": "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US"
        });
        let items = parse_windows_inventory_json(
            &json!({"registry": registry, "store": store}).to_string(),
        );
        let item = |source, identifier, name, version, publisher| {
            InstalledSoftware::new(source, identifier, name, version, publisher).unwrap()
        };
        let microsoft = Some("Microsoft Corporation");
        assert_eq!(
            items,
            [
                item(
                    SoftwareSource::WindowsRegistry,
                    "Zoom",
                    "Zoom Workplace",
                    None,
                    None
                ),
                item(
                    SoftwareSource::WindowsRegistry,
                    "{90160000-0011-0000-1000-0000000FF1CE}",
                    "Microsoft Office Professional Plus 2016",
                    Some("16.0.4266.1001"),
                    microsoft,
                ),
                item(
                    SoftwareSource::WindowsRegistry,
                    "{kbd}",
                    "Kbd Layout Tool",
                    None,
                    None
                ),
                item(
                    SoftwareSource::WindowsStore,
                    "Microsoft.WindowsTerminal_8wekyb3d8bbwe",
                    "Microsoft.WindowsTerminal",
                    Some("1.21.2911.0"),
                    microsoft,
                ),
            ]
        );
    }

    #[test]
    fn probe_output_shapes() {
        let one = program("7-Zip", "7-Zip 24.08 (x64)");
        // (probe output, items)
        for (output, count) in [
            (json!({"registry": one.clone(), "store": null}).to_string(), 1),
            (json!({"registry": {"value": [one.clone()], "Count": 1}, "store": []}).to_string(), 1),
            (
                json!({"registry": [one.clone()], "store": [{"family": "A_1", "name": "A"}, {"family": "B_2", "name": "B"}]}).to_string(),
                3,
            ),
            // The probe failed before the registry: no snapshot.
            (json!({"registry": null, "store": [{"family": "A_1", "name": "A"}]}).to_string(), 0),
            (String::new(), 0),
            ("not json".to_string(), 0),
        ] {
            assert_eq!(parse_windows_inventory_json(&output).len(), count, "{output}");
        }
    }

    #[test]
    fn publisher_common_names() {
        for (dn, cn) in [
            (
                "CN=Microsoft Corporation, O=Microsoft Corporation, C=US",
                Some("Microsoft Corporation"),
            ),
            (
                "O=Example, CN=\"Example, Inc.\", C=JP",
                Some("Example, Inc."),
            ),
            (
                "CN=77D3B6E3-4E0E-4E96-89D9-3FC8C3AB1F7C",
                Some("77D3B6E3-4E0E-4E96-89D9-3FC8C3AB1F7C"),
            ),
            ("O=No Common Name", None),
            ("", None),
        ] {
            assert_eq!(publisher_common_name(dn).as_deref(), cn, "{dn}");
        }
    }
}
