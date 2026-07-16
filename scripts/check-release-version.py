#!/usr/bin/env python3
"""Verify that release-facing metadata matches the root VERSION file."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION_PATTERN = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def fail(message: str) -> None:
    print(f"version check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_toml(path: Path) -> dict:
    with path.open("rb") as file:
        return tomllib.load(file)


def expect(actual: str | None, expected: str, source: str) -> None:
    if actual != expected:
        fail(f"{source} is {actual!r}; expected {expected!r}")


def yaml_scalar(path: Path, key: str) -> str | None:
    pattern = re.compile(rf"^{re.escape(key)}:\s*[\"']?([^\"'#\s]+)")
    for line in path.read_text(encoding="utf-8").splitlines():
        match = pattern.match(line)
        if match:
            return match.group(1)
    return None


def main() -> None:
    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    if not VERSION_PATTERN.fullmatch(version):
        fail(f"VERSION contains invalid semantic version {version!r}")

    workspace = load_toml(ROOT / "Cargo.toml")["workspace"]
    for member in workspace["members"]:
        manifest = ROOT / member / "Cargo.toml"
        package_version = load_toml(manifest)["package"].get("version")
        expect(package_version, version, str(manifest.relative_to(ROOT)))

    frontend = json.loads((ROOT / "apps/frontend/package.json").read_text(encoding="utf-8"))
    expect(frontend.get("version"), version, "apps/frontend/package.json")

    chart = ROOT / "charts/observable/Chart.yaml"
    expect(yaml_scalar(chart, "version"), version, "charts/observable/Chart.yaml version")
    expect(yaml_scalar(chart, "appVersion"), version, "charts/observable/Chart.yaml appVersion")

    common_chart = ROOT / "charts/observable-common/Chart.yaml"
    expect(yaml_scalar(common_chart, "version"), version, "charts/observable-common/Chart.yaml version")

    print(f"release metadata matches VERSION={version}")


if __name__ == "__main__":
    main()
