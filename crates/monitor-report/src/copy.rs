//! Report wording in English and Japanese. Finding titles are the console's
//! (`sanka-react` `monitor-inventory.i18n.ts`), so a report and the console
//! name a risk the same way.

use monitor_findings::{Area, Finding, FindingType, Lifecycle, LifecycleStatus, Severity};
use monitor_posture::PostureCheckId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Ja,
}

impl Lang {
    /// `ja`, `ja-JP`, `ja_JP.UTF-8` → Japanese; anything else → English.
    pub fn from_tag(tag: &str) -> Lang {
        if tag.trim().to_ascii_lowercase().starts_with("ja") {
            Lang::Ja
        } else {
            Lang::En
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ja => "ja",
        }
    }

    fn pick(self, en: &'static str, ja: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Ja => ja,
        }
    }
}

pub fn title(finding_type: FindingType, lang: Lang) -> &'static str {
    use FindingType::*;
    match finding_type {
        DeviceDiskUnencrypted => {
            lang.pick("Disk is not encrypted", "ディスクが暗号化されていません")
        }
        DeviceFirewallOff => lang.pick("Firewall is off", "ファイアウォールが無効です"),
        DeviceAppProtectionOff => lang.pick("App protection is off", "アプリの保護が無効です"),
        DeviceSystemIntegrityOff => lang.pick(
            "System Integrity Protection is off",
            "システム整合性保護（SIP）が無効です",
        ),
        DeviceOsEndOfSupport => lang.pick(
            "OS support ended or is ending soon",
            "OSのサポートが終了、または終了間近です",
        ),
        DeviceOsUpdatesPending => lang.pick("OS updates are pending", "OSの更新が必要です"),
        DeviceAutomaticUpdatesOff => {
            lang.pick("Automatic updates are off", "自動アップデートが無効です")
        }
        DeviceAntivirusOff => {
            lang.pick("Antivirus is not running", "ウイルス対策が稼働していません")
        }
        DeviceLocalAdmin => lang.pick(
            "User has local administrator rights",
            "ユーザーがローカル管理者権限を持っています",
        ),
        DeviceScreenLockOff => lang.pick(
            "Screen lock does not meet the policy",
            "画面ロックがポリシーを満たしていません",
        ),
        DeviceUnmanaged => lang.pick("Device is not managed", "デバイス管理に登録されていません"),
        DeviceSecureBootOff => lang.pick("Secure Boot is off", "セキュアブートが無効です"),
        SoftwareEndOfSupport => lang.pick(
            "Software support ended or is ending soon",
            "ソフトウェアのサポートが終了、または終了間近です",
        ),
        SoftwareOutdated => lang.pick(
            "Software version is outdated",
            "ソフトウェアのバージョンが古い状態です",
        ),
    }
}

/// The finding's title with its subject: the product for lifecycle findings.
pub fn headline(finding: &Finding, lang: Lang) -> String {
    match &finding.lifecycle {
        Some(lifecycle) => format!(
            "{}: {}",
            title(finding.finding_type, lang),
            product(lifecycle, lang)
        ),
        None => title(finding.finding_type, lang).to_string(),
    }
}

pub fn product(lifecycle: &Lifecycle, lang: Lang) -> &str {
    match lang {
        Lang::En => &lifecycle.product,
        Lang::Ja => &lifecycle.product_ja,
    }
}

pub fn successor(lifecycle: &Lifecycle, lang: Lang) -> &str {
    match lang {
        Lang::En => &lifecycle.successor,
        Lang::Ja => &lifecycle.successor_ja,
    }
}

/// How to fix the finding on this platform, as one or two sentences.
pub fn fix(finding: &Finding, platform: &str, lang: Lang) -> String {
    use FindingType::*;
    let mac = platform == "macos";
    let text = match finding.finding_type {
        DeviceDiskUnencrypted if mac => lang.pick(
            "Turn on FileVault in System Settings > Privacy & Security > FileVault.",
            "システム設定 > プライバシーとセキュリティ > FileVault で FileVault をオンにしてください。",
        ),
        DeviceDiskUnencrypted => lang.pick(
            "Turn on BitLocker or Device encryption (Settings > Privacy & security > Device encryption).",
            "BitLocker またはデバイスの暗号化をオンにしてください（設定 > プライバシーとセキュリティ > デバイスの暗号化）。",
        ),
        DeviceFirewallOff if mac => lang.pick(
            "Turn on the firewall in System Settings > Network > Firewall.",
            "システム設定 > ネットワーク > ファイアウォール でファイアウォールをオンにしてください。",
        ),
        DeviceFirewallOff => lang.pick(
            "Turn on Microsoft Defender Firewall in Windows Security > Firewall & network protection.",
            "Windows セキュリティ > ファイアウォールとネットワーク保護 で Microsoft Defender ファイアウォールをオンにしてください。",
        ),
        DeviceAppProtectionOff if mac => lang.pick(
            "Allow apps only from the App Store and identified developers (System Settings > Privacy & Security).",
            "システム設定 > プライバシーとセキュリティ で、App Store と確認済みの開発元からのアプリのみを許可してください。",
        ),
        DeviceAppProtectionOff => lang.pick(
            "Turn on Check apps and files in Windows Security > App & browser control > Reputation-based protection.",
            "Windows セキュリティ > アプリとブラウザーのコントロール > 評価ベースの保護 で「アプリとファイルの確認」をオンにしてください。",
        ),
        DeviceSystemIntegrityOff => lang.pick(
            "Start up in macOS Recovery, open Terminal and run csrutil enable, then restart.",
            "macOS 復旧で起動し、ターミナルで csrutil enable を実行してから再起動してください。",
        ),
        DeviceOsEndOfSupport => {
            let to = finding
                .lifecycle
                .as_ref()
                .map(|l| successor(l, lang))
                .filter(|s| !s.is_empty());
            return match (lang, to) {
                (Lang::En, Some(to)) => {
                    format!("Upgrade to {to}, or replace the PC if it cannot run it.")
                }
                (Lang::Ja, Some(to)) => {
                    format!("{to} にアップグレードしてください。対応できない PC は入れ替えを検討してください。")
                }
                (Lang::En, None) => {
                    "Upgrade to a supported OS release, or replace the PC if it cannot run one."
                        .to_string()
                }
                (Lang::Ja, None) => "サポート中の OS にアップグレードしてください。対応できない PC は入れ替えを検討してください。".to_string(),
            };
        }
        DeviceOsUpdatesPending if mac => lang.pick(
            "Install the pending updates in System Settings > General > Software Update.",
            "システム設定 > 一般 > ソフトウェアアップデート で保留中のアップデートをインストールしてください。",
        ),
        DeviceOsUpdatesPending => lang.pick(
            "Install the pending updates in Settings > Windows Update.",
            "設定 > Windows Update で保留中の更新プログラムをインストールしてください。",
        ),
        DeviceAutomaticUpdatesOff if mac => lang.pick(
            "Turn on automatic updates, including Security Responses and system files (System Settings > General > Software Update > Automatic updates).",
            "システム設定 > 一般 > ソフトウェアアップデート > 自動アップデート で、セキュリティ対応とシステムファイルを含む自動アップデートをオンにしてください。",
        ),
        DeviceAutomaticUpdatesOff => lang.pick(
            "Make sure Windows Update is not paused or switched off by policy (Settings > Windows Update).",
            "Windows Update が一時停止やポリシーで無効になっていないか確認してください（設定 > Windows Update）。",
        ),
        DeviceAntivirusOff if mac => lang.pick(
            "Update macOS to restore XProtect, or check that your company's antivirus is running.",
            "macOS をアップデートして XProtect を復元するか、会社指定のウイルス対策ソフトが稼働しているか確認してください。",
        ),
        DeviceAntivirusOff => lang.pick(
            "Turn on real-time protection in Windows Security > Virus & threat protection, or check that your company's antivirus is running.",
            "Windows セキュリティ > ウイルスと脅威の防止 でリアルタイム保護をオンにするか、会社指定のウイルス対策ソフトが稼働しているか確認してください。",
        ),
        DeviceLocalAdmin => lang.pick(
            "Use a standard account for daily work, and keep a separate administrator account for IT tasks.",
            "普段の作業は標準ユーザーで行い、管理作業には別の管理者アカウントを使ってください。",
        ),
        DeviceScreenLockOff if mac => lang.pick(
            "Require a password soon after the screen saver starts or the display turns off (System Settings > Lock Screen).",
            "システム設定 > ロック画面 で、スクリーンセーバー開始後やディスプレイのオフ後すぐにパスワードを要求するよう設定してください。",
        ),
        DeviceScreenLockOff => lang.pick(
            "Set the screen to lock after a few minutes of inactivity (Settings > Accounts > Sign-in options, and the screen saver settings).",
            "数分間操作がないと画面がロックされるよう設定してください（設定 > アカウント > サインイン オプション、およびスクリーンセーバーの設定）。",
        ),
        DeviceUnmanaged if mac => lang.pick(
            "Enroll the Mac in your device management (MDM) service, such as Intune, Jamf or Kandji.",
            "Intune、Jamf、Kandji などのデバイス管理（MDM）にこの Mac を登録してください。",
        ),
        DeviceUnmanaged => lang.pick(
            "Enroll the PC in your device management (MDM) service, such as Intune, or join it to your domain.",
            "Intune などのデバイス管理（MDM）にこの PC を登録するか、ドメインに参加させてください。",
        ),
        DeviceSecureBootOff => lang.pick(
            "Turn on Secure Boot in the PC's UEFI firmware settings.",
            "PC の UEFI ファームウェア設定でセキュアブートをオンにしてください。",
        ),
        SoftwareOutdated => lang.pick(
            "Update to the latest version, and keep automatic updates on.",
            "最新バージョンに更新し、自動アップデートをオンのままにしてください。",
        ),
        SoftwareEndOfSupport => {
            let to = finding
                .lifecycle
                .as_ref()
                .map(|l| successor(l, lang))
                .filter(|s| !s.is_empty());
            return match (lang, to) {
                (Lang::En, Some(to)) => {
                    format!("Move to {to}, or uninstall it if it is no longer needed.")
                }
                (Lang::Ja, Some(to)) => {
                    format!("{to} へ移行するか、不要であればアンインストールしてください。")
                }
                (Lang::En, None) => {
                    "Upgrade to a supported version, or uninstall it if it is no longer needed."
                        .to_string()
                }
                (Lang::Ja, None) => "サポート中のバージョンへ更新するか、不要であればアンインストールしてください。".to_string(),
            };
        }
    };
    text.to_string()
}

/// The finding's evidence in one line: what the check reported, or the
/// matching programs and the vendor's last day of support.
pub fn evidence(finding: &Finding, lang: Lang) -> String {
    let mut parts = Vec::new();
    if let Some(detail) = finding.detail.as_deref().filter(|d| !d.is_empty()) {
        parts.push(check_detail(detail, lang));
    }
    if !finding.programs.is_empty() {
        let mut names: Vec<String> = finding
            .programs
            .iter()
            .map(|p| match &p.version {
                Some(version) => format!("{} {version}", p.name),
                None => p.name.clone(),
            })
            .collect();
        let more = finding.program_count.saturating_sub(finding.programs.len());
        if more > 0 {
            names.push(match lang {
                Lang::En => format!("and {more} more"),
                Lang::Ja => format!("ほか{more}件"),
            });
        }
        parts.push(names.join(", "));
    }
    if let Some(lifecycle) = &finding.lifecycle {
        parts.push(format!(
            "{}{}",
            lang.pick("Last day of support: ", "サポート最終日："),
            lifecycle.support_ends_on
        ));
    }
    parts.join(" · ")
}

/// What a check reported, as the checks word it (English, the wire value).
const CHECK_DETAILS_JA: &[(&str, &str)] = &[
    ("Active Directory domain", "Active Directory ドメインに参加しています"),
    ("Antivirus status unavailable", "ウイルス対策の状態を取得できません"),
    ("Automatic update checks are off", "アップデートの自動確認がオフです"),
    (
        "Automatic updates are turned off by policy",
        "ポリシーで自動アップデートがオフになっています",
    ),
    ("BitLocker is decrypting", "BitLocker が暗号化を解除しています"),
    ("BitLocker is off", "BitLocker がオフです"),
    ("BitLocker is still encrypting", "BitLocker が暗号化を進めています"),
    ("BitLocker protection is suspended", "BitLocker の保護が一時停止しています"),
    ("BitLocker status unavailable", "BitLocker の状態を取得できません"),
    ("Device management status unavailable", "デバイス管理の状態を取得できません"),
    ("Enrolled", "登録されています"),
    ("Enrolled (user approved)", "ユーザーの承認で登録されています"),
    (
        "Enrolled via Automated Device Enrollment",
        "自動デバイス登録で登録されています",
    ),
    ("Firewall status unavailable", "ファイアウォールの状態を取得できません"),
    ("Group membership unavailable", "グループのメンバーシップを取得できません"),
    ("Intune (MDM)", "Intune（MDM）"),
    ("Lock settings unavailable", "画面ロックの設定を取得できません"),
    ("MDM enrollment status unavailable", "MDM の登録状態を取得できません"),
    (
        "macOS updates also install automatically",
        "macOS のアップデートも自動でインストールされます",
    ),
    (
        "macOS updates wait for the user",
        "macOS のアップデートはユーザーの操作を待ちます",
    ),
    ("No antivirus product registered", "ウイルス対策製品が登録されていません"),
    (
        "Not enrolled in MDM or joined to a domain",
        "MDM に登録されておらず、ドメインにも参加していません",
    ),
    ("Not enrolled in MDM", "MDM に登録されていません"),
    (
        "Run as the signed-in user to check this (the scan ran as a system account)",
        "確認するには、サインイン中のユーザーとして実行してください（システムアカウントでスキャンしました）",
    ),
    ("Secure Boot is off", "セキュアブートがオフです"),
    ("Secure Boot status unavailable", "セキュアブートの状態を取得できません"),
    (
        "Security responses do not install automatically",
        "セキュリティ対応が自動でインストールされません",
    ),
    ("SIP is disabled", "SIP が無効です"),
    ("SIP status unavailable", "SIP の状態を取得できません"),
    ("SIP uses a custom configuration", "SIP がカスタム構成になっています"),
    ("SmartScreen status unavailable", "SmartScreen の状態を取得できません"),
    (
        "Software Update settings unavailable",
        "ソフトウェアアップデートの設定を取得できません",
    ),
    ("The signed-in user is a local administrator", "サインイン中のユーザーがローカル管理者です"),
    ("The signed-in user is an administrator", "サインイン中のユーザーが管理者です"),
    ("The Windows Update service is disabled", "Windows Update サービスが無効です"),
    ("Windows Update settings unavailable", "Windows Update の設定を取得できません"),
    ("Windows Update status unavailable", "Windows Update の状態を取得できません"),
    ("XProtect not found", "XProtect が見つかりません"),
];

/// A check's detail in `lang`. The checks report it in English, which stays
/// the wire value (the CSV and connected check-ins keep it); a report in
/// Japanese shows the known ones in Japanese. Product names and versions
/// ("XProtect 5367", "Microsoft Defender Antivirus") pass through.
pub fn check_detail(detail: &str, lang: Lang) -> String {
    if lang == Lang::En {
        return detail.to_string();
    }
    if let Some((_, ja)) = CHECK_DETAILS_JA.iter().find(|(en, _)| *en == detail) {
        return (*ja).to_string();
    }
    if let Some(count) = detail
        .strip_suffix(" update(s) pending")
        .filter(|n| n.parse::<u32>().is_ok())
    {
        return format!("{count}件の更新が保留中です");
    }
    if let Some(off) = detail.strip_prefix("SIP custom configuration, off: ") {
        return format!("SIP がカスタム構成です（無効：{off}）");
    }
    if let Some(products) = detail.strip_prefix("Antivirus off: ") {
        return format!("ウイルス対策がオフです：{products}");
    }
    if let Some(minutes) = detail
        .strip_prefix("No lock within ")
        .and_then(|rest| rest.strip_suffix(" min (policy, screensaver, or sleep + sign-in)"))
    {
        return format!(
            "{minutes}分以内に画面がロックされません（ポリシー、スクリーンセーバー、スリープ後のサインインのいずれも該当しません）"
        );
    }
    if let Some(product) = detail.strip_suffix(" (definitions out of date)") {
        return format!("{product}（定義ファイルが古い状態です）");
    }
    detail.to_string()
}

pub fn severity(severity: Severity, lang: Lang) -> &'static str {
    match severity {
        Severity::High => lang.pick("High", "高"),
        Severity::Medium => lang.pick("Medium", "中"),
        Severity::Low => lang.pick("Low", "低"),
    }
}

pub fn area(area: Area, lang: Lang) -> &'static str {
    match area {
        Area::Device => lang.pick("Device", "デバイス"),
        Area::Software => lang.pick("Software", "ソフトウェア"),
    }
}

pub fn lifecycle_status(status: LifecycleStatus, lang: Lang) -> &'static str {
    match status {
        LifecycleStatus::Ended => lang.pick("Support ended", "サポート終了"),
        LifecycleStatus::Ending => lang.pick("Support ending soon", "サポート終了間近"),
        LifecycleStatus::Supported => lang.pick("Supported", "サポート中"),
    }
}

/// A software row's status: outdated browsers say so rather than "ended".
pub fn software_status(lifecycle: Option<&Lifecycle>, lang: Lang) -> &'static str {
    match lifecycle {
        None => lang.pick("Not assessed", "未判定"),
        Some(l)
            if l.kind == monitor_findings::LifecycleKind::Outdated
                && l.status == LifecycleStatus::Ended =>
        {
            lang.pick("Outdated version", "古いバージョン")
        }
        Some(l) => lifecycle_status(l.status, lang),
    }
}

pub fn check_result(pass: Option<bool>, lang: Lang) -> &'static str {
    match pass {
        Some(true) => lang.pick("Pass", "合格"),
        Some(false) => lang.pick("Fail", "不合格"),
        None => lang.pick("Unknown", "判定不可"),
    }
}

/// A check's name on this platform, phrased as the passing state.
pub fn check_label(id: PostureCheckId, platform: &str, lang: Lang) -> &'static str {
    use PostureCheckId::*;
    let windows = platform == "windows";
    match (id, windows) {
        (DiskEncryption, false) => {
            lang.pick("Disk encrypted (FileVault)", "ディスク暗号化（FileVault）")
        }
        (DiskEncryption, true) => {
            lang.pick("Disk encrypted (BitLocker)", "ディスク暗号化（BitLocker）")
        }
        (Firewall, _) => lang.pick("Firewall on", "ファイアウォール有効"),
        (Gatekeeper, false) => lang.pick("Gatekeeper on", "Gatekeeper 有効"),
        (Gatekeeper, true) => lang.pick("SmartScreen on", "SmartScreen 有効"),
        (ScreenLock, _) => lang.pick("Screen lock set", "画面ロック設定済み"),
        (OsUpdate, _) => lang.pick("OS up to date", "OS 最新"),
        (AutomaticUpdates, _) => lang.pick("Automatic updates on", "自動アップデート有効"),
        (Antivirus, false) => lang.pick("XProtect active", "XProtect 稼働"),
        (Antivirus, true) => lang.pick("Antivirus on", "ウイルス対策有効"),
        (LocalAdmin, _) => lang.pick("No local admin rights", "管理者権限なし"),
        (DeviceManagement, false) => lang.pick("MDM enrolled", "MDM 登録済み"),
        (DeviceManagement, true) => {
            lang.pick("Managed (MDM or domain)", "管理対象（MDM またはドメイン）")
        }
        (SecureBoot, _) => lang.pick("Secure Boot on", "セキュアブート有効"),
        (SystemIntegrity, _) => lang.pick("SIP on", "SIP 有効"),
    }
}

pub fn platform(platform: &str) -> &str {
    match platform {
        "macos" => "macOS",
        "windows" => "Windows",
        other => other,
    }
}

/// The OS as people read it: `macOS 14.6.1`, `Windows 11 24H2 (26100.4351) Professional`.
pub fn os_name(device: &crate::DeviceRecord) -> String {
    let base = if device.platform == "macos" {
        format!("macOS {}", device.os_version)
    } else {
        device.os_version.clone()
    };
    match &device.os_edition {
        Some(edition) => format!("{base} {edition}"),
        None => base,
    }
}

/// Labels shared by the workbook, the HTML page and the app.
pub struct Labels {
    pub report_title: &'static str,
    pub summary: &'static str,
    pub device: &'static str,
    pub devices: &'static str,
    pub software: &'static str,
    pub findings: &'static str,
    pub checks: &'static str,
    pub scanned_at: &'static str,
    pub hostname: &'static str,
    pub model: &'static str,
    pub serial: &'static str,
    pub os: &'static str,
    pub edition: &'static str,
    pub os_support: &'static str,
    pub os_support_ends: &'static str,
    pub asset_tag: &'static str,
    pub user: &'static str,
    pub department: &'static str,
    pub location: &'static str,
    pub notes: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub publisher: &'static str,
    pub source: &'static str,
    pub identifier: &'static str,
    pub support_status: &'static str,
    pub support_ends: &'static str,
    pub recommended: &'static str,
    pub vendor_source: &'static str,
    pub number: &'static str,
    pub severity: &'static str,
    pub area: &'static str,
    pub risk: &'static str,
    pub evidence: &'static str,
    pub fix: &'static str,
    pub status: &'static str,
    pub owner: &'static str,
    pub check: &'static str,
    pub result: &'static str,
    pub detail: &'static str,
    pub high: &'static str,
    pub medium: &'static str,
    pub low: &'static str,
    pub open_findings: &'static str,
    pub statuses: [&'static str; 4],
    pub agent_version: &'static str,
    pub catalog_verified: &'static str,
    pub programs: &'static str,
    pub programs_past_support: &'static str,
    pub checks_passed: &'static str,
    pub made_locally: &'static str,
    pub no_findings_caveat: &'static str,
    pub top_findings: &'static str,
    pub no_findings: &'static str,
    pub legacy_title: &'static str,
    pub legacy_body: &'static str,
    pub legacy_none: &'static str,
    pub assessment: &'static str,
    pub count_suffix: &'static str,
}

pub fn labels(lang: Lang) -> Labels {
    match lang {
        Lang::En => Labels {
            report_title: "IT risk report",
            summary: "Summary",
            device: "Device",
            devices: "Devices",
            software: "Software",
            findings: "Risks",
            checks: "Security checks",
            scanned_at: "Scanned at",
            hostname: "Computer name",
            model: "Model",
            serial: "Serial number",
            os: "OS",
            edition: "Edition",
            os_support: "OS support",
            os_support_ends: "OS last day of support",
            asset_tag: "Asset tag",
            user: "User",
            department: "Department",
            location: "Location",
            notes: "Notes",
            name: "Name",
            version: "Version",
            publisher: "Publisher",
            source: "Type",
            identifier: "Identifier",
            support_status: "Support status",
            support_ends: "Last day of support",
            recommended: "Recommended action",
            vendor_source: "Vendor source",
            number: "No.",
            severity: "Severity",
            area: "Area",
            risk: "Risk",
            evidence: "Evidence",
            fix: "How to fix",
            status: "Status",
            owner: "Owner",
            check: "Check",
            result: "Result",
            detail: "Detail",
            high: "High",
            medium: "Medium",
            low: "Low",
            open_findings: "Risks found",
            statuses: ["Open", "In progress", "Accepted risk", "Resolved"],
            agent_version: "Sanka Monitor version",
            catalog_verified: "Lifecycle data checked on",
            programs: "Installed programs",
            programs_past_support: "Programs past support",
            checks_passed: "Checks passed",
            made_locally: "Created on this computer by Sanka Monitor. Nothing was sent to Sanka or anyone else; you decide whether to share this file.",
            no_findings_caveat: "Findings come from vendor support dates and the device's own settings. No finding does not prove a device is safe.",
            top_findings: "Top risks",
            no_findings: "No risks found on this device.",
            legacy_title: "Legacy systems and migration candidates",
            legacy_body: "Software whose vendor support has ended or ends soon, with the vendor's recommended next step.",
            legacy_none: "No installed software is past or near the end of vendor support.",
            assessment: "Send this report to your Sanka contact for a free migration assessment: sanka.com",
            count_suffix: "",
        },
        Lang::Ja => Labels {
            report_title: "ITリスクレポート",
            summary: "概要",
            device: "端末",
            devices: "端末台帳",
            software: "ソフトウェア台帳",
            findings: "リスク一覧",
            checks: "セキュリティチェック",
            scanned_at: "スキャン日時",
            hostname: "コンピューター名",
            model: "機種",
            serial: "シリアル番号",
            os: "OS",
            edition: "エディション",
            os_support: "OSサポート状況",
            os_support_ends: "OSサポート最終日",
            asset_tag: "管理番号",
            user: "使用者",
            department: "部署",
            location: "設置場所",
            notes: "備考",
            name: "名称",
            version: "バージョン",
            publisher: "提供元",
            source: "種別",
            identifier: "識別子",
            support_status: "サポート状況",
            support_ends: "サポート最終日",
            recommended: "推奨する対応",
            vendor_source: "提供元の情報",
            number: "No.",
            severity: "重要度",
            area: "対象",
            risk: "リスク",
            evidence: "根拠",
            fix: "対処方法",
            status: "ステータス",
            owner: "担当者",
            check: "チェック項目",
            result: "結果",
            detail: "詳細",
            high: "高",
            medium: "中",
            low: "低",
            open_findings: "検出したリスク",
            statuses: ["未対応", "対応中", "リスク受容", "解消済み"],
            agent_version: "Sanka Monitor バージョン",
            catalog_verified: "ライフサイクル情報の確認日",
            programs: "インストール済みソフトウェア",
            programs_past_support: "サポート終了のソフトウェア",
            checks_passed: "合格したチェック",
            made_locally: "このレポートは Sanka Monitor がこのコンピューター上で作成しました。Sanka を含め、どこにも送信していません。共有するかどうかはご自身で決められます。",
            no_findings_caveat: "リスクは提供元のサポート期限と端末の設定から判定しています。リスクが表示されなくても、端末が安全とは限りません。",
            top_findings: "主なリスク",
            no_findings: "この端末ではリスクを検出しませんでした。",
            legacy_title: "レガシーシステムと移行候補",
            legacy_body: "提供元のサポートが終了した、または終了間近のソフトウェアと、提供元が推奨する移行先です。",
            legacy_none: "サポート終了、または終了間近のソフトウェアはありません。",
            assessment: "このレポートを Sanka の担当者に送ると、無料で移行アセスメントを受けられます：sanka.com",
            count_suffix: "件",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags_pick_the_language() {
        for (tag, lang) in [
            ("ja", Lang::Ja),
            ("ja-JP", Lang::Ja),
            ("ja_JP.UTF-8", Lang::Ja),
            ("JA", Lang::Ja),
            ("en-US", Lang::En),
            ("", Lang::En),
            ("jv", Lang::En),
        ] {
            assert_eq!(Lang::from_tag(tag), lang, "{tag}");
        }
    }

    #[test]
    fn lifecycle_fixes_name_the_vendors_successor() {
        let report = crate::fixtures::windows_report();
        let find = |key: &str| {
            report
                .findings
                .iter()
                .find(|f| f.subject_key == key || f.finding_type.as_str() == key)
                .unwrap()
                .clone()
        };
        let office = find("office-2016");
        let os = find("device_os_end_of_support");
        for (finding, lang, successor) in [
            (&office, Lang::En, "Microsoft 365 Apps or Office 2024"),
            (&office, Lang::Ja, "Microsoft 365 アプリまたは Office 2024"),
            (&os, Lang::En, "Windows 11"),
        ] {
            assert!(
                fix(finding, "windows", lang).contains(successor),
                "{lang:?}"
            );
        }
        // Without a successor the fix still reads as a sentence.
        let mut bare = office.clone();
        bare.lifecycle.as_mut().unwrap().successor.clear();
        assert!(!fix(&bare, "windows", Lang::En).contains("Move to ,"));
    }

    #[test]
    fn check_details_read_in_the_reports_language() {
        // (detail as the check reports it, Japanese)
        for (detail, ja) in [
            ("BitLocker is off", "BitLocker がオフです"),
            ("1 update(s) pending", "1件の更新が保留中です"),
            ("12 update(s) pending", "12件の更新が保留中です"),
            (
                "SIP custom configuration, off: Filesystem Protections",
                "SIP がカスタム構成です（無効：Filesystem Protections）",
            ),
            (
                "Antivirus off: Norton 360",
                "ウイルス対策がオフです：Norton 360",
            ),
            (
                "Microsoft Defender Antivirus (definitions out of date)",
                "Microsoft Defender Antivirus（定義ファイルが古い状態です）",
            ),
            // Product names and versions are not translated.
            ("XProtect 5367", "XProtect 5367"),
            ("many update(s) pending", "many update(s) pending"),
        ] {
            assert_eq!(check_detail(detail, Lang::Ja), ja, "{detail}");
            assert_eq!(check_detail(detail, Lang::En), detail);
        }
        assert!(check_detail(
            "No lock within 30 min (policy, screensaver, or sleep + sign-in)",
            Lang::Ja
        )
        .starts_with("30分以内に"));
    }
}
