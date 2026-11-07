#!/usr/bin/env python3
"""Validate mandatory UX transitions and scenario references; structural only."""
from pathlib import Path
import sys
import yaml

ROOT = Path(__file__).resolve().parents[1]
UX = ROOT / "spec/business/experience/ux-workflows.yaml"
EXPECTED = {
    "UX-01": {"select-service"},
    "UX-02": {"select-interval", "refine-filter", "compare-baseline", "open-evidence"},
    "UX-03": {"search-transaction", "open-transaction", "inspect-step"},
    "UX-04": {"notification-selected", "open-incident", "acknowledge", "inspect-evidence", "record-hypothesis", "retry"},
    "UX-05": {"open-dependencies"},
    "UX-06": {"notification-selected", "open-alert", "acknowledge", "retry"},
    "UX-07": {"select-service"},
    "UX-08": {"open-journey"},
    "UX-09": {"acknowledge"},
    "UX-10": {"link-opened", "search-restricted", "request-elevation", "inspect-evidence"},
}

def check():
    doc = yaml.safe_load(UX.read_text(encoding="utf-8"))
    experiences = {x["id"]: x for x in doc.get("experiences", [])}
    views = {v["id"]: v for v in doc.get("views", [])}
    errors = []
    for eid, triggers in EXPECTED.items():
        e = experiences.get(eid)
        if not e:
            errors.append(f"{eid}: missing")
            continue
        interactions = {i["id"]: i for i in e.get("interactions", [])}
        present = {i["trigger"] for i in interactions.values()}
        missing = triggers - present
        if missing:
            errors.append(f"{eid}: missing required triggers {sorted(missing)}")
        scenarios = e.get("scenarios", [])
        for iid, interaction in interactions.items():
            positive = [s for s in scenarios if s.get("kind", "positive") == "positive" and iid in s.get("interactions", [])]
            negative = [s for s in scenarios if s.get("kind") == "negative" and iid in s.get("interactions", [])]
            if not positive:
                errors.append(f"{eid}/{iid}: no positive scenario")
            if interaction.get("failure_modes") and not negative:
                errors.append(f"{eid}/{iid}: no negative scenario")
            if not interaction.get("outcome"):
                errors.append(f"{eid}/{iid}: missing observable outcome")
            source = interaction.get("from_view")
            if source and interaction["trigger"] not in views.get(source, {}).get("actions", []):
                errors.append(f"{eid}/{iid}: trigger not offered by source view")
            for scenario in positive + negative:
                if not set(scenario.get("requirements", [])).issubset(set(e.get("requirements", []))):
                    errors.append(f"{eid}/{scenario['id']}: scenario requirement not realized by experience")
    for err in errors:
        print(err, file=sys.stderr)
    print(f"UX contract check: {len(EXPECTED)} workflows, {len(errors)} errors")
    return int(bool(errors))

if __name__ == "__main__":
    sys.exit(check())
