# Security Policy

## Supported versions

Sanka Monitor is pre-1.0. Security fixes go into the latest release, published on
this repository's [releases](https://github.com/sankaHQ/monitor/releases) and on
`https://release.sanka.com/monitor/`; older releases do not receive backports. The
app updates itself from that feed unless the person, or device management, turned
automatic update checks off. Releases up to 0.3.x were published on
[sankaHQ/monitor-releases](https://github.com/sankaHQ/monitor-releases); they don't update
to 0.4.0 by themselves, so reinstall from https://sanka.com/monitor/download/.

## Verifying a download

Every release file is listed in the release's `SHA256SUMS` and carries a GitHub
build-provenance attestation that ties it to this repository's release workflow:

```bash
shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify Sanka-Monitor-X.Y.Z-arm64.dmg --repo sankaHQ/monitor
```

On a Mac, `spctl --assess --type open --context context:primary-signature -v` on the
DMG and `pkgutil --check-signature` on the `.pkg` show the Developer ID signature and
the notarization. On Windows, the installer and the MSI are signed by Sanka, Inc.

## Reporting a vulnerability

Do not open a public GitHub issue for a suspected vulnerability.

Use private vulnerability reporting on this repository's Security tab when it is
available, or email `hey@sanka.com` with a subject beginning `[Security][monitor]`.
Include the affected version and platform, reproduction steps, expected impact and
any suggested mitigation. Do not include real device data from other people,
access tokens, enrollment codes, passwords or other credentials.

We will use the reporting channel to coordinate validation, remediation and
responsible disclosure. No response or resolution time is guaranteed by this policy.

## Security boundaries

Sanka Monitor reads a computer's security settings and installed programs with the
signed-in person's rights: no administrator rights, no system service and no macOS
privacy permission.

- **Local mode (the default).** Nothing is uploaded. The only automatic network
  request is the update check, which downloads `latest.json` from the release feed
  and sends nothing about the computer. Updates are verified against the updater
  public key in `apps/agent/src-tauri/tauri.conf.json` before they install.
- **Browser check (opt-in).** Off until the person turns it on in the app. Then a
  scan opens the Chrome, Edge and Firefox history files read-only in place in
  SQLite's `immutable` mode (no copy, no lock, no side files), keeps only each
  visit's host name and day in memory, and keeps per SaaS in the catalog the last
  day used and the days used in the last 30. Those two values appear in reports
  the person saves, and leave the computer only with a report the person sends, or,
  on a connected PC whose person turns on SaaS sharing, as each SaaS's catalog key
  and last day used (nothing else). Turning sharing or the browser check off,
  Disconnect and a revoke delete what the workspace received. Safari is read only
  when the app already has Full Disk Access, which it never requests. Web addresses, page titles
  and history are never stored, written to a report or sent. The command-line scan
  never reads browser history.
- **Reports.** Excel, CSV and HTML reports are written only where the person
  chooses. Text read from the device (computer and program names) is written as
  values, never as formulas or markup: CSV cells that Excel would run are prefixed
  with `'`, and the HTML report escapes all device text and loads nothing external.
- **Sending a report to Sanka** happens only on the person's explicit action, as
  the Sanka account they signed in with in the system browser (PKCE; the code
  comes back through `sanka-monitor://auth/callback` and is bound to a verifier
  only the app holds). The sign-in's device secret lives only in the OS keychain
  or credential store, and Sign out revokes it (waiting at most 15 seconds for
  Sanka, then forgetting it here anyway). A keychain prompt the person declines
  never signs the app out. The app sends exactly the file it showed, and compares
  Sanka's SHA-256 receipt with its own.
- **Connected mode (opt-in enrollment).** The device secret lives only in the OS
  keychain or credential store. It is exchanged for a short-lived runtime token
  before each request; apart from that exchange, every request body is shown in
  the app's Privacy pane before it is sent. App usage is recorded only after the
  person opts in, and the event types have no fields for window titles, URLs,
  document contents, keystrokes or screen captures.

The Sanka API (`api-v2.sanka.com`) and the Monitor console are maintained
separately and are not in this repository. Report suspected vulnerabilities in them
through the same private contact.

## Opt-in work context boundary

Work History is independent of enrollment and existing inventory/usage contracts.
Capture starts only after explicit app/site selection and personal OS-login
confirmation. The app and companion each require consent. Unknown/private browser
surfaces, editable content and unsupported native profiles are excluded. Only
closed normalized actions/categories are retained; no raw labels, titles, URLs,
document contents, screen/audio, inputs or clipboard enter the context store.

Context keys and the ephemeral browser-relay secret live in the current OS user's
secure storage. Files use owner-only permissions on Unix and the user's private
AppData profile on Windows. Account namespaces never adopt another account's
unsent records. OS session notifications pause without waiting for storage locks;
foreground/session checks and consent generation checks veto stale results.
The app does not resume on unlock/wake. Missing notification setup/key access
fails closed. The native host validates the committed extension origin and 4 KiB
closed frames, authenticates loopback requests, and handles one bounded client
at a time with deadlines. No externally connectable page channel exists.

Preview tokens bind immutable bytes, account/workspace/device, destination and
current local source evidence. Sends and retries recheck identity, policy and
consent. Only the signed-in account's short-lived runtime token is used, never
the enrolled device credential. Cloud grants are obtained only on explicit Send.
Receipt hashes and same-origin canonical assessment URLs are checked. Daily
company grants freeze only approved apps/descriptions; edits/revocation stop local
scheduling before any network wait. Failed delivery/deletion remains unconfirmed.

Raw context expires after 48 hours, minimized occurrences after 30 days; saved
user-authored descriptions are separately deletable. Prior account stores are
pruned while the app runs. If their secure keys are unavailable, the UI reports
that retention cannot be completed and offers key-independent erase-all. Erasure
locks each exact namespace and removes its encrypted snapshots and orphan files.
Remote erasure is separate and needs a confirmed server response; sign-out also
uses the server's account/device revocation hook. Shared OS logins must remain
inventory-only. A malicious process running as the same OS user is outside this
local-isolation boundary; this is not protection against an OS administrator.
