//! The one-page IT risk report: a self-contained HTML page (no scripts, no
//! external fonts or images) that reads on screen and prints on one A4
//! sheet, so it can be saved as PDF from any browser. Sections: the risk
//! summary, the top eight risks with their fix, the device and its checks, and
//! "Legacy systems and migration candidates".

use monitor_findings::{FindingType, LifecycleStatus, Severity};

use crate::copy::{self, Lang};
use crate::ScanReport;

const TOP_FINDINGS: usize = 8;
const LEGACY_ROWS: usize = 5;

/// Escapes text for HTML element content and attribute values.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = r#"
:root { --ink:#101828; --muted:#475467; --line:#D0D5DD; --high:#B42318; --medium:#B54708; --low:#475467; --ok:#067647; --accent:#1F2A44; }
* { box-sizing:border-box; }
body { margin:0; background:#F2F4F7; color:var(--ink); font:12px/1.45 -apple-system, "Segoe UI", "Hiragino Sans", "Yu Gothic UI", Meiryo, sans-serif; }
.page { width:210mm; min-height:297mm; margin:16px auto; padding:14mm 14mm 10mm; background:#fff; }
header { display:flex; justify-content:space-between; align-items:flex-end; border-bottom:2px solid var(--accent); padding-bottom:6px; }
h1 { font-size:20px; margin:0; }
h2 { font-size:13px; margin:14px 0 6px; color:var(--accent); }
.meta { color:var(--muted); text-align:right; font-size:11px; }
.tiles { display:grid; grid-template-columns:repeat(5,1fr); gap:6px; margin-top:10px; }
.tile { border:1px solid var(--line); border-radius:6px; padding:6px 8px; }
.tile b { display:block; font-size:20px; }
.tile.high b { color:var(--high); } .tile.medium b { color:var(--medium); } .tile.low b { color:var(--low); }
table { width:100%; border-collapse:collapse; }
th, td { text-align:left; vertical-align:top; padding:4px 6px; border-bottom:1px solid var(--line); }
th { font-size:10.5px; color:var(--muted); font-weight:600; white-space:nowrap; }
.sev { font-weight:700; white-space:nowrap; }
.sev.high { color:var(--high); } .sev.medium { color:var(--medium); } .sev.low { color:var(--low); }
.fix { color:var(--muted); }
.cols { display:grid; grid-template-columns:1fr 1fr; gap:14px; }
dl { display:grid; grid-template-columns:auto 1fr; gap:2px 10px; margin:0; }
dt { color:var(--muted); } dd { margin:0; }
ul.checks { list-style:none; margin:0; padding:0; columns:2; }
ul.checks li { padding:1px 0; break-inside:avoid; }
.pass { color:var(--ok); } .fail { color:var(--high); } .unknown { color:var(--muted); }
.legacy { border:1px solid var(--line); border-radius:6px; padding:8px 10px; margin-top:14px; }
.legacy h2 { margin-top:0; }
.cta { margin-top:6px; font-weight:600; }
footer { margin-top:12px; color:var(--muted); font-size:10px; border-top:1px solid var(--line); padding-top:6px; }
@page { size:A4; margin:0; }
@media print {
  body { background:#fff; font-size:10.5px; }
  .page { margin:0; min-height:auto; padding:10mm 12mm 8mm; }
  h1 { font-size:18px; } h2 { margin:10px 0 4px; }
  .tile { padding:4px 8px; } .tile b { font-size:16px; }
  th, td { padding:3px 5px; }
  tr, .legacy { break-inside:avoid; }
}
"#;

fn severity_class(severity: Severity) -> &'static str {
    severity.as_str()
}

/// Writes the one-page report for one device.
pub fn write(report: &ScanReport, lang: Lang) -> String {
    let l = copy::labels(lang);
    let device = &report.device;
    let platform = device.platform.as_str();
    let counts = report.counts();
    let os = report.os_lifecycle.as_ref();
    let scanned = report
        .scanned_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string();
    let mut html = String::new();
    html.push_str(&format!(
        "<!doctype html>\n<html lang=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{} — {}</title><style>{STYLE}</style></head><body><main class=\"page\">",
        lang.code(),
        esc(l.report_title),
        esc(&device.hostname)
    ));

    html.push_str(&format!(
        "<header><div><h1>{}</h1><div>{}</div></div><div class=\"meta\">{}: {}<br>Sanka Monitor {}</div></header>",
        esc(l.report_title),
        esc(&device.hostname),
        esc(l.scanned_at),
        esc(&scanned),
        esc(&report.agent_version)
    ));

    // Summary tiles
    let tile = |class: &str, value: String, label: &str| {
        format!(
            "<div class=\"tile {class}\"><b>{}</b>{}</div>",
            esc(&value),
            esc(label)
        )
    };
    html.push_str("<div class=\"tiles\">");
    html.push_str(&tile("high", counts.high.to_string(), l.high));
    html.push_str(&tile("medium", counts.medium.to_string(), l.medium));
    html.push_str(&tile("low", counts.low.to_string(), l.low));
    html.push_str(&tile(
        "",
        format!(
            "{}/{}",
            counts.checks_passed,
            counts.checks_passed + counts.checks_failed + counts.checks_unknown
        ),
        l.checks_passed,
    ));
    html.push_str(&tile(
        "",
        format!("{}/{}", counts.programs_past_support, counts.programs),
        l.programs_past_support,
    ));
    html.push_str("</div>");

    // Top risks
    html.push_str(&format!("<h2>{}</h2>", esc(l.top_findings)));
    if report.findings.is_empty() {
        html.push_str(&format!("<p>{}</p>", esc(l.no_findings)));
    } else {
        html.push_str(&format!(
            "<table><thead><tr><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
            esc(l.severity),
            esc(l.risk),
            esc(l.fix)
        ));
        for finding in report.findings.iter().take(TOP_FINDINGS) {
            let evidence = copy::evidence(finding, lang);
            html.push_str(&format!(
                "<tr><td class=\"sev {}\">{}</td><td><b>{}</b>{}</td><td class=\"fix\">{}</td></tr>",
                severity_class(finding.severity),
                esc(copy::severity(finding.severity, lang)),
                esc(&copy::headline(finding, lang)),
                if evidence.is_empty() {
                    String::new()
                } else {
                    format!("<br><span class=\"fix\">{}</span>", esc(&evidence))
                },
                esc(&copy::fix(finding, platform, lang))
            ));
        }
        html.push_str("</tbody></table>");
        let more = report.findings.len().saturating_sub(TOP_FINDINGS);
        if more > 0 {
            html.push_str(&format!(
                "<p class=\"fix\">+{more}{} — {}</p>",
                esc(l.count_suffix),
                esc(l.findings)
            ));
        }
    }

    // Device and checks
    html.push_str("<div class=\"cols\"><section>");
    html.push_str(&format!("<h2>{}</h2><dl>", esc(l.device)));
    let mut fact = |label: &str, value: &str| {
        if !value.is_empty() {
            html.push_str(&format!("<dt>{}</dt><dd>{}</dd>", esc(label), esc(value)));
        }
    };
    fact(l.hostname, &device.hostname);
    fact(l.model, device.device_model.as_deref().unwrap_or_default());
    fact(
        l.serial,
        device.serial_number.as_deref().unwrap_or_default(),
    );
    fact(l.os, &copy::os_name(device));
    if let Some(os) = &os {
        fact(
            l.os_support,
            &format!(
                "{} ({} {})",
                copy::lifecycle_status(os.status, lang),
                l.support_ends,
                os.support_ends_on
            ),
        );
    }
    fact(l.programs, &counts.programs.to_string());
    html.push_str("</dl></section><section>");
    html.push_str(&format!("<h2>{}</h2><ul class=\"checks\">", esc(l.checks)));
    for check in &report.checks {
        let (mark, class) = match check.pass {
            Some(true) => ("✓", "pass"),
            Some(false) => ("✕", "fail"),
            None => ("–", "unknown"),
        };
        html.push_str(&format!(
            "<li><span class=\"{class}\">{mark}</span> {}</li>",
            esc(copy::check_label(check.id, platform, lang))
        ));
    }
    html.push_str("</ul></section></div>");

    // Legacy systems and migration candidates
    let mut legacy: Vec<_> = report
        .findings
        .iter()
        .filter(|f| {
            f.finding_type != FindingType::SoftwareOutdated
                && f.lifecycle
                    .as_ref()
                    .is_some_and(|l| l.status != LifecycleStatus::Supported)
        })
        .collect();
    legacy.truncate(LEGACY_ROWS);
    html.push_str(&format!(
        "<section class=\"legacy\"><h2>{}</h2><p class=\"fix\">{}</p>",
        esc(l.legacy_title),
        esc(l.legacy_body)
    ));
    if legacy.is_empty() {
        html.push_str(&format!("<p>{}</p>", esc(l.legacy_none)));
    } else {
        html.push_str(&format!(
            "<table><thead><tr><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
            esc(l.name),
            esc(l.support_ends),
            esc(l.recommended)
        ));
        for finding in legacy {
            let lifecycle = finding.lifecycle.as_ref().expect("filtered above");
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(copy::product(lifecycle, lang)),
                esc(&lifecycle.support_ends_on),
                esc(copy::successor(lifecycle, lang))
            ));
        }
        html.push_str("</tbody></table>");
    }
    html.push_str(&format!(
        "<p class=\"cta\">{}</p></section>",
        esc(l.assessment)
    ));

    html.push_str(&format!(
        "<footer>{} {}<br>{}: {}</footer>",
        esc(l.made_locally),
        esc(l.no_findings_caveat),
        esc(l.catalog_verified),
        report
            .catalog_verified_on
            .map(|d| d.to_string())
            .unwrap_or_default()
    ));
    html.push_str("</main></body></html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn device_text_is_escaped_and_nothing_loads_from_elsewhere() {
        let mut report = fixtures::windows_report();
        report.device.hostname = "<script>alert(1)</script>".into();
        let html = write(&report, Lang::En);
        assert!(!html.contains("<script"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        for external in ["src=", "href=", "@import", "url("] {
            assert!(!html.contains(external), "{external}");
        }
    }

    #[test]
    fn the_page_lists_the_top_risks_and_the_legacy_software() {
        let report = fixtures::windows_report();
        for lang in [Lang::En, Lang::Ja] {
            let html = write(&report, lang);
            for finding in &report.findings {
                assert!(
                    html.contains(&esc(&copy::headline(finding, lang))),
                    "{lang:?}"
                );
            }
            // Office 2016 is a migration candidate; an outdated Chrome is not.
            let legacy = html.split("class=\"legacy\"").nth(1).unwrap();
            assert!(legacy.contains("Office 2016"), "{lang:?}");
            assert!(!legacy.contains("Chrome"), "{lang:?}");
        }
    }
}
