# Provenance

Sanka Monitor reuses a small amount of infrastructure from Sanka's earlier, internal
desktop app, which was retired in May 2026. That app was a macOS web-view shell with
a meeting recorder; none of its product code is included here.

## Adapted from Sanka's earlier desktop app

| Here | Notes |
| --- | --- |
| `.github/workflows/release.yml` | The same signing, notarization and updater-manifest flow, rewritten for Tauri v2 env-driven signing (`APPLE_*`, `TAURI_SIGNING_*`), with a dry-run mode; since 0.4.0 it publishes to this repository and downloads.sanka.com. |
| `apps/agent/src-tauri/icons/` | The Sanka app icon, copied as is. |
| `apps/agent/src-tauri/tauri.conf.json` | The skeleton only. New identifier `com.sanka.monitor`, scheme `sanka-monitor://`, a macOS 13.0 floor, and an updater endpoint on the public releases repository. |

## Deliberately not reused

- The meeting recorder (ScreenCaptureKit / AVFoundation / Swift helper) and its
  Screen Recording permission.
- The web-view-shell approach (bundling a web app as the desktop UI).
- The old desktop API. Every server endpoint this app talks to is under
  `/api/v2/monitor/agent/*`.
- The old URL scheme, mobile configs and other desktop-only screens.

The Tauri updater keypair was generated fresh on 2026-09-01. The old keypair was
never shipped to users and is not inherited.

## Sanka Recorder

Sanka Recorder, Sanka's call recorder, is a separate product in its own repository.
It shares no code with Sanka Monitor.
