# Sanka Monitor Sheets companion

This opt-in companion recognizes selected Google Sheets commands in Chrome and Edge.
It sends only action categories to the running local Sanka Monitor app. It does not
read cell contents, keystrokes, clipboard, document names or page URLs into history.
A recognized click is an inference, not proof that the operation completed.

## Development setup

1. Build the Monitor desktop app from this checkout. In Work History, explicitly
   register the native messaging host for the browser you want to use.
2. Open the browser's extensions page, enable developer mode, and load this directory
   unpacked. The public development key in `identity.json` fixes the extension ID.
3. Enable Work History and Google Sheets in the desktop app. In the extension popup,
   choose **Enable in this browser** and grant access to `docs.google.com`.
4. Review local history in Monitor. Cloud sharing is a separate explicit action in
   the desktop app; this companion has no cloud transport.

Capture starts off and must be re-enabled after restarting the browser. Private
windows are unavailable. Only the focused, active, top-level Sheets page can send
observations. Switching tabs, losing window focus, removing permission or stopping
capture cancels pending observations. Unknown pages or controls are excluded.

Current profile covers exact English/Japanese commands: Download, Import, Make a
copy, Share and Print in a toolbar or menu. Keyboard shortcuts, Excel desktop,
other browser apps and unrecognized/localized controls are unsupported. App/SaaS
inventory discovery remains available separately. Repetition may suggest a workflow;
it never establishes business purpose, automation suitability or savings.

## Checks and distribution

Run `node --test apps/browser-extension/rules.test.cjs` from the repo root. The
integration check uses an isolated Chromium profile with a synthetic Sheets page;
no real Google account, user profile, native-host registration or history is used.
Real Chrome/Edge releases and native platform permission behavior require device
qualification. The development identity is not a published store identity: align
store IDs and native-host allowed origins in a reviewed release before distribution.
No signing private key is stored in the repo.
