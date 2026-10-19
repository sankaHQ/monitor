//! The command-line scan, for device management tools (Intune, Jamf, Group
//! Policy, any scheduler):
//!
//! ```text
//! sanka-monitor-agent scan --out <folder> [--format csv,xlsx,html|all] [--lang ja|en]
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
//! Exit codes: 0 written, 1 a file could not be written, 2 bad arguments.

use std::path::{Path, PathBuf};

use chrono::Utc;
use monitor_report::{device_file_key, Lang, ScanReport};

use crate::agent::AGENT_VERSION;

const USAGE: &str = "Sanka Monitor command-line scan

Usage:
  sanka-monitor-agent scan --out <folder> [--format csv,xlsx,html|all] [--lang ja|en]

Scans this computer and writes its report to <folder>, one file per format,
named <computer>_<serial>.<ext>. A later scan replaces the computer's file.
Nothing is sent anywhere.

Options:
  --out <folder>     Where to write (created if missing). A network share works.
  --format <list>    csv (default), xlsx, html, or all. Comma-separated.
  --lang <ja|en>     Language of the xlsx and html reports (default: from LANG,
                     else en). The CSV has both languages.
  -h, --help         Show this help.
  -V, --version      Show the version.

Exit codes: 0 written, 1 a file could not be written, 2 bad arguments.
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Scan(ScanArgs),
    Help,
    Version,
}

/// The command in `args` (program name first), or `None` when they are the
/// app's own (no arguments, `--background`, a deep link): the app starts.
pub fn parse(args: &[String], env_lang: Option<&str>) -> Option<Result<Command, String>> {
    match args.get(1).map(String::as_str) {
        Some("scan") => Some(parse_scan(&args[2..], env_lang).map(Command::Scan)),
        Some("-h" | "--help" | "help") => Some(Ok(Command::Help)),
        Some("-V" | "--version") => Some(Ok(Command::Version)),
        _ => None,
    }
}

fn parse_scan(args: &[String], env_lang: Option<&str>) -> Result<ScanArgs, String> {
    let mut out = None;
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
    Ok(ScanArgs { out, formats, lang })
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
        Err(err) => {
            eprintln!("sanka-monitor: {err}\n\n{USAGE}");
            2
        }
        Ok(Command::Scan(args)) => {
            let report = crate::scan::run_scan(Utc::now());
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
                        "{}: {} high, {} medium, {} low risks; {} programs",
                        report.device.hostname,
                        counts.high,
                        counts.medium,
                        counts.low,
                        counts.programs
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
        ] {
            assert_eq!(parse(&args(&list), env), parsed, "{list:?}");
        }
        for bad in [
            vec!["scan"],
            vec!["scan", "--out"],
            vec!["scan", "--out", "x", "--format", "pdf"],
            vec!["scan", "--out", "x", "--lang", "fr"],
            vec!["scan", "--out", "x", "--verbose"],
        ] {
            assert!(matches!(parse(&args(&bad), None), Some(Err(_))), "{bad:?}");
        }
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
