"""Verify a component's declared build-input identity from committed bytes and material locks.

Usage: python scripts/verify_component_inputs.py COMPONENT [--ref HEAD]
       [--repo PATH] [--platform linux/amd64]... [--build-arg NAME=VALUE]...
       [--declared-input FILE]

Verifies:
- Component version and package manifest consistency
- Committed source closure integrity (component-source-v1)
- Pinned recipe integrity and Dockerfile frontend
- Pinned base images against Docker Hub OCI manifests and platform descriptors
- Locked external materials (Cargo and APT for Rust; npm for Web)
- Input fingerprint rederivation (component-input-v2)
- Consistency with declared inputs if supplied
"""

import argparse
import json
from pathlib import Path
import sys

from ci_changed_components import ALL_COMPONENTS
import collect_component_inputs as inputs
from collect_component_source import ROOT, canonical, digest
import component_apt_materials as apt
import component_cargo_materials as cargo
import component_npm_materials as npm
import plan_component_release as release
import verify_component_base_images as base_images


class VerificationFailed(ValueError):
    """Component build-input verification failed."""


def verify_materials(root, component, revision, platforms, declared):
    """Verify complete, frozen material closures for the component."""
    materials = {}
    unresolved = declared.get("unresolved_materials", {})
    if component == "web":
        try:
            _, _, npm_plan = npm.source_plan(root, revision)
        except Exception as e:
            raise VerificationFailed(f"npm material plan verification failed: {e}") from None

        if unresolved.get("apt") or unresolved.get("rustup") or unresolved.get("downloads"):
            raise VerificationFailed("web component has unreviewed unresolved materials")

        materials["npm"] = {
            "algorithm": npm.ALGORITHM,
            "material_digest": digest(canonical(npm_plan)),
            "package_count": len(npm_plan.get("packages", [])),
            "verified": True,
        }
    else:
        # Rust components require Cargo and APT materials
        try:
            _, _, cargo_plan = cargo.source_plan(root, revision)
        except Exception as e:
            raise VerificationFailed(f"Cargo material plan verification failed: {e}") from None

        if unresolved.get("downloads"):
            raise VerificationFailed("Rust component has unreviewed download materials")
        # rustup must only contain standard reviewed components
        if set(unresolved.get("rustup", [])) - {"clippy", "rustfmt"}:
            raise VerificationFailed("Rust component has unreviewed rustup components")

        materials["cargo"] = {
            "algorithm": cargo.ALGORITHM,
            "material_digest": digest(canonical(cargo_plan)),
            "package_count": len(cargo_plan.get("packages", [])),
            "verified": True,
        }

        apt_sets = []
        for platform in sorted(platforms):
            for kind in ("builder", "runtime"):
                try:
                    _, apt_record = apt.locked(root, revision, kind, platform)
                    apt_sets.append({
                        "kind": kind,
                        "platform": platform,
                        "material_digest": digest(canonical(apt_record)),
                        "package_count": len(apt_record.get("packages", [])),
                    })
                except Exception as e:
                    raise VerificationFailed(f"APT lock verification failed for {kind}/{platform}: {e}") from None

        materials["apt"] = {
            "algorithm": apt.LOCK_ALGORITHM,
            "sets": apt_sets,
            "verified": True,
        }

    return materials


def verify(root, component, ref="HEAD", targets=("linux/amd64",), *,
           build_args=None, declared_input=None, fetch_image=None):
    """Verify all declared build inputs for one component."""
    if component not in ALL_COMPONENTS:
        raise VerificationFailed(f"unknown component: {component}")

    root = Path(root)
    # 1. Resolve declared inputs from committed Git bytes
    try:
        declared = inputs.resolve(root, component, ref, targets, build_args=build_args)
    except Exception as e:
        raise VerificationFailed(f"failed to resolve declared build inputs: {e}") from None

    revision = declared["source_revision"]
    platforms = declared["platforms"]

    # 2. Base image registry and platform descriptor verification
    try:
        fetch_kw = {"fetch": fetch_image} if fetch_image is not None else {}
        base_images.verify(root, component, revision, platforms, build_args=build_args, **fetch_kw)
    except Exception as e:
        raise VerificationFailed(f"pinned base image verification failed: {e}") from None

    # 3. Materials verification (Cargo, APT, npm)
    materials = verify_materials(root, component, revision, platforms, declared)

    # 4. Consistency check against external declared input if supplied
    if declared_input is not None:
        check_fields = (
            "component", "version", "source_revision", "source_digest",
            "input_algorithm", "input_digest", "platforms", "build_args"
        )
        for key in check_fields:
            if declared_input.get(key) != declared.get(key):
                raise VerificationFailed(f"declared input mismatch for field '{key}'")

    # 5. Form the authenticated input record
    result = dict(declared)
    result["materials"] = materials
    result["registry_images_verified"] = True
    result["materials_verified"] = True
    result["identity_verified"] = True
    result["publication_authorized"] = False
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", choices=ALL_COMPONENTS)
    parser.add_argument("--ref", default="HEAD")
    parser.add_argument("--repo", default=ROOT, type=Path)
    parser.add_argument("--platform", action="append", dest="platforms")
    parser.add_argument("--build-arg", action="append", dest="build_args")
    parser.add_argument("--declared-input", default=None, type=Path)
    args = parser.parse_args(argv)

    targets = args.platforms or ["linux/amd64"]
    build_args = None
    if args.build_args:
        build_args = {}
        for item in args.build_args:
            if "=" not in item:
                print("build argument must be NAME=VALUE", file=sys.stderr)
                return 1
            k, v = item.split("=", 1)
            build_args[k] = v

    declared_data = None
    if args.declared_input:
        try:
            declared_data = json.loads(args.declared_input.read_text(encoding="utf-8"),
                                       object_pairs_hook=release.unique_object)
        except Exception:
            print("failed to read declared input JSON", file=sys.stderr)
            return 1

    try:
        verified = verify(
            args.repo, args.component, args.ref, targets,
            build_args=build_args, declared_input=declared_data
        )
    except VerificationFailed as e:
        print(f"verification failed: {e}", file=sys.stderr)
        return 1
    except Exception:
        print("verification failed; unexpected error", file=sys.stderr)
        return 1

    print(json.dumps(verified, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
