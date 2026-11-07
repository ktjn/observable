"""Collect source-only release evidence from a committed Git tree (stdlib only).

Usage: python scripts/collect_component_source.py COMPONENT [--ref HEAD] [--repo PATH]

This does not verify external build materials or authorize publication. Recipe
hashes deliberately fail closed when Docker inputs need selection review.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

from ci_changed_components import ALL_COMPONENTS, RUST_COMPONENTS, closure, load_workspace


ROOT = Path(__file__).resolve().parents[1]
ALGORITHM = "component-source-v1"
RECIPES = {
    "Dockerfile": "81ccccef973009f7bfcd5e821f372a424e771fbbe9c4622dd3284cbf53c0a2ff",
    "apps/frontend/Dockerfile": "f1f4d8b71fad48003126f31a12de7eeb9a234c03c1c97ea1334cae7407932c36",
}
# The image stage each recipe builds into a deployable component artifact. A
# `None` target is the recipe's last stage (the frontend serves from nginx).
RECIPE_TARGETS = {"Dockerfile": "component-runtime", "apps/frontend/Dockerfile": None}
BUILD_SCRIPTS = {".github/workflows/build.yml", "scripts/build-component-images.sh",
                 "scripts/ci_changed_components.py", "scripts/ci-changed-surfaces.sh",
                 "scripts/collect_component_source.py"}


class InvalidSource(ValueError):
    """Unsupported or unverifiable source snapshot."""


def git(root, *args, data=None):
    try:
        return subprocess.run(["git", *args], cwd=root, input=data, capture_output=True,
                              check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        raise InvalidSource("Git source inspection failed") from None


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def collect(root, component, ref="HEAD"):
    try:
        return _collect(root, component, ref)
    except InvalidSource:
        raise
    except (ValueError, KeyError, TypeError, IndexError, AttributeError, UnicodeError):
        raise InvalidSource("invalid or unsupported Git source layout") from None


class GitSnapshot:
    """Read raw, regular Git blobs at one immutable commit, without extraction."""

    def __init__(self, root, ref):
        self.root = root
        self.revision = git(root, "rev-parse", "--verify", "--end-of-options", ref + "^{commit}").decode().strip()
        if re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", self.revision) is None:
            raise InvalidSource("invalid source revision")
        self.entries, self.contents = {}, {}
        for raw in git(root, "ls-tree", "-rz", "--full-tree", self.revision).split(b"\0"):
            if not raw:
                continue
            metadata, path = raw.split(b"\t", 1)
            mode, kind, oid = metadata.decode("ascii").split()
            self.entries[path.decode("utf-8")] = (mode, kind, oid)

    def read_many(self, paths):
        pending = sorted(set(paths) - self.contents.keys())
        for path in pending:
            if path not in self.entries or self.entries[path][:2] not in (("100644", "blob"), ("100755", "blob")):
                raise InvalidSource("missing or unsupported source file")
        if not pending:
            return
        output = git(self.root, "cat-file", "--batch",
                     data="".join(self.entries[path][2] + "\n" for path in pending).encode("ascii"))
        offset = 0
        for path in pending:
            end = output.index(b"\n", offset)
            oid, kind, size = output[offset:end].decode("ascii").split()
            if oid != self.entries[path][2] or kind != "blob":
                raise InvalidSource("unexpected Git object")
            start = end + 1
            offset = start + int(size)
            if output[offset:offset + 1] != b"\n":
                raise InvalidSource("incomplete Git object")
            self.contents[path] = output[start:offset]
            offset += 1
        if offset != len(output):
            raise InvalidSource("unexpected Git object data")

    def read_bytes(self, path):
        self.read_many([path])
        return self.contents[path]


def _collect(root, component, ref):
    """Return a source inventory and digest; worktree bytes are never read."""
    if component not in ALL_COMPONENTS:
        raise InvalidSource("unknown runtime component")
    snapshot = GitSnapshot(root, ref)
    revision, entries = snapshot.revision, snapshot.entries
    read_bytes, read_many = snapshot.read_bytes, snapshot.read_many

    recipe = "apps/frontend/Dockerfile" if component == "web" else "Dockerfile"
    if hashlib.sha256(read_bytes(recipe)).hexdigest() != RECIPES[recipe]:
        raise InvalidSource("Docker recipe changed; source selection review required")

    selected = {recipe, ".dockerignore"}
    if component == "web":
        package = json.loads(read_bytes("package.json"))
        if not isinstance(package, dict) or package.get("workspaces") != ["apps/frontend"]:
            raise InvalidSource("unsupported npm workspace layout")
        selected.update({"package.json", "package-lock.json", "apps/frontend/package.json"})
        prefixes = ("apps/frontend/",)
    else:
        read_many(path for path in entries if path.endswith("Cargo.toml"))
        graph = load_workspace(root, lambda path: read_bytes(path).decode("utf-8"), strict=True)
        if RUST_COMPONENTS[component] not in graph:
            raise InvalidSource("component package missing from workspace")
        selected.update({"Cargo.toml", "Cargo.lock"})
        selected.update(member + "/Cargo.toml" for member, _ in graph.values())
        directories = [graph[package][0] for package in closure(graph, RUST_COMPONENTS[component])]
        prefixes = tuple(directory + "/" for directory in directories) + ("proto/",)
        known = tuple(member + "/" for member, _ in graph.values()) + ("services/.junie/",)
        if any(path.startswith(("libs/", "services/")) and not path.startswith(known)
               for path in entries):
            raise InvalidSource("unknown workspace source layout")
        if not any(path.startswith("proto/otlp/") for path in entries):
            raise InvalidSource("runtime protocol sources missing")

    selected.update(path for path in entries if path.startswith(prefixes) or path in BUILD_SCRIPTS)
    if component != "web" and any(path.endswith("/build.rs") for path in selected):
        raise InvalidSource("build script requires external-input selection review")
    read_many(selected)
    files = [{"path": path, "mode": entries[path][0] if path in entries else "",
              "digest": digest(read_bytes(path))} for path in sorted(selected)]
    source_digest = digest(canonical({"algorithm": ALGORITHM, "component": component, "files": files}))
    return {"component": component, "source_revision": revision, "source_algorithm": ALGORITHM,
            "source_digest": source_digest, "files": files, "identity_verified": False}


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", choices=ALL_COMPONENTS)
    parser.add_argument("--ref", default="HEAD")
    parser.add_argument("--repo", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    try:
        result = collect(args.repo, args.component, args.ref)
    except (InvalidSource, ValueError, KeyError, TypeError, IndexError, UnicodeError):
        print("source collection rejected: invalid or unsupported Git snapshot", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
