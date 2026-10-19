//! Writes the reports for a sample Windows 10 PC, to review the layouts
//! without a Windows machine:
//!
//! ```text
//! cargo run -p monitor-report --example sample_reports -- <folder>
//! ```
//!
//! The PC runs Windows 10 with Office 2016, SharePoint Server 2019, an old
//! Chrome and an unencrypted disk; nothing is scanned or sent.

use chrono::{Local, Utc};
use monitor_findings::Catalog;
use monitor_inventory::{InstalledSoftware, SoftwareSource};
use monitor_posture::{PostureCheckId, PostureCheckResult};
use monitor_report::{DeviceRecord, Lang, ScanReport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: sample_reports <folder>")?;
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let now = Utc::now();
    let check = |id, pass, detail: Option<&str>| PostureCheckResult {
        id,
        pass,
        detail: detail.map(str::to_string),
        collected_at: now,
    };
    let program = |name: &str, version: &str, publisher: &str| {
        InstalledSoftware::new(
            SoftwareSource::WindowsRegistry,
            &format!("{{{name}}}"),
            name,
            Some(version),
            Some(publisher),
        )
        .expect("valid item")
    };
    let report = ScanReport::build(
        DeviceRecord {
            hostname: "SALES-PC-07".into(),
            platform: "windows".into(),
            os_version: "Windows 10 22H2 (19045.6456)".into(),
            os_edition: Some("Professional".into()),
            device_model: Some("LENOVO 20XW".into()),
            serial_number: Some("PF2ABC12".into()),
        },
        vec![
            check(PostureCheckId::DiskEncryption, Some(false), None),
            check(PostureCheckId::Firewall, Some(true), None),
            check(PostureCheckId::Gatekeeper, Some(true), None),
            check(PostureCheckId::ScreenLock, Some(true), None),
            check(PostureCheckId::OsUpdate, Some(true), None),
            check(PostureCheckId::AutomaticUpdates, Some(true), None),
            check(
                PostureCheckId::Antivirus,
                Some(true),
                Some("Microsoft Defender Antivirus"),
            ),
            check(PostureCheckId::LocalAdmin, Some(false), None),
            check(PostureCheckId::DeviceManagement, Some(false), None),
            check(PostureCheckId::SecureBoot, Some(true), None),
        ],
        vec![
            program(
                "Microsoft Office Professional Plus 2016",
                "16.0.4266.1001",
                "Microsoft Corporation",
            ),
            program(
                "Microsoft SharePoint Server 2019",
                "16.0.10337.12109",
                "Microsoft Corporation",
            ),
            program("Google Chrome", "140.0.7339.81", "Google LLC"),
            program("弥生給与 26", "31.0.1", "弥生株式会社"),
            program("7-Zip 24.08 (x64)", "24.08", "Igor Pavlov"),
        ],
        Catalog::bundled(),
        now.with_timezone(&Local).date_naive(),
        now,
        env!("CARGO_PKG_VERSION"),
    );
    for lang in [Lang::En, Lang::Ja] {
        let stem = out.join(format!("{}-{}", report.file_stem(), lang.code()));
        std::fs::write(
            stem.with_extension("xlsx"),
            monitor_report::xlsx::write(std::slice::from_ref(&report), lang)?,
        )?;
        std::fs::write(
            stem.with_extension("html"),
            monitor_report::html::write(&report, lang),
        )?;
    }
    std::fs::write(
        out.join(format!("{}.csv", report.file_stem())),
        monitor_report::csv::write(&report),
    )?;
    println!("wrote {}", out.display());
    Ok(())
}
