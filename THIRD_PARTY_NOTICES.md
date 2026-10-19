# Third-party notices

## screenpipe (architecture reference)

Parts of this project's architecture (Cargo workspace shape, tray/updater wiring
patterns, local buffering approach) reference the screenpipe project at commit
`892199f742e46d0c5d9e8c06687b35ca7c2b6547` (https://github.com/screenpipe/screenpipe),
which is licensed under the MIT License at that commit:

> MIT License
>
> Copyright (c) 2024-2026 louis030195

No screenpipe source code has been copied into this repository as of the P0 bootstrap.
If any file later incorporates code from that pinned commit, list the file here and
retain the MIT notice in the file header. Do not copy code from screenpipe commits
after the pinned commit without re-verifying the license.

## Rust and npm dependencies

Cargo and npm dependencies carry their own licenses; see `Cargo.lock` /
`package-lock.json` for the exact dependency trees.
