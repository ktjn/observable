#!/usr/bin/env python3
"""Verify that release-facing version metadata matches the canonical VERSION file."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def fail(message: str) -> None:
    print(f"version check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def check_equal(label: str, actual: str | None, expected: str) -> None:
    if actual != expected:
        fail(f"{label} is {actual!r}; expected {expected!r}")


def main() -> None:
    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", version):
        fail(f"VERSION contains invalid semantic version {version!r}")

    workspace = read_toml(ROOT / "Cargo.toml")
    for member in workspace["workspace"]["members"]:
        manifest = ROOT / member / "Cargo.toml"
        package_version = read_toml(manifest)["package"].get("version")
        check_equal(str(manifest.relative_to(ROOT)), package_version, version)

    frontend = json.loads((ROOT / "apps/frontend/package.json").read_text(encoding="utf-8"))
    check_equal("apps/frontend/package.json", frontend.get("version"), version)

    chart_text = (ROOT / "charts/observable/Chart.yaml").read_text(encoding="utf-8")
    chart_version = re.search(r"^version:\s*[\"']?([^\"'\s]+)", chart_text, re.MULTILINE)
    app_version = re.search(r"^appVersion:\s*[\"']?([^\"'\s]+)", chart_text, re.MULTILINE)
    check_equal("charts/observable/Chart.yaml version", chart_version.group(1) if chart_version else None, version)
    check_equal("charts/observable/Chart.yaml appVersion", app_version.group(1) if app_version else None, version)

    print(f"release version metadata is synchronized at {version}")


if __name__ == "__main__":
    main()
