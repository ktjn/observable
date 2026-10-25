"""Authoritative release history loading, registry adapter, and publication boundary enforcement.

Bridges inspect_component_registry and verify_component_attestations into trusted
observations for plan_component_release, preventing publication races and tag overwrite.
"""

import json
from pathlib import Path
import sys

from ci_changed_components import ALL_COMPONENTS
import inspect_component_registry as registry
import plan_component_release as release


HISTORY_ALGORITHM = "release-history-v1"


class AdapterRejected(release.Rejected):
    """Publication boundary or release adapter failure."""


def load_history(source):
    """Load and validate authoritative release history from file path or dict.

    Returns a dict mapping (component, version) -> validated record dict.
    Returns empty dict if source is None or file does not exist.
    """
    if source is None:
        return {}
    if isinstance(source, dict):
        payload = source
    elif isinstance(source, (str, Path)):
        path = Path(source)
        if not path.exists():
            return {}
        try:
            payload = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=release.unique_object)
        except Exception as e:
            raise AdapterRejected(f"failed to parse history JSON: {e}") from None
    else:
        raise AdapterRejected("unsupported history source type")

    if not isinstance(payload, dict):
        raise AdapterRejected("history root must be a JSON object")

    try:
        release.fields(payload, {"algorithm", "records"})
    except release.Rejected as e:
        raise AdapterRejected(f"invalid history fields: {e}") from None
    if payload.get("algorithm") != HISTORY_ALGORITHM:
        raise AdapterRejected(f"unsupported history algorithm: {payload.get('algorithm')}")

    records_list = payload.get("records")
    if not isinstance(records_list, list):
        raise AdapterRejected("history records must be a list")

    history = {}
    for item in records_list:
        try:
            rec = release.record(item)
        except Exception as e:
            raise AdapterRejected(f"invalid record in history: {e}") from None

        if rec["identity_verified"] is not True:
            raise AdapterRejected("history entry has unverified identity")

        key = (rec["identity"]["component"], rec["identity"]["version"])
        if key in history:
            raise AdapterRejected(f"duplicate history entry for {key}")
        history[key] = rec

    return history


def lookup_history(history, component, version):
    """Look up a historical record by (component, version)."""
    return history.get((component, version))


def make_observation_adapter(component, version, *, inspect_fn=None, history=None, attest_fn=None, request=None):
    """Create a 0-argument callable that returns trusted planner observations.

    Translates raw registry inspection and verified history/attestations into
    the shape expected by plan_component_release.plan_release.
    """
    if inspect_fn is None:
        inspect_fn = registry.inspect
    hist_map = history if isinstance(history, dict) else (load_history(history) if history else {})

    def observe():
        try:
            result = inspect_fn(component, version)
        except Exception:
            raise AdapterRejected("registry inspection failed") from None

        if not isinstance(result, dict):
            raise AdapterRejected("invalid registry inspection result")

        status = result.get("status")
        if status == "missing":
            release.fields(result, {"status", "component", "version"})
            return {"status": "missing", "component": component, "version": version}

        if status == "found":
            digest = result.get("digest")
            if not release.matches(release.DIGEST, digest):
                raise AdapterRejected("invalid observed digest")

            hist_record = lookup_history(hist_map, component, version)
            if hist_record is not None:
                if hist_record["digest"] != digest:
                    raise AdapterRejected("observed registry digest does not match historical digest")
                return {"status": "found", "record": hist_record}

            if attest_fn is not None and request is not None:
                try:
                    attest_res = attest_fn(component, digest, request["source_revision"])
                    if attest_res.get("attestations_verified") is True:
                        constructed = {
                            "identity": release.identity(request),
                            "digest": digest,
                            "identity_verified": True,
                            "sbom_subject": attest_res.get("sbom_subject"),
                            "provenance_subject": attest_res.get("provenance_subject"),
                        }
                        return {"status": "found", "record": release.record(constructed)}
                except Exception:
                    pass

            raise AdapterRejected("observed found image has unverified release identity")

        raise AdapterRejected(f"unexpected observation status: {status}")

    return observe


def plan_component_release_with_adapter(request, history=None, *, inspect_fn=None, attest_fn=None,
                                        allow_initial_miss_with_history=False):
    """Plan an immutable component release using the registry and history adapter."""
    desired = release.identity(request)
    hist_map = history if isinstance(history, dict) else (load_history(history) if history else {})
    hist_record = lookup_history(hist_map, desired["component"], desired["version"])

    obs_fn = make_observation_adapter(
        desired["component"],
        desired["version"],
        inspect_fn=inspect_fn,
        history=hist_map,
        attest_fn=attest_fn,
        request=desired,
    )

    passed_history = None if allow_initial_miss_with_history else hist_record
    return release.plan_release(desired, obs_fn, history=passed_history)


def enforce_publication_boundary(plan, *, registry_write_enabled=False, lock_token=None):
    """Enforce writer serialization, tag overwrite prevention, and publication authorization."""
    if not isinstance(plan, dict) or "action" not in plan:
        raise AdapterRejected("invalid plan object")

    action = plan["action"]
    if action == "publish":
        if not registry_write_enabled:
            raise AdapterRejected("registry write is not enabled; publication prohibited")
        if not lock_token or not isinstance(lock_token, str):
            raise AdapterRejected("publication lock token required")
        return plan | {
            "publication_authorized": True,
            "lock_token": lock_token,
        }

    if action in ("reuse", "complete-evidence"):
        # Image is already published and immutable; never rebuild or push image
        return plan | {
            "publication_authorized": False,
        }

    raise AdapterRejected(f"unrecognized release action: {action}")


def main(argv=None):
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("request", help="Path to release request JSON file")
    parser.add_argument("--history", default=None, help="Path to release history JSON file")
    parser.add_argument("--enforce", action="store_true", help="Enforce publication boundary")
    parser.add_argument("--write", action="store_true", help="Enable registry write authorization")
    parser.add_argument("--lock-token", default=None, help="Writer concurrency lock token")

    args = parser.parse_args(argv)

    try:
        request_data = json.loads(Path(args.request).read_text(encoding="utf-8"),
                                  object_pairs_hook=release.unique_object)
        history_path = args.history
        if history_path is None:
            default_history = Path(__file__).resolve().parents[1] / "distribution" / "release-history.json"
            if default_history.exists():
                history_path = default_history

        plan = plan_component_release_with_adapter(request_data, history=history_path)
        if args.enforce:
            plan = enforce_publication_boundary(
                plan, registry_write_enabled=args.write, lock_token=args.lock_token
            )
    except (AdapterRejected, release.Rejected, OSError, UnicodeError, ValueError, TypeError, RecursionError) as e:
        print("release adapter rejected: invalid input or insufficient release evidence", file=sys.stderr)
        return 1

    print(json.dumps(plan, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
