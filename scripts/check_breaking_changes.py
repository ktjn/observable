#!/usr/bin/env python3
"""Detect breaking changes to Modelable entities/projections whose version number
did not change between a base and head models/ tree.

Rule: for any entity or projection ref (e.g. "tracing.Span@1") that exists in both
trees at the *same* version number, its field set must not shrink or change type.
- Entities marked "additive" may gain new fields on the same version.
- Removing a field, changing a field's type, or renaming a field (which looks like
  a remove + add of a different line) is always breaking, additive or not.
- Projections (derived, never additive here) may not change their field set at all
  on the same version number.

This does not replace bumping the version number for an intentional breaking
change -- it exists to catch an *unintentional* one that `modelable compile`'s
referential-integrity checks would not otherwise reject.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path


def run_modelable(*args: str) -> str:
    result = subprocess.run(
        ["modelable", *args], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"modelable {' '.join(args)} failed:\n{result.stdout}\n{result.stderr}"
        )
    return result.stdout


def export_graph(models_dir: Path) -> dict:
    with tempfile.NamedTemporaryFile(
        suffix=".json", delete=False, dir=str(models_dir.parent)
    ) as tmp:
        out_path = Path(tmp.name)
    try:
        run_modelable("graph", "export", str(models_dir), "--out", str(out_path))
        return json.loads(out_path.read_text(encoding="utf-8"))
    finally:
        out_path.unlink(missing_ok=True)


def versioned_refs(graph: dict) -> dict[str, dict]:
    refs = {}
    for node in graph["nodes"]:
        if node["kind"] in ("model_version", "projection_version"):
            refs[node["target_ref"]] = node
    return refs


def field_lines(describe_output: str) -> set[str]:
    return {line for line in describe_output.splitlines() if line.startswith("- ")}


def is_additive(describe_output: str) -> bool:
    return "change: additive" in describe_output


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True, type=Path)
    parser.add_argument("--head", required=True, type=Path)
    args = parser.parse_args()

    base_graph = export_graph(args.base)
    head_graph = export_graph(args.head)
    base_refs = versioned_refs(base_graph)
    head_refs = versioned_refs(head_graph)

    shared_refs = sorted(set(base_refs) & set(head_refs))
    findings = []

    for ref in shared_refs:
        base_describe = run_modelable("describe", ref, "--path", str(args.base))
        head_describe = run_modelable("describe", ref, "--path", str(args.head))

        base_fields = field_lines(base_describe)
        head_fields = field_lines(head_describe)
        removed = base_fields - head_fields
        added = head_fields - base_fields

        if not removed and not added:
            continue

        additive = is_additive(head_describe)
        breaking = bool(removed) or (bool(added) and not additive)

        if breaking:
            findings.append(
                {
                    "ref": ref,
                    "removed": sorted(removed),
                    "added": sorted(added),
                    "additive": additive,
                }
            )

    if not findings:
        print("No breaking changes detected on unchanged model/projection versions.")
        return 0

    print("Breaking change(s) detected without a version bump:\n")
    for finding in findings:
        print(f"  {finding['ref']} (additive={finding['additive']})")
        for line in finding["removed"]:
            print(f"    - removed: {line}")
        for line in finding["added"]:
            print(f"    + added:   {line}")
        print()

    print(
        "Each ref above changed shape while keeping its existing version number.\n"
        "If this is intentional, bump the version (e.g. `@1` -> `@2`) using an\n"
        "`evolves` declaration instead of editing the existing version in place.\n"
        "If it's not intentional, revert the field change."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
