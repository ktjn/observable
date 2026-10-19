"""Plan immutable component releases without building or publishing anything.

Usage: python scripts/plan_component_release.py INPUT.json

The JSON file contains request, observations (one found observation, or two
observations when the first is missing), and optional history. Observations are
assertions from a trusted verifier, not proof verified by this tool. See
distribution/README.md for the schema and remaining publication gates.
"""

import json
from pathlib import Path
import re
import sys

from ci_changed_components import ALL_COMPONENTS


class Rejected(ValueError):
    """Unsafe or insufficient release evidence; never emit a publish plan."""


IDENTITY_FIELDS = {"component", "version", "source_revision", "input_algorithm",
                   "input_digest", "platforms"}
RECORD_FIELDS = {"identity", "digest", "identity_verified", "sbom_subject", "provenance_subject"}
DIGEST = r"sha256:[0-9a-f]{64}"
NUMBER = r"(?:0|[1-9][0-9]*)"
PRERELEASE = rf"(?:{NUMBER}|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)"
VERSION = rf"{NUMBER}\.{NUMBER}\.{NUMBER}(?:-{PRERELEASE}(?:\.{PRERELEASE})*)?"


def fields(value, required, optional=frozenset()):
    if not isinstance(value, dict) or not required <= value.keys() <= required | optional:
        raise Rejected("invalid record fields")


def matches(pattern, value):
    return isinstance(value, str) and re.fullmatch(pattern, value) is not None


def identity(value):
    fields(value, IDENTITY_FIELDS)
    if value["component"] not in ALL_COMPONENTS:
        raise Rejected("unknown runtime component")
    # OCI tags cannot contain SemVer build metadata (+), and are at most 128 chars.
    if not matches(VERSION, value["version"]) or len(value["version"]) > 128:
        raise Rejected("version is not an OCI-compatible SemVer tag")
    if not matches(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", value["source_revision"]):
        raise Rejected("source revision must be a full Git object id")
    if not matches(r"[a-z][a-z0-9-]*-v[1-9][0-9]*", value["input_algorithm"]):
        raise Rejected("input identity algorithm must be versioned")
    if not matches(DIGEST, value["input_digest"]):
        raise Rejected("invalid build input digest")
    platforms = value["platforms"]
    if (not isinstance(platforms, list) or not platforms
            or any(not matches(r"linux/[a-z0-9_]+(?:/v[0-9]+)?", p) for p in platforms)
            or len(set(platforms)) != len(platforms)):
        raise Rejected("invalid target platforms")
    return value | {"platforms": sorted(platforms)}


def record(value):
    fields(value, RECORD_FIELDS)
    release_identity = identity(value["identity"])
    if value["identity_verified"] is not True:
        raise Rejected("release identity is unverified")
    if not matches(DIGEST, value["digest"]):
        raise Rejected("invalid image digest")
    for field in ("sbom_subject", "provenance_subject"):
        if value[field] is not None and value[field] != value["digest"]:
            raise Rejected("release evidence subject differs from image digest")
    return value | {"identity": release_identity}


def same_inputs(desired, published):
    return all(desired[key] == published[key] for key in IDENTITY_FIELDS - {"source_revision"})


def observation(inspect, desired):
    try:
        value = inspect()
    except Exception:
        # Upstream diagnostics may contain credentials. Do not echo them.
        raise Rejected("registry inspection failed") from None
    if not isinstance(value, dict):
        raise Rejected("invalid registry observation")
    if value.get("status") == "missing":
        fields(value, {"status", "component", "version"})
        if any(value[key] != desired[key] for key in ("component", "version")):
            raise Rejected("registry observation targets a different component/version")
        return None
    if value.get("status") == "found":
        fields(value, {"status", "record"})
        return record(value["record"])
    raise Rejected("registry observation unavailable")


def plan_release(request, inspect, history=None):
    """Return a non-executable intent from trusted observations.

    inspect() must observe the requested component/version. It is called again
    after an initial definitive miss. The caller must supply verified identities
    and authoritative historical records. This function supplies neither
    cryptographic verification, locking, nor permission to overwrite a tag.
    """
    desired = identity(request)
    historical = record(history) if history is not None else None
    if historical is not None and not same_inputs(desired, historical["identity"]):
        raise Rejected("historical identity differs; investigate or bump component version")

    published = observation(inspect, desired)
    if published is None:
        if historical is not None:
            raise Rejected("historical image is missing; recovery required")
        published = observation(inspect, desired)

    if published is not None:
        if not same_inputs(desired, published["identity"]):
            raise Rejected("published identity differs; investigate or bump component version")
        if historical is not None and (
                historical["digest"] != published["digest"]
                or historical["identity"] != published["identity"]):
            raise Rejected("historical release identity or digest was replaced")
        missing = [name for name in ("sbom", "provenance")
                   if published[f"{name}_subject"] is None]
        action = "complete-evidence" if missing else "reuse"
        selected, digest = published["identity"], published["digest"]
    else:
        action, selected, digest, missing = "publish", desired, None, ["sbom", "provenance"]

    return selected | {
        "action": action,
        "image": f"ghcr.io/ktjn/observable-{desired['component']}:{desired['version']}",
        "digest": digest,
        "missing_evidence": missing,
        "publication_authorized": False,
    }


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Rejected("duplicate JSON key")
        value[key] = item
    return value


def main(argv):
    if len(argv) != 1:
        print("usage: plan_component_release.py INPUT.json", file=sys.stderr)
        return 1
    try:
        payload = json.loads(Path(argv[0]).read_text(encoding="utf-8"),
                             object_pairs_hook=unique_object)
        fields(payload, {"request", "observations"}, {"history"})
        supplied = payload["observations"]
        if not isinstance(supplied, list) or not 1 <= len(supplied) <= 2:
            raise Rejected("supply one or two registry observations")
        observations = iter(supplied)
        result = plan_release(payload["request"], lambda: next(observations), payload.get("history"))
        sentinel = object()
        if next(observations, sentinel) is not sentinel:
            raise Rejected("unused registry observations")
    except (Rejected, OSError, UnicodeError, ValueError, TypeError, RecursionError):
        print("release plan rejected: invalid input or insufficient release evidence", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
