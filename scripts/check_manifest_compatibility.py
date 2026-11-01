#!/usr/bin/env python3
"""Verifies component-version compatibility from the distribution manifest

Evaluates whether the pinned component versions in distribution/manifest.yaml
satisfy all cross-component capability constraints declared in spec/system.yaml.
Runs standalone (stdlib only) and integrates into CI / release-candidate gates.

Usage:
  python scripts/check-manifest-compatibility.py [manifest_path] [spec_path] [--quiet]
"""

from pathlib import Path
import re
import sys
from typing import Dict, List, Optional, Tuple

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "distribution/manifest.yaml"
DEFAULT_SPEC = ROOT / "spec/system.yaml"

MANIFEST_TO_SPEC = {
    "auth": "observable-auth",
    "control": "observable-control",
    "ingest": "observable-ingest",
    "process": "observable-process",
    "storeClickhouse": "observable-store-clickhouse",
    "query": "observable-query",
    "alerting": "observable-alerting",
    "web": "observable-web",
}
SPEC_TO_MANIFEST = {v: k for k, v in MANIFEST_TO_SPEC.items()}


def parse_semver(v: str) -> Tuple[int, int, int]:
    """Parse semver string 'X.Y.Z[-prerelease]' into (major, minor, patch)."""
    m = re.match(r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?", v.strip())
    if not m:
        raise ValueError(f"Invalid semver: '{v}'")
    return int(m.group(1)), int(m.group(2)), int(m.group(3))


def satisfies_constraint(version: str, constraint: str) -> bool:
    """Check if version satisfies semver constraint (e.g. '^0.1', '~0.1', '>=0.1.0', '0.1.0')."""
    v_maj, v_min, v_pat = parse_semver(version)
    c = constraint.strip()

    if c.startswith("^"):
        req = c[1:].strip()
        parts = req.split(".")
        req_maj = int(parts[0])
        req_min = int(parts[1]) if len(parts) > 1 else 0
        req_pat = int(parts[2]) if len(parts) > 2 else 0

        if req_maj == 0:
            if req_min == 0:
                return (v_maj, v_min, v_pat) == (req_maj, req_min, req_pat)
            return v_maj == 0 and v_min == req_min and v_pat >= req_pat
        return v_maj == req_maj and (
            v_min > req_min or (v_min == req_min and v_pat >= req_pat)
        )

    if c.startswith("~"):
        req = c[1:].strip()
        parts = req.split(".")
        req_maj = int(parts[0])
        req_min = int(parts[1]) if len(parts) > 1 else 0
        req_pat = int(parts[2]) if len(parts) > 2 else 0
        return v_maj == req_maj and v_min == req_min and v_pat >= req_pat

    if c.startswith(">="):
        req = c[2:].strip()
        r_maj, r_min, r_pat = parse_semver(req)
        return (v_maj, v_min, v_pat) >= (r_maj, r_min, r_pat)

    # Exact equality
    r_maj, r_min, r_pat = parse_semver(c)
    return (v_maj, v_min, v_pat) == (r_maj, r_min, r_pat)


def parse_manifest(text: str) -> Tuple[str, Dict[str, str]]:
    """Parse distribution/manifest.yaml content into product version and component versions."""
    pm = re.search(r"(?m)^productVersion:\s*(\S+)", text)
    product_version = pm.group(1) if pm else ""
    components: Dict[str, str] = {}
    in_comps = False
    for line in text.splitlines():
        if line.startswith("components:"):
            in_comps = True
            continue
        if in_comps:
            if line and not line[0].isspace() and not line.startswith("#"):
                break
            cm = re.match(r"^\s{2}([A-Za-z0-9_]+):\s*(\S+)", line)
            if cm:
                components[cm.group(1)] = cm.group(2)
    return product_version, components


def parse_spec_components(text: str) -> List[Dict[str, object]]:
    """Parse components list from spec/system.yaml content."""
    lines = text.splitlines()
    in_comps = False
    blocks: List[List[str]] = []
    cur: List[str] = []

    for line in lines:
        if re.match(r"^components:\s*$", line):
            in_comps = True
            continue
        if in_comps:
            if line and not line[0].isspace() and not line.startswith("#"):
                break
            if line.startswith("  - id:"):
                if cur:
                    blocks.append(cur)
                cur = [line]
            elif cur:
                cur.append(line)
    if cur:
        blocks.append(cur)

    results = []
    for b in blocks:
        block_str = "\n".join(b)
        cid_m = re.search(r"^\s*-\s*id:\s*(\S+)", block_str, re.M)
        if not cid_m:
            continue
        cid = cid_m.group(1)

        ver_m = re.search(r'^\s*version:\s*"?([^"\n]+)"?', block_str, re.M)
        ver = ver_m.group(1) if ver_m else "0.1.0"

        provides = []
        prov_tail = block_str.partition("provides:\n")[2]
        for s in prov_tail.splitlines():
            if s.startswith("      - "):
                provides.append(s.removeprefix("      - ").strip())
            else:
                break

        requires = []
        req_tail = block_str.partition("requires:\n")[2]
        for s in req_tail.splitlines():
            if s.startswith("      - "):
                requires.append(s.removeprefix("      - ").strip())
            else:
                break

        results.append(
            {
                "id": cid,
                "version": ver,
                "provides": provides,
                "requires": requires,
            }
        )
    return results


def verify_compatibility(
    manifest_text: str, spec_text: str
) -> Tuple[bool, List[str], List[str]]:
    """Evaluate whether manifest component versions satisfy spec requirements.

    Returns:
        (is_valid, log_messages, error_messages)
    """
    prod_ver, manifest_comps = parse_manifest(manifest_text)
    spec_components = parse_spec_components(spec_text)

    logs: List[str] = []
    errors: List[str] = []

    # Map each capability to the component providing it
    cap_providers: Dict[str, str] = {}
    for comp in spec_components:
        cid = str(comp["id"])
        for cap in comp["provides"]:  # type: ignore[union-attr]
            cap_providers[str(cap)] = cid

    # Verify each component's requirements
    for comp in spec_components:
        consumer_id = str(comp["id"])
        manifest_key = SPEC_TO_MANIFEST.get(consumer_id)
        if not manifest_key or manifest_key not in manifest_comps:
            continue

        consumer_ver = manifest_comps[manifest_key]
        comp_logs: List[str] = []

        for req in comp["requires"]:  # type: ignore[union-attr]
            req_str = str(req)
            if "@" in req_str:
                cap_name, constraint = req_str.split("@", 1)
            else:
                cap_name = req_str
                constraint = "^0.1"

            if cap_name not in cap_providers:
                errors.append(
                    f"Component {consumer_id} requires unknown capability '{cap_name}'"
                )
                continue

            provider_id = cap_providers[cap_name]
            provider_mkey = SPEC_TO_MANIFEST.get(provider_id)
            if not provider_mkey or provider_mkey not in manifest_comps:
                errors.append(
                    f"Component {consumer_id} requires '{cap_name}', but provider "
                    f"{provider_id} is missing from distribution manifest"
                )
                continue

            provider_ver = manifest_comps[provider_mkey]
            if not satisfies_constraint(provider_ver, constraint):
                errors.append(
                    f"Incompatible component version: {consumer_id} {consumer_ver} requires "
                    f"'{cap_name}@{constraint}', but {provider_id} is pinned to version "
                    f"{provider_ver} in distribution manifest"
                )
            else:
                comp_logs.append(
                    f"  - {cap_name}@{constraint} satisfied by {provider_id} {provider_ver}"
                )

        if comp_logs:
            logs.append(f"OK  {consumer_id} ({consumer_ver}) satisfies:")
            logs.extend(comp_logs)

    is_valid = len(errors) == 0
    return is_valid, logs, errors


def main() -> int:
    manifest_path = DEFAULT_MANIFEST
    spec_path = DEFAULT_SPEC
    quiet = False

    args = sys.argv[1:]
    paths = []
    for arg in args:
        if arg == "--quiet":
            quiet = True
        elif arg in ("--help", "-h"):
            print(__doc__.strip())
            return 0
        else:
            paths.append(Path(arg))

    if len(paths) >= 1:
        manifest_path = paths[0]
    if len(paths) >= 2:
        spec_path = paths[1]

    if not manifest_path.is_file():
        print(f"FAIL: manifest not found: {manifest_path}", file=sys.stderr)
        return 1
    if not spec_path.is_file():
        print(f"FAIL: system spec not found: {spec_path}", file=sys.stderr)
        return 1

    manifest_text = manifest_path.read_text(encoding="utf-8")
    spec_text = spec_path.read_text(encoding="utf-8")

    is_valid, logs, errors = verify_compatibility(manifest_text, spec_text)

    if not quiet:
        for log in logs:
            print(log)

    if not is_valid:
        print(
            f"\nFAIL: component compatibility check failed ({len(errors)} error(s)):",
            file=sys.stderr,
        )
        for err in errors:
            print(f"  * {err}", file=sys.stderr)
        return 1

    if not quiet:
        print(
            f"\nOK  distribution manifest component compatibility verified across all components"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
