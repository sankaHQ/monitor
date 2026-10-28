//! Raw observations for Sanka's internal review, separate from commercial recommendations.

use crate::ScanReport;
use monitor_posture::PostureCheckId;
use serde_json::{json, Value};

pub fn review_evidence(report: &ScanReport, accepted: &[PostureCheckId]) -> Value {
    json!({
        "schemaVersion": 1,
        "reportId": report.report_id,
        "scannedAt": report.scanned_at,
        "agentVersion": report.agent_version,
        "catalogVerifiedOn": report.catalog_verified_on,
        "platform": report.device.platform,
        "osVersion": report.device.os_version,
        "osEdition": report.device.os_edition,
        "checks": report.checks.iter().map(|check| json!({
            "id": check.id,
            "pass": check.pass,
            "accepted": matches!(check.id, PostureCheckId::LocalAdmin | PostureCheckId::DeviceManagement)
                && accepted.contains(&check.id),
        })).collect::<Vec<_>>(),
        "software": report.software.iter().map(|item| &item.item).collect::<Vec<_>>(),
        "saas": report.saas.iter().map(|item| json!({
            "key": item.key, "allowed": item.allowed,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observations_keep_measured_results_without_identity_or_individual_history() {
        let mut report = crate::fixtures::windows_report_with_saas();
        report.device.hostname = "PRIVATE-HOST".into();
        report.device.serial_number = Some("PRIVATE-SERIAL".into());
        for check in &mut report.checks {
            check.detail = Some("PRIVATE-DETAIL".into());
            if check.id == PostureCheckId::LocalAdmin {
                check.pass = Some(false);
            }
        }
        report.checks.push(monitor_posture::PostureCheckResult {
            id: PostureCheckId::LocalAdmin,
            pass: Some(false),
            detail: Some("PRIVATE-DETAIL".into()),
            collected_at: report.scanned_at,
        });
        let evidence = review_evidence(&report, &[PostureCheckId::LocalAdmin]);
        let text = evidence.to_string();
        for forbidden in [
            "PRIVATE-HOST",
            "PRIVATE-SERIAL",
            "PRIVATE-DETAIL",
            "lastUsedOn",
            "daysUsed30",
            "opportunities",
        ] {
            assert!(!text.contains(forbidden), "{forbidden}");
        }
        let admin = evidence["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "local_admin")
            .unwrap();
        assert_eq!(admin["pass"], false);
        assert_eq!(admin["accepted"], true);
    }
}
