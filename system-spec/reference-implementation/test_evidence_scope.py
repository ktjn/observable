import hashlib, json, shutil
from pathlib import Path

import yaml

from metamodel import (Criterion, Evidence, HandoffReturn, Refinement, SpecImport, check_imports, check_return, deferred_scope,
                       evidence_report, load_spec, main, validate_spec, verified_requirements)

EXAMPLE = Path(__file__).parent / "example.yaml"
def spec(): return load_spec(EXAMPLE)
def ids(errs): return {e[1:e.index("]")] for e in errs}
def req(s, rid): return next(r for r in s.requirements if r.id == rid)
def ev(rid, eid, **kw):
    return Evidence(**{"id": eid, "requirement_id": rid, "requirement_version": "1.0", "method": "test", "passed": True, "provenance": f"pytest::{eid}",
                       "subject_revision": "1.0", "timestamp": "2026-11-07", "verifier": "ci", **kw})

# --- criterion IDs (#116)

def with_ids(s):
    r = req(s, "req-order")
    r.criteria = [Criterion(id="created", text="order created when stock available"), Criterion(id="rejected", text="order rejected when stock is short")]
    s.evidence = [e for e in s.evidence if e.requirement_id != "req-order"]
    return s, r

def test_criteria_bind_by_id_or_text():
    s, r = with_ids(spec())
    s.evidence += [ev("req-order", "ev-a", criteria=["created"]), ev("req-order", "ev-b", criteria=["order rejected when stock is short"])]
    assert validate_spec(s) == [] and ("req-order", "1.0") in verified_requirements(s)

def test_uncovered_criterion_reported_by_id():
    s, r = with_ids(spec())
    s.evidence += [ev("req-order", "ev-a", criteria=["created"])]
    errs = validate_spec(s)
    assert ids(errs) == {"REQ-05"} and any("['rejected']" in e for e in errs)

def test_unknown_and_duplicate_criterion_ids():
    s, r = with_ids(spec())
    s.evidence += [ev("req-order", "ev-a", criteria=["created", "ghost"])]
    assert "EV-06" in ids(validate_spec(s, "inventory"))
    r.criteria.append(Criterion(id="created", text="again"))
    assert "REQ-10" in ids(validate_spec(s, "inventory"))

def test_handoff_return_accepts_criterion_ids():
    s, r = with_ids(spec())
    comp = next(c for c in s.components if r.target in c.provides)
    other = [q for q in s.requirements if q.target in {comp.id, *comp.provides, *comp.owns_data} and q.id != r.id]
    ret = HandoffReturn(component=comp.id, results=[
        {"requirement": r.id, "requirement_version": "1.0", "method": "test", "passed": True, "provenance": "pytest::a", "criteria": ["created"]},
        {"requirement": r.id, "requirement_version": "1.0", "method": "test", "passed": True, "provenance": "pytest::b", "criteria": ["order rejected when stock is short"]},
    ], unmet=[{"requirement": q.id, "reason": "not in this handoff"} for q in other])
    assert not [e for e in check_return(s, ret) if "req-order" in e and "criteria" in e]

# --- holder and evidence methods (#115)

def test_holder_restricts_methods():
    s = spec(); r = req(s, "req-order"); r.holder = "organization"
    s.system.evidence_methods = {"organization": ["inspection"]}
    assert ids(validate_spec(s, "inventory")) == {"EV-07"}
    s.system.evidence_methods = {"organization": ["inspection", "test"]}
    assert validate_spec(s, "inventory") == []
    r.holder = "platform"
    s.system.evidence_methods = {"organization": ["inspection"]}
    assert validate_spec(s, "inventory") == []

# --- evidence policy and report (#113)

def test_require_criteria_rejects_blanket_records():
    s = spec()
    assert ("req-order", "1.0") in verified_requirements(s)
    s.system.evidence_policy.require_criteria = True
    assert ("req-order", "1.0") not in verified_requirements(s)
    assert "EV-08" in ids(validate_spec(s, "verification")) and "EV-08" not in ids(validate_spec(s, "architecture"))

def test_evidence_report():
    s = spec(); s.evidence.append(ev("req-order", "ev-skip", criteria=["order created when stock available"], skipped=["db down"]))
    row = next(x for x in evidence_report(s) if x["requirement"] == "req-order")
    assert row["verified"] is False and row["blanket"] == ["ev-order-test"] and row["skipped"] == ["ev-skip"] and row["methods"] == ["test"]
    assert all(x["requirement"] != "ghost" for x in evidence_report(s))

def test_evidence_report_cli(capsys):
    assert main([str(EXAMPLE), "--evidence-report", "--json"]) == 0
    rows = json.loads(capsys.readouterr().out)
    assert {"requirement", "verified", "failing", "uncovered", "blanket", "skipped", "holder", "priority"} <= set(rows[0])

# --- release scope (#114)

def deferred_spec():
    s = spec(); r = req(s, "req-reserve"); r.priority = "P1"
    s.evidence = [e for e in s.evidence if e.requirement_id != "req-reserve"]
    return s

def test_release_scope_defers_out_of_scope_requirements():
    s = deferred_spec()
    assert {"REQ-05", "RISK-04"} <= ids(validate_spec(s))
    assert validate_spec(s, release_scope=["P0"]) == []
    d = deferred_scope(s, ["P0"])
    assert d["requirements"] == ["req-reserve"] and d["risks"] == ["risk-oversell"] and req(s, "req-reserve").target in d["entities"]
    assert any(c.id in d["entities"] for c in s.components if req(s, "req-reserve").target in c.provides)
    s.system.release_scope = ["P0"]
    assert validate_spec(s) == []

def test_release_scope_keeps_in_scope_dependencies():
    s = deferred_spec(); req(s, "req-order").depends_on = ["req-reserve"]
    assert "REQ-06" in ids(validate_spec(s, release_scope=["P0"]))

def test_release_scope_cli(capsys, tmp_path):
    s = deferred_spec(); path = tmp_path / "spec.yaml"
    path.write_text(yaml.safe_dump(s.model_dump(mode="json", exclude_defaults=True)), encoding="utf-8")
    assert main([str(path)]) == 1
    capsys.readouterr()
    assert main([str(path), "--release-scope", "P0"]) == 0
    assert "deferred by release scope" in capsys.readouterr().out
    assert main([str(path), "--release-scope", "P0", "--json"]) == 0
    assert json.loads(capsys.readouterr().out)["deferred"]["requirements"] == ["req-reserve"]

# --- baseline import (#117)

def baseline(tmp_path, change=None, **imp):
    (tmp_path / "base").mkdir()
    shutil.copy(EXAMPLE, tmp_path / "base/spec.yaml")
    digest = hashlib.sha256((tmp_path / "base/spec.yaml").read_bytes()).hexdigest()
    s = spec()
    s.system.imports = [SpecImport(id="up", path="base/spec.yaml", version="1.0", mode="baseline", artifacts={"spec.yaml": digest}, **imp)]
    if change: change(s)
    return s

def errs(s, tmp_path): return check_imports(s, tmp_path, tmp_path)

def test_baseline_unchanged_passes(tmp_path):
    assert errs(baseline(tmp_path), tmp_path) == []

def test_baseline_reports_undeclared_changes_and_removals(tmp_path):
    def change(s):
        req(s, "req-order").criteria = ["something else"]
        s.risks = [k for k in s.risks if k.id != "risk-erp-delay"]
    e = errs(baseline(tmp_path, change), tmp_path)
    assert any("requirements/req-order/criteria" in x for x in e) and any("risks/risk-erp-delay" in x for x in e)

def test_baseline_refinements_and_policies(tmp_path):
    def change(s):
        req(s, "req-order").criteria = ["something else"]
        req(s, "req-reserve").target = "place-order"
        req(s, "req-latency").depends_on = ["req-order"]
        for k in s.risks: k.id = "up-" + k.id
    s = baseline(tmp_path, change, allow_fields={"requirements": ["target"]}, id_prefix={"risks": "up-"},
                 refinements=[Refinement(path="requirements/req-order", reason="clarified", decision="ADR-1")])
    assert [x for x in errs(s, tmp_path) if "req-latency" in x]
    s.system.imports[0].refinements.append(Refinement(path="requirements/req-latency/depends_on", reason="order first", decision="ADR-2"))
    assert errs(s, tmp_path) == []
    s.system.imports[0].refinements.append(Refinement(path="requirements/req-scale", reason="stale", decision="ADR-3"))
    assert any("names no deviation" in x for x in errs(s, tmp_path))

def test_baseline_additions_allowed_and_artifacts_checked(tmp_path):
    def change(s):
        req(s, "req-order").depends_on = []
        s.requirements.append(req(s, "req-order").model_copy(update={"id": "req-extra"}))
    s = baseline(tmp_path, change)
    assert errs(s, tmp_path) == []
    (tmp_path / "base/spec.yaml").write_text((tmp_path / "base/spec.yaml").read_text() + "\n", encoding="utf-8")
    assert any("modified" in x for x in errs(s, tmp_path))
