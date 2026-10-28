# Deploying Sanka Monitor with device management

IT can install Sanka Monitor and connect it to the company's workspace with no step
for the employee: install the `.pkg` (macOS) or the MSI (Windows), and place a
deployment token in managed preferences. The app connects at launch and checks in
from then on.

Device management can connect a computer, but it can't turn on app-usage sharing:
that stays the employee's own choice in the app (local-first ADR,
`docs/adr/2026-10-19-sanka-monitor-local-first.md` in the umbrella repo). A managed
computer shows "Managed by <workspace> IT" in the app, and its Privacy page lists
everything it sends.

## 1. Create a deployment token

In the console, **monitor.sanka.com → Devices → Add devices → Deploy with MDM**:
choose the tool, set an expiry (default 90 days), a device limit and, optionally, the
company's email domain, then copy the token (shown once) and download the profile.
The token only enrolls computers; rotating or revoking it doesn't disconnect
computers already connected.

## 2. Install the app

| Platform | File | Notes |
| --- | --- | --- |
| macOS | `https://sanka.com/monitor/download/mac-pkg` | Not in 0.4.0: until a release lists a `.pkg`, deploy the universal DMG instead (below). Installs `/Applications/Sanka Monitor.app` (universal) and `/Library/LaunchAgents/com.sanka.monitor.agent.plist`, which opens the app in the menu bar at every login. The postinstall opens it at once for whoever is signed in. Signed with the Developer ID Installer certificate, notarized. |
| Windows | `https://sanka.com/monitor/download/windows-msi` | Per-machine install. Authenticode-signed by Sanka, Inc. |

Files with stable names for scripts: `https://release.sanka.com/monitor/latest/Sanka-Monitor.pkg`
and `.../latest/Sanka-Monitor-x64.msi`. Each release lists SHA-256s in `SHA256SUMS`
(see [SECURITY.md](../SECURITY.md#verifying-a-download)).

### macOS without the .pkg

Releases without a `.pkg` (0.4.0) still deploy through device management: use the
universal DMG, `https://sanka.com/monitor/download/mac-universal` or
`https://release.sanka.com/monitor/latest/Sanka-Monitor-universal.dmg`. Jamf Pro,
Kandji and Intune install an app from a DMG. The DMG doesn't add the login agent, so
the app first starts when the person opens it; from then on it starts at login, and it
connects with the deployment token at that first launch.

## 3. Place the managed preferences

macOS: preference domain `com.sanka.monitor` (a Custom Settings payload, read from
`/Library/Managed Preferences/com.sanka.monitor.plist`).
Windows: registry key `HKLM\SOFTWARE\Policies\Sanka\Monitor`.

| Key | Type | Meaning |
| --- | --- | --- |
| `DeploymentToken` | string | The token from step 1. The app connects at launch, then retries hourly, while it isn't connected. |
| `OwnerEmail` | string | Optional. The computer's user, from an MDM variable (`$EMAIL` in Jamf Pro and Kandji, `{{userprincipalname}}` in Intune). |
| `AllowDisconnect` | boolean | Optional. Default: the workspace's setting (off for managed computers). |
| `DisableUpdateChecks` | boolean | Optional. For fleets that push new versions themselves. |

The company allowlist for the SaaS check stays a file:
`/Library/Application Support/Sanka Monitor/allowlist.json` (macOS) or
`%ProgramData%\Sanka Monitor\allowlist.json` (Windows).

Example `.mobileconfig` payload content (the console's profile download fills in the
token and a fresh UUID):

```xml
<dict>
  <key>PayloadType</key><string>com.sanka.monitor</string>
  <key>PayloadIdentifier</key><string>com.sanka.monitor.settings</string>
  <key>PayloadUUID</key><string>…</string>
  <key>PayloadVersion</key><integer>1</integer>
  <key>DeploymentToken</key><string>smdt_…</string>
  <key>OwnerEmail</key><string>$EMAIL</string>
</dict>
```

Windows (`.reg`, or the same values from an Intune script):

```
Windows Registry Editor Version 5.00

[HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Sanka\Monitor]
"DeploymentToken"="smdt_…"
"OwnerEmail"="user@example.com"
```

## 4. Check it worked

The computer appears in **Devices** as Managed within a minute of the app opening. On
the computer, the Workspace page shows "Connected" with "Managed by <workspace> IT".

If the workspace removes a managed computer, it doesn't connect itself again with the
same token: rotate the token (or place a new one) to connect it again.

## Per-tool notes

- **Jamf Pro:** upload the `.pkg` to a policy scoped to the computers, and the
  profile as a Configuration Profile (Application & Custom Settings → Upload).
- **Kandji:** add a Custom App with the `.pkg` (Install once, Audit and enforce) and a
  Custom Profile with the `.mobileconfig`.
- **Intune (macOS):** add the `.pkg` as a macOS app (line-of-business) and a
  Preference file profile for `com.sanka.monitor`.
- **Intune (Windows):** add the MSI as a line-of-business app and push the registry
  values with a PowerShell script or a Settings catalog custom policy.
