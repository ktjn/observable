import copy, json, re, yaml
import pytest
from datetime import date
from pathlib import Path
from metamodel import load_spec, validate_spec, resolve_capabilities, check_compatibility, check_data_compatibility, compare_specs, context_slice, json_schema, INVARIANTS, impact_analysis, diagram, residual_risks, main, Component, Evidence, Risk, Spec

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
    assert 'subgraph application["application"]' in out
    assert "checkout -->|reserve-inventory| inventory" in out
    assert "checkout -.->|owns| order" in out
    assert "order_stock_projection -.->|derived_from| order" in out
    assert "class checkout,inventory verified" in out

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
    for f, st in [(d / "old.yaml", "architecture"), (d.parent / "examples/greenfield/spec.yaml", "release"), (b / "level0.yaml", "inventory"),
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
