#!/usr/bin/env python3
"""Release files for the publish job in .github/workflows/release.yml.

    release_manifest.py <builds> <out>          name assets, write SHA256SUMS,
                                                latest.json and stable.json
    release_manifest.py --verify <out> <base>   download every file from the CDN
                                                and compare it with SHA256SUMS

<builds> holds the build jobs' artifacts (one folder each). <out> gets:

    release/   versioned files, the GitHub release and release.sanka.com/monitor/vX.Y.Z/
    latest/    the same installers under stable names, for IT scripts
    latest.json  the updater feed (Tauri), URLs on the CDN
    stable.json  channels/stable.json, which sanka.com/monitor/download/<target> follows

Names and the stable.json shape are the shared contract with sanka-public
(tracking/monitor-0.4-redesign/contract.md in the umbrella repo).
"""

import hashlib
import json
import os
import shutil
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

CDN_BASE = "https://release.sanka.com/monitor"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build(builds: Path, out: Path) -> None:
    tag = os.environ["RELEASE_TAG"]
    version = tag.removeprefix("v")
    arm = builds / "monitor-macos-arm64"
    uni = builds / "monitor-macos-universal"
    win = builds / "monitor-windows"
    stem = f"Sanka-Monitor-{version}"

    # source → (versioned name, stable name or None, stable.json key or None)
    assets = {
        arm / "monitor-arm64.dmg": (f"{stem}-arm64.dmg", "Sanka-Monitor-arm64.dmg", "mac-arm64"),
        uni / "monitor-universal.dmg": (f"{stem}-universal.dmg", "Sanka-Monitor-universal.dmg", "mac-universal"),
        win / "monitor-setup.exe": (f"{stem}-x64-setup.exe", "Sanka-Monitor-x64-setup.exe", "windows-x64"),
        uni / "monitor.pkg": (f"{stem}.pkg", "Sanka-Monitor.pkg", "mac-pkg"),
        win / "monitor.msi": (f"{stem}-x64.msi", "Sanka-Monitor-x64.msi", "windows-msi"),
        arm / "monitor-arm64.app.tar.gz": (f"{stem}-arm64.app.tar.gz", None, None),
        arm / "monitor-arm64.app.tar.gz.sig": (f"{stem}-arm64.app.tar.gz.sig", None, None),
        uni / "monitor-universal.app.tar.gz": (f"{stem}-universal.app.tar.gz", None, None),
        uni / "monitor-universal.app.tar.gz.sig": (f"{stem}-universal.app.tar.gz.sig", None, None),
        win / "monitor-setup.exe.sig": (f"{stem}-x64-setup.exe.sig", None, None),
    }

    release, latest = out / "release", out / "latest"
    release.mkdir(parents=True)
    latest.mkdir()
    files = {}
    sums = []
    for source, (name, stable, key) in assets.items():
        if not source.is_file() or source.stat().st_size == 0:
            sys.exit(f"missing or empty build output: {source}")
        target = release / name
        shutil.copy2(source, target)
        digest = sha256(target)
        sums.append(f"{digest}  {name}")
        if stable:
            shutil.copy2(source, latest / stable)
        if key:
            files[key] = {"url": f"{CDN_BASE}/{tag}/{name}", "sha256": digest, "size": target.stat().st_size}
    (release / "SHA256SUMS").write_text("\n".join(sums) + "\n")

    def signature(name: str) -> str:
        return (release / name).read_text().strip()

    notes = os.environ.get("RELEASE_NOTES", "")
    pub_date = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    # Apple silicon Macs get the smaller arm64 app; Intel Macs keep the universal
    # one. A universal install on Apple silicon moves to arm64 on this update.
    latest_json = {
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": {
            "darwin-aarch64": {
                "signature": signature(f"{stem}-arm64.app.tar.gz.sig"),
                "url": f"{CDN_BASE}/{tag}/{stem}-arm64.app.tar.gz",
            },
            "darwin-x86_64": {
                "signature": signature(f"{stem}-universal.app.tar.gz.sig"),
                "url": f"{CDN_BASE}/{tag}/{stem}-universal.app.tar.gz",
            },
            "windows-x86_64": {
                "signature": signature(f"{stem}-x64-setup.exe.sig"),
                "url": f"{CDN_BASE}/{tag}/{stem}-x64-setup.exe",
            },
        },
    }
    (out / "latest.json").write_text(json.dumps(latest_json, indent=2) + "\n")
    stable = {"version": version, "pub_date": pub_date, "notes": notes, "files": files}
    (out / "stable.json").write_text(json.dumps(stable, indent=2) + "\n")
    for path in sorted(out.rglob("*")):
        if path.is_file():
            print(path.relative_to(out))


def verify(out: Path, base: str) -> None:
    tag = "v" + json.loads((out / "stable.json").read_text())["version"]
    failures = 0
    for line in (out / "release" / "SHA256SUMS").read_text().splitlines():
        expected, name = line.split("  ", 1)
        url = f"{base}/{tag}/{name}"
        digest = hashlib.sha256()
        with urllib.request.urlopen(url, timeout=120) as response:
            for chunk in iter(lambda: response.read(1 << 20), b""):
                digest.update(chunk)
        status = "ok" if digest.hexdigest() == expected else "MISMATCH"
        failures += status != "ok"
        print(f"{status}  {url}")
    if failures:
        sys.exit(f"{failures} file(s) on the CDN differ from the build")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--verify":
        verify(Path(sys.argv[2]), sys.argv[3].rstrip("/"))
    elif len(sys.argv) == 3:
        build(Path(sys.argv[1]), Path(sys.argv[2]))
    else:
        sys.exit(__doc__)
