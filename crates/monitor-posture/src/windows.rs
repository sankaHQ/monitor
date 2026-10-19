//! Windows posture checks — user-level, no admin rights.
//!
//! One PowerShell probe (see [`POSTURE_SCRIPT`]) gathers every raw signal as
//! JSON; [`parse_posture_json`] turns it into the contract checks and is
//! pure, so the mapping is unit-tested on every platform. Each check reads
//! only the OS setting it names:
//!
//! - disk encryption: the system drive's `System.Volume.BitLockerProtection`
//!   shell property (the same value Explorer uses for its lock icon; works
//!   without admin, unlike `manage-bde`)
//! - firewall: `Get-NetFirewallProfile` — every profile must be on
//! - "gatekeeper": Windows SmartScreen for apps and files (policy first,
//!   then the Explorer setting)
//! - screen lock: any of — an inactivity-lock policy, a secure screensaver,
//!   or "require sign-in on wake" with a sleep timeout — within 15 minutes
//! - OS update: pending software updates from the Windows Update cache
//!   (offline search, no network) plus the OS build for the detail line
//! - automatic updates: the `NoAutoUpdate` machine policy and the Windows
//!   Update service's start type
//! - antivirus: Security Center's registered products (`productState`)
//! - local admin: the signed-in user's token holds `BUILTIN\Administrators`
//!   (`S-1-5-32-544`), even as "deny only" under UAC — the user can elevate
//! - device management: an MDM enrollment (Intune) in the registry, else
//!   an Active Directory domain join
//! - Secure Boot: `SecureBoot\State\UEFISecureBootEnabled`
//!
//! TPM is not checked: `Get-Tpm` and `Win32_Tpm` need admin rights.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{PostureCheckId, PostureCheckResult};

/// Lock must engage within this many seconds of inactivity to pass.
pub const SCREEN_LOCK_MAX_SECS: u64 = 900;

/// The probe. `$ErrorActionPreference` keeps a single failing signal from
/// taking the others down; every key is optional on the parsing side. The
/// output is ASCII only: `powershell.exe` writes redirected output in the
/// OEM code page, where a localized product or group name (Japanese
/// Windows, "ウイルスバスター") would arrive as invalid UTF-8 and lose every
/// signal.
pub const POSTURE_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$r = @{}
try {
  $shell = New-Object -ComObject Shell.Application
  $r.bitlocker = $shell.NameSpace($env:SystemDrive).Self.ExtendedProperty('System.Volume.BitLockerProtection')
} catch { $r.bitlocker = $null }
try { $r.firewall = @(Get-NetFirewallProfile | ForEach-Object { [bool]$_.Enabled }) } catch { $r.firewall = $null }
try { $r.smartscreen = [string](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer' -Name SmartScreenEnabled).SmartScreenEnabled } catch { $r.smartscreen = $null }
try { $r.smartscreen_policy = (Get-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\System' -Name EnableSmartScreen).EnableSmartScreen } catch { $r.smartscreen_policy = $null }
try {
  $d = Get-ItemProperty 'HKCU:\Control Panel\Desktop'
  $r.screensaver = @{ active = [string]$d.ScreenSaveActive; secure = [string]$d.ScreenSaverIsSecure; timeout = [string]$d.ScreenSaveTimeOut }
} catch { $r.screensaver = $null }
try { $r.inactivity = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' -Name InactivityTimeoutSecs).InactivityTimeoutSecs } catch { $r.inactivity = $null }
try { $r.consolelock = (powercfg /q SCHEME_CURRENT SUB_NONE CONSOLELOCK) -join "`n" } catch { $r.consolelock = $null }
try { $r.standby = (powercfg /q SCHEME_CURRENT SUB_SLEEP STANDBYIDLE) -join "`n" } catch { $r.standby = $null }
try {
  $nt = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
  $r.os = @{ display = [string]$nt.DisplayVersion; build = [string]$nt.CurrentBuild; ubr = [string]$nt.UBR }
} catch { $r.os = $null }
try {
  $s = (New-Object -ComObject Microsoft.Update.Session).CreateUpdateSearcher()
  $s.Online = $false
  $r.pending_updates = $s.Search("IsInstalled=0 and Type='Software' and IsHidden=0").Updates.Count
} catch { $r.pending_updates = $null }
try { $r.wu_policy = (Get-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU' -Name NoAutoUpdate).NoAutoUpdate } catch { $r.wu_policy = $null }
try { $r.wu_service = [string](Get-Service -Name wuauserv -ErrorAction Stop).StartType } catch { $r.wu_service = $null }
try {
  $r.antivirus = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct -ErrorAction Stop | ForEach-Object { @{ name = [string]$_.displayName; state = [int64]$_.productState } })
} catch { $r.antivirus = $null }
try { $r.groups = (whoami /groups /fo csv /nh) -join ' ' } catch { $r.groups = $null }
try {
  $enrollments = 'HKLM:\SOFTWARE\Microsoft\Enrollments'
  if (Test-Path $enrollments) {
    $r.mdm = @(Get-ChildItem $enrollments -ErrorAction Stop | ForEach-Object { Get-ItemProperty $_.PSPath } | Where-Object { $_.EnrollmentState -eq 1 -and $_.ProviderID } | ForEach-Object { [string]$_.ProviderID })
  } else { $r.mdm = @() }
} catch { $r.mdm = $null }
try { $r.domain_joined = [bool](Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop).PartOfDomain } catch { $r.domain_joined = $null }
try { $r.secure_boot = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\State' -Name UEFISecureBootEnabled -ErrorAction Stop).UEFISecureBootEnabled } catch { $r.secure_boot = $null }
$json = $r | ConvertTo-Json -Compress -Depth 4
[regex]::Replace($json, '[^\x00-\x7F]', { param($m) '\u{0:x4}' -f [int][char]$m.Value })
"#;

/// Reads a JSON scalar as an integer: numbers, numeric strings, and
/// booleans (true = 1). PowerShell's `ConvertTo-Json` is loose about which.
pub fn as_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

/// Reads a JSON scalar as a boolean, accepting `true`, `1`, `"1"`, `"True"`.
pub fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// `powercfg /q` prints the current setting as
/// `Current AC Power Setting Index: 0x0000003c`; this returns that value.
pub fn parse_powercfg_ac_index(output: &str) -> Option<u64> {
    output
        .lines()
        .find(|line| line.contains("Current AC Power Setting Index"))
        .and_then(|line| line.rsplit(':').next())
        .map(str::trim)
        .and_then(|hex| u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
}

/// `System.Volume.BitLockerProtection`: 1 on, 2 off, 3 encrypting,
/// 4 decrypting, 5 suspended, 6 locked. Returns (pass, detail).
pub fn bitlocker_state(code: Option<i64>) -> (Option<bool>, Option<String>) {
    match code {
        Some(1) | Some(6) => (Some(true), None),
        Some(2) => (Some(false), Some("BitLocker is off".into())),
        Some(3) => (Some(false), Some("BitLocker is still encrypting".into())),
        Some(4) => (Some(false), Some("BitLocker is decrypting".into())),
        Some(5) => (
            Some(false),
            Some("BitLocker protection is suspended".into()),
        ),
        _ => (None, Some("BitLocker status unavailable".into())),
    }
}

/// SmartScreen: the machine policy wins, then the Explorer setting
/// (`Warn` / `Block` / `RequireAdmin` / `Prompt` = on, `Off` = off).
pub fn smartscreen_state(policy: Option<i64>, setting: Option<&str>) -> Option<bool> {
    match policy {
        Some(0) => return Some(false),
        Some(_) => return Some(true),
        None => {}
    }
    match setting.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("off") => Some(false),
        Some("warn") | Some("block") | Some("requireadmin") | Some("prompt") => Some(true),
        _ => None,
    }
}

/// A list in the probe's JSON: `ConvertTo-Json` writes a one-element list as
/// the bare element, and Windows PowerShell 5.1 sometimes wraps a list as
/// `{"value": [...], "Count": n}`.
fn list(value: &Value) -> Vec<&Value> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter().collect(),
        Value::Object(map) => match map.get("value") {
            Some(Value::Array(items)) => items.iter().collect(),
            _ => vec![value],
        },
        other => vec![other],
    }
}

/// Windows Update installs updates on its own unless a machine policy turns
/// automatic updates off (`NoAutoUpdate = 1`) or its service is disabled.
pub fn automatic_updates_state(
    no_auto_update_policy: Option<i64>,
    service_start: Option<&str>,
) -> (Option<bool>, Option<String>) {
    if no_auto_update_policy == Some(1) {
        return (
            Some(false),
            Some("Automatic updates are turned off by policy".into()),
        );
    }
    match service_start
        .map(str::trim)
        .filter(|start| !start.is_empty())
    {
        Some(start) if start.eq_ignore_ascii_case("disabled") => (
            Some(false),
            Some("The Windows Update service is disabled".into()),
        ),
        Some(_) => (Some(true), None),
        None => (None, Some("Windows Update settings unavailable".into())),
    }
}

/// One product registered with Windows Security Center.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntivirusProduct {
    pub name: String,
    pub product_state: i64,
}

impl AntivirusProduct {
    /// `productState` read as six hex digits: digits 2–3 are the scanner
    /// (`10` or `11` = on), digits 4–5 the definitions (`00` = up to date).
    /// Returns (on, definitions up to date).
    pub fn decode(&self) -> (bool, bool) {
        let digits = format!("{:06x}", self.product_state & 0xFF_FFFF);
        (matches!(&digits[2..4], "10" | "11"), &digits[4..6] == "00")
    }
}

/// Passes when any registered product is on; the detail names it and notes
/// out-of-date definitions. `None` = Security Center was unavailable (it is
/// absent on Windows Server).
pub fn antivirus_state(products: Option<&[AntivirusProduct]>) -> (Option<bool>, Option<String>) {
    let Some(products) = products else {
        return (None, Some("Antivirus status unavailable".into()));
    };
    if products.is_empty() {
        return (Some(false), Some("No antivirus product registered".into()));
    }
    let on: Vec<String> = products
        .iter()
        .filter_map(|product| {
            let (on, current) = product.decode();
            on.then(|| {
                if current {
                    product.name.clone()
                } else {
                    format!("{} (definitions out of date)", product.name)
                }
            })
        })
        .collect();
    if on.is_empty() {
        let names: Vec<&str> = products
            .iter()
            .map(|product| product.name.as_str())
            .collect();
        (
            Some(false),
            Some(format!("Antivirus off: {}", names.join(", "))),
        )
    } else {
        (Some(true), Some(on.join(", ")))
    }
}

/// `whoami /groups` → passes unless the token holds `BUILTIN\Administrators`
/// (`S-1-5-32-544`), which it does as "deny only" too: UAC filters the
/// group, but the user can elevate without another account.
pub fn local_admin_state(groups: Option<&str>) -> (Option<bool>, Option<String>) {
    match groups {
        Some(groups) if groups.contains("S-1-5-32-544") => (
            Some(false),
            Some("The signed-in user is a local administrator".into()),
        ),
        // Any SID shows the probe read the token.
        Some(groups) if groups.contains("S-1-") => (Some(true), None),
        _ => (None, Some("Group membership unavailable".into())),
    }
}

/// An active Intune enrollment (provider `MS DM Server`) passes, else a
/// domain join does.
pub fn device_management_state(
    mdm_providers: Option<&[String]>,
    domain_joined: Option<bool>,
) -> (Option<bool>, Option<String>) {
    let intune =
        mdm_providers.is_some_and(|providers| providers.iter().any(|p| p == "MS DM Server"));
    if intune {
        return (Some(true), Some("Intune (MDM)".into()));
    }
    match (mdm_providers, domain_joined) {
        (_, Some(true)) => (Some(true), Some("Active Directory domain".into())),
        (Some(_), Some(false)) => (
            Some(false),
            Some("Not enrolled in MDM or joined to a domain".into()),
        ),
        _ => (None, Some("Device management status unavailable".into())),
    }
}

/// `UEFISecureBootEnabled`: 1 on, 0 off; missing on legacy BIOS machines.
pub fn secure_boot_state(enabled: Option<i64>) -> (Option<bool>, Option<String>) {
    match enabled {
        Some(1) => (Some(true), None),
        Some(0) => (Some(false), Some("Secure Boot is off".into())),
        _ => (None, Some("Secure Boot status unavailable".into())),
    }
}

struct ScreenLockSignals {
    inactivity_policy_secs: Option<u64>,
    screensaver_active: Option<bool>,
    screensaver_secure: Option<bool>,
    screensaver_timeout_secs: Option<u64>,
    console_lock_on_wake: Option<bool>,
    standby_ac_secs: Option<u64>,
}

impl ScreenLockSignals {
    fn evaluate(&self) -> (Option<bool>, Option<String>) {
        let within = |secs: Option<u64>| secs.is_some_and(|s| s > 0 && s <= SCREEN_LOCK_MAX_SECS);
        let policy_ok = within(self.inactivity_policy_secs);
        let saver_ok = self.screensaver_active == Some(true)
            && self.screensaver_secure == Some(true)
            && within(self.screensaver_timeout_secs);
        let sleep_ok = self.console_lock_on_wake == Some(true) && within(self.standby_ac_secs);
        if policy_ok || saver_ok || sleep_ok {
            return (Some(true), None);
        }
        let any_signal = self.inactivity_policy_secs.is_some()
            || self.screensaver_active.is_some()
            || self.console_lock_on_wake.is_some()
            || self.standby_ac_secs.is_some();
        if !any_signal {
            return (None, Some("Lock settings unavailable".into()));
        }
        (
            Some(false),
            Some(format!(
                "No lock within {} min (policy, screensaver, or sleep + sign-in)",
                SCREEN_LOCK_MAX_SECS / 60
            )),
        )
    }
}

/// Maps the probe's JSON to the contract checks (every id but macOS's
/// `system_integrity`). Missing or malformed signals become `pass: null`
/// (never a silent pass).
pub fn parse_posture_json(json: &str, now: DateTime<Utc>) -> Vec<PostureCheckResult> {
    let raw: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let get = |key: &str| raw.get(key).filter(|v| !v.is_null());

    let (disk_pass, disk_detail) = bitlocker_state(as_i64(get("bitlocker")));

    let firewall = get("firewall").map(|v| match v {
        Value::Array(items) => items.iter().filter_map(as_bool).collect::<Vec<_>>(),
        other => as_bool(other).into_iter().collect(),
    });
    let (firewall_pass, firewall_detail) = match firewall.as_deref() {
        Some([]) | None => (None, Some("Firewall status unavailable".into())),
        Some(profiles) => {
            let on = profiles.iter().filter(|enabled| **enabled).count();
            if on == profiles.len() {
                (Some(true), None)
            } else {
                (
                    Some(false),
                    Some(format!(
                        "{on} of {} firewall profiles enabled",
                        profiles.len()
                    )),
                )
            }
        }
    };

    let smartscreen = smartscreen_state(
        as_i64(get("smartscreen_policy")),
        get("smartscreen").and_then(Value::as_str),
    );

    let saver = get("screensaver");
    let signals = ScreenLockSignals {
        inactivity_policy_secs: as_i64(get("inactivity")).and_then(|v| u64::try_from(v).ok()),
        screensaver_active: saver.and_then(|s| s.get("active")).and_then(as_bool),
        screensaver_secure: saver.and_then(|s| s.get("secure")).and_then(as_bool),
        screensaver_timeout_secs: saver
            .and_then(|s| as_i64(s.get("timeout")))
            .and_then(|v| u64::try_from(v).ok()),
        console_lock_on_wake: get("consolelock")
            .and_then(Value::as_str)
            .and_then(parse_powercfg_ac_index)
            .map(|v| v == 1),
        standby_ac_secs: get("standby")
            .and_then(Value::as_str)
            .and_then(parse_powercfg_ac_index),
    };
    let (lock_pass, lock_detail) = signals.evaluate();

    let pending = as_i64(get("pending_updates"));
    let build = get("os").map(|os| {
        let field = |k: &str| {
            os.get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string()
        };
        (field("display"), field("build"), field("ubr"))
    });
    let update_detail = match (pending, &build) {
        (Some(n), _) if n > 0 => Some(format!("{n} update(s) pending")),
        (None, _) => Some("Windows Update status unavailable".into()),
        (Some(_), Some((display, build, ubr))) if !build.is_empty() => {
            Some(format!("Windows {display} build {build}.{ubr}").replace("  ", " "))
        }
        _ => None,
    };

    let antivirus: Option<Vec<AntivirusProduct>> = get("antivirus").map(|products| {
        list(products)
            .into_iter()
            .filter_map(|product| {
                Some(AntivirusProduct {
                    name: product.get("name")?.as_str()?.trim().to_string(),
                    product_state: as_i64(product.get("state"))?,
                })
            })
            .collect()
    });
    let mdm_providers: Option<Vec<String>> = get("mdm").map(|providers| {
        list(providers)
            .into_iter()
            .filter_map(Value::as_str)
            .map(|provider| provider.trim().to_string())
            .collect()
    });
    let device_checks = [
        (
            PostureCheckId::AutomaticUpdates,
            automatic_updates_state(
                as_i64(get("wu_policy")),
                get("wu_service").and_then(Value::as_str),
            ),
        ),
        (
            PostureCheckId::Antivirus,
            antivirus_state(antivirus.as_deref()),
        ),
        (
            PostureCheckId::LocalAdmin,
            local_admin_state(get("groups").and_then(Value::as_str)),
        ),
        (
            PostureCheckId::DeviceManagement,
            device_management_state(
                mdm_providers.as_deref(),
                get("domain_joined").and_then(as_bool),
            ),
        ),
        (
            PostureCheckId::SecureBoot,
            secure_boot_state(as_i64(get("secure_boot"))),
        ),
    ];

    let mut results = vec![
        PostureCheckResult {
            id: PostureCheckId::DiskEncryption,
            pass: disk_pass,
            detail: disk_detail,
            collected_at: now,
        },
        PostureCheckResult {
            id: PostureCheckId::Firewall,
            pass: firewall_pass,
            detail: firewall_detail,
            collected_at: now,
        },
        PostureCheckResult {
            id: PostureCheckId::Gatekeeper,
            pass: smartscreen,
            detail: smartscreen
                .is_none()
                .then(|| "SmartScreen status unavailable".to_string()),
            collected_at: now,
        },
        PostureCheckResult {
            id: PostureCheckId::ScreenLock,
            pass: lock_pass,
            detail: lock_detail,
            collected_at: now,
        },
        PostureCheckResult {
            id: PostureCheckId::OsUpdate,
            pass: pending.map(|n| n == 0),
            detail: update_detail,
            collected_at: now,
        },
    ];
    results.extend(
        device_checks
            .into_iter()
            .map(|(id, (pass, detail))| PostureCheckResult {
                id,
                pass,
                detail,
                collected_at: now,
            }),
    );
    results
}

#[cfg(target_os = "windows")]
pub mod collect {
    //! Runs the probe through `powershell.exe` (Windows PowerShell 5.1 ships
    //! with every supported Windows) without flashing a console window.

    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use chrono::Utc;

    use super::{parse_posture_json, POSTURE_SCRIPT};
    use crate::{PostureCheckResult, PostureSource};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Runs a PowerShell script and returns its stdout, or `None` if it could
    /// not start or exceeded `timeout` (the process is killed). Stdout is
    /// drained on a helper thread so a chatty script can never block on a
    /// full pipe.
    pub fn run_powershell(script: &str, timeout: Duration) -> Option<String> {
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = stdout.read_to_string(&mut buffer);
            buffer
        });
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
        reader.join().ok()
    }

    pub struct WindowsPostureSource;

    impl PostureSource for WindowsPostureSource {
        fn collect(&self) -> Vec<PostureCheckResult> {
            let output =
                run_powershell(POSTURE_SCRIPT, Duration::from_secs(60)).unwrap_or_default();
            parse_posture_json(&output, Utc::now())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 2, 6, 0, 0).unwrap()
    }

    fn by_id(results: &[PostureCheckResult], id: PostureCheckId) -> &PostureCheckResult {
        results.iter().find(|r| r.id == id).unwrap()
    }

    const POWERCFG_ON: &str = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)\n  Subgroup GUID: fea3413e-7e05-4911-9a71-700331f1c294  (Settings belonging to no subgroup)\n    Power Setting GUID: 0e796bdb-100d-47d6-a2d5-f7d2daa51f51  (Require a password on wakeup)\n      Current AC Power Setting Index: 0x00000001\n      Current DC Power Setting Index: 0x00000001\n";
    const POWERCFG_SLEEP_10MIN: &str = "    Power Setting GUID: 29f6c1db-86da-48c5-9fdb-f2b67b1f44da  (Sleep after)\n      Current AC Power Setting Index: 0x00000258\n      Current DC Power Setting Index: 0x0000012c\n";

    #[test]
    fn powercfg_index_parses_hex() {
        assert_eq!(parse_powercfg_ac_index(POWERCFG_ON), Some(1));
        assert_eq!(parse_powercfg_ac_index(POWERCFG_SLEEP_10MIN), Some(600));
        assert_eq!(parse_powercfg_ac_index("garbage"), None);
    }

    /// `whoami /groups /fo csv /nh`, joined: a standard user, and an
    /// administrator whose token UAC filtered.
    const GROUPS_STANDARD: &str = r#""Everyone","Well-known group","S-1-1-0","Mandatory group, Enabled by default, Enabled group" "BUILTIN\Users","Alias","S-1-5-32-545","Mandatory group, Enabled by default, Enabled group""#;
    const GROUPS_ADMIN_DENY_ONLY: &str = r#""Everyone","Well-known group","S-1-1-0","Mandatory group, Enabled by default, Enabled group" "BUILTIN\Administrators","Alias","S-1-5-32-544","Group used for deny only""#;

    #[test]
    fn healthy_windows_11_passes_every_check() {
        let json = serde_json::json!({
            "bitlocker": 1,
            "firewall": [true, true, true],
            "smartscreen": "Warn",
            "smartscreen_policy": null,
            "screensaver": {"active": "0", "secure": "0", "timeout": "600"},
            "inactivity": null,
            "consolelock": POWERCFG_ON,
            "standby": POWERCFG_SLEEP_10MIN,
            "os": {"display": "24H2", "build": "26100", "ubr": "4351"},
            "pending_updates": 0,
            "wu_policy": null,
            "wu_service": "Manual",
            "antivirus": [{"name": "Microsoft Defender Antivirus", "state": 397568}],
            "groups": GROUPS_STANDARD,
            "mdm": ["MS DM Server"],
            "domain_joined": false,
            "secure_boot": 1
        })
        .to_string();
        let results = parse_posture_json(&json, now());
        assert_eq!(results.len(), 10);
        for result in &results {
            assert_eq!(result.pass, Some(true), "{:?}", result.id);
        }
        assert_eq!(
            by_id(&results, PostureCheckId::OsUpdate).detail.as_deref(),
            Some("Windows 24H2 build 26100.4351")
        );
    }

    #[test]
    fn unencrypted_open_machine_fails_honestly() {
        let json = serde_json::json!({
            "bitlocker": 2,
            "firewall": [true, false, true],
            "smartscreen": "Off",
            "screensaver": {"active": "1", "secure": "0", "timeout": "600"},
            "inactivity": 0,
            "consolelock": POWERCFG_ON,
            "standby": "      Current AC Power Setting Index: 0x00000000\n",
            "os": {"display": "23H2", "build": "22631", "ubr": "3"},
            "pending_updates": 3
        })
        .to_string();
        let results = parse_posture_json(&json, now());
        let disk = by_id(&results, PostureCheckId::DiskEncryption);
        assert_eq!(disk.pass, Some(false));
        assert_eq!(disk.detail.as_deref(), Some("BitLocker is off"));
        let firewall = by_id(&results, PostureCheckId::Firewall);
        assert_eq!(firewall.pass, Some(false));
        assert_eq!(
            firewall.detail.as_deref(),
            Some("2 of 3 firewall profiles enabled")
        );
        assert_eq!(
            by_id(&results, PostureCheckId::Gatekeeper).pass,
            Some(false)
        );
        let lock = by_id(&results, PostureCheckId::ScreenLock);
        assert_eq!(lock.pass, Some(false));
        assert!(lock.detail.as_deref().unwrap().contains("15 min"));
        let update = by_id(&results, PostureCheckId::OsUpdate);
        assert_eq!(update.pass, Some(false));
        assert_eq!(update.detail.as_deref(), Some("3 update(s) pending"));
    }

    #[test]
    fn policy_lock_and_secure_screensaver_each_pass() {
        let policy = serde_json::json!({"inactivity": 300}).to_string();
        assert_eq!(
            by_id(
                &parse_posture_json(&policy, now()),
                PostureCheckId::ScreenLock
            )
            .pass,
            Some(true)
        );
        let saver = serde_json::json!({
            "screensaver": {"active": "1", "secure": "1", "timeout": "300"}
        })
        .to_string();
        assert_eq!(
            by_id(
                &parse_posture_json(&saver, now()),
                PostureCheckId::ScreenLock
            )
            .pass,
            Some(true)
        );
        let slow_saver = serde_json::json!({
            "screensaver": {"active": "1", "secure": "1", "timeout": "3600"}
        })
        .to_string();
        assert_eq!(
            by_id(
                &parse_posture_json(&slow_saver, now()),
                PostureCheckId::ScreenLock
            )
            .pass,
            Some(false)
        );
    }

    #[test]
    fn device_check_fixtures() {
        use serde_json::json;
        let defender_off = json!({"name": "Windows Defender", "state": 393472});
        // (probe signals, check, pass, detail)
        for (signals, id, pass, detail) in [
            (
                json!({"wu_service": "Manual"}),
                PostureCheckId::AutomaticUpdates,
                Some(true),
                None,
            ),
            (
                json!({"wu_policy": 1, "wu_service": "Manual"}),
                PostureCheckId::AutomaticUpdates,
                Some(false),
                Some("Automatic updates are turned off by policy"),
            ),
            (
                json!({"wu_service": "Disabled"}),
                PostureCheckId::AutomaticUpdates,
                Some(false),
                Some("The Windows Update service is disabled"),
            ),
            // One product: ConvertTo-Json drops the list.
            (
                json!({"antivirus": {"name": "Trend Micro Apex One", "state": 266240}}),
                PostureCheckId::Antivirus,
                Some(true),
                Some("Trend Micro Apex One"),
            ),
            (
                json!({"antivirus": [defender_off, {"name": "Norton 360", "state": 397584}]}),
                PostureCheckId::Antivirus,
                Some(true),
                Some("Norton 360 (definitions out of date)"),
            ),
            (
                json!({"antivirus": [defender_off]}),
                PostureCheckId::Antivirus,
                Some(false),
                Some("Antivirus off: Windows Defender"),
            ),
            (
                json!({"antivirus": []}),
                PostureCheckId::Antivirus,
                Some(false),
                Some("No antivirus product registered"),
            ),
            (
                json!({"groups": GROUPS_ADMIN_DENY_ONLY}),
                PostureCheckId::LocalAdmin,
                Some(false),
                Some("The signed-in user is a local administrator"),
            ),
            (
                json!({"groups": GROUPS_STANDARD}),
                PostureCheckId::LocalAdmin,
                Some(true),
                None,
            ),
            (
                json!({"groups": "ERROR: Access is denied."}),
                PostureCheckId::LocalAdmin,
                None,
                Some("Group membership unavailable"),
            ),
            (
                json!({"mdm": "MS DM Server", "domain_joined": false}),
                PostureCheckId::DeviceManagement,
                Some(true),
                Some("Intune (MDM)"),
            ),
            (
                json!({"mdm": [], "domain_joined": true}),
                PostureCheckId::DeviceManagement,
                Some(true),
                Some("Active Directory domain"),
            ),
            (
                json!({"mdm": [], "domain_joined": false}),
                PostureCheckId::DeviceManagement,
                Some(false),
                Some("Not enrolled in MDM or joined to a domain"),
            ),
            (
                json!({"mdm": null, "domain_joined": false}),
                PostureCheckId::DeviceManagement,
                None,
                Some("Device management status unavailable"),
            ),
            (
                json!({"secure_boot": 1}),
                PostureCheckId::SecureBoot,
                Some(true),
                None,
            ),
            (
                json!({"secure_boot": 0}),
                PostureCheckId::SecureBoot,
                Some(false),
                Some("Secure Boot is off"),
            ),
        ] {
            let results = parse_posture_json(&signals.to_string(), now());
            let result = by_id(&results, id);
            assert_eq!(
                (result.pass, result.detail.as_deref()),
                (pass, detail),
                "{signals}"
            );
        }
    }

    #[test]
    fn smartscreen_policy_wins_over_setting() {
        assert_eq!(smartscreen_state(Some(1), Some("Off")), Some(true));
        assert_eq!(smartscreen_state(Some(0), Some("Warn")), Some(false));
        assert_eq!(smartscreen_state(None, Some("Block")), Some(true));
        assert_eq!(smartscreen_state(None, None), None);
    }

    #[test]
    fn missing_or_broken_probe_output_is_unknown_not_passing() {
        for json in ["", "not json", "{}", "null"] {
            let results = parse_posture_json(json, now());
            assert_eq!(results.len(), 10, "{json:?}");
            for result in &results {
                assert_eq!(result.pass, None, "{json:?} {:?}", result.id);
                assert!(result.detail.is_some(), "{json:?} {:?}", result.id);
            }
        }
    }

    #[test]
    fn loose_powershell_scalars_are_accepted() {
        let json = serde_json::json!({
            "bitlocker": "1",
            "firewall": "True",
            "smartscreen_policy": "1",
            "pending_updates": "0"
        })
        .to_string();
        let results = parse_posture_json(&json, now());
        assert_eq!(
            by_id(&results, PostureCheckId::DiskEncryption).pass,
            Some(true)
        );
        assert_eq!(by_id(&results, PostureCheckId::Firewall).pass, Some(true));
        assert_eq!(by_id(&results, PostureCheckId::Gatekeeper).pass, Some(true));
        assert_eq!(by_id(&results, PostureCheckId::OsUpdate).pass, Some(true));
    }
}
