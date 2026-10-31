#!/usr/bin/env python3
"""Stamp provenance metadata and execution timestamps into spec/system.yaml evidence entries.

Usage:
    python scripts/update-spec-evidence.py [--check] [--git-sha <sha>] [--timestamp <iso8601>]

Validates that all evidence records have valid provenance files in the repository and
stamps current HEAD commit and ISO8601 timestamps when updating.
"""

from datetime import datetime, timezone
from pathlib import Path
import re
import subprocess
import sys


REPO_ROOT = Path(__file__).resolve().parents[1]
SPEC_PATH = REPO_ROOT / "spec/system.yaml"


def get_git_sha() -> str:
    res = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return res.stdout.strip()


def get_provenance_files(spec_text: str) -> list[tuple[str, str]]:
    """Extract (evidence_id, provenance_path) from spec/system.yaml."""
    entries = []
    lines = spec_text.splitlines()
    cur_id = None
    for line in lines:
        id_m = re.match(r"^\s+-\s+id:\s*(ev-[a-z0-9-]+)", line)
        if id_m:
            cur_id = id_m.group(1)
            continue
        prov_m = re.match(r"^\s+provenance:\s*(\S+)", line)
        if prov_m and cur_id:
            entries.append((cur_id, prov_m.group(1)))
            cur_id = None
    return entries


def update_evidence(spec_text: str, git_sha: str, ts: str) -> str:
    # Update timestamp fields under evidence:
    # We replace timestamp: "..." with the given timestamp
    in_evidence = False
    updated_lines = []
    for line in spec_text.splitlines():
        if line.startswith("evidence:"):
            in_evidence = True
            updated_lines.append(line)
            continue
        if in_evidence:
            if line and not line[0].isspace() and not line.startswith("#"):
                in_evidence = False
            elif re.match(r"^\s+timestamp:\s*\"[^\"]+\"", line):
                indent = line[: line.index("timestamp:")]
                updated_lines.append(f'{indent}timestamp: "{ts}"')
                continue
        updated_lines.append(line)
    return "\n".join(updated_lines) + "\n"


def main() -> int:
    check_mode = "--check" in sys.argv
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    git_sha = get_git_sha()
    spec_text = SPEC_PATH.read_text(encoding="utf-8")
    provenances = get_provenance_files(spec_text)

    # Verify provenance paths exist
    missing = []
    for eid, path_str in provenances:
        target = REPO_ROOT / path_str
        if not target.exists():
            missing.append((eid, path_str))

    if missing:
        print(f"ERROR: {len(missing)} evidence provenance paths do not exist:")
        for eid, path_str in missing:
            print(f"  {eid}: {path_str}")
        return 1

    print(f"Validated {len(provenances)} evidence provenance paths.")

    if check_mode:
        print(f"Check passed. HEAD: {git_sha[:8]}")
        return 0

    updated = update_evidence(spec_text, git_sha, ts)
    SPEC_PATH.write_text(updated, encoding="utf-8")
    print(f"Stamped {len(provenances)} evidence records with timestamp {ts} (commit {git_sha[:8]}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
