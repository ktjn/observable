#!/usr/bin/env python3
"""UX contract checks the metamodel does not perform; structural only, not browser verification.

The metamodel (system-spec --stage verification) already enforces EXP-01..05 and EXP-08:
references resolve, triggers are offered by the source view, failure modes map to view states,
and critical interactions appear in scenarios. This script adds:

  UXC-01  Required trigger floor per workflow (guards against deleting journey steps).
  UXC-02  Every interaction has a positive scenario.
  UXC-03  Every failure mode is exercised by a negative scenario of that interaction ending in its state.
  UXC-04  Negative scenarios exercise exactly one interaction, so each failure state has one cause.
  UXC-05  Scenario states belong to the source or target view of the scenario's interactions.
  UXC-06  Scenario requirements are realized by the experience.
  UXC-07  Every view action is the trigger of at least one interaction (no unmodeled actions).
  UXC-08  Every view-specific state is reached by at least one scenario.
  UXC-09  `preserves` uses the declared context vocabulary.
"""
from pathlib import Path
import sys

import yaml

ROOT = Path(__file__).resolve().parents[1]
UX = ROOT / "spec/business/experience/ux-workflows.yaml"
PRESERVABLE = {"service", "environment", "time-range", "filters", "selection", "purpose"}
EXPECTED = {
    "UX-01": {"select-service", "search-service", "open-incident", "explore-evidence", "back"},
    "UX-02": {"select-interval", "refine-filter", "compare-baseline", "open-evidence", "refresh", "back"},
    "UX-03": {"search-transaction", "open-transaction", "inspect-step", "back"},
    "UX-04": {"notification-selected", "open-incident", "acknowledge", "inspect-evidence", "record-hypothesis", "retry", "back"},
    "UX-05": {"open-dependencies", "select-dependency", "open-service", "back"},
    "UX-06": {"notification-selected", "open-alert", "acknowledge", "retry"},
    "UX-07": {"select-service", "refresh"},
    "UX-08": {"open-journey", "inspect-impact", "back"},
    "UX-09": {"acknowledge"},
    "UX-10": {"link-opened", "search-restricted", "request-elevation", "inspect-evidence", "elevation-expired", "confirm-elevated-action"},
}


def check() -> int:
    doc = yaml.safe_load(UX.read_text(encoding="utf-8"))
    experiences = {x["id"]: x for x in doc.get("experiences", [])}
    views = {v["id"]: v for v in doc.get("views", [])}
    errors: list[str] = []
    used_actions: set[tuple[str, str]] = set()
    reached: set[str] = set()

    for eid, triggers in EXPECTED.items():
        if eid not in experiences:
            errors.append(f"UXC-01 {eid}: missing")
            continue
        present = {i["trigger"] for i in experiences[eid].get("interactions", [])}
        if missing := triggers - present:
            errors.append(f"UXC-01 {eid}: missing required triggers {sorted(missing)}")

    for eid, e in experiences.items():
        interactions = {i["id"]: i for i in e.get("interactions", [])}
        scenarios = e.get("scenarios", [])
        realized = set(e.get("requirements", []))
        for i in interactions.values():
            if i.get("from_view"):
                used_actions.add((i["from_view"], i["trigger"]))
            if bad := set(i.get("preserves", [])) - PRESERVABLE:
                errors.append(f"UXC-09 {eid}/{i['id']}: unknown preserved context {sorted(bad)}")
        for iid, i in interactions.items():
            pos = [s for s in scenarios if s.get("kind", "positive") == "positive" and iid in s.get("interactions", [])]
            neg = [s for s in scenarios if s.get("kind") == "negative" and iid in s.get("interactions", [])]
            if not pos:
                errors.append(f"UXC-02 {eid}/{iid}: no positive scenario")
            ends = {v for v in (i.get("from_view"), i["to_view"]) if v}
            for mode, state in (i.get("failure_modes") or {}).items():
                targets = {f"{v}.{state}" for v in ends}
                if not any(targets & set(s.get("states", [])) for s in neg):
                    errors.append(f"UXC-03 {eid}/{iid}: failure mode '{mode}' ({state}) has no negative scenario reaching it")
        for s in scenarios:
            sid = f"{eid}/{s['id']}"
            inter = [interactions[x] for x in s.get("interactions", []) if x in interactions]
            if s.get("kind") == "negative" and len(s.get("interactions", [])) != 1:
                errors.append(f"UXC-04 {sid}: negative scenario must exercise exactly one interaction")
            allowed = {v for i in inter for v in (i.get("from_view"), i["to_view"]) if v}
            for st in s.get("states", []):
                reached.add(st)
                if st.partition(".")[0] not in allowed:
                    errors.append(f"UXC-05 {sid}: state '{st}' is not a view of its interactions")
            if bad := set(s.get("requirements", [])) - realized:
                errors.append(f"UXC-06 {sid}: requirements {sorted(bad)} not realized by the experience")

    for vid, v in views.items():
        for action in v.get("actions", []):
            if (vid, action) not in used_actions:
                errors.append(f"UXC-07 {vid}: action '{action}' is the trigger of no interaction")
        for state in v.get("states", {}) or {}:
            if f"{vid}.{state}" not in reached:
                errors.append(f"UXC-08 {vid}: state '{state}' is reached by no scenario")

    for err in errors:
        print(err, file=sys.stderr)
    print(f"UX contract check: {len(experiences)} workflows, {len(views)} views, {len(errors)} errors")
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(check())
