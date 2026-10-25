# Security Policy

## Supported versions

Sanka Monitor is pre-1.0. Security fixes go into the latest release published on
[sankaHQ/monitor-releases](https://github.com/sankaHQ/monitor-releases); older
releases do not receive backports. The app updates itself from that feed unless the
person turned automatic update checks off.

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
- **Reports.** Excel, CSV and HTML reports are written only where the person
  chooses. Text read from the device (computer and program names) is written as
  values, never as formulas or markup: CSV cells that Excel would run are prefixed
  with `'`, and the HTML report escapes all device text and loads nothing external.
- **Sending a report to Sanka** happens only on the person's explicit action, as
  the Sanka account they signed in with in the system browser (PKCE; the code
  comes back through `sanka-monitor://auth/callback` and is bound to a verifier
  only the app holds). The sign-in's device secret lives only in the OS keychain
  or credential store, and Sign out revokes it. The app sends exactly the file it
  showed, and compares Sanka's SHA-256 receipt with its own.
- **Connected mode (opt-in enrollment).** The device secret lives only in the OS
  keychain or credential store. It is exchanged for short-lived runtime tokens, and
  every request body is shown in the app's Privacy pane before it is sent. App usage
  is recorded only after the person opts in, and the event types have no fields for
  window titles, URLs, document contents, keystrokes or screen captures.

The Sanka API (`api-v2.sanka.com`) and the Monitor console are maintained
separately and are not in this repository. Report suspected vulnerabilities in them
through the same private contact.
