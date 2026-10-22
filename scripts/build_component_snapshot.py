"""Build a local snapshot image from immutable committed Git bytes.

Usage: python scripts/build_component_snapshot.py COMPONENT --output NEW_DIRECTORY
       [--ref HEAD] [--repo PATH] [--platform linux/amd64] [--build-arg NAME=VALUE]...

No push, arbitrary Docker arguments, release tags, or prebuilt binaries. Local
build evidence does not authenticate a complete immutable release identity.
"""

import argparse
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile

from ci_changed_components import ALL_COMPONENTS, RUST_COMPONENTS
import collect_component_inputs as inputs
from component_snapshot_metadata import component_version
from collect_component_source import (ROOT, RECIPE_TARGETS, GitSnapshot, canonical, collect, digest)
from plan_component_release import DIGEST, VERSION, matches, unique_object


class BuildFailed(ValueError):
    """No verified local build result is available."""


def write_json(path, value):
    path.write_bytes(canonical(value) + b"\n")


def prepare(root, component, ref, output, platform="linux/amd64", *, build_args=None):
    try:
        return _prepare(root, component, ref, Path(output), platform, build_args)
    except Exception:
        raise BuildFailed("snapshot preparation failed; use a fresh output directory and supported Git source") from None


def snapshot_image(component, revision, platform, input_digest):
    tag = digest(canonical({"source_revision": revision, "platform": platform,
                            "input_digest": input_digest}))[7:]
    return f"observable-{component}:snapshot-{tag}"


def _prepare(root, component, ref, output, platform, build_args):
    if platform not in ("linux/amd64", "linux/arm64"):
        raise BuildFailed("unsupported local build platform")
    if output.exists() or output.is_symlink():
        raise BuildFailed("output directory already exists")
    snapshot = GitSnapshot(root, ref)
    source = collect(root, component, snapshot.revision)
    version = component_version(snapshot, component)
    evidence = inputs.resolve(root, component, snapshot.revision, (platform,), build_args=build_args)
    if component == "web":
        required = {".dockerignore", "package.json", "package-lock.json", "apps/frontend/Dockerfile"}
        prefixes = ("apps/frontend/",)
        recipe = "apps/frontend/Dockerfile"
    else:
        required = {".dockerignore", "Dockerfile", "Cargo.toml", "Cargo.lock"}
        prefixes = ("libs/", "services/", "proto/")
        recipe = "Dockerfile"
    paths = sorted(required | {path for path in snapshot.entries if path.startswith(prefixes)})
    for path in paths:
        if (path.startswith("/") or "\\" in path or ":" in path
                or any(part in (".", "..", "") for part in path.split("/"))
                or str(PurePosixPath(path)) != path):
            raise BuildFailed("unsafe context path")
    snapshot.read_many(paths)  # Reject symlinks/gitlinks even in unrelated copied crates.
    context = io.BytesIO()
    with tarfile.open(fileobj=context, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for path in paths:
            data = snapshot.read_bytes(path)
            info = tarfile.TarInfo(path)
            info.size = len(data)
            info.mode = 0o755 if snapshot.entries[path][0] == "100755" else 0o644
            info.mtime = info.uid = info.gid = 0
            info.uname = info.gname = ""
            archive.addfile(info, io.BytesIO(data))
    context_bytes = context.getvalue()
    request = {"component": component, "version": version, "source_revision": snapshot.revision,
               "source_algorithm": source["source_algorithm"], "source_digest": source["source_digest"],
               "context_digest": digest(context_bytes), "recipe": recipe, "platform": platform,
               "input_algorithm": evidence["input_algorithm"], "input_digest": evidence["input_digest"],
               "build_args": evidence["build_args"],
               "image": snapshot_image(component, snapshot.revision, platform, evidence["input_digest"]),
               "identity_verified": False}
    output.mkdir()
    (output / "context.tar").write_bytes(context_bytes)
    write_json(output / "source.json", source)
    write_json(output / "inputs.json", evidence)
    write_json(output / "context.json", {"digest": request["context_digest"], "files": [
        {"path": path, "mode": snapshot.entries[path][0], "digest": digest(snapshot.read_bytes(path))}
        for path in paths]})
    write_json(output / "request.json", request)
    return request


def labels(request):
    return {"org.opencontainers.image.revision": request["source_revision"],
            "org.opencontainers.image.version": request["version"],
            "io.observable.source.algorithm": request["source_algorithm"],
            "io.observable.source.digest": request["source_digest"],
            "io.observable.context.digest": request["context_digest"],
            "io.observable.input.algorithm": request["input_algorithm"],
            "io.observable.input.digest": request["input_digest"]}


def verify_runtime(request, image_id, run):
    if request["component"] == "web":
        return []
    package = RUST_COMPONENTS[request["component"]]
    checked = run(["docker", "run", "--rm", "--pull", "never", "--network", "none", "--platform", request["platform"],
                   "--entrypoint", "/usr/bin/ldd", image_id, f"/usr/local/bin/{package}"], capture_output=True)
    # glibc ldd can exit zero even when a needed shared library is missing.
    if (checked.returncode != 0 or b"not found" in checked.stdout.lower()
            or b"not found" in checked.stderr.lower()):
        raise BuildFailed("Rust snapshot has unresolved runtime linkage")
    return ["dynamic-linkage"]


def execute(request, output, *, run=subprocess.run):
    try:
        return _execute(request, Path(output), run)
    except Exception:
        raise BuildFailed("local snapshot build or image verification failed; no success record written") from None


def _execute(request, output, run):
    for name in ("result.json", "metadata.json"):
        if (output / name).exists() or (output / name).is_symlink():
            raise BuildFailed("build output already exists; use a fresh attempt")
    persisted = json.loads((output / "request.json").read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    if persisted != request or request.get("identity_verified") is not False:
        raise BuildFailed("prepared build request changed")
    # The parsed JSON is an independent snapshot; never retain the caller's
    # mutable dictionary while a potentially long Docker subprocess runs.
    request = persisted
    component, revision, platform = request["component"], request["source_revision"], request["platform"]
    expected_image = snapshot_image(component, revision, platform, request["input_digest"])
    if (component not in ALL_COMPONENTS or platform not in ("linux/amd64", "linux/arm64")
            or not matches(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", revision)
            or request["recipe"] != ("apps/frontend/Dockerfile" if component == "web" else "Dockerfile")
            or request["image"] != expected_image or not matches(VERSION, request["version"])
            or request["input_algorithm"] != inputs.ALGORITHM or not matches(DIGEST, request["input_digest"])
            or not matches(DIGEST, request["source_digest"]) or not matches(DIGEST, request["context_digest"])):
        raise BuildFailed("unsupported build request")
    # Feed the exact bytes checked here through stdin, eliminating a later file reread.
    context = (output / "context.tar").read_bytes()
    if digest(context) != request["context_digest"]:
        raise BuildFailed("build context changed")
    # Recompute the declared inputs from exactly the recipe bytes sent to Docker.
    # No extraction or checkout reads; duplicate/symlink recipe entries are rejected.
    with tarfile.open(fileobj=io.BytesIO(context)) as archive:
        recipes = [member for member in archive.getmembers() if member.name == request["recipe"]]
        if len(recipes) != 1 or not recipes[0].isfile():
            raise BuildFailed("invalid context recipe")
        recipe_bytes = archive.extractfile(recipes[0]).read()
    overrides = request["build_args"] if component == "web" else None
    recomputed = inputs.recipe_evidence(component, [platform], request, request["recipe"], recipe_bytes, overrides)
    expected_evidence = {"component": component, "version": request["version"], "platforms": [platform],
                         "source_revision": revision, "source_algorithm": request["source_algorithm"],
                         "source_digest": request["source_digest"], **recomputed, "identity_verified": False}
    saved_evidence = json.loads((output / "inputs.json").read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    if (saved_evidence != expected_evidence or request["build_args"] != recomputed["build_args"]
            or request["input_digest"] != recomputed["input_digest"]):
        raise BuildFailed("declared build inputs changed")
    command = ["docker", "buildx", "build", "--load", "--platform", request["platform"],
               "--metadata-file", str(output / "metadata.json"), "--tag", request["image"],
               "--file", request["recipe"]]
    target = RECIPE_TARGETS[request["recipe"]]
    if target is not None:
        command += ["--target", target]
    for key, value in sorted(request["build_args"].items()):
        command += ["--build-arg", f"{key}={value}"]
    for key, value in labels(request).items():
        command += ["--label", f"{key}={value}"]
    command.append("-")
    build_result = run(command, input=context, capture_output=True)
    if build_result.returncode != 0:
        raise BuildFailed("Docker build failed")
    metadata = json.loads((output / "metadata.json").read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    config_digest, image_digest = metadata.get("containerimage.config.digest"), metadata.get("containerimage.digest")
    if not matches(DIGEST, image_digest) or (config_digest is not None and not matches(DIGEST, config_digest)):
        raise BuildFailed("missing Docker digest metadata")
    inspected = run(["docker", "image", "inspect", request["image"]], capture_output=True)
    if inspected.returncode != 0:
        raise BuildFailed("local image inspection failed")
    images = json.loads(inspected.stdout, object_pairs_hook=unique_object)
    if not isinstance(images, list) or len(images) != 1:
        raise BuildFailed("ambiguous local image")
    image = images[0]
    image_labels = image["Config"]["Labels"]
    if "Descriptor" in image:
        if image["Descriptor"]["digest"] != image_digest or image["Id"] != image_digest:
            raise BuildFailed("local image root descriptor differs from build metadata")
        binding = "descriptor"
    else:
        if not matches(DIGEST, config_digest) or image["Id"] != config_digest:
            raise BuildFailed("local image configuration differs from build metadata")
        binding = "configuration"
    if (f"{image['Os']}/{image['Architecture']}" != request["platform"]
            or any(image_labels.get(key) != value for key, value in labels(request).items())):
        raise BuildFailed("local image does not match the build evidence")
    runtime_checks = verify_runtime(request, image["Id"], run)
    result = request | {"image_id": image["Id"], "image_digest": image_digest,
                        "image_identity_binding": binding, "identity_verified": False,
                        "runtime_checks": runtime_checks,
                        "builder_digest": digest(Path(__file__).read_bytes())}
    write_json(output / "result.json", result)
    return result


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", choices=ALL_COMPONENTS)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--ref", default="HEAD")
    parser.add_argument("--repo", type=Path, default=ROOT)
    parser.add_argument("--platform", default="linux/amd64", choices=("linux/amd64", "linux/arm64"))
    parser.add_argument("--build-arg", action="append", default=[], dest="build_args")
    args = parser.parse_args(argv)
    try:
        request = prepare(args.repo, args.component, args.ref, args.output, args.platform,
                          build_args=inputs.parse_build_args(args.build_args))
        result = execute(request, args.output.resolve())
    except (BuildFailed, inputs.InvalidInputs):
        print("snapshot preparation, build or verification failed", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
