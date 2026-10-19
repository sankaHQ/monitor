//! Device posture checks.
//!
//! Every check is user-level — no root daemon, no admin rights, no TCC-gated
//! permissions. The pure output parsers are testable on any platform; command
//! execution lives behind `cfg(target_os = ...)` in [`macos`] and [`windows`].
//!
//! The serialized shape is the sanka-api `/monitor/agent/checkins` contract:
//! camelCase keys, snake_case check ids, and `pass: true | false | null`
//! (null = could not determine; never silently reported as passing). `true`
//! is always the safe state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostureCheckId {
    DiskEncryption,
    Firewall,
    Gatekeeper,
    ScreenLock,
    OsUpdate,
    /// Updates install without the user (macOS security responses, Windows
    /// Update not switched off).
    AutomaticUpdates,
    /// An antivirus product is on (XProtect on macOS).
    Antivirus,
    /// Passes when the signed-in user is *not* a local administrator.
    LocalAdmin,
    /// Enrolled in MDM (or, on Windows, joined to an Active Directory domain).
    DeviceManagement,
    /// UEFI Secure Boot (Windows only).
    SecureBoot,
    /// System Integrity Protection (macOS only).
    SystemIntegrity,
}

impl PostureCheckId {
    /// Human label for the agent's Checks pane. The ids are the cross-platform
    /// contract; the wording names the OS feature behind each one, phrased as
    /// the passing state.
    pub fn label(self) -> &'static str {
        if cfg!(target_os = "windows") {
            match self {
                PostureCheckId::DiskEncryption => "BitLocker Enabled",
                PostureCheckId::Firewall => "Firewall Enabled",
                PostureCheckId::Gatekeeper => "SmartScreen Enabled",
                PostureCheckId::ScreenLock => "Screen Lock Configured",
                PostureCheckId::OsUpdate => "OS Up To Date",
                PostureCheckId::AutomaticUpdates => "Automatic Updates On",
                PostureCheckId::Antivirus => "Antivirus On",
                PostureCheckId::LocalAdmin => "No Local Admin Rights",
                PostureCheckId::DeviceManagement => "Managed (MDM or Domain)",
                PostureCheckId::SecureBoot => "Secure Boot Enabled",
                PostureCheckId::SystemIntegrity => "System Integrity Protected",
            }
        } else {
            match self {
                PostureCheckId::DiskEncryption => "Disk Encrypted",
                PostureCheckId::Firewall => "Firewall Enabled",
                PostureCheckId::Gatekeeper => "Gatekeeper Enabled",
                PostureCheckId::ScreenLock => "Screenlock Configured",
                PostureCheckId::OsUpdate => "OS Up To Date",
                PostureCheckId::AutomaticUpdates => "Automatic Updates On",
                PostureCheckId::Antivirus => "XProtect Active",
                PostureCheckId::LocalAdmin => "No Admin Rights",
                PostureCheckId::DeviceManagement => "MDM Enrolled",
                PostureCheckId::SecureBoot => "Secure Boot Enabled",
                PostureCheckId::SystemIntegrity => "SIP Enabled",
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostureCheckResult {
    pub id: PostureCheckId,
    pub pass: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub collected_at: DateTime<Utc>,
}

/// Implemented per platform (`macos::collect`, `windows::collect`).
pub trait PostureSource {
    fn collect(&self) -> Vec<PostureCheckResult>;
}

pub mod windows;

pub mod macos {
    //! Pure parsers for the macOS user-level check commands, plus (on macOS
    //! builds) the collector that runs them.

    /// `fdesetup status` → FileVault state.
    pub fn parse_fdesetup_status(output: &str) -> Option<bool> {
        let o = output.trim();
        if o.contains("FileVault is On") {
            Some(true)
        } else if o.contains("FileVault is Off") {
            Some(false)
        } else {
            None
        }
    }

    /// `spctl --status` → Gatekeeper assessment state.
    pub fn parse_spctl_status(output: &str) -> Option<bool> {
        let o = output.trim();
        if o.contains("assessments enabled") {
            Some(true)
        } else if o.contains("assessments disabled") {
            Some(false)
        } else {
            None
        }
    }

    /// `defaults read /Library/Preferences/com.apple.alf globalstate` →
    /// application firewall state (0 = off, 1 = on, 2 = on + block all).
    pub fn parse_alf_globalstate(output: &str) -> Option<bool> {
        match output.trim() {
            "0" => Some(false),
            "1" | "2" => Some(true),
            _ => None,
        }
    }

    /// `sysadminctl -screenLock status` (writes to stderr) → screen lock
    /// configured state.
    pub fn parse_screen_lock_status(output: &str) -> Option<bool> {
        let o = output.trim();
        if o.contains("screenLock is off") {
            Some(false)
        } else if o.contains("screenLock delay") || o.contains("screenLock is on") {
            Some(true)
        } else {
            None
        }
    }

    /// `defaults read /Library/Preferences/com.apple.SoftwareUpdate
    /// LastRecommendedUpdatesAvailable` → cached pending-update count.
    /// Returns (pass, pending_count).
    pub fn parse_pending_updates(output: &str) -> Option<(bool, u32)> {
        let count: u32 = output.trim().parse().ok()?;
        Some((count == 0, count))
    }

    /// The Software Update settings that decide whether updates install
    /// without the user. `None` = not set, which means macOS's default (on).
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct SoftwareUpdatePrefs {
        /// `AutomaticCheckEnabled`: "Check for updates".
        pub automatic_check: Option<bool>,
        /// `CriticalUpdateInstall`: "Install Security Responses and system files".
        pub critical_install: Option<bool>,
        /// `AutomaticallyInstallMacOSUpdates`: "Install macOS updates".
        pub install_macos_updates: Option<bool>,
    }

    impl SoftwareUpdatePrefs {
        /// A configuration profile (`/Library/Managed Preferences`) wins key
        /// by key over the local settings.
        pub fn overridden_by(self, managed: SoftwareUpdatePrefs) -> SoftwareUpdatePrefs {
            SoftwareUpdatePrefs {
                automatic_check: managed.automatic_check.or(self.automatic_check),
                critical_install: managed.critical_install.or(self.critical_install),
                install_macos_updates: managed.install_macos_updates.or(self.install_macos_updates),
            }
        }
    }

    /// `defaults read <com.apple.SoftwareUpdate domain>` → its update
    /// settings. A domain that does not exist holds no settings (all
    /// defaults); output that is neither is unreadable (`None`).
    pub fn parse_software_update_prefs(output: &str) -> Option<SoftwareUpdatePrefs> {
        if output.contains("does not exist") {
            return Some(SoftwareUpdatePrefs::default());
        }
        if !output.trim_start().starts_with('{') {
            return None;
        }
        // Top-level keys are the lines indented by exactly four spaces;
        // nested dictionaries and arrays are indented further.
        let top_level = |key: &str| {
            output.lines().find_map(|line| {
                let rest = line.strip_prefix("    ")?;
                let (name, value) = rest.split_once(" = ")?;
                (name == key).then(|| match value.trim_end_matches(';').trim() {
                    "1" => Some(true),
                    "0" => Some(false),
                    _ => None,
                })?
            })
        };
        Some(SoftwareUpdatePrefs {
            automatic_check: top_level("AutomaticCheckEnabled"),
            critical_install: top_level("CriticalUpdateInstall"),
            install_macos_updates: top_level("AutomaticallyInstallMacOSUpdates"),
        })
    }

    /// Passes while macOS checks for updates and installs security responses
    /// on its own; the detail says whether macOS updates install too.
    pub fn automatic_updates_state(
        prefs: Option<SoftwareUpdatePrefs>,
    ) -> (Option<bool>, Option<String>) {
        let Some(prefs) = prefs else {
            return (None, Some("Software Update settings unavailable".into()));
        };
        if prefs.automatic_check == Some(false) {
            return (Some(false), Some("Automatic update checks are off".into()));
        }
        if prefs.critical_install == Some(false) {
            return (
                Some(false),
                Some("Security responses do not install automatically".into()),
            );
        }
        let detail = match prefs.install_macos_updates {
            Some(true) => Some("macOS updates also install automatically"),
            Some(false) => Some("macOS updates wait for the user"),
            None => None,
        };
        (Some(true), detail.map(str::to_string))
    }

    /// `csrutil status` → System Integrity Protection. A custom
    /// configuration (some protections switched off) fails and names them.
    pub fn parse_csrutil_status(output: &str) -> (Option<bool>, Option<String>) {
        let Some(status) = output
            .lines()
            .find_map(|line| line.split_once("System Integrity Protection status:"))
            .map(|(_, status)| status.trim().to_ascii_lowercase())
        else {
            return (None, Some("SIP status unavailable".into()));
        };
        if status.contains("custom configuration") {
            let off: Vec<&str> = output
                .lines()
                .filter_map(|line| line.trim().strip_suffix(": disabled"))
                // Not a protection: off on every customer Mac.
                .filter(|name| *name != "Apple Internal")
                .collect();
            let detail = if off.is_empty() {
                "SIP uses a custom configuration".to_string()
            } else {
                format!("SIP custom configuration, off: {}", off.join(", "))
            };
            return (Some(false), Some(detail));
        }
        if status.starts_with("enabled") {
            (Some(true), None)
        } else if status.starts_with("disabled") {
            (Some(false), Some("SIP is disabled".into()))
        } else {
            (None, Some("SIP status unavailable".into()))
        }
    }

    /// `id -Gn` (the signed-in user's groups) → passes when the user is not
    /// in `admin`.
    pub fn parse_id_groups(output: &str) -> (Option<bool>, Option<String>) {
        let groups: Vec<&str> = output.split_whitespace().collect();
        if groups.is_empty() {
            (None, Some("Group membership unavailable".into()))
        } else if groups.contains(&"admin") {
            (
                Some(false),
                Some("The signed-in user is an administrator".into()),
            )
        } else {
            (Some(true), None)
        }
    }

    /// `profiles status -type enrollment` → MDM enrollment.
    pub fn parse_mdm_enrollment(output: &str) -> (Option<bool>, Option<String>) {
        let value = |label: &str| {
            output
                .lines()
                .find_map(|line| line.trim().strip_prefix(label))
                .map(str::trim)
        };
        let via_dep = value("Enrolled via DEP:").is_some_and(|v| v.starts_with("Yes"));
        match value("MDM enrollment:") {
            Some(mdm) if mdm.starts_with("Yes") => {
                let detail = if via_dep {
                    "Enrolled via Automated Device Enrollment"
                } else if mdm.contains("User Approved") {
                    "Enrolled (user approved)"
                } else {
                    "Enrolled"
                };
                (Some(true), Some(detail.to_string()))
            }
            Some(mdm) if mdm.starts_with("No") => (Some(false), Some("Not enrolled in MDM".into())),
            _ => (None, Some("MDM enrollment status unavailable".into())),
        }
    }

    /// XProtect, macOS's built-in malware scanner, cannot be switched off
    /// by the user: present passes, with its version when readable
    /// (`defaults read …/XProtect.bundle/Contents/Info.plist
    /// CFBundleShortVersionString`).
    pub fn xprotect_state(present: bool, version: Option<&str>) -> (Option<bool>, Option<String>) {
        if !present {
            return (None, Some("XProtect not found".into()));
        }
        let version = version
            .map(str::trim)
            .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.'));
        let detail = match version {
            Some(version) => format!("XProtect {version}"),
            None => "XProtect".to_string(),
        };
        (Some(true), Some(detail))
    }

    #[cfg(target_os = "macos")]
    pub mod collect {
        use super::*;
        use crate::{PostureCheckId, PostureCheckResult, PostureSource};
        use chrono::Utc;
        use std::process::Command;

        /// Runs a read-only probe, returning stdout+stderr combined —
        /// several of these tools (sysadminctl) report on stderr, and a
        /// non-zero exit with recognizable output still parses. `None` only
        /// when the command could not be spawned.
        fn probe(cmd: &str, args: &[&str]) -> Option<String> {
            let output = Command::new(cmd).args(args).output().ok()?;
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Some(text)
        }

        /// Stdout of a probe that exited successfully; `None` otherwise, so
        /// an error message is never read as a result.
        fn probe_ok(cmd: &str, args: &[&str]) -> Option<String> {
            let output = Command::new(cmd).args(args).output().ok()?;
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        }

        const SOFTWARE_UPDATE: &str = "/Library/Preferences/com.apple.SoftwareUpdate";
        const MANAGED_SOFTWARE_UPDATE: &str =
            "/Library/Managed Preferences/com.apple.SoftwareUpdate";
        const XPROTECT: &str = "/Library/Apple/System/Library/CoreServices/XProtect.bundle";

        /// Local Software Update settings, overridden by a configuration
        /// profile's when one is installed.
        fn software_update_prefs() -> Option<SoftwareUpdatePrefs> {
            let local = probe("defaults", &["read", SOFTWARE_UPDATE])
                .as_deref()
                .and_then(parse_software_update_prefs)?;
            let managed_plist = format!("{MANAGED_SOFTWARE_UPDATE}.plist");
            if !std::path::Path::new(&managed_plist).exists() {
                return Some(local);
            }
            let managed = probe("defaults", &["read", MANAGED_SOFTWARE_UPDATE])
                .as_deref()
                .and_then(parse_software_update_prefs)?;
            Some(local.overridden_by(managed))
        }

        fn result(
            id: PostureCheckId,
            (pass, detail): (Option<bool>, Option<String>),
            now: chrono::DateTime<Utc>,
        ) -> PostureCheckResult {
            PostureCheckResult {
                id,
                pass,
                detail,
                collected_at: now,
            }
        }

        pub struct MacosPostureSource;

        impl PostureSource for MacosPostureSource {
            fn collect(&self) -> Vec<PostureCheckResult> {
                let now = Utc::now();
                let mut results = Vec::with_capacity(10);

                let filevault = probe("fdesetup", &["status"]);
                results.push(PostureCheckResult {
                    id: PostureCheckId::DiskEncryption,
                    pass: filevault.as_deref().and_then(parse_fdesetup_status),
                    detail: None,
                    collected_at: now,
                });

                let alf = probe(
                    "defaults",
                    &["read", "/Library/Preferences/com.apple.alf", "globalstate"],
                );
                let firewall = alf.as_deref().and_then(parse_alf_globalstate).or_else(|| {
                    probe(
                        "/usr/libexec/ApplicationFirewall/socketfilterfw",
                        &["--getglobalstate"],
                    )
                    .as_deref()
                    .map(|o| o.contains("enabled"))
                });
                results.push(PostureCheckResult {
                    id: PostureCheckId::Firewall,
                    pass: firewall,
                    detail: None,
                    collected_at: now,
                });

                let spctl = probe("spctl", &["--status"]);
                results.push(PostureCheckResult {
                    id: PostureCheckId::Gatekeeper,
                    pass: spctl.as_deref().and_then(parse_spctl_status),
                    detail: None,
                    collected_at: now,
                });

                let screen_lock = probe("sysadminctl", &["-screenLock", "status"]);
                results.push(PostureCheckResult {
                    id: PostureCheckId::ScreenLock,
                    pass: screen_lock.as_deref().and_then(parse_screen_lock_status),
                    detail: None,
                    collected_at: now,
                });

                let updates = probe(
                    "defaults",
                    &[
                        "read",
                        "/Library/Preferences/com.apple.SoftwareUpdate",
                        "LastRecommendedUpdatesAvailable",
                    ],
                );
                let parsed = updates.as_deref().and_then(parse_pending_updates);
                results.push(PostureCheckResult {
                    id: PostureCheckId::OsUpdate,
                    pass: parsed.map(|(pass, _)| pass),
                    detail: parsed.and_then(|(_, count)| {
                        (count > 0).then(|| format!("{count} update(s) pending"))
                    }),
                    collected_at: now,
                });

                results.push(result(
                    PostureCheckId::AutomaticUpdates,
                    automatic_updates_state(software_update_prefs()),
                    now,
                ));

                let xprotect_version = probe_ok(
                    "defaults",
                    &[
                        "read",
                        &format!("{XPROTECT}/Contents/Info.plist"),
                        "CFBundleShortVersionString",
                    ],
                );
                results.push(result(
                    PostureCheckId::Antivirus,
                    xprotect_state(
                        std::path::Path::new(XPROTECT).is_dir(),
                        xprotect_version.as_deref(),
                    ),
                    now,
                ));

                let groups = probe_ok("id", &["-Gn"]);
                results.push(result(
                    PostureCheckId::LocalAdmin,
                    parse_id_groups(groups.as_deref().unwrap_or("")),
                    now,
                ));

                let enrollment = probe_ok("profiles", &["status", "-type", "enrollment"]);
                results.push(result(
                    PostureCheckId::DeviceManagement,
                    parse_mdm_enrollment(enrollment.as_deref().unwrap_or("")),
                    now,
                ));

                let sip = probe_ok("csrutil", &["status"]);
                results.push(result(
                    PostureCheckId::SystemIntegrity,
                    parse_csrutil_status(sip.as_deref().unwrap_or("")),
                    now,
                ));

                results
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fdesetup_fixtures() {
            assert_eq!(parse_fdesetup_status("FileVault is On.\n"), Some(true));
            assert_eq!(parse_fdesetup_status("FileVault is Off.\n"), Some(false));
            assert_eq!(parse_fdesetup_status("Error: not authorized"), None);
        }

        #[test]
        fn spctl_fixtures() {
            assert_eq!(parse_spctl_status("assessments enabled\n"), Some(true));
            assert_eq!(parse_spctl_status("assessments disabled\n"), Some(false));
            assert_eq!(parse_spctl_status(""), None);
        }

        #[test]
        fn alf_fixtures() {
            assert_eq!(parse_alf_globalstate("0\n"), Some(false));
            assert_eq!(parse_alf_globalstate("1\n"), Some(true));
            assert_eq!(parse_alf_globalstate("2\n"), Some(true));
            assert_eq!(
                parse_alf_globalstate("The domain/default pair does not exist"),
                None
            );
        }

        #[test]
        fn screen_lock_fixtures() {
            assert_eq!(
                parse_screen_lock_status("2026-09-01 screenLock is off\n"),
                Some(false)
            );
            assert_eq!(
                parse_screen_lock_status("screenLock delay is 60 seconds\n"),
                Some(true)
            );
            assert_eq!(parse_screen_lock_status("usage: sysadminctl"), None);
        }

        const SOFTWARE_UPDATE_DOMAIN: &str = "{\n    AutomaticDownload = 1;\n    AutomaticallyInstallMacOSUpdates = 1;\n    CriticalUpdateInstall = 1;\n    DDMPersistedErrorKey =     {\n        AutomaticCheckEnabled = 0;\n    };\n    LastRecommendedUpdatesAvailable = 1;\n}\n";

        #[test]
        fn automatic_update_fixtures() {
            let local = parse_software_update_prefs(SOFTWARE_UPDATE_DOMAIN);
            let off = |key: &str| parse_software_update_prefs(&format!("{{\n    {key} = 0;\n}}\n"));
            let missing =
                "Domain /Library/Managed Preferences/com.apple.SoftwareUpdate does not exist";
            // (prefs, pass, detail)
            for (prefs, pass, detail) in [
                // A nested AutomaticCheckEnabled = 0 is not the setting.
                (
                    local,
                    Some(true),
                    Some("macOS updates also install automatically"),
                ),
                (parse_software_update_prefs(missing), Some(true), None),
                (
                    off("AutomaticCheckEnabled"),
                    Some(false),
                    Some("Automatic update checks are off"),
                ),
                (
                    off("CriticalUpdateInstall"),
                    Some(false),
                    Some("Security responses do not install automatically"),
                ),
                (
                    off("AutomaticallyInstallMacOSUpdates"),
                    Some(true),
                    Some("macOS updates wait for the user"),
                ),
                // A configuration profile wins over the local setting.
                (
                    local.map(|l| l.overridden_by(off("CriticalUpdateInstall").unwrap())),
                    Some(false),
                    Some("Security responses do not install automatically"),
                ),
                (
                    parse_software_update_prefs("defaults: command not found"),
                    None,
                    Some("Software Update settings unavailable"),
                ),
            ] {
                assert_eq!(
                    automatic_updates_state(prefs),
                    (pass, detail.map(str::to_string)),
                    "{prefs:?}"
                );
            }
        }

        #[test]
        fn device_check_fixtures() {
            let custom = "System Integrity Protection status: unknown (Custom Configuration).\n\nConfiguration:\n\tApple Internal: disabled\n\tKext Signing: enabled\n\tFilesystem Protections: disabled\n\tDebugging Restrictions: disabled\n\nThis is an unsupported configuration.\n";
            // (result, pass, detail)
            for (result, pass, detail) in [
                (parse_csrutil_status("System Integrity Protection status: enabled.\n"), Some(true), None),
                (parse_csrutil_status("System Integrity Protection status: disabled.\n"), Some(false), Some("SIP is disabled")),
                (
                    parse_csrutil_status(custom),
                    Some(false),
                    Some("SIP custom configuration, off: Filesystem Protections, Debugging Restrictions"),
                ),
                (parse_csrutil_status(""), None, Some("SIP status unavailable")),
                (parse_id_groups("staff everyone localaccounts _appserverusr admin _lpadmin\n"), Some(false), Some("The signed-in user is an administrator")),
                (parse_id_groups("staff everyone localaccounts _lpadmin com.apple.access_ssh\n"), Some(true), None),
                (parse_id_groups(""), None, Some("Group membership unavailable")),
                (parse_mdm_enrollment("Enrolled via DEP: No\nMDM enrollment: No\n"), Some(false), Some("Not enrolled in MDM")),
                (parse_mdm_enrollment("Enrolled via DEP: No\nMDM enrollment: Yes (User Approved)\n"), Some(true), Some("Enrolled (user approved)")),
                (parse_mdm_enrollment("Enrolled via DEP: Yes\nMDM enrollment: Yes (User Approved)\n"), Some(true), Some("Enrolled via Automated Device Enrollment")),
                (parse_mdm_enrollment("profiles: error"), None, Some("MDM enrollment status unavailable")),
                (xprotect_state(true, Some("5367\n")), Some(true), Some("XProtect 5367")),
                (xprotect_state(true, Some("The domain/default pair does not exist")), Some(true), Some("XProtect")),
                (xprotect_state(false, None), None, Some("XProtect not found")),
            ] {
                assert_eq!(result, (pass, detail.map(str::to_string)));
            }
        }

        #[test]
        fn pending_update_fixtures() {
            assert_eq!(parse_pending_updates("0\n"), Some((true, 0)));
            assert_eq!(parse_pending_updates("3\n"), Some((false, 3)));
            assert_eq!(
                parse_pending_updates("The domain/default pair does not exist"),
                None
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn posture_result_serializes_to_the_checkin_wire_shape() {
        let result = PostureCheckResult {
            id: PostureCheckId::DiskEncryption,
            pass: Some(true),
            detail: None,
            collected_at: Utc.with_ymd_and_hms(2026, 9, 1, 3, 0, 0).unwrap(),
        };
        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "id": "disk_encryption",
                "pass": true,
                "collectedAt": "2026-09-01T03:00:00Z"
            })
        );
    }
}
