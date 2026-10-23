"""Shared version lookup from committed source; no builder dependency."""

import json
import re
import tomllib

from ci_changed_components import RUST_COMPONENTS, load_workspace
from plan_component_release import VERSION, matches


def component_version(snapshot, component):
    key = "storeClickhouse" if component == "store-clickhouse" else component
    manifest = snapshot.read_bytes("distribution/manifest.yaml").decode("utf-8")
    versions = re.findall(rf"^  {key}:[ \t]+([^\s#]+)[ \t]*$", manifest, re.MULTILINE)
    if len(versions) != 1 or not matches(VERSION, versions[0]) or len(versions[0]) > 128:
        raise ValueError("invalid snapshot component version")
    if component == "web":
        actual = json.loads(snapshot.read_bytes("apps/frontend/package.json"))["version"]
    else:
        graph = load_workspace(snapshot.root, lambda path: snapshot.read_bytes(path).decode(), strict=True)
        member = graph[RUST_COMPONENTS[component]][0]
        actual = tomllib.loads(snapshot.read_bytes(member + "/Cargo.toml").decode())["package"]["version"]
    if actual != versions[0]:
        raise ValueError("snapshot package and distribution version disagree")
    return actual
