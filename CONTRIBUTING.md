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
  A change to what leaves the computer needs an issue and a maintainer's agreement
  first.
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
