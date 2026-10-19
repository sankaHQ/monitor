# Sanka Monitor

Sanka Monitor finds IT risks on a PC: security settings that are off, and an
operating system or programs past (or near) the end of vendor support. It is
local-first:

- **Local scan, the default.** No account and no enrollment. The app scans the
  computer, reads the result against the lifecycle catalog bundled in the app
  (`crates/monitor-findings/catalog/`) and shows the risks. It saves an Excel
  workbook in the IT資産台帳 (IT asset ledger) layout, a CSV and a one-page IT risk
  report (HTML, prints to one A4 page), in Japanese or English, where the person
  chooses. Nothing is uploaded. The only automatic network request is the hourly
  update check, which downloads `latest.json` from the release feed, sends nothing
  about the computer, and can be turned off under Privacy.
- **Upload this report to Sanka, on the person's word.** The app reads one report
  file, shows its name, size and SHA-256, and sends exactly those bytes (plus
  anything the person typed) to `POST /api/v2/monitor/reports`; Sanka's receipt
  carries its own SHA-256 to compare. The request appears in the Privacy pane.
- **Fleet scan.** `sanka-monitor-agent scan --out <folder>` writes one CSV per PC
  for device management tools, and the workbooks in `templates/fleet/` merge a
  folder of them with Power Query. See [docs/fleet-scan.md](docs/fleet-scan.md).
- **Connected mode, opt-in.** Enrolling with a workspace's code connects the
  device to monitor.sanka.com: check-ins send the security checks and installed
  software for continuous visibility.
- **Usage, connected and opted in only.** App usage is recorded only while the
  device is connected *and* the person has turned sharing on (default off); the
  console shows it as daily app presence, never durations.
- **Who opened your data, connected only.** Each time an IT admin opens the
  person's daily app presence, the workspace logs it with a reason. The Privacy
  pane lists those views (when, who or "An IT admin", the days viewed, the reason),
  fetched from `GET /api/v2/monitor/agent/person-views` with the device's runtime
  token when the pane opens, never in the background. It keeps working after the
  person stops sharing. A device that is not connected makes no such request.

## Privacy model

Checks (user-level: no admin rights, no root daemon, no macOS privacy prompt; `pass:
null` when a check cannot tell): disk encryption, firewall, Gatekeeper / SmartScreen,
screen lock, pending OS updates, automatic updates, antivirus (XProtect on macOS,
Security Center on Windows), the signed-in user is not a local admin, device management
(MDM; or an Active Directory domain on Windows), Secure Boot (Windows) and System
Integrity Protection (macOS). TPM is not checked: Windows only reports it to admins.

Installed software: the `.app` bundles in `/Applications` (one level of subfolders,
never inside a bundle, no symlinks) and `~/Applications` on macOS; the Uninstall
registry entries (minus system components, child entries and patches) and the user's
Microsoft Store packages on Windows. Each item is a source, an identifier (bundle id,
registry key name or package family), a name, a version and, on Windows, a publisher.

Findings are computed on the device with the rules sanka-api applies to connected
devices (`device_*` and `software_*` finding types, same severities), so a local
report and the console agree. The catalog is ported from the sanka-api lifecycle
seed plus a snapshot of the public `https://sanka.com/lab/eol-catalog.json`.

Connected mode: after each check-in the agent sends the full software snapshot when
it changed, and at least daily (the workspace's `uploadIntervalSeconds`).

Structurally minimal telemetry: the event types have no fields for window titles, URLs,
document contents, keystrokes, or captures, and inventory items none for install
locations, file names or user names. See the in-app Privacy pane and
`docs/PROVENANCE.md`.

Local data rules: usage is recorded only while the device is enrolled and the
person has opted in, and only usage that started after the person last turned
sharing on is ever uploaded (anything earlier is withheld for good); the device
keeps its own history for 14 days; **Disconnect** revokes the device in the
workspace (`/agent/revoke`, which deletes the usage it received), then wipes that history,
the keychain credential and the login item. Without an answer the wipe happens anyway and
only the credential and the login item stay, to retry the revoke hourly and after a
restart until the workspace confirms. A device the workspace revoked, or
no longer knows (`MONITOR_DEVICE_REVOKED` / `MONITOR_DEVICE_INVALID`), wipes itself the
same way. The workspace's kill switch stops collection and uploads (check-ins continue
without posture or software, to learn when it lifts), and a version below the
workspace's minimum stops uploads until the self-updater installs a newer build. A
connected agent opens at login in the tray (`--background`).

Backend surface: `/api/v2/monitor/agent/*` in sanka-api (enroll → token →
check-in; devices registry at `/api/v2/monitor/devices`), landing in
sankaHQ/sanka-api#3585. Spec: `sanka-project/docs/products/monitor-desktop-agent-implementation-spec.md`.

## Layout

```
apps/agent/              Monitor: Tauri v2 shell (tray, windows, updater, deep-link), the local
                         scan and the `scan` command line, + small React UI
crates/monitor-core      Shared types, ids, API constants, privacy invariants
crates/monitor-findings  The bundled lifecycle catalog and the on-device finding rules
crates/monitor-report    Local reports: IT asset ledger (xlsx), CSV, one-page report (HTML), JA/EN
crates/monitor-inventory Installed software (macOS app bundles; Windows Uninstall registry + Store)
crates/monitor-posture   Posture trait + per-platform checks (macOS, Windows)
crates/monitor-usage     Focus tracking + idle detection (pure interval state machine)
crates/monitor-store     Local event queue (in-memory now; SQLite WAL in P2)
crates/monitor-sync      Agent API endpoint contract and HTTP client
templates/fleet/         Excel workbooks (Power Query) that merge a folder of per-PC CSVs
```

## Develop

Requirements: Rust (stable), Node 24+, Xcode command line tools.

```bash
(cd apps/agent && npm ci && npm run build)       # generate_context reads dist/: build before cargo
cargo test --workspace                           # from repo root
(cd apps/agent && npm run tauri dev)             # run Monitor with hot reload
cargo run -p monitor-report --example sample_reports -- /tmp/r   # sample reports, no scan
python3 templates/fleet/build_fleet_template.py  # rebuild the fleet workbooks
```

The lifecycle catalog is data: refresh the public EOL snapshot with
`curl -fsS https://sanka.com/lab/eol-catalog.json -o crates/monitor-findings/catalog/eol-catalog.json`
and keep `rules.json` in step with the sanka-api lifecycle seed.

`cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` must be
clean; CI enforces both.

## Release

The agent publishes to the public [sankaHQ/monitor-releases](https://github.com/sankaHQ/monitor-releases)
repository. Signing and notarization secrets are Actions secrets only — never committed.

`Release (macOS + Windows)` (`.github/workflows/release.yml`, manual dispatch) builds
and signs both platforms and publishes them together as release `vX.Y.Z`, the
repository's latest release:

- **macOS:** one universal app for Apple silicon and Intel Macs, signed with the
  Developer ID, notarized and stapled.
- **Windows:** the per-user NSIS installer (x64), Authenticode-signed through Microsoft
  Artifact Signing with the publisher Sanka, Inc. The job checks the signatures of the
  installer and of the app inside it. See [docs/windows.md](docs/windows.md#release).
- **Updates:** both are signed with Monitor's updater key (the public key is in
  `tauri.conf.json`). `latest.json` lists `darwin-aarch64` and `darwin-x86_64` (both the
  universal app) and `windows-x86_64` (the installer).

Monitor's updater reads `releases/latest/download/latest.json` and installs it in the
background, so the workflow refuses a tag other than `v` + the version in
`apps/agent/src-tauri/Cargo.toml` (the agent's only version source). It also checks
that every secret below exists before either build starts.

| Actions secret | Used for |
| --- | --- |
| `MACOS_APPLE_CERTIFICATE`, `MACOS_APPLE_CERTIFICATE_PASSWORD`, `MACOS_APPLE_SIGNING_IDENTITY` | Developer ID signing |
| `MACOS_APPLE_ID`, `MACOS_APPLE_APP_SPECIFIC_PASSWORD`, `MACOS_APPLE_TEAM_ID` | Notarization |
| `TAURI_UPDATER_PRIVATE_KEY`, `TAURI_UPDATER_PRIVATE_KEY_PASSWORD` | Updater signatures (`.sig`) |
| `MONITOR_AZURE_TENANT_ID`, `MONITOR_AZURE_CLIENT_ID`, `MONITOR_AZURE_CLIENT_SECRET` | Windows: the signing credential |
| `MONITOR_AZURE_SIGNING_ENDPOINT`, `MONITOR_AZURE_SIGNING_ACCOUNT`, `MONITOR_AZURE_SIGNING_PROFILE` | Windows: the Artifact Signing account and certificate profile |
| `MONITOR_RELEASES_TOKEN` (optional) | Publishing; without it, publish a dry run by hand |

A dry run builds and signs everything and uploads the release files to the workflow run
as the artifact `sanka-monitor-vX.Y.Z`: `sanka-monitor_X.Y.Z_universal.dmg`,
`sanka-monitor_X.Y.Z_universal.app.tar.gz` and its `.sig`,
`sanka-monitor_X.Y.Z_x64-setup.exe` and its `.sig`, and `latest.json`. To publish it by
hand:

```bash
gh workflow run release.yml --repo sankaHQ/monitor -f version=vX.Y.Z -f dry_run=true -f release_notes="<notes>"
gh run download <run-id> --repo sankaHQ/monitor -n sanka-monitor-vX.Y.Z -D out
gh release create vX.Y.Z --repo sankaHQ/monitor-releases --title "Sanka Monitor vX.Y.Z" \
  --notes "<notes>" --latest out/*
```

Use the same notes in both commands: the workflow writes them into `latest.json`.

To propose changes, see [CONTRIBUTING.md](CONTRIBUTING.md).

## License, security and contributing

Sanka Monitor is licensed under the [Apache License 2.0](LICENSE); see [NOTICE](NOTICE)
and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Report vulnerabilities privately
as described in [SECURITY.md](SECURITY.md). To contribute, start with
[CONTRIBUTING.md](CONTRIBUTING.md). The Sanka API and the Monitor console are not part
of this repository.
