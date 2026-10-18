"""Print the deployable components whose artifacts a diff affects (stdlib only).

Phase 7, docs/component-decomposition.md: only changed components rebuild.

Usage:
    python3 scripts/ci_changed_components.py BASE_REF HEAD_REF
    python3 scripts/ci_changed_components.py --all
    python3 scripts/ci_changed_components.py --packages COMPONENT...

Prints a space-separated list of component names (as used by
scripts/build-component-images.sh). --packages maps Rust component names to
their Cargo package names. A Rust component is affected when a changed
path falls inside its service crate or inside any workspace crate its release
binary transitively depends on (normal and build dependencies; dev-dependencies
do not reach the release artifact). Paths that feed every image (root Cargo
manifest/lockfile, Dockerfile, proto/, the build workflow and this script) mark
every Rust component. A path under libs/ or services/ that belongs to no known
workspace crate also marks every Rust component, so unknown layout fails safe.
"""

from pathlib import Path, PurePosixPath
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]

# component name -> Cargo package (mirrors scripts/build-component-images.sh).
RUST_COMPONENTS = {
    "auth": "auth-service",
    "control": "admin-service",
    "ingest": "ingest-gateway",
    "process": "stream-processor",
    "store-clickhouse": "storage-writer",
    "query": "query-api",
    "alerting": "alert-evaluator",
}
ALL_COMPONENTS = [*RUST_COMPONENTS, "web"]

GLOBAL_RUST_PATHS = {
    "Cargo.toml",
    "Cargo.lock",
    "Dockerfile",
    ".dockerignore",
    ".github/workflows/build.yml",
    "scripts/build-component-images.sh",
    "scripts/ci-changed-surfaces.sh",
    "scripts/ci_changed_components.py",
}
GLOBAL_RUST_PREFIXES = ("proto/",)
WEB_PATHS = {"package.json", "package-lock.json"}
WEB_PREFIXES = ("apps/frontend/",)


def load_workspace(root):
    """Return {package: (member_dir, {workspace path dependency packages})}."""
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    workspace = manifest["workspace"]
    workspace_paths = {
        name: spec["path"]
        for name, spec in workspace.get("dependencies", {}).items()
        if isinstance(spec, dict) and "path" in spec
    }

    members = {}
    dir_to_package = {}
    for member in workspace["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text())["package"]["name"]
        members[package] = member
        dir_to_package[member] = package

    def dependency_tables(crate):
        for key in ("dependencies", "build-dependencies"):
            yield crate.get(key, {})
        for target in crate.get("target", {}).values():
            for key in ("dependencies", "build-dependencies"):
                yield target.get(key, {})

    graph = {}
    for package, member in members.items():
        crate = tomllib.loads((root / member / "Cargo.toml").read_text())
        deps = set()
        for table in dependency_tables(crate):
            for name, spec in table.items():
                if not isinstance(spec, dict):
                    continue
                name = spec.get("package", name)
                if "path" in spec:
                    target = PurePosixPath(member, spec["path"])
                elif spec.get("workspace") and name in workspace_paths:
                    target = PurePosixPath(workspace_paths[name])
                else:
                    continue
                normalized = str(PurePosixPath(*_normalize(target.parts)))
                if normalized in dir_to_package:
                    deps.add(dir_to_package[normalized])
        graph[package] = (member, deps)
    return graph


def _normalize(parts):
    out = []
    for part in parts:
        if part == "..":
            out.pop()
        elif part not in (".", ""):
            out.append(part)
    return out


def closure(graph, package):
    seen = set()
    stack = [package]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        stack.extend(graph[current][1])
    return seen


def affected_components(paths, root=ROOT):
    graph = load_workspace(root)
    member_dirs = sorted(((member, package) for package, (member, _) in graph.items()),
                         key=lambda item: len(item[0]), reverse=True)
    reach = {component: closure(graph, package) for component, package in RUST_COMPONENTS.items()}

    affected = set()
    for path in paths:
        if path in GLOBAL_RUST_PATHS or path.startswith(GLOBAL_RUST_PREFIXES):
            affected.update(RUST_COMPONENTS)
            continue
        if path in WEB_PATHS or path.startswith(WEB_PREFIXES):
            affected.add("web")
            continue
        owner = next((package for member, package in member_dirs
                      if path == member or path.startswith(member + "/")), None)
        if owner is None:
            if path.startswith(("libs/", "services/")):
                affected.update(RUST_COMPONENTS)
            continue
        affected.update(component for component, deps in reach.items() if owner in deps)
    return [component for component in ALL_COMPONENTS if component in affected]


def changed_paths(base, head):
    result = subprocess.run(["git", "diff", "--name-only", base, head],
                            cwd=ROOT, check=True, capture_output=True, text=True)
    return [line for line in result.stdout.splitlines() if line]


def main(argv):
    if argv == ["--all"]:
        print(" ".join(ALL_COMPONENTS))
        return 0
    if argv[:1] == ["--packages"]:
        unknown = [c for c in argv[1:] if c not in RUST_COMPONENTS]
        if unknown:
            print(f"not a Rust component: {' '.join(unknown)}", file=sys.stderr)
            return 2
        print(" ".join(RUST_COMPONENTS[c] for c in argv[1:]))
        return 0
    if len(argv) != 2:
        print(__doc__.strip().split("\n\n")[1], file=sys.stderr)
        return 2
    print(" ".join(affected_components(changed_paths(*argv))))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
