# Windows agent (3b)

Windows is the main platform of the IT teams Sanka Monitor serves. This page
lists what the Windows build does, what is verified where, and the checklist for
the Windows 11 VM.

## What runs on Windows

| Area | Implementation | Permission |
|---|---|---|
| Posture | One PowerShell probe (`monitor-posture::windows::POSTURE_SCRIPT`) → BitLocker (shell property `System.Volume.BitLockerProtection`), firewall profiles, SmartScreen, screen lock (inactivity policy / secure screensaver / sleep + sign-in on wake within 15 min), pending updates from the Windows Update cache, automatic updates (`NoAutoUpdate` policy, `wuauserv` start type), antivirus (`root/SecurityCenter2` `AntiVirusProduct.productState`), local admin (`whoami /groups` holds `S-1-5-32-544`, also as UAC "deny only"), device management (an `MS DM Server` enrollment under `HKLM\SOFTWARE\Microsoft\Enrollments`, else `Win32_ComputerSystem.PartOfDomain`), Secure Boot (`SecureBoot\State\UEFISecureBootEnabled`). TPM is not checked: `Get-Tpm` / `Win32_Tpm` need admin. | none (user-level, no admin) |
| Installed software | One PowerShell probe (`monitor-inventory::windows::INVENTORY_SCRIPT`) → the Uninstall keys in HKLM, HKLM\WOW6432Node and HKCU, minus `SystemComponent = 1`, entries with a `ParentKeyName`, patches (`ReleaseType` Update / Hotfix / Security Update / Service Pack, `KB…`, "Security Update for …", "Update for Microsoft …"); plus the user's Store packages (`Get-AppxPackage`, no frameworks, `SignatureKind = Store`). Sent after a check-in when it changed, at least daily. A probe without the registry section sends nothing. | none |
| Usage | `GetForegroundWindow` → executable name (`msedge.exe`) + `FileDescription`; `GetLastInputInfo` for idle; Store apps resolved through `ApplicationFrameHost` | none |
| Login item | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` entry with `--background` (tauri-plugin-autostart), set once at enrollment, removed on Disconnect | none |
| Device info | Registry build/display version (Windows 11 is inferred from build ≥ 22000) and `EditionID` (sent as `osEdition`), `Win32_ComputerSystem`, `Win32_BIOS` | none |
| Packaging | `tauri.windows.conf.json`: NSIS (per-user, EN/JA; the released installer and the update package) + MSI (unsigned test builds only), WebView2 bootstrapper; `icons/icon.ico` | — |

The posture and inventory probes print ASCII-only JSON (anything else escaped as
`\uXXXX`): `powershell.exe` writes redirected output in the OEM code page, so a
Japanese product name or an `®` would otherwise arrive as invalid UTF-8 and lose the
whole probe.

App ids on Windows are lowercase executable names; the server-side signature
catalog already maps the common ones (`msedge.exe` → `edge`, `excel.exe` →
`excel`, `claude.exe` → `claude`, …).

## Verification status

- Compiled and clippy-clean on `windows-latest` (CI job "Lint, test, build
  (Windows)"); the pure parsers (posture JSON → checks, powercfg output,
  inventory JSON → programs, app-id derivation) are unit-tested on every platform.
- CI also runs the command-line scan on the `windows-latest` runner (the real
  PowerShell probes and report writers) and keeps its CSV, workbook and report
  as an artifact for a week. A runner is Windows Server, not a person's PC.
- **Not yet exercised on a real Windows session.** The items below are the
  assumptions to confirm on the VM, in order.

## VM checklist (Parallels, Windows 11 ARM)

Installers for the VM come from the on-demand workflow **Windows test build
(unsigned)** (`gh workflow run windows-test-build.yml`): download the artifact
from the run, copy the `.exe` into the VM, accept the SmartScreen warning. A
release dry run (below) gives the signed installer instead.

0. Local scan (0.3), before enrolling: the window opens on Scan and finishes a
   scan; Save… writes the Excel ledger, the report and the CSV where chosen,
   and they open in Excel and Edge (the report prints on one A4 page). Run
   `sanka-monitor-agent.exe scan --out C:\Temp\scans --format all` from a
   terminal: it prints the files and a summary, and the CSV merges in
   `templates/fleet/sanka-monitor-fleet-en.xlsx` (Refresh All).
1. Install the NSIS build; the window opens and the app appears in the tray;
   a tray click opens its menu.
2. Enroll with a console code; the Devices page shows the machine with
   `Windows 11 <version> (build …)`, model, serial (Parallels reports its own).
3. Scan → Security checks: ten checks resolve to true/false (never all "unknown"); a
   standard account passes "No local admin rights", the default VM account
   (an admin) fails it. Note how long "Check in now" takes — the Windows
   Update offline search is the slow signal; if it exceeds ~10 s, replace it
   with the last-install timestamp.
4. Workspace → "Installed software" shows a count and a time; Privacy
   shows the `/agent/software` body. Compare it with Settings → Apps →
   Installed apps: the same programs, no KB updates, no runtime components,
   Store apps by package name. Install a program with a Japanese name (or
   switch the display language to Japanese) and confirm the name arrives
   intact and the checks still resolve (screen lock reads English `powercfg`
   labels, so there it may rely on the policy or screensaver signal).
5. With usage sharing on, Workspace → "Apps used today" lists Edge / Excel /
   Store apps by display name; with it off, nothing is recorded.
6. Sign out of Windows and back in: the agent starts in the tray without
   opening its window (Workspace → "Open Sanka Monitor when I log in" turns
   this off).
7. Sleep/lock the VM and return: usage intervals close on idle and resume.
8. Workspace → Disconnect: the Connect screen returns, the Run entry is
   gone, `%APPDATA%\com.sanka.monitor\usage.db` holds no usage rows, and the
   console lists the device as revoked.

## Release

Windows ships from the same **Release (macOS + Windows)** workflow as the Mac app
(`.github/workflows/release.yml`; tag, dry run and manual publishing are in the
README's Release section). One tag publishes both, and one `latest.json` serves
both updaters. The Windows job, on `windows-latest`:

1. Downloads Microsoft's Artifact Signing client (`Microsoft.ArtifactSigning.Client`
   from NuGet, the version and SHA-256 pinned in `scripts/setup-windows-signing.ps1`)
   and finds the Windows SDK SignTool (10.0.22621 or later). It signs only with the
   service principal in the secrets below: no Azure CLI, managed identity or
   interactive login.
2. Signs a small throwaway .NET program first, so a signing problem stops the job
   before the Rust build.
3. Builds the NSIS installer (`--bundles nsis`). Tauri calls
   `scripts/sign-windows.ps1` for the executables it packages and for the installer
   through a structured `signCommand`, so spaces in paths survive. Each file gets a
   SHA-256 Authenticode signature with the description "Sanka Monitor" and a SHA-256
   RFC 3161 timestamp from `http://timestamp.acs.microsoft.com`. The build also signs
   the installer for the updater (`*-setup.exe.sig`, Monitor's updater key): the
   installer is the update package, and `latest.json` points `windows-x86_64` at
   `sanka-monitor_<version>_x64-setup.exe`.
4. Extracts `sanka-monitor-agent.exe` from the installer with 7-Zip and checks it and
   the installer with `Get-AuthenticodeSignature`: status Valid, publisher
   `Sanka, Inc.`, timestamped. Tauri puts an unsigned copy of the executable back in
   `target/` after bundling, so only the extracted one counts. The results and the
   SHA-256 of both files stay in the run as `windows-signatures.json`, next to the
   extracted executable, in the `monitor-windows` artifact.

Windows signing secrets (Actions secrets on this repository; the workflow stops
before building if any is missing):

| Secret | Value |
| --- | --- |
| `MONITOR_AZURE_TENANT_ID` | The Microsoft Entra tenant of the signing service principal |
| `MONITOR_AZURE_CLIENT_ID` | The service principal's client (application) ID |
| `MONITOR_AZURE_CLIENT_SECRET` | The service principal's client secret |
| `MONITOR_AZURE_SIGNING_ENDPOINT` | The Artifact Signing account's regional endpoint URL |
| `MONITOR_AZURE_SIGNING_ACCOUNT` | The Artifact Signing account name |
| `MONITOR_AZURE_SIGNING_PROFILE` | The certificate profile name (publisher Sanka, Inc.) |

There is no unsigned release path. Unsigned installers for testing come from
**Windows test build (unsigned)** above; SmartScreen warns on first launch of those.
