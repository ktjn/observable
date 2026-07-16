#!/usr/bin/env python3
"""Verify that release-bearing manifests match the root VERSION file."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def fail(message: str) -> None:
    print(f"version check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def yaml_value(path: Path, key: str) -> str | None:
    prefix = f"{key}:"
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith(prefix):
            return line.split(":", 1)[1].strip().strip('"\'')
    return None


def main() -> None:
    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    if not SEMVER.fullmatch(version):
        fail(f"VERSION is not valid SemVer: {version!r}")

    mismatches: list[str] = []

    for manifest in sorted([*ROOT.glob("services/*/Cargo.toml"), *ROOT.glob("libs/*/Cargo.toml")]):
        package = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {})
        declared = package.get("version")
        if declared is not None and declared != version:
            mismatches.append(f"{manifest.relative_to(ROOT)}: package.version={declared}")

    frontend = ROOT / "apps/frontend/package.json"
    frontend_version = json.loads(frontend.read_text(encoding="utf-8")).get("version")
    if frontend_version != version:
        mismatches.append(f"{frontend.relative_to(ROOT)}: version={frontend_version}")

    for chart in sorted(ROOT.glob("charts/*/Chart.yaml")):
        chart_version = yaml_value(chart, "version")
        if chart_version != version:
            mismatches.append(f"{chart.relative_to(ROOT)}: version={chart_version}")
        app_version = yaml_value(chart, "appVersion")
        if app_version is not None and app_version != version:
            mismatches.append(f"{chart.relative_to(ROOT)}: appVersion={app_version}")

    if mismatches:
        fail("manifests do not match VERSION=" + version + "\n  " + "\n  ".join(mismatches))

    print(f"version contract valid: {version}")


if __name__ == "__main__":
    main()
