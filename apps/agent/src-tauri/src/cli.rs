//! The command-line scan, for device management tools (Intune, Jamf, Group
//! Policy, any scheduler):
//!
//! ```text
//! sanka-monitor-agent scan --out <folder> [--format csv,xlsx,html|all] [--lang ja|en] [--allowlist <file>]
//! sanka-monitor-agent saas-catalog
//! ```
//!
//! It scans the device and writes one file per format to the folder, named
//! after the device (`<computer>_<serial>.csv`), replacing that device's
//! previous file, so the folder holds the latest scan of every PC. Each
//! file is written under a temporary name and renamed into place, so a
//! reader of the folder never sees half a file. It starts no window, tray,
//! updater or background work, reads and writes no app data, and sends
//! nothing anywhere.
//!
//! The SaaS it lists come from installed desktop apps only: it never reads
//! browser history, because device management runs it without the person.
//! The company allowlist is `--allowlist <file>`, else the machine-wide file
//! device management places (`monitor_saas::allowlist::machine_path`).
//! `saas-catalog` prints the catalog's keys, for writing that file.
//!
//! Exit codes: 0 written, 1 a file could not be written, 2 bad arguments or
//! an allowlist that cannot be read.

use std::path::{Path, PathBuf};

use chrono::Utc;
use monitor_report::{copy, device_file_key, Lang, ScanReport};
use monitor_saas::{Allowlist, AllowlistOrigin, SaasCatalog};

use crate::agent::AGENT_VERSION;

const USAGE: &str = "Sanka Monitor command-line scan

Usage:
  sanka-monitor-agent scan --out <folder> [--format csv,xlsx,html|all] [--lang ja|en]
                           [--allowlist <file>]
  sanka-monitor-agent saas-catalog

Scans this computer and writes its report to <folder>, one file per format,
named <computer>_<serial>.<ext>. A later scan replaces the computer's file.
Nothing is sent anywhere.

The report lists the SaaS whose desktop apps are installed. The command line
never reads browser history; the browser check is the person's choice in the
app. With a company allowlist, SaaS that are not on it are flagged as risks.

Options:
  --out <folder>       Where to write (created if missing). A network share works.
  --format <list>      csv (default), xlsx, html, or all. Comma-separated.
  --lang <ja|en>       Language of the xlsx and html reports (default: from LANG,
                       else en). The CSV has both languages.
  --allowlist <file>   The company's allowed SaaS (JSON). Default: the file device
                       management places at
                         macOS:   /Library/Application Support/Sanka Monitor/allowlist.json
                         Windows: %ProgramData%\\Sanka Monitor\\allowlist.json
  -h, --help           Show this help.
  -V, --version        Show the version.

saas-catalog prints every SaaS the catalog knows (key, name, vendor, category,
AI, Sanka migration page) as CSV. An allowlist names the allowed ones by key:
  {\"schemaVersion\": 1, \"name\": \"Example K.K.\", \"allowed\": [\"slack\", \"zoom\"]}

Exit codes: 0 written, 1 a file could not be written, 2 bad arguments or an
allowlist that cannot be read.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Xlsx,
    Html,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Xlsx => "xlsx",
            Format::Html => "html",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanArgs {
    pub out: PathBuf,
    pub formats: Vec<Format>,
    pub lang: Lang,
    pub allowlist: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Scan(ScanArgs),
    SaasCatalog,
    Help,
    Version,
}

/// The command in `args` (program name first), or `None` when they are the
/// app's own (no arguments, `--background`, a deep link): the app starts.
pub fn parse(args: &[String], env_lang: Option<&str>) -> Option<Result<Command, String>> {
    match args.get(1).map(String::as_str) {
        Some("scan") => Some(parse_scan(&args[2..], env_lang).map(Command::Scan)),
        Some("saas-catalog") => Some(Ok(Command::SaasCatalog)),
        Some("-h" | "--help" | "help") => Some(Ok(Command::Help)),
        Some("-V" | "--version") => Some(Ok(Command::Version)),
        _ => None,
    }
}

fn parse_scan(args: &[String], env_lang: Option<&str>) -> Result<ScanArgs, String> {
    let mut out = None;
    let mut allowlist = None;
    let mut formats = vec![Format::Csv];
    let mut lang = env_lang.map(Lang::from_tag).unwrap_or_default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| rest.next().cloned())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--allowlist" => allowlist = Some(PathBuf::from(value("--allowlist")?)),
            "--format" => {
                formats = Vec::new();
                for name in value("--format")?.split(',').map(str::trim) {
                    match name.to_ascii_lowercase().as_str() {
                        "csv" => formats.push(Format::Csv),
                        "xlsx" | "excel" => formats.push(Format::Xlsx),
                        "html" => formats.push(Format::Html),
                        "all" => formats.extend([Format::Csv, Format::Xlsx, Format::Html]),
                        other => return Err(format!("unknown format {other:?}")),
                    }
                }
                let mut seen = Vec::new();
                formats.retain(|format| {
                    let first = !seen.contains(format);
                    seen.push(*format);
                    first
                });
            }
            "--lang" => {
                lang = match value("--lang")?.to_ascii_lowercase().as_str() {
                    "ja" => Lang::Ja,
                    "en" => Lang::En,
                    other => return Err(format!("unknown language {other:?}; use ja or en")),
                }
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    let out = out.ok_or("--out <folder> is required")?;
    Ok(ScanArgs {
        out,
        formats,
        lang,
        allowlist,
    })
}

/// The allowlist a command-line scan uses: `--allowlist`, which must be
/// readable, else the machine-wide file if there is one.
pub fn scan_allowlist(args: &ScanArgs) -> Result<Option<Allowlist>, String> {
    let catalog = SaasCatalog::bundled();
    if let Some(path) = &args.allowlist {
        return Allowlist::read(path, catalog, AllowlistOrigin::CommandLine).map(Some);
    }
    let machine: Vec<(PathBuf, AllowlistOrigin)> = monitor_saas::allowlist::machine_path()
        .map(|path| (path, AllowlistOrigin::Machine))
        .into_iter()
        .collect();
    monitor_saas::allowlist::find(&machine, catalog).transpose()
}

/// The catalog as CSV: what an allowlist can name.
pub fn saas_catalog_csv() -> String {
    let quote = |cell: &str| {
        if cell.contains([',', '"']) {
            format!("\"{}\"", cell.replace('"', "\"\""))
        } else {
            cell.to_string()
        }
    };
    let mut out = String::from("key,product,vendor,category,ai,sanka_migrate_url\n");
    for entry in SaasCatalog::bundled().entries() {
        let cells = [
            entry.key.clone(),
            entry.product.clone(),
            entry.vendor.clone(),
            copy::saas_category(entry.category, Lang::En).to_string(),
            entry.ai.to_string(),
            entry.migrate_url("en").unwrap_or_default(),
        ];
        let line: Vec<String> = cells.iter().map(|cell| quote(cell)).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

/// Encodes the report in `format`.
pub fn encode(report: &ScanReport, format: Format, lang: Lang) -> Result<Vec<u8>, String> {
    match format {
        Format::Csv => Ok(monitor_report::csv::write(report)),
        Format::Xlsx => monitor_report::xlsx::write(std::slice::from_ref(report), lang)
            .map_err(|err| err.to_string()),
        Format::Html => Ok(monitor_report::html::write(report, lang).into_bytes()),
    }
}

/// Writes `bytes` to `path` through a temporary file in the same folder, so
/// the file appears whole or not at all (a `.tmp` name is not a report).
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Writes `report` to `args.out`, one file per format; returns the paths.
pub fn write_report(report: &ScanReport, args: &ScanArgs) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(&args.out)
        .map_err(|err| format!("cannot create {}: {err}", args.out.display()))?;
    let key = device_file_key(&report.device);
    let mut written = Vec::new();
    for format in &args.formats {
        let path = args.out.join(format!("{key}.{}", format.extension()));
        let bytes = encode(report, *format, args.lang)?;
        write_whole(&path, &bytes)
            .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// Lets a release build on Windows (a GUI program, with no console of its
/// own) print to the console it was started from.
fn attach_parent_console() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Runs a command-line invocation and returns the process exit code.
pub fn run(command: Result<Command, String>) -> i32 {
    attach_parent_console();
    match command {
        Ok(Command::Help) => {
            print!("{USAGE}");
            0
        }
        Ok(Command::Version) => {
            println!("Sanka Monitor {AGENT_VERSION}");
            0
        }
        Ok(Command::SaasCatalog) => {
            print!("{}", saas_catalog_csv());
            0
        }
        Err(err) => {
            eprintln!("sanka-monitor: {err}\n\n{USAGE}");
            2
        }
        Ok(Command::Scan(args)) => {
            let allowlist = match scan_allowlist(&args) {
                Ok(list) => list,
                Err(err) => {
                    eprintln!("sanka-monitor: {err}");
                    return 2;
                }
            };
            if let Some(list) = allowlist.as_ref().filter(|l| !l.unknown.is_empty()) {
                eprintln!(
                    "sanka-monitor: warning: the allowlist names SaaS the catalog does not know: {}",
                    list.unknown.join(", ")
                );
            }
            let mut report = crate::scan::run_scan(Utc::now());
            report.set_saas(SaasCatalog::bundled(), None, allowlist.as_ref());
            if report.software.is_empty() {
                eprintln!("sanka-monitor: warning: no installed software could be read");
            }
            match write_report(&report, &args) {
                Ok(paths) => {
                    let counts = report.counts();
                    for path in paths {
                        println!("{}", path.display());
                    }
                    println!(
                        "{}: {} high, {} medium, {} low risks; {} programs; {} SaaS ({} not allowed)",
                        report.device.hostname,
                        counts.high,
                        counts.medium,
                        counts.low,
                        counts.programs,
                        counts.saas,
                        counts.saas_not_allowed
                    );
                    0
                }
                Err(err) => {
                    eprintln!("sanka-monitor: {err}");
                    1
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("sanka-monitor-agent")
            .chain(list.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn only_a_command_leaves_the_app_to_the_cli() {
        // (arguments, LANG, parsed)
        let scan = |out: &str, formats: Vec<Format>, lang| {
            Some(Ok(Command::Scan(ScanArgs {
                out: PathBuf::from(out),
                formats,
                lang,
                allowlist: None,
            })))
        };
        for (list, env, parsed) in [
            (vec![], None, None),
            (vec!["--background"], None, None),
            (vec!["sanka-monitor://enroll?code=x"], None, None),
            (
                vec!["scan", "--out", r"\\fs01\monitor$"],
                None,
                scan(r"\\fs01\monitor$", vec![Format::Csv], Lang::En),
            ),
            (
                vec!["scan", "--out=/tmp/r", "--format", "xlsx,html"],
                Some("ja_JP.UTF-8"),
                scan("/tmp/r", vec![Format::Xlsx, Format::Html], Lang::Ja),
            ),
            (
                vec!["scan", "--format=all", "--lang", "en", "--out", "out"],
                Some("ja_JP.UTF-8"),
                scan(
                    "out",
                    vec![Format::Csv, Format::Xlsx, Format::Html],
                    Lang::En,
                ),
            ),
            (vec!["--help"], None, Some(Ok(Command::Help))),
            (vec!["-V"], None, Some(Ok(Command::Version))),
            (vec!["saas-catalog"], None, Some(Ok(Command::SaasCatalog))),
            (
                vec![
                    "scan",
                    "--out",
                    "out",
                    "--allowlist",
                    r"\\fs01\it\allowlist.json",
                ],
                None,
                Some(Ok(Command::Scan(ScanArgs {
                    out: PathBuf::from("out"),
                    formats: vec![Format::Csv],
                    lang: Lang::En,
                    allowlist: Some(PathBuf::from(r"\\fs01\it\allowlist.json")),
                }))),
            ),
        ] {
            assert_eq!(parse(&args(&list), env), parsed, "{list:?}");
        }
        for bad in [
            vec!["scan"],
            vec!["scan", "--out"],
            vec!["scan", "--out", "x", "--format", "pdf"],
            vec!["scan", "--out", "x", "--lang", "fr"],
            vec!["scan", "--out", "x", "--verbose"],
            vec!["scan", "--out", "x", "--allowlist"],
        ] {
            assert!(matches!(parse(&args(&bad), None), Some(Err(_))), "{bad:?}");
        }
    }

    #[test]
    fn a_given_allowlist_must_be_readable_and_the_catalog_lists_every_key() {
        let args = |allowlist: Option<PathBuf>| ScanArgs {
            out: PathBuf::from("out"),
            formats: vec![Format::Csv],
            lang: Lang::En,
            allowlist,
        };
        let dir = std::env::temp_dir().join(format!("monitor-cli-allow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("allowlist.json");
        std::fs::write(
            &good,
            r#"{"schemaVersion": 1, "allowed": ["slack", "nope"]}"#,
        )
        .unwrap();
        let list = scan_allowlist(&args(Some(good))).unwrap().unwrap();
        assert_eq!(list.origin, AllowlistOrigin::CommandLine);
        assert_eq!(list.unknown, ["nope"]);
        assert!(scan_allowlist(&args(Some(dir.join("missing.json")))).is_err());
        std::fs::remove_dir_all(dir).unwrap();

        let csv = saas_catalog_csv();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "key,product,vendor,category,ai,sanka_migrate_url");
        assert_eq!(lines.len(), SaasCatalog::bundled().entries().len() + 1);
        assert!(lines
            .iter()
            .any(|line| line.starts_with("salesforce,Salesforce,")
                && line.ends_with("https://sanka.com/migrate/salesforce/")));
    }

    #[test]
    fn a_rescan_replaces_the_devices_file_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("monitor-cli-{}", uuid::Uuid::new_v4()));
        let path = dir.join("PC_1.csv");
        std::fs::create_dir_all(&dir).unwrap();
        write_whole(&path, b"first").unwrap();
        write_whole(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["PC_1.csv"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
