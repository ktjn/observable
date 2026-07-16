#!/usr/bin/env python3
"""Verify that product manifests match the repository VERSION file."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION_FILE = ROOT / "VERSION"


def fail(message: str) -> None:
    print(f"version check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_version() -> str:
    version = VERSION_FILE.read_text(encoding="utf-8").strip()
    if not re.fullmatch(r"0|[1-9]\d*\.0|[1-9]\d*\.\d+|0\.\d+\.\d+", version):
        fail(f"VERSION contains an invalid semantic version: {version!r}")
    return version


def check_cargo_manifests(expected: str) -> None:
    manifests = sorted((ROOT / "services").glob("*/Cargo.toml"))
    manifests += sorted((ROOT / "libs").glob("*/Cargo.toml"))
    for manifest in manifests:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        actual = data.get("package", {}).get("version")
        if actual != expected:
            fail(f"{manifest.relative_to(ROOT)} has package.version={actual!r}, expected {expected!r}")


def check_json_manifest(path: Path, expected: str) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    actual = data.get("version")
    if actual != expected:
        fail(f"{path.relative_to(ROOT)} has version={actual!r}, expected {expected!r}")


def check_python_manifest(path: Path, expected: str) -> None:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    actual = data.get("project", {}).get("version")
    if actual != expected:
        fail(f"{path.relative_to(ROOT)} has project.version={actual!r}, expected {expected!r}")


def chart_field(text: str, field: str) -> str | None:
    match = re.search(rf"(?m)^{re.escape(field)}:\s*[\"']?([^\s\"']+)[\"']?\s*$", text)
    return match.group(1) if match else None


def check_helm_charts(expected: str) -> None:
    for chart in sorted((ROOT / "charts").glob("*/Chart.yaml")):
        text = chart.read_text(encoding="utf-8")
        actual = chart_field(text, "version")
        if actual != expected:
            fail(f"{chart.relative_to(ROOT)} has version={actual!r}, expected {expected!r}")

        app_version = chart_field(text, "appVersion")
        if app_version is not None and app_version != expected:
            fail(f"{chart.relative_to(ROOT)} has appVersion={app_version!r}, expected {expected!r}")

    observable_chart = (ROOT / "charts/observable/Chart.yaml").read_text(encoding="utf-8")
    dependency = re.search(
        r"(?ms)-\s+name:\s+observable-common\s+version:\s*[\"']?([^\s\"']+)",
        observable_chart,
    )
    if dependency is None or dependency.group(1) != expected:
        actual = dependency.group(1) if dependency else None
        fail(
            "charts/observable/Chart.yaml has observable-common dependency "
            f"version={actual!r}, expected {expected!r}"
        )


def main() -> None:
    expected = read_version()
    check_cargo_manifests(expected)
    check_json_manifest(ROOT / "apps/frontend/package.json", expected)
    check_python_manifest(ROOT / "models/pyproject.toml", expected)
    check_helm_charts(expected)
    print(f"all product manifests match VERSION={expected}")


if __name__ == "__main__":
    main()
