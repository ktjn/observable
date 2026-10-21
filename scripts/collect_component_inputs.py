"""Resolve a component's declared build-input identity from committed bytes.

Usage: python scripts/collect_component_inputs.py COMPONENT [--ref HEAD]
       [--repo PATH] [--platform linux/amd64]...

`component-input-v1` fingerprints the *declared* inputs of one component image:
the committed source closure, the reviewed recipe, every external image
reference reachable from the component's build target (each pinned by
`@sha256:` digest), and the target platforms. The recipe must keep every base
and frontend image digest-pinned; an unreviewed mutable tag fails closed.

This is not complete or authenticated input identity. The digests are not
verified against a registry, no build argument values are covered, and what
apt, rustup, crates.io and npm resolve at build time stays mutable. Those remain
release gates; this tool never authorizes publication.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

from build_component_snapshot import component_version
from ci_changed_components import ALL_COMPONENTS
from collect_component_source import (RECIPE_TARGETS, ROOT, GitSnapshot, canonical, collect, digest)


ALGORITHM = "component-input-v1"
STAGE_NAME = re.compile(r"[A-Za-z][A-Za-z0-9_.-]*")
IMAGE_DIGEST = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9._:/-]*@sha256:[0-9a-f]{64}$")
PLATFORM = re.compile(r"linux/(?:amd64|arm64)")
SYNTAX = re.compile(r"#\s*syntax=(\S+)")
APT = re.compile(r"apt-get\s+install\b([^&;|]*)")
RUSTUP = re.compile(r"rustup\s+component\s+add\b([^&;|]*)")
DOWNLOAD = re.compile(r"https?://[^\s\"'\\]+")


class InvalidInputs(ValueError):
    """No complete, immutable build-input identity is available."""


def syntax_directive(text):
    """Return the Dockerfile frontend reference, or None if there is no directive.

    BuildKit honors `# syntax=` only as the first non-blank line; a directive
    anywhere else is ignored by the build and must not be recorded as an input.
    """
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        match = SYNTAX.fullmatch(stripped)
        return match[1] if match else None
    return None


def instructions(text):
    """Yield logical Dockerfile instructions with continuations joined.

    Comment lines are removed wherever they appear, including inside a
    continuation, matching the Dockerfile parser.
    """
    buffer = ""
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("#"):
            continue
        if not buffer and not line:
            continue
        if line.endswith("\\"):
            buffer += line[:-1].rstrip() + " "
            continue
        buffer += line
        yield buffer
        buffer = ""
    if buffer:
        yield buffer


def resolve_reference(stages, names, value):
    """Return the referenced earlier stage index, or None for an external reference.

    `names` holds only the stages defined so far, so a forward reference is not a
    stage: Docker rejects it, and this tool then treats it as an unpinned image.
    """
    if value.lower() in names:
        return names[value.lower()]
    if value.isdigit():
        index = int(value)
        if index >= len(stages):
            raise InvalidInputs("unresolved Dockerfile stage reference")
        return index
    return None


def record_instruction(stage, stages, names, line):
    """Append one stage instruction, resolving any `COPY --from` immediately."""
    stage["instructions"].append(line)
    keyword, _, rest = line.partition(" ")
    if keyword.lower() != "copy":
        return
    for token in rest.split():
        if token.startswith("--from="):
            value = token[len("--from="):]
            stage["copies"].append((resolve_reference(stages, names, value), value))


def stage_graph(text):
    """Return (stages, names); reject unreviewed instruction shapes."""
    stages, names = [], {}
    for line in instructions(text):
        keyword, _, rest = line.partition(" ")
        if keyword.lower() == "from":
            tokens = rest.split()
            # A per-stage platform or unknown flag changes build inputs in an
            # unreviewed way; fail closed instead of ignoring it.
            if tokens and tokens[0].startswith("--"):
                raise InvalidInputs("unsupported Dockerfile stage flag")
            if len(tokens) not in (1, 3) or (len(tokens) == 3 and tokens[1].lower() != "as"):
                raise InvalidInputs("unsupported Dockerfile layout")
            if len(tokens) == 3 and STAGE_NAME.fullmatch(tokens[2]) is None:
                raise InvalidInputs("unsupported Dockerfile layout")
            reference = tokens[0]
            stage = {"index": len(stages), "name": tokens[2] if len(tokens) == 3 else str(len(stages)),
                     "reference": reference,
                     "internal": resolve_reference(stages, names, reference),
                     "instructions": [], "copies": []}
            stages.append(stage)
            names[stage["name"].lower()] = stage["index"]
            continue
        if not stages:
            raise InvalidInputs("unsupported Dockerfile layout")
        record_instruction(stages[-1], stages, names, line)
    if not stages:
        raise InvalidInputs("unsupported Dockerfile layout")
    return stages, names


def add_image(images, reference, stage):
    """Record one digest-pinned external image and the stages that reach it."""
    if IMAGE_DIGEST.fullmatch(reference) is None:
        raise InvalidInputs("unresolved stage or unpinned external image reference")
    entry = images.setdefault(
        reference, {"digest": "sha256:" + reference.rsplit("@sha256:", 1)[1], "stages": set()})
    entry["stages"].add(stage["name"])


def reachable(stages, start):
    """Walk the target stage's base and `COPY --from` graph."""
    images = {}
    materials = {"apt": set(), "rustup": set(), "downloads": set()}
    pending, seen = [start], set()
    while pending:
        index = pending.pop()
        if index in seen:
            continue
        seen.add(index)
        stage = stages[index]
        if stage["internal"] is None:
            add_image(images, stage["reference"], stage)
        else:
            pending.append(stage["internal"])
        for target, value in stage["copies"]:
            if target is None:
                add_image(images, value, stage)
            else:
                pending.append(target)
        for line in stage["instructions"]:
            keyword, _, rest = line.partition(" ")
            if keyword.lower() != "run":
                continue
            for pattern, kind in ((APT, "apt"), (RUSTUP, "rustup")):
                for args in pattern.findall(rest):
                    materials[kind].update(t for t in args.split() if not t.startswith("-"))
            materials["downloads"].update(DOWNLOAD.findall(rest))
    return images, materials


def parse_recipe(text, target):
    """Validate one reviewed recipe and return its reachable images and materials."""
    match = syntax_directive(text)
    frontend = None
    if match:
        if IMAGE_DIGEST.fullmatch(match) is None:
            raise InvalidInputs("Dockerfile frontend is not digest-pinned")
        frontend = {"reference": match, "digest": "sha256:" + match.rsplit("@sha256:", 1)[1]}
    stages, names = stage_graph(text)
    if target is None:
        start = len(stages) - 1
    else:
        found = [stage["index"] for stage in stages if stage["name"].lower() == target.lower()]
        if len(found) != 1:
            raise InvalidInputs("unknown build target")
        start = found[0]
    images, materials = reachable(stages, start)
    return ({reference: {"digest": entry["digest"], "stages": sorted(entry["stages"])}
             for reference, entry in images.items()},
            {kind: sorted(values) for kind, values in materials.items()}, frontend)


def build_platforms(values):
    if (not isinstance(values, (list, tuple)) or not values or len(set(values)) != len(values)
            or any(not isinstance(value, str) or PLATFORM.fullmatch(value) is None
                   for value in values)):
        raise InvalidInputs("invalid build platforms")
    return sorted(values)


def declared_identity(component, targets, source, recipe, recipe_digest, images, frontend):
    """Canonical declared inputs; the value the release planner would fingerprint."""
    return {"algorithm": ALGORITHM, "component": component, "platforms": targets,
            "source": {"algorithm": source["source_algorithm"], "digest": source["source_digest"]},
            "recipe": {"path": recipe, "target": RECIPE_TARGETS[recipe], "digest": recipe_digest,
                       "frontend": frontend},
            "images": [{"reference": reference, "digest": entry["digest"]}
                       for reference, entry in sorted(images.items())]}


def resolve(root, component, ref="HEAD", targets=("linux/amd64",)):
    """Return declared build-input evidence for one component; never authorization."""
    try:
        return _resolve(root, component, ref, targets)
    except InvalidInputs:
        raise
    except Exception:
        raise InvalidInputs("invalid or unsupported component build inputs") from None


def _resolve(root, component, ref, targets):
    if component not in ALL_COMPONENTS:
        raise InvalidInputs("unknown runtime component")
    platform_list = build_platforms(targets)
    snapshot = GitSnapshot(root, ref)
    source = collect(root, component, snapshot.revision)
    version = component_version(snapshot, component)
    recipe = "apps/frontend/Dockerfile" if component == "web" else "Dockerfile"
    recipe_bytes = snapshot.read_bytes(recipe)
    images, materials, frontend = parse_recipe(recipe_bytes.decode("utf-8"), RECIPE_TARGETS[recipe])
    if not images or frontend is None:
        raise InvalidInputs("no external image inputs found")
    recipe_digest = "sha256:" + hashlib.sha256(recipe_bytes).hexdigest()
    declared = declared_identity(component, platform_list, source, recipe, recipe_digest, images,
                                frontend)
    return {
        "component": component, "version": version, "platforms": platform_list,
        "source_revision": source["source_revision"],
        "source_algorithm": source["source_algorithm"], "source_digest": source["source_digest"],
        "recipe": declared["recipe"],
        "images": [{"reference": reference, "digest": entry["digest"], "stages": entry["stages"]}
                   for reference, entry in sorted(images.items())],
        "unresolved_materials": materials,
        "input_algorithm": ALGORITHM, "input_digest": digest(canonical(declared)),
        "identity_verified": False,
    }


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", choices=ALL_COMPONENTS)
    parser.add_argument("--ref", default="HEAD")
    parser.add_argument("--repo", type=Path, default=ROOT)
    parser.add_argument("--platform", action="append", dest="targets")
    args = parser.parse_args(argv)
    try:
        result = resolve(args.repo, args.component, args.ref, args.targets or ["linux/amd64"])
    except (InvalidInputs, OSError, UnicodeError, ValueError, TypeError, KeyError, IndexError):
        print("build-input collection rejected: invalid or unsupported component inputs",
              file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
