# Scanning a fleet from device management

Sanka Monitor can scan every PC from Intune, Jamf, Group Policy or any scheduler
and collect the results in one folder that your company controls. Nothing is sent
to Sanka: each PC writes one file to the folder, and an Excel workbook merges them
into one IT asset ledger.

## The command

```text
sanka-monitor-agent scan --out <folder> [--format csv,xlsx,html|all] [--lang ja|en] [--allowlist <file>]
```

- Scans the computer (security checks, installed programs and versions) and reads
  them against the vendor support dates bundled in the app.
- Lists the SaaS whose desktop apps are installed (Slack, Zoom, ChatGPT, …), with
  whether the company allows each one and whether Sanka can migrate it. The
  command line never reads browser history: device management runs it without the
  person, and the browser check is the person's choice in the app.
- Writes `<computer>_<serial>.csv` to `<folder>` (created if missing). A later scan
  of the same computer replaces its file, so the folder always holds the latest
  scan of each PC. Files are written under a temporary name and renamed into
  place, so a reader never sees half a file.
- `--format xlsx` and `--format html` also write the IT asset ledger workbook and
  the one-page IT risk report for that PC. `--lang` picks their language; the CSV
  carries both languages.
- Opens no window, starts nothing in the background, reads and writes no app
  data and makes no network request.
- Exit code 0 when the files were written, 1 when a file could not be written,
  2 for bad arguments or an allowlist that cannot be read. It prints the files it
  wrote and a one-line summary.

### The company allowlist

Write the SaaS your company allows into one JSON file:

```json
{ "schemaVersion": 1, "name": "Example K.K.", "allowed": ["microsoft-365", "microsoft-teams", "slack", "zoom", "box"] }
```

Each entry is a catalog key or a product name. `sanka-monitor-agent saas-catalog`
prints every key as CSV (key, product, vendor, category, AI, Sanka migration page).
An entry the catalog does not know is reported (on stderr, and in the report's
summary), never silently dropped. Each SaaS in use that is not on the list becomes
a risk: an AI tool is high, any other SaaS medium.

The scan uses `--allowlist <file>` when given (a missing or broken file is exit
code 2), otherwise the file device management places at:

| Platform | Path |
| --- | --- |
| macOS | `/Library/Application Support/Sanka Monitor/allowlist.json` |
| Windows | `%ProgramData%\Sanka Monitor\allowlist.json` |

The app reads the same machine-wide file, so a person who scans in the app sees
the company's verdicts too. Without a list, SaaS are listed but not judged. A list
that exists but cannot be read judges nothing, and the report says so.

On Windows, standard users may create folders under `%ProgramData%`. Create
`%ProgramData%\Sanka Monitor` from device management (as SYSTEM) with write access
for administrators only before anyone else can, or pass `--allowlist` with a file on
a share that only IT can change. On macOS, `/Library/Application Support` already
needs administrator rights.

Where the program is:

| Platform | Path |
| --- | --- |
| macOS | `/Applications/Sanka Monitor.app/Contents/MacOS/sanka-monitor-agent` |
| Windows (per-machine MSI) | `C:\Program Files\Sanka Monitor\sanka-monitor-agent.exe` |
| Windows (per-user installer) | `%LOCALAPPDATA%\Sanka Monitor\sanka-monitor-agent.exe` |

### Run it as the signed-in person when you can

Two checks describe the person rather than the device: local administrator rights
and screen lock. Run as root (Jamf's default) or as SYSTEM (Intune's and Group
Policy's default), the scan reports them as "could not determine" rather than
judging the service account. Per-user programs (`~/Applications` on macOS, the
user's own uninstall entries and Microsoft Store apps on Windows) are only seen
when the scan runs as that person.

### The shared folder

Give every computer write access to the folder and only IT read access, for
example a hidden share `\\fileserver\SankaMonitor$` where Domain Computers (or
Authenticated Users) may create files and IT administrators may read them. Each
file names its computer and serial number and lists the device's installed
programs and security settings; treat the folder like any other IT asset
inventory.

## Examples

### Intune (Windows, PowerShell script or Remediation)

The app is a Windows GUI program, so start it with `Start-Process -Wait` and read
its exit code:

```powershell
$exe = "C:\Program Files\Sanka Monitor\sanka-monitor-agent.exe"
$p = Start-Process -FilePath $exe -ArgumentList 'scan', '--out', '\\fileserver\SankaMonitor$' `
  -Wait -PassThru -WindowStyle Hidden
exit $p.ExitCode
```

Set "Run this script using the logged-on credentials" to Yes to include the
person-level checks.

### Group Policy (Windows)

A user logon script (User Configuration > Policies > Windows Settings > Scripts >
Logon) with the same PowerShell, or a scheduled task that runs at logon as the
signed-in user.

### Jamf Pro (macOS)

A policy script, run at check-in or daily, that runs the scan as the person who is
logged in:

```bash
#!/bin/bash
app="/Applications/Sanka Monitor.app/Contents/MacOS/sanka-monitor-agent"
out="/Volumes/SankaMonitor"   # or a path your script mounts first
user=$(stat -f%Su /dev/console)
if [ "$user" != "root" ] && [ "$user" != "loginwindow" ]; then
  uid=$(id -u "$user")
  launchctl asuser "$uid" sudo -u "$user" "$app" scan --out "$out"
else
  "$app" scan --out "$out"
fi
```

## Merging the folder in Excel

`templates/fleet/` has two workbooks, `sanka-monitor-fleet-en.xlsx` and
`sanka-monitor-fleet-ja.xlsx` (Excel for Windows or Mac with Power Query). They
read every CSV in one folder and fill five sheets:

| Sheet (EN / JA) | One row per |
| --- | --- |
| Devices / 端末台帳 | computer: OS and its support status, risk counts, each security check |
| Software / ソフトウェア台帳 | installed program and version per computer, with support status |
| SaaS / SaaS台帳 | SaaS per computer: category, AI, where it was found, whether the company allows it, the Sanka migration page |
| Risks / リスク一覧 | risk, most severe first, with evidence and how to fix it |
| Checks / セキュリティチェック | security check per computer |

1. Open the workbook and enable content if Excel asks.
2. Data > Get Data > Launch Power Query Editor, select `FolderPath` and set its
   Current Value to your folder (for example `\\fileserver\SankaMonitor$`).
3. Close & Load, then Data > Refresh All. Refresh again whenever new scans arrive.

If Excel asks about privacy levels, choose Organizational for the folder. To hand
out the workbook with your folder already set, rebuild it:

```bash
python3 templates/fleet/build_fleet_template.py --folder '\\fileserver\SankaMonitor$' --out ./my-fleet
```

`fleet-queries-en.m` and `fleet-queries-ja.m` hold the same Power Query code as
text, for building the queries by hand (Data > Get Data > From Other Sources >
Blank Query > Advanced Editor, one query per `shared` member).

The CSV columns are stable: `record` is `device`, `check`, `software`, `saas` or
`finding`, and every row carries `hostname`, `serial_number`, `scanned_at` and
`report_id`, so the files also load into any other tool. New columns are only added
at the end (the `saas_*`, `sanka_*`, `browser_check` and `allowlist` columns came
with the SaaS check), so a folder that still holds older files merges by column
name.

## Verification status

- The command-line scan, the CSV, workbook and report writers are tested in CI
  on macOS and Windows; the scan itself has run on macOS.
- The Windows build compiles and its unit tests run in CI, but no real Windows PC
  has run it yet.
- The fleet workbooks open in Excel for Mac 16.114 with all six queries and four
  connections recognized; a full refresh from a folder has not been confirmed in
  Excel yet. The SaaS query and sheet (added with the SaaS check) have not been
  refreshed in Excel yet either.
