# Contributing to Sanka Monitor

Thanks for your interest. Sanka Monitor finds IT risks on a PC and keeps the data on
the PC unless the person chooses otherwise; contributions should keep it that way.

## Start with an issue

1. Search the issues and open pull requests first, and reuse an existing issue
   when it describes the same problem.
2. Open an issue before a pull request. Describe the problem, the expected
   behavior, the scope you propose and how the change can be checked. For bugs,
   include the Sanka Monitor version, the OS and version, and a minimal
   reproduction with personal data removed.
3. For a new check, a new data field, a new dependency or any change to what the
   app reads or sends, wait for a maintainer to agree on the approach before
   implementing it.
4. Fork the repository, create a focused branch and open a pull request against
   `main`, with `Closes #123` or `Refs #123` in its description.

## What we review for

- **Privacy invariants.** The app reads with the signed-in person's rights and
  sends nothing in local mode. Do not add fields that could carry window titles,
  URLs, document or file contents, keystrokes, screen captures or install paths.
  The browser check keeps per SaaS only the last day used and the days used in the
  last 30 (`BrowserUse`); do not widen it, `SaasRecord` or the shared
  `SaasUploadRequest` (key and last day used), and do not let the command-line
  scan read browser history. A change to what leaves the computer
  needs an issue and a maintainer's agreement first.
- **SaaS catalog.** `crates/monitor-saas/catalog/saas.json` lists each SaaS once.
  Web patterns name the hosts the product runs on, not the vendor's marketing,
  help or status sites (list those in `webExclude`). Desktop identifiers must be
  ones you have seen on a real install. Add a case to the matching tests in
  `crates/monitor-saas` for what a new entry matches and what it must not.
- **Lifecycle catalog.** `crates/monitor-findings/catalog/rules.json` holds the
  vendor support dates. A new or changed rule needs the vendor's own source URL and
  the date you checked it (`sourceUrl`, `verifiedOn`), and a test case in
  `crates/monitor-findings` that shows what it matches and what it must not.
  `eol-catalog.json` is a snapshot of `https://sanka.com/lab/eol-catalog.json`;
  refresh it rather than editing it.
- **Windows.** CI compiles, lints and tests the Windows code and runs a scan on a
  Windows runner. Say in the pull request what you could check on a real Windows PC
  and what you could not.
- **Tests and checks.** Include a regression test for a behavior fix, keep one
  problem per pull request, and run what CI runs:

  ```bash
  (cd apps/agent && npm ci && npm run build)
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

  If you change `templates/fleet/build_fleet_template.py`, regenerate the
  workbooks with it and commit the result.

Passing CI does not guarantee acceptance: maintainers review correctness, privacy,
compatibility and maintenance cost. AI-assisted contributions follow the same
process, and contributors are responsible for understanding and validating their
changes.

Never put real people's device data, enrollment codes, tokens or other credentials
in issues, pull requests or test fixtures. Report security problems privately as
described in [SECURITY.md](SECURITY.md).

## License

Sanka Monitor is licensed under the [Apache License 2.0](LICENSE). Unless you say
otherwise, any contribution you submit for inclusion is licensed under the same
terms, as section 5 of the license describes.

## Work History and browser bridge changes

Keep `monitor-context` local-only and preserve the existing UsageEvent and report
contracts. Add owning behavioral checks for privacy/consent, account/source
invalidation, exact-byte transport, receipt validation and closed native frames.
The synthetic cross-language package and Python-canonical summary hash are in
`apps/agent/src-tauri/fixtures/`; they contain no real activity or account data.
Native frame/relay checks live in `browser_context.rs`; desktop consent checks in
`context_history.rs`; HTTP wire checks in `monitor-sync/src/context_tests.rs`.

Do not grant TCC/UIA permissions, register a native host in a real browser profile,
or collect real work history as part of automated verification. Use isolated
browser profiles and synthetic provider pages. A macOS build and a Windows API
compile harness do not qualify native-device behavior. Keep actual Chrome, Edge,
Windows linking, lock/sleep behavior and resource-budget checks explicit in the
change report. Public extension-store identity and publishing are separate reviews.

### Synthetic History UI regressions

`apps/agent/tests/history-ui.test.mjs` runs the built desktop UI in an isolated,
headless Chromium with a synthetic Tauri boundary. It checks account-switch
editor/history isolation, delayed action results and offline disclosure changes;
Rust tests own the native consent/store/scheduler behavior. It never opens a real
browser profile or invokes native capture. The URL guard permits only HTTP
loopback IP origins (`127.0.0.1` or `[::1]`); all other browser requests are blocked.

Playwright is an **optional verification dependency**, not a runtime dependency.
Use an existing Playwright installation with Chromium available by setting
`MONITOR_PLAYWRIGHT_MODULE` to its absolute `index.mjs` path, or make the
`playwright` package available to Node's module resolution. From the repo root:

```bash
npm --prefix apps/agent run build
# Keep this task-owned preview server running in a separate terminal:
(cd apps/agent && npm exec vite preview -- --host 127.0.0.1 --port 1439 --strictPort)
# Point at your existing Playwright installation; no real browser profile is used.
MONITOR_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs MONITOR_UI_URL=http://127.0.0.1:1439 node --test apps/agent/tests/history-ui.test.mjs
```

Stop the preview server when verification finishes. A passing UI test proves UI
state transitions against synthetic inputs, not real OS capture or cloud behavior.
