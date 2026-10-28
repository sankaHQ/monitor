//! One scan of this device: its facts, security checks and installed
//! software, read against the lifecycle catalog bundled in the app. A scan
//! reads the device and nothing else: it needs no account, writes nothing
//! and sends nothing. The local report, the command-line scan and connected
//! check-ins all collect through here.

use chrono::{DateTime, Local, Utc};
use monitor_findings::Catalog;
use monitor_posture::{PostureCheckId, PostureCheckResult};
use monitor_report::{DeviceRecord, ScanReport};
use serde::Serialize;

use crate::agent::AGENT_VERSION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub platform: String,
    pub os_version: String,
    /// Windows `EditionID`; `None` on macOS.
    pub os_edition: Option<String>,
    pub hostname: String,
    pub serial_number: Option<String>,
    pub device_model: Option<String>,
}

/// How long the window reuses the device's facts before reading them again.
const DEVICE_INFO_TTL: std::time::Duration = std::time::Duration::from_secs(600);

static DEVICE_INFO: std::sync::Mutex<Option<(std::time::Instant, DeviceInfo)>> =
    std::sync::Mutex::new(None);

/// Reads the device's facts and keeps them for [`device_info_cached`].
pub fn device_info() -> DeviceInfo {
    let info = collect_device_info();
    *DEVICE_INFO.lock().unwrap() = Some((std::time::Instant::now(), info.clone()));
    info
}

/// The device's facts for the window's status, read at most every ten
/// minutes: the probes (PowerShell on Windows) would otherwise run on every
/// status refresh. A scan or a check-in reads them fresh.
pub fn device_info_cached() -> DeviceInfo {
    if let Some((at, info)) = DEVICE_INFO.lock().unwrap().as_ref() {
        if at.elapsed() < DEVICE_INFO_TTL {
            return info.clone();
        }
    }
    device_info()
}

#[cfg(target_os = "macos")]
fn probe(cmd: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(cmd).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(target_os = "macos")]
pub fn collect_device_info() -> DeviceInfo {
    let serial = probe("ioreg", &["-rd1", "-c", "IOPlatformExpertDevice"]).and_then(|out| {
        out.lines()
            .find(|line| line.contains("IOPlatformSerialNumber"))
            .and_then(|line| line.split('"').nth(3))
            .map(|value| value.to_string())
    });
    DeviceInfo {
        platform: "macos".to_string(),
        os_version: probe("sw_vers", &["-productVersion"]).unwrap_or_else(|| "unknown".into()),
        os_edition: None,
        hostname: probe("scutil", &["--get", "ComputerName"])
            .or_else(|| probe("hostname", &[]))
            .unwrap_or_else(|| "unknown".into()),
        serial_number: serial,
        device_model: probe("sysctl", &["-n", "hw.model"]),
    }
}

#[cfg(target_os = "windows")]
pub fn collect_device_info() -> DeviceInfo {
    use monitor_posture::windows::collect::run_powershell;

    // Registry + CIM, user-level. ProductName still says "Windows 10" on
    // Windows 11, so the major version comes from the build number.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$nt = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$cs = Get-CimInstance -ClassName Win32_ComputerSystem
$bios = Get-CimInstance -ClassName Win32_BIOS
@{
  display = [string]$nt.DisplayVersion
  build = [string]$nt.CurrentBuild
  ubr = [string]$nt.UBR
  edition = [string]$nt.EditionID
  product = [string]$nt.ProductName
  hostname = [string]$env:COMPUTERNAME
  serial = [string]$bios.SerialNumber
  model = (("{0} {1}" -f $cs.Manufacturer, $cs.Model).Trim())
} | ConvertTo-Json -Compress
"#;
    let raw = run_powershell(SCRIPT, std::time::Duration::from_secs(30))
        .and_then(|out| serde_json::from_str::<serde_json::Value>(&out).ok());
    let field = |key: &str| {
        raw.as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let os_version = windows_os_version(
        field("display").as_deref(),
        field("build").as_deref().unwrap_or_default(),
        field("ubr").as_deref(),
        field("edition").as_deref(),
        field("product").as_deref(),
    );
    // OEM placeholders are not identities.
    let serial = field("serial").filter(|value| {
        let lower = value.to_ascii_lowercase();
        !(lower.contains("to be filled") || lower == "default string" || lower == "none")
    });
    DeviceInfo {
        platform: "windows".to_string(),
        os_version,
        os_edition: field("edition"),
        hostname: field("hostname")
            .or_else(|| std::env::var("COMPUTERNAME").ok())
            .unwrap_or_else(|| "unknown".into()),
        serial_number: serial,
        device_model: field("model"),
    }
}

/// The Windows version as the lifecycle catalog reads it:
/// `Windows 11 24H2 (26100.4351)`. ProductName still says "Windows 10" on
/// Windows 11, so a client's major version comes from the build number. A
/// server edition is named by its ProductName (`Windows Server 2025
/// Datacenter`), so it never passes for the Windows 11 release that shares
/// its build.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_os_version(
    display: Option<&str>,
    build: &str,
    ubr: Option<&str>,
    edition: Option<&str>,
    product: Option<&str>,
) -> String {
    let server = edition.is_some_and(|e| e.starts_with("Server"));
    let major = match (server, product, build.parse::<u32>().unwrap_or(0)) {
        (true, Some(product), _) => product,
        (true, None, _) => "Windows Server",
        (false, _, 0) => "Windows",
        (false, _, b) if b >= 22000 => "Windows 11",
        (false, _, _) => "Windows 10",
    };
    match (display, build.is_empty(), ubr) {
        (Some(display), false, Some(ubr)) => format!("{major} {display} ({build}.{ubr})"),
        (Some(display), _, _) => format!("{major} {display}"),
        (None, false, _) => format!("{major} build {build}"),
        _ => "unknown".to_string(),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn collect_device_info() -> DeviceInfo {
    DeviceInfo {
        platform: std::env::consts::OS.to_string(),
        os_version: "unknown".into(),
        os_edition: None,
        hostname: "unknown".into(),
        serial_number: None,
        device_model: None,
    }
}

#[cfg(target_os = "macos")]
pub fn collect_posture() -> Vec<PostureCheckResult> {
    use monitor_posture::macos::collect::MacosPostureSource;
    use monitor_posture::PostureSource;
    MacosPostureSource.collect()
}

#[cfg(target_os = "windows")]
pub fn collect_posture() -> Vec<PostureCheckResult> {
    use monitor_posture::windows::collect::WindowsPostureSource;
    use monitor_posture::PostureSource;
    WindowsPostureSource.collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn collect_posture() -> Vec<PostureCheckResult> {
    Vec::new()
}

/// True when the scan runs as root (macOS) or as the LocalSystem account
/// (Windows), as device management tools often start scripts.
pub fn running_as_system_account() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "0")
    }
    #[cfg(target_os = "windows")]
    {
        // LocalSystem's USERNAME is the computer account, `NAME$`.
        std::env::var("USERNAME").is_ok_and(|name| name.ends_with('$'))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Checks that describe the signed-in person, not the device. Run as root or
/// SYSTEM they would describe that account instead, so they become
/// "could not determine" rather than a misleading result.
const PERSON_CHECKS: [PostureCheckId; 2] = [PostureCheckId::LocalAdmin, PostureCheckId::ScreenLock];

pub fn without_person_checks(posture: Vec<PostureCheckResult>) -> Vec<PostureCheckResult> {
    posture
        .into_iter()
        .map(|check| {
            if PERSON_CHECKS.contains(&check.id) {
                PostureCheckResult {
                    pass: None,
                    detail: Some(
                        "Run as the signed-in user to check this (the scan ran as a system account)"
                            .into(),
                    ),
                    ..check
                }
            } else {
                check
            }
        })
        .collect()
}

/// Scans this device now. Blocking: the probes take a few seconds.
pub fn run_scan(now: DateTime<Utc>) -> ScanReport {
    let device = device_info();
    let mut posture = collect_posture();
    if running_as_system_account() {
        posture = without_person_checks(posture);
    }
    let software = monitor_inventory::collect();
    ScanReport::build(
        DeviceRecord {
            hostname: device.hostname,
            platform: device.platform,
            os_version: device.os_version,
            os_edition: device.os_edition,
            device_model: device.device_model,
            serial_number: device.serial_number,
        },
        posture,
        software,
        Catalog::bundled(),
        now.with_timezone(&Local).date_naive(),
        now,
        AGENT_VERSION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_account_scan_does_not_judge_the_person() {
        let at = Utc::now();
        let check = |id, pass| PostureCheckResult {
            id,
            pass: Some(pass),
            detail: None,
            collected_at: at,
        };
        let posture = without_person_checks(vec![
            check(PostureCheckId::LocalAdmin, false),
            check(PostureCheckId::ScreenLock, true),
            check(PostureCheckId::DiskEncryption, false),
        ]);
        let passes: Vec<Option<bool>> = posture.iter().map(|c| c.pass).collect();
        assert_eq!(passes, [None, None, Some(false)]);
    }

    #[test]
    fn windows_versions_read_as_the_catalog_expects() {
        let catalog = monitor_findings::Catalog::bundled();
        // (display, build, ubr, edition, product, version, catalog rule)
        for (display, build, ubr, edition, product, version, rule) in [
            (
                Some("24H2"),
                "26100",
                Some("4351"),
                Some("Professional"),
                Some("Windows 10 Pro"),
                "Windows 11 24H2 (26100.4351)",
                Some("windows-11-24h2"),
            ),
            (
                Some("22H2"),
                "19045",
                Some("6456"),
                Some("Core"),
                Some("Windows 10 Home"),
                "Windows 10 22H2 (19045.6456)",
                Some("windows-10"),
            ),
            // A server shares Windows 11's build but not its lifecycle.
            (
                Some("24H2"),
                "26100",
                Some("33438"),
                Some("ServerDatacenter"),
                Some("Windows Server 2025 Datacenter"),
                "Windows Server 2025 Datacenter 24H2 (26100.33438)",
                None,
            ),
            (None, "", None, None, None, "unknown", None),
        ] {
            let read = windows_os_version(display, build, ubr, edition, product);
            assert_eq!(read, version);
            assert_eq!(
                catalog
                    .match_os("windows", &read, edition)
                    .map(|r| r.rule_key.as_str()),
                rule,
                "{read}"
            );
        }
    }
}
