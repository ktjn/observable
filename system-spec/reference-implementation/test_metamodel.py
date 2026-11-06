import copy, json, re, yaml
import pytest
from datetime import date
from pathlib import Path
from metamodel import load_spec, validate_spec, resolve_capabilities, check_compatibility, check_data_compatibility, compare_specs, context_slice, json_schema, INVARIANTS, impact_analysis, diagram, residual_risks, main, Component, Domain, Evidence, Risk, Spec, Requirement, check_report, evidence_from_report, Constraint, change_plan, CATEGORIES, check_artifacts

EXAMPLE = Path(__file__).parent / "example.yaml"

def spec(): return load_spec(EXAMPLE)

def test_valid(): assert validate_spec(spec()) == []

def test_resolution(): assert resolve_capabilities(spec())["checkout"]["reserve-inventory"] == "inventory"

def test_compatibility():
    c = spec().capabilities[0]
    assert check_compatibility(c, c.model_copy(update={"postconditions": [*c.postconditions, "logged"]})) == []
    assert check_compatibility(c, c.model_copy(update={"preconditions": [*c.preconditions, "extra"]})) != []
    assert any("major version" in e for e in check_compatibility(c, c.model_copy(update={"failure_modes": ["timeout"]})))
    assert check_compatibility(c, c.model_copy(update={"failure_modes": ["timeout"], "version": "2.0"})) == ["[COMPAT-04] Added failure modes: ['timeout']"]

def test_compare_specs():
    s, n = spec(), spec(); n.capabilities.pop()
    assert any("removed without deprecation" in e for e in compare_specs(s, n))
    s.capabilities[-1].status = "deprecated"
    assert compare_specs(s, n) == []

def test_impact():
    imp = impact_analysis(spec(), "reserve-inventory")
    assert ("checkout" in imp["components"]) and ("req-reserve" in imp["requirements"]) and ("ev-reserve-test" in imp["invalidated_evidence"])
    assert {"req-order", "req-latency"} <= set(imp["requirements"])
    s = spec(); s.evidence[0].subject_id = "inventory"
    assert "ev-order-test" in impact_analysis(s, "inventory")["invalidated_evidence"]

def test_impact_data():
    imp = impact_analysis(spec(), "order")
    assert "order-stock-projection" in imp["data_models"] and "inventory" in imp["components"]

def err(s, msg, stage="release", today=None): return any(msg in e for e in validate_spec(s, stage, today))

def test_ambiguous_provider():
    s = spec(); s.components.append(Component(id="inv2", version="1.0", layer="domain", provides=["reserve-inventory"]))
    assert err(s, "ambiguous providers")
    s.components[0].bindings = {"reserve-inventory": "inventory"}
    assert not err(s, "ambiguous") and resolve_capabilities(s)["checkout"]["reserve-inventory"] == "inventory"

def test_ambiguous_target():
    s = spec(); s.data_models[0].id = "place-order"
    assert err(s, "ambiguous target")

def test_cli(capsys, tmp_path):
    assert main([str(EXAMPLE)]) == 0 and "VALID" in capsys.readouterr().out
    bad = tmp_path / "bad.yaml"; bad.write_text("system: {}")
    assert main([str(bad), "--json"]) == 1 and "schema_errors" in capsys.readouterr().out
    assert main([str(EXAMPLE), "--impact", "order"]) == 0 and "order-stock-projection" in capsys.readouterr().out
    assert main([str(EXAMPLE), "--compat", str(EXAMPLE)]) == 0

def test_treatments():
    s = spec(); r = s.risks[0]; r.treatment = "avoidance"
    assert not err(s, "Risk 'risk-oversell'")
    r.residual_accepted = True; assert err(s, "must not carry residual risk")
    r.treatment, r.mitigated_by = "transfer", []
    assert err(s, "requires transferred_to") and not err(s, "requires residual_accepted")
    r.transferred_to, r.residual_accepted = "insurer", False
    assert err(s, "requires residual_accepted")
    r.residual_accepted = True; assert not err(s, "Risk 'risk-oversell'")
    assert residual_risks(s) == ["risk-oversell", "risk-erp-delay", "risk-payment-fraud"]

def test_diagram(capsys):
    assert main([str(EXAMPLE), "--diagram"]) == 0
    out = capsys.readouterr().out
    assert 'subgraph l_application["application"]' in out
    assert "c_checkout -->|reserve-inventory| c_inventory" in out
    assert "c_checkout -.->|owns| d_order" in out
    assert "d_order_stock_projection -.->|derived_from| d_order" in out
    assert "class c_checkout,c_inventory verified" in out
    assert 'd_order("order<br>confidential")' in out and "logical" in out.splitlines()[-1]
    s = spec()  # component named like its layer, owning a persisted model with the same ID
    c, d = s.components[0], next(d for d in s.data_models if d.owner == s.components[0].id)
    s.system.layers[s.system.layers.index(c.layer)] = c.id = c.layer = d.owner = d.id = "x"
    d.namespace = "sales"
    out = diagram(s).splitlines()
    i = out.index('    subgraph l_x["x"]')
    assert out[i + 1].startswith('      c_x["x v') and out[i + 2].startswith('      d_x[("x<br>sales<br>')
    s = spec(); s.components[1].external, s.components[0].via = True, {"reserve-inventory": "x"}
    out = diagram(s).splitlines()
    end = out.index("  end")
    assert out[end + 1] == '  c_inventory{{"inventory v1.0<br>external verified<br>API: reserve-inventory"}}' and out[end + 2].startswith("  d_stock_item")
    assert "  c_checkout -.->|reserve-inventory via x| c_inventory" in out and not any("c_inventory[" in l for l in out)
    s = spec(); s.system.domains = [Domain(id="sales", uses=["stock"]), Domain(id="stock")]
    s.components[0].domain, s.components[1].domain = "sales", "stock"
    assert validate_spec(s) == []
    s.system.domains[1].uses = ["sales"]
    s.risks.append(Risk(id="w", target="inventory", treatment="mitigation", mitigated_by=[s.requirements[0].id], waives="dependency-cycle", expires="release"))
    assert not any("DOM-03" in e for e in validate_spec(s, "verification"))
    out = diagram(s).splitlines()
    i = out.index('    subgraph l_application["application"]')
    assert out[i + 1] == '      subgraph g_application_sales["sales"]' and out[i + 2].startswith("        c_checkout[") and out[i + 3].startswith("        d_order")

def test_impact_unknown(capsys):
    assert main([str(EXAMPLE), "--impact", "nope"]) == 1 and "Unknown --impact ID 'nope'" in capsys.readouterr().out

def test_extra_forbidden(capsys, tmp_path):
    bad = tmp_path / "bad.yaml"; bad.write_text(EXAMPLE.read_text().replace("layer: domain", "layer: domain\n    colour: red"))
    assert main([str(bad)]) == 1 and "Extra inputs are not permitted" in capsys.readouterr().out

def test_stages():
    s = spec(); s.capabilities[0].status = "draft"; s.evidence = []
    assert validate_spec(s, "inventory") == [] and err(s, "Capability 'place-order' status 'draft' below 'specified'", "discovery")
    assert not err(s, "missing valid verification evidence", "verification") and err(s, "missing valid verification evidence")
    s = spec(); s.capabilities[0].invariants = []
    assert err(s, "incomplete contract clauses", "discovery")

def test_component_budget_override():
    s = spec(); s.system.context_budget.max_provides = 0
    assert err(s, "exceeds context budget: provides")
    for c in s.components: c.context_budget = {"max_provides": 5}
    assert not err(s, "exceeds context budget: provides")

def test_waivers():
    s = spec(); s.components[1].requires.append("place-order@^1.0")
    w = Risk(id="w", target="inventory", treatment="mitigation", mitigated_by=["req-reserve"], waives="layer-violation", expires="verification")
    s.risks.append(w)
    assert not err(s, "layer violation", "architecture") and not err(s, "layer violation", "verification")
    w.waives = "COMP-05"; assert not err(s, "layer violation", "architecture")
    assert err(s, "waiver expired", "release") and err(s, "no waivers permitted at release") and err(s, "layer violation")
    w.expires = "architecture"; assert err(s, "waiver expired", "verification") and err(s, "layer violation", "verification")
    w.expires = date(2000, 1, 1); assert err(s, "waiver expired", "architecture") and err(s, "layer violation", "architecture")
    w.expires, w.mitigated_by = "release", []; assert err(s, "waiver requires", "inventory") and err(s, "layer violation", "architecture")

def test_budget():
    s = spec(); s.system.context_budget.max_owns_data = 1
    assert err(s, "Component 'inventory' exceeds context budget: owns_data > 1", "architecture")
    s.risks.append(Risk(id="w", target="inventory", treatment="acceptance", mitigated_by=["req-reserve"], waives="context-budget", expires="release"))
    assert not err(s, "exceeds context budget", "verification")

def test_interface():
    s = spec(); s.capabilities[1].interface = None
    assert err(s, "'reserve-inventory' is required cross-component but has no interface", "architecture")
    s = spec(); s.capabilities[1].interface.errors.pop("product_not_found"); s.capabilities[1].interface.request = "ghost"
    assert err(s, "interface must define operation, idempotent, auth and map errors to failure_modes") and err(s, "interface references unknown data model 'ghost'", "inventory")
    s = spec(); s.capabilities[1].interface.request = ["stock-item", "order"]
    assert not err(s, "interface references unknown data model", "inventory")
    s.capabilities[1].interface.response = ["stock-item", "ghost"]
    assert err(s, "interface references unknown data model 'ghost'", "inventory")

def test_categories():
    s = spec(); s.requirements[3].category = None
    assert err(s, "Missing verification categories: ['performance']", "verification")
    s.risks.append(Risk(id="w", target="commerce-system", treatment="mitigation", mitigated_by=["req-latency"], waives="verification-category", expires="release"))
    assert validate_spec(s, "verification") == []

def test_conflict_resolution():
    s = spec(); s.requirements[0].conflicts_with = ["req-reserve"]; s.requirements[0].resolved_by = "req-latency"
    assert not err(s, "unresolved conflict")
    s.requirements[3].status = "deprecated"; assert err(s, "unresolved conflict")
    s.requirements[1].status = "deprecated"; assert not err(s, "unresolved conflict")
    s = spec(); s.requirements[0].mandatory = False; s.requirements[1].resolved_by = "req-order"; s.evidence.pop(0)
    assert err(s, "Requirement 'req-order@1.0' missing valid verification evidence")

def test_supersedes():
    s = spec(); s.evidence.append(Evidence(id="ev-fail", requirement_id="req-order", requirement_version="1.0", method="test", passed=False, provenance="ci", subject_revision="1.0", timestamp=date(2026, 10, 30), verifier="ci"))
    s.evidence.append(Evidence(id="ev-fix", requirement_id="req-order", requirement_version="1.0", method="test", passed=True, provenance="ci", supersedes="ev-fail", subject_revision="1.0", timestamp=date(2026, 10, 30), verifier="ci"))
    assert err(s, "without supersession_reason") and err(s, "unresolved failing evidence")
    s.evidence[-1].supersession_reason = "flaky fixture fixed"; assert validate_spec(s) == []
    s.evidence[-1].supersedes = "ev-order-test"; assert err(s, "supersede a failed record")

def test_declassification():
    s = spec(); s.data_models[2].classification = "public"; s.data_models[2].declassified_by = "req-projection"
    assert validate_spec(s) == []
    s.evidence = [e for e in s.evidence if e.requirement_id != "req-projection"]; s.requirements[2].mandatory = False
    assert err(s, "Requirement 'req-projection@1.0' missing valid verification evidence")
    s.requirements[2].status = "deprecated"; assert err(s, "violates classification propagation", "architecture")
    s.data_models[2].declassified_by = "ghost"; assert err(s, "declassified_by unknown requirement 'ghost'", "inventory")

def test_deprecated_requirement():
    s = spec(); s.requirements[0].status = s.requirements[4].status = "deprecated"; s.evidence.pop(0)
    assert not err(s, "req-order@") and err(s, "Capability 'place-order' has no associated requirements")

def test_data_compat():
    d = spec().data_models[1]
    assert check_data_compatibility(d, d.model_copy(update={"fields": {**d.fields, "note": "string"}})) == []
    errs = check_data_compatibility(d, d.model_copy(update={"fields": {"order_id": "string"}, "classification": "restricted"}))
    assert len(errs) == 4 and "major version" in errs[-1]
    n = spec(); n.data_models[1].fields.pop("status")
    assert any("DataModel 'order': Removed fields: ['status']" in e for e in compare_specs(spec(), n))
    n.data_models.pop(1)
    assert any("[COMPAT-05] DataModel 'order': removed data model" in e for e in compare_specs(spec(), n))

def test_context(capsys):
    sl = context_slice(spec(), "checkout")
    assert [c["id"] for c in sl["capabilities"]] == ["place-order", "reserve-inventory"]
    assert {d["id"] for d in sl["data_models"]} == {"order", "stock-item"} and "evidence" not in sl
    assert {r["id"] for r in sl["requirements"]} == {"req-order", "req-reserve", "req-order-contract", "req-order-serial", "req-latency", "req-scale", "req-resilience", "req-security", "req-observability"}
    assert main([str(EXAMPLE), "--context", "checkout"]) == 0 and "place-order" in capsys.readouterr().out
    assert main([str(EXAMPLE), "--context", "nope"]) == 1

def test_examples():
    d = Path(__file__).parent; b = d.parent / "examples/brownfield"
    if not b.exists(): pytest.skip("examples/ not mounted")
    for f, st in [(d / "old.yaml", "architecture"), (d.parent / "examples/greenfield/spec.yaml", "release"), (b.parent / "business/spec.yaml", "verification"), (b.parent / "experience/spec.yaml", "verification"), (b / "level0.yaml", "inventory"),
                  (b / "level1.yaml", "architecture"), (b / "old.yaml", "verification"), (b / "spec.yaml", "release")]:
        assert validate_spec(load_spec(f), st) == [], f
    assert compare_specs(load_spec(b / "old.yaml"), load_spec(b / "spec.yaml")) == []

def test_schema_committed():
    assert json.loads((Path(__file__).parent / "schema.json").read_text()) == json_schema(), "regenerate: metamodel.py --schema > schema.json"

def test_invariants_match_spec():
    md = Path(__file__).parent.parent / "spec.md"
    if not md.exists(): pytest.skip("spec.md not mounted")
    rows = {l.split("|")[1].strip(" `"): l for l in md.read_text(encoding="utf-8").splitlines() if re.match(r"\| `[A-Z]+-\d+` \|", l)}
    assert rows == {i: f"| `{i}` | {f'`{st}`' if st else 'change'} | {f'`{w}`' if w else '—'} | {t} |" for i, (st, w, t) in INVARIANTS.items()}

CORPUS = yaml.safe_load((Path(__file__).parent / "corpus.yaml").read_text())

def patch(d, p):
    d = copy.deepcopy(d)
    def at(path):
        *a, k = path.split("."); o = d
        for x in a: o = o[int(x) if isinstance(o, list) else x]
        return o, int(k) if isinstance(o, list) and k != "-" else k
    for path, v in p.get("set", {}).items():
        o, k = at(path); o.extend(v) if k == "-" else o.__setitem__(k, v)
    for path in p.get("drop", []): o, k = at(path); del o[k]
    return Spec.model_validate(d)

@pytest.mark.parametrize("c", CORPUS, ids=lambda c: c["id"])
def test_corpus(c):
    base = yaml.safe_load(EXAMPLE.read_text()); new = patch(base, c)
    errs = validate_spec(new, c.get("stage", "release")) + (compare_specs(patch(base, c["old"]), new) if "old" in c else [])
    assert {e[1:e.index("]")] for e in errs} == {c["id"], *c.get("also", [])}, errs

def test_corpus_covers_invariants(): assert {c["id"] for c in CORPUS} == set(INVARIANTS)

def test_format_blocks_match_schema():
    md = Path(__file__).parent.parent / "spec.md"
    if not md.exists(): pytest.skip("spec.md not mounted")
    blocks = "\n".join(l for l in md.read_text(encoding="utf-8").splitlines() if l.startswith("**Current reference format:**"))
    defs = json_schema()["$defs"]
    missing = {f"{m}.{k}" for m, d in defs.items() for k in d.get("properties", {}) if f"`{k}`" not in blocks}
    assert not missing, missing

def test_prompts_cite_invariants_and_schema():
    root = Path(__file__).parent.parent
    if not (root / "GREENFIELD.md").exists(): pytest.skip("docs not mounted")
    for name in ("GREENFIELD.md", "BROWNFIELD.md"):
        text = (root / name).read_text(encoding="utf-8").split("## Prompts", 1)[1]
        cited = set(re.findall(r"\b(?:REF|DATA|EV|RISK|STAT|CAP|COMP|REQ|COMPAT)-\d{2}\b", text))
        assert "schema.json" in text and cited and cited <= set(INVARIANTS), (name, cited - set(INVARIANTS))

def test_formal_models_present():
    root = Path(__file__).parent.parent
    if not (root / "examples").exists(): pytest.skip("examples not mounted")
    gf = load_spec(root / "examples/greenfield/spec.yaml")
    tla = (root / "examples/greenfield/Inventory.tla").read_text(encoding="utf-8")
    cfg = (root / "examples/greenfield/Inventory.cfg").read_text(encoding="utf-8")
    als = (root / "examples/greenfield/Inventory.als").read_text(encoding="utf-8")
    mals = (root / "reference-implementation/metamodel.als").read_text(encoding="utf-8")
    ev_tla = next(e for e in gf.evidence if e.id == "ev-reserve-tla")
    ev_als = next(e for e in gf.evidence if e.id == "ev-projection-alloy")
    assert "StockNeverNegative" in tla and "INVARIANT StockNeverNegative" in cfg and f"MaxQty = {ev_tla.bounds['max_qty']}" in cfg
    assert "assert ProjectionReferencesExisting" in als and f"for {ev_als.bounds['scope']}" in als
    for tok in ("Draft, Specified, Verified, Deprecated", "Public, Internal, Confidential, Restricted",
                "Mitigation, Acceptance, Avoidance, Transfer", "Test, Proof, ModelCheck, Analysis, Inspection"):
        assert tok in mals, tok

def test_docs_invariant_counts():
    root = Path(__file__).parent.parent
    if not (root / "README.md").exists(): pytest.skip("docs not mounted")
    n_all, n_st, n_cp = len(INVARIANTS), sum(1 for s, _, _ in INVARIANTS.values() if s), sum(1 for s, _, _ in INVARIANTS.values() if not s)
    assert f"{n_all} stage/compatibility invariants" in (root / "README.md").read_text(encoding="utf-8")
    assert f"{n_st} stage invariants + {n_cp} `--compat` invariants" in (root / "PLAN.md").read_text(encoding="utf-8")

def sub_req(**kw): return Requirement(id="req-sub", version="1.0", status="verified", category="functional", target="place-order", refines=["req-order"], criteria=["x"], **kw)

def ids(errs): return {e[1:e.index("]")] for e in errs}

def test_subrequirement_inherits_obligation():
    s = spec(); s.requirements.append(sub_req(mandatory=False))
    assert ids(validate_spec(s)) == {"REQ-05", "REQ-08"}

def test_subrequirement_rolls_up():
    s = spec(); s.requirements.append(sub_req()); s.evidence.append(s.evidence[0].model_copy(update={"id": "ev-sub", "requirement_id": "req-sub"}))
    assert validate_spec(s) == []
    s.evidence[-1].passed = False
    assert "REQ-08" in ids(validate_spec(s))

def report(tmp_path, *cases, suite=""):
    f = tmp_path / "r.xml"
    f.write_text(f'<testsuites><testsuite {suite}>' + "".join(
        f'<testcase classname="t" name="{n}">' + (f'<properties>{"".join(f"<property name={chr(34)}{k}{chr(34)} value={chr(34)}{v}{chr(34)}/>" for k, v in props)}</properties>' if props else "") + ("<failure/>" if fail else "") + "</testcase>"
        for n, props, fail in cases) + "</testsuite></testsuites>")
    return str(f)

def bound(name, ref="req-order@1.0", fail=False): return (name, [("verifies", ref), ("test", f"tests/{name}.py::{name}")], fail)

def test_report_agrees(tmp_path):
    s = spec(); s.evidence = [s.evidence[0]]; s.evidence[0].provenance = "tests/test_checkout.py::test_checkout"
    assert check_report(s, [report(tmp_path, bound("test_checkout"))]) == []

def test_report_rejects_unbound_unknown_stale(tmp_path):
    f = report(tmp_path, ("orphan", [], False), bound("a", "nope@1.0"), bound("b", "req-order@0.1"), ("c", [("verifies", "req-order@1.0"), ("verifies", "req-reserve@1.0")], False))
    errs = check_report(spec(), [f])
    assert sum("exactly one requirement" in e for e in errs) == 2 and any("unknown requirement 'nope'" in e for e in errs) and any("current version is 1.0" in e for e in errs)

def test_report_checks_test_evidence(tmp_path):
    s = spec(); s.evidence = [s.evidence[0]]; s.evidence[0].provenance = "tests/test_checkout.py::test_checkout"
    assert any("not in the run reports" in e for e in check_report(s, [report(tmp_path, bound("other"))]))
    assert any("passed=True but the run reports passed=False" in e for e in check_report(s, [report(tmp_path, bound("test_checkout", fail=True))]))
    assert any("binds ['req-reserve@1.0']" in e for e in check_report(s, [report(tmp_path, bound("test_checkout", "req-reserve@1.0"))]))

def test_emit_evidence_and_cli(tmp_path, capsys):
    f = report(tmp_path, bound("test_checkout"), bound("x", "nope@1.0"), suite='timestamp="2026-11-02T10:00:00" hostname="ci"')
    ev = evidence_from_report(spec(), [f])
    assert ev == [{"id": "ev-tests-test_checkout-py-test_checkout", "requirement_id": "req-order", "requirement_version": "1.0", "method": "test", "passed": True,
                   "provenance": "tests/test_checkout.py::test_checkout", "timestamp": "2026-11-02T10:00:00", "environment": "ci"}]
    assert main([str(EXAMPLE), "--emit-evidence", f]) == 0 and "ev-tests-test_checkout-py-test_checkout" in capsys.readouterr().out
    assert main([str(EXAMPLE), "--check-report", f, "--json"]) == 1 and "binds unknown requirement" in capsys.readouterr().out
    assert main([str(EXAMPLE), "--check-report", str(tmp_path / "missing.xml")]) == 1 and "report:" in capsys.readouterr().out

def test_new_optional_fields_default_off():
    s = spec(); r = s.requirements[0]
    assert (r.statement, r.priority, r.owner, r.superseded_by) == (None,) * 4 and s.components[0].description == "" and s.system.verification_categories is None
    assert validate_spec(s) == []

def test_superseded_by():
    s = spec(); s.requirements[0].superseded_by = "req-reserve"
    assert ids(validate_spec(s)) == {"REQ-09"}
    s.requirements[0].status = "deprecated"
    assert "REQ-09" not in ids(validate_spec(s))
    s.requirements[0].superseded_by = "nope"
    assert "REQ-09" in ids(validate_spec(s))
    s.requirements[0].superseded_by = "req-reserve"; s.requirements[1].status = "deprecated"
    assert "REQ-09" in ids(validate_spec(s))

def test_verification_categories_profile():
    s = spec(); s.requirements = [r for r in s.requirements if r.category != "scalability"]
    assert "REQ-01" in ids(validate_spec(s, "verification"))
    s.system.verification_categories = [c for c in CATEGORIES if c != "scalability"]
    assert "REQ-01" not in ids(validate_spec(s, "verification"))
    with pytest.raises(ValueError): Spec.model_validate({**s.model_dump(mode="json"), "system": {**s.system.model_dump(mode="json"), "verification_categories": []}})

def test_omitted_categories_reported(capsys):
    assert main([str(Path(__file__).parent.parent / "examples/business/spec.yaml"), "--stage", "verification", "--json"]) == 0
    assert "serialization" in json.loads(capsys.readouterr().out)["omitted_categories"]
    assert main([str(Path(__file__).parent.parent / "examples/business/spec.yaml"), "--stage", "verification"]) == 0
    assert "note: verification categories not required" in capsys.readouterr().out
    assert main([str(EXAMPLE), "--json"]) == 0 and json.loads(capsys.readouterr().out)["omitted_categories"] == []

def test_plan_lists_priorities(capsys):
    s = spec(); s.requirements[0].priority = "P0"
    assert change_plan(s, "place-order")["priorities"] == {"req-order": "P0"} and "priorities" not in change_plan(spec(), "place-order")

def test_experiences_are_optional():
    s = spec()
    assert s.views == [] and s.experiences == [] and not any(e.startswith("[EXP-") for e in validate_spec(s))

def test_experience_example():
    s = load_spec(Path(__file__).parent.parent / "examples/experience/spec.yaml")
    assert validate_spec(s, "verification") == [] and "EXP-07" in ids(validate_spec(s))
    s.experiences[0].scenarios = [sc for sc in s.experiences[0].scenarios if sc.kind != "negative"]
    assert ids(validate_spec(s, "verification")) == {"EXP-05"}

def test_experience_references_checked_as_artifacts():
    root = Path(__file__).parent.parent / "examples/experience"
    s = load_spec(root / "spec.yaml")
    assert check_artifacts(s, root) == []
    s.views[1].references[0].sha256 = "0" * 64
    assert any("hash mismatch" in e for e in check_artifacts(s, root))
    s.views[1].references[0].path = "assets/missing.svg"
    assert any("missing" in e for e in check_artifacts(s, root))

def exp_spec(): return load_spec(Path(__file__).parent.parent / "examples/experience/spec.yaml")

def exp_ids(s, stage="verification"): return {e[1:e.index("]")] for e in validate_spec(s, stage) if e.startswith("[EXP-")}

def test_experience_example_uses_new_features():
    s = exp_spec(); x = s.experiences[0]
    assert exp_ids(s) == set() and x.entries and s.state_sets and isinstance(x.constraints["accessibility"], Constraint)
    assert x.scenarios[0].requirements and any(i.from_entry for i in x.interactions) and any(i.from_view == i.to_view for i in x.interactions)

def test_scenario_requirements():
    s = exp_spec(); sc = s.experiences[0].scenarios[0]
    sc.requirements = ["req-journey", "nope"]
    assert exp_ids(s) == {"EXP-02"}
    sc.requirements, sc.requirement = [], None
    assert exp_ids(s) == {"EXP-02"}
    sc.requirement, sc.requirements = "req-journey", ["req-a11y"]
    assert exp_ids(s) == set() and "EXP-07" in exp_ids(s, "release")

def test_entry_source():
    s = exp_spec(); i = next(i for i in s.experiences[0].interactions if i.from_entry)
    i.from_entry = "ghost"
    assert exp_ids(s) == {"EXP-02"}
    i.from_entry, i.from_view = None, None
    assert exp_ids(s) == {"EXP-02"}
    i.from_entry, i.from_view = "alert-notification", "service-detail"
    assert "EXP-02" in exp_ids(s)

def test_state_sets():
    s = exp_spec(); s.views[0].state_sets.append("ghost")
    assert exp_ids(s) == {"EXP-02"}
    s = exp_spec(); s.state_sets["data-conditions"]["stale"] = " "
    assert exp_ids(s) == {"EXP-04"}
    s = exp_spec(); s.views[2].state_sets = []
    assert exp_ids(s) == {"EXP-02", "EXP-04"}
    s = exp_spec(); s.views[0].states["error"] = "own handling overrides the set"
    assert exp_ids(s) == set()

def test_constraint_requirement():
    s = exp_spec(); x = s.experiences[0]
    x.constraints["accessibility"] = Constraint(value="WCAG-2.2-AA", requirement="ghost")
    assert exp_ids(s) == {"EXP-02"}
    x.constraints["accessibility"] = "WCAG-2.2-AA"
    assert exp_ids(s) == set() and "EXP-07" in exp_ids(exp_spec(), "release")

def test_consistent_with():
    s = exp_spec(); s.views[0].references[0].consistent_with = ["ghost"]
    assert exp_ids(s) == {"EXP-02"}

def test_exhaustive_experience():
    s = exp_spec(); x = s.experiences[0]; x.exhaustive = True
    assert exp_ids(s) == set()
    x.requirements.append("req-detail")
    assert exp_ids(s) == {"EXP-08"}
    x.not_exercised = {"req-detail": "covered by the workflow scenarios"}
    assert exp_ids(s) == set()
    x.not_exercised["ghost"] = "not realized"
    assert exp_ids(s) == {"EXP-08"}
