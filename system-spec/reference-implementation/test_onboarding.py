import hashlib
import json
import yaml
from pathlib import Path

import pytest
import metamodel as mm

EXAMPLE = Path(__file__).with_name("example.yaml")


def spec(): return mm.load_spec(EXAMPLE)


def bound_spec():
    data = spec().model_dump()
    data["subject_bindings"] = {"place-order": {"version": "1.0", "revision": "commit-a", "artifacts": {"source.py": "a" * 64}}}
    for e in data["evidence"]:
        if e["requirement_id"] in ("req-order", "req-order-contract"):
            e.update(subject_version="1.0", subject_revision="commit-a")
    return mm.Spec.model_validate(data)


def test_unclassified_data_is_not_implicitly_public():
    data = spec().data_models[0].model_dump()
    data.pop("classification")
    assert mm.DataModel.model_validate(data).classification == "unknown"


def test_unknown_classification_requires_review_and_blocks_architecture():
    data = spec().model_dump()
    data["data_models"][0].update(classification="unknown", classification_review="req-security")
    s = mm.Spec.model_validate(data)
    assert mm.validate_spec(s, "inventory") == []
    assert any(e.startswith("[DATA-05]") for e in mm.validate_spec(s, "architecture"))
    s.data_models[0].classification_review = None
    assert any(e.startswith("[DATA-06]") for e in mm.validate_spec(s, "inventory"))
    s.data_models[0].classification_review = "missing"
    assert any(e.startswith("[DATA-06]") for e in mm.validate_spec(s, "inventory"))


def test_unknown_source_cannot_be_bypassed_by_declassification():
    data = spec().model_dump()
    data["data_models"][0].update(classification="unknown", classification_review="req-security")
    data["data_models"][2].update(classification="public", declassified_by="req-projection")
    s = mm.Spec.model_validate(data)
    assert any(e.startswith("[DATA-05]") for e in mm.validate_spec(s, "release"))


def test_bound_source_revision_is_distinct_from_contract_version():
    assert mm.validate_spec(bound_spec()) == []


def test_context_retains_only_relevant_source_bindings():
    s = bound_spec()
    s.subject_bindings["inventory"] = s.subject_bindings["place-order"].model_copy()
    assert set(mm.context_slice(s, "checkout")["subject_bindings"]) == {"place-order"}


@pytest.mark.parametrize("mutation", ["version", "revision", "missing-binding", "binding-version"])
def test_stale_or_unbound_evidence_does_not_pass(mutation):
    s = bound_spec()
    if mutation == "version": s.evidence[0].subject_version = "0.9"
    elif mutation == "revision": s.evidence[0].subject_revision = "commit-old"
    elif mutation == "missing-binding": s.subject_bindings.clear()
    else: s.subject_bindings["place-order"].version = "0.9"
    assert any(e.startswith("[EV-04]") for e in mm.validate_spec(s))


def test_binding_must_resolve_and_have_a_revision():
    s = bound_spec()
    s.subject_bindings["missing"] = s.subject_bindings.pop("place-order")
    assert any(e.startswith("[REF-14]") for e in mm.validate_spec(s, "inventory"))
    s.subject_bindings["place-order"] = s.subject_bindings.pop("missing")
    s.subject_bindings["place-order"].revision = " "
    assert any(e.startswith("[REF-14]") for e in mm.validate_spec(s, "inventory"))


def test_skipped_checks_are_not_passing_requirement_evidence():
    data = spec().model_dump()
    data["evidence"][0]["skipped"] = ["directory symlinks unavailable"]
    s = mm.Spec.model_validate(data)
    assert ("req-order", "1.0") not in mm.verified_requirements(s)
    assert not any(e.startswith("[EV-05]") for e in mm.validate_spec(s, "verification"))
    assert any(e.startswith("[EV-05]") for e in mm.validate_spec(s))


def test_artifact_hashes_sources_schemas_and_evidence(tmp_path):
    s = bound_spec()
    (tmp_path / "source.py").write_bytes(b"source")
    (tmp_path / "result.txt").write_bytes(b"passed\n")
    s.subject_bindings["place-order"].artifacts = {"source.py": hashlib.sha256(b"source").hexdigest()}
    s.evidence[0].artifacts = {"result.txt": hashlib.sha256(b"passed\n").hexdigest()}
    s.components[0].source_paths = ["source.py"]
    s.data_models[0].schema_path = "source.py"
    assert mm.check_artifacts(s, tmp_path) == []
    (tmp_path / "result.txt").write_bytes(b"changed\n")
    assert any("hash mismatch" in e for e in mm.check_artifacts(s, tmp_path))
    s.data_models[0].schema_path = "missing.json"
    assert any("missing.json" in e for e in mm.check_artifacts(s, tmp_path))


@pytest.mark.parametrize("unsafe", ["../outside", "C:/outside", "C:outside", "/outside", "..\\outside"])
def test_artifact_paths_cannot_escape_root(tmp_path, unsafe):
    s = spec(); s.components[0].source_paths = [unsafe]
    assert any("unsafe path" in e for e in mm.check_artifacts(s, tmp_path))


def test_artifact_symlink_escape_is_rejected(tmp_path):
    root = tmp_path / "repo"; root.mkdir()
    (tmp_path / "outside.txt").write_text("outside")
    try: (root / "link").symlink_to(tmp_path / "outside.txt")
    except OSError: pytest.skip("file symlinks unavailable")
    s = spec(); s.components[0].source_paths = ["link"]
    assert any("unsafe path" in e for e in mm.check_artifacts(s, root))


def test_artifact_check_does_not_claim_git_revision_verification(tmp_path):
    s = bound_spec()
    s.subject_bindings["place-order"].artifacts = {"missing.py": "a" * 64}
    assert any("missing.py" in e for e in mm.check_artifacts(s, tmp_path))
    (tmp_path / "missing.py").write_bytes(b"wrong source")
    assert any("hash mismatch" in e for e in mm.check_artifacts(s, tmp_path))
    s.subject_bindings["place-order"].artifacts = {"missing.py": "bad-digest"}
    assert any("invalid SHA-256" in e for e in mm.check_artifacts(s, tmp_path))


def test_artifact_cli_requires_root_and_reports_failed_checks(tmp_path, capsys):
    with pytest.raises(SystemExit) as e: mm.main([str(EXAMPLE), "--check-artifacts"])
    assert e.value.code == 2
    s = spec(); s.components[0].source_paths = ["absent"]
    path = tmp_path / "spec.yaml"; path.write_text(json.dumps(s.model_dump(mode="json")))
    assert mm.main([str(path), "--check-artifacts", "--root", str(tmp_path), "--json"]) == 1
    assert any("absent" in e for e in json.loads(capsys.readouterr().out)["errors"])


def test_artifact_checks_evidence_provenance_files_when_path_like(tmp_path):
    s = spec()
    s.evidence[0].provenance = "tests/test_foo.py::test_bar"
    assert any("test_foo.py" in e for e in mm.check_artifacts(s, tmp_path))
    (tmp_path / "tests").mkdir()
    (tmp_path / "tests" / "test_foo.py").write_text("def test_bar(): pass")
    assert not any("test_foo.py" in e for e in mm.check_artifacts(s, tmp_path))


def drift_repo(tmp_path, checkout_src, inventory_src="x = 1\n"):
    for rel, text in {"src/checkout/app.py": checkout_src, "src/inventory/api.py": inventory_src}.items():
        (tmp_path / rel).parent.mkdir(parents=True, exist_ok=True)
        (tmp_path / rel).write_text(text)
    s = spec()
    s.components[0].source_paths, s.components[1].source_paths = ["src/checkout"], ["src/inventory"]
    return s


def test_drift_import_matching_a_required_capability_conforms(tmp_path):
    s = drift_repo(tmp_path, "from src.inventory import api\n")
    assert mm.check_drift(s, tmp_path) == {"errors": [], "unchecked": []}


def test_drift_undeclared_import_between_components_is_reported(tmp_path):
    s = drift_repo(tmp_path, "x = 1\n", "import src.checkout.app\n")
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert len(errors) == 1 and "'inventory' imports 'checkout'" in errors[0] and "src/inventory/api.py" in errors[0]


def test_drift_relative_and_same_component_imports_are_ignored(tmp_path):
    s = drift_repo(tmp_path, "from . import helper\nimport os\n")
    (tmp_path / "src/checkout/helper.py").write_text("from .app import x\n")
    assert mm.check_drift(s, tmp_path)["errors"] == []


def test_drift_unparseable_source_is_reported_not_executed(tmp_path):
    s = drift_repo(tmp_path, "def (:\n")
    assert any("cannot parse" in e and "app.py" in e for e in mm.check_drift(s, tmp_path)["errors"])


def test_drift_json_schema_fields_must_match_model_fields(tmp_path):
    s = spec(); s.data_models[0].schema_path = "stock.json"
    (tmp_path / "stock.json").write_text(json.dumps({"properties": {"product_id": {}, "extra": {}}}))
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert any("missing from schema" in e and "available_qty" in e for e in errors)
    assert any("undeclared in model" in e and "extra" in e for e in errors)
    (tmp_path / "stock.json").write_text(json.dumps({"properties": {"product_id": {}, "available_qty": {}}}))
    assert mm.check_drift(s, tmp_path)["errors"] == []


def test_drift_unsupported_schema_format_is_listed_as_unchecked(tmp_path):
    s = spec(); s.data_models[0].schema_path = "stock.sql"
    (tmp_path / "stock.sql").write_text("create table t(a int);")
    assert mm.check_drift(s, tmp_path) == {"errors": [], "unchecked": ["checkout: no source_paths", "inventory: no source_paths", "stock.sql"]}


def test_drift_cli_requires_root_and_reports_errors(tmp_path, capsys):
    with pytest.raises(SystemExit) as e: mm.main([str(EXAMPLE), "--check-drift"])
    assert e.value.code == 2
    s = drift_repo(tmp_path, "x = 1\n", "import src.checkout.app\n")
    path = tmp_path / "spec.yaml"; path.write_text(json.dumps(s.model_dump(mode="json")))
    assert mm.main([str(path), "--check-drift", "--root", str(tmp_path), "--json"]) == 1
    assert "imports" in json.loads(capsys.readouterr().out)["errors"][0]


def test_drift_ignores_symlinked_sources_outside_root(tmp_path):
    root = tmp_path / "repo"; (root / "src/checkout").mkdir(parents=True)
    outside = tmp_path / "evil.py"; outside.write_text("def (:\n")
    try: (root / "src/checkout/evil.py").symlink_to(outside)
    except OSError: pytest.skip("file symlinks unavailable")
    s = spec(); s.components[0].source_paths = ["src/checkout"]
    assert mm.check_drift(s, root) == {"errors": [], "unchecked": ["inventory: no source_paths"]}


def comp_spec(evidence_provenance, test_paths=("tests/checkout",)):
    s = spec(); s.components[0].test_paths = list(test_paths)
    for e in s.evidence:
        if e.method == "test": e.provenance = evidence_provenance
    return s


def test_component_test_evidence_must_live_in_its_test_paths():
    ok = comp_spec("tests/checkout/test_order.py::test_order")
    assert not any("COMP-08" in e for e in mm.validate_spec(ok, "verification"))
    bad = comp_spec("tests/inventory/test_order.py::test_order")
    assert any("COMP-08" in e and "'checkout'" in e for e in mm.validate_spec(bad, "verification"))
    assert not any("COMP-08" in e for e in mm.validate_spec(bad, "architecture"))
    assert not any("COMP-08" in e for e in mm.validate_spec(comp_spec("x::y", test_paths=()), "verification"))


def test_drift_covers_a_components_own_tests(tmp_path):
    s = drift_repo(tmp_path, "x = 1\n")
    (tmp_path / "tests/inventory").mkdir(parents=True)
    (tmp_path / "tests/inventory/test_api.py").write_text("import src.checkout.app\n")
    s.components[1].test_paths = ["tests/inventory"]
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert len(errors) == 1 and "tests of 'inventory' import 'checkout'" in errors[0] and "tests/inventory/test_api.py" in errors[0]


def test_artifacts_check_test_paths_exist(tmp_path):
    s = spec(); s.components[0].test_paths = ["tests/absent"]
    assert any("tests/absent" in e for e in mm.check_artifacts(s, tmp_path))


COMPONENTS = Path(__file__).parent.parent / "examples" / "components"


@pytest.mark.skipif(not COMPONENTS.exists(), reason="examples not mounted")
def test_components_example_passes_all_checks_and_fails_when_broken(tmp_path):
    s = mm.load_spec(COMPONENTS / "spec.yaml")
    assert mm.validate_spec(s) == []
    assert mm.check_artifacts(s, COMPONENTS) == []
    assert mm.check_drift(s, COMPONENTS) == {"errors": [], "unchecked": []}
    import shutil
    root = tmp_path / "c"; shutil.copytree(COMPONENTS, root)
    with (root / "src/inventory/api.py").open("a") as f: f.write("import src.checkout.app\n")
    assert any("'inventory' imports 'checkout'" in e for e in mm.check_drift(s, root)["errors"])


def fakes_spec(fakes=None, test_paths=("tests/checkout",)):
    s = comp_spec("tests/checkout/test_order.py::test_order", test_paths)
    s.components[0].fakes = {"reserve-inventory": "tests/checkout/fakes/reserve_inventory.py"} if fakes is None else fakes
    return s


def test_required_capabilities_need_a_fake_under_test_paths():
    assert not any("COMP-09" in e for e in mm.validate_spec(fakes_spec(), "verification"))
    assert any("COMP-09" in e and "reserve-inventory" in e for e in mm.validate_spec(fakes_spec({}), "verification"))
    assert any("COMP-09" in e and "outside" in e for e in mm.validate_spec(fakes_spec({"reserve-inventory": "tests/inventory/fake.py"}), "verification"))
    assert any("COMP-09" in e and "ghost" in e for e in mm.validate_spec(fakes_spec({"reserve-inventory": "tests/checkout/f.py", "ghost": "tests/checkout/g.py"}), "verification"))
    assert not any("COMP-09" in e for e in mm.validate_spec(fakes_spec({}, test_paths=()), "verification"))
    assert not any("COMP-09" in e for e in mm.validate_spec(fakes_spec({}), "architecture"))
    s = fakes_spec({}); s.components[0].test_scope = "integration"
    assert not any("COMP-09" in e for e in mm.validate_spec(s, "verification"))


def test_artifacts_check_fakes_exist(tmp_path):
    s = fakes_spec()
    assert any("fakes/reserve_inventory.py" in e for e in mm.check_artifacts(s, tmp_path))


def test_drift_tests_may_not_import_other_components_even_when_declared(tmp_path):
    s = drift_repo(tmp_path, "from src.inventory import api\n")
    s.components[0].test_paths = ["tests/checkout"]
    (tmp_path / "tests/checkout").mkdir(parents=True)
    (tmp_path / "tests/checkout/test_app.py").write_text("from src.inventory import api\n")
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert len(errors) == 1 and "tests of 'checkout' import 'inventory'" in errors[0] and "fake" in errors[0] and "tests/checkout/test_app.py" in errors[0]
    (tmp_path / "tests/checkout/test_app.py").write_text("from src.checkout import app\n")
    assert mm.check_drift(s, tmp_path)["errors"] == []


SEL = "nextest::-p checkout-crate"


def selector_spec(provenance, test_paths=(SEL,), fakes=None):
    s = comp_spec(provenance, test_paths)
    s.components[0].fakes = {"reserve-inventory": SEL + "::fakes"} if fakes is None else fakes
    return s


def test_selector_test_paths_match_selector_provenance_and_fakes():
    ok = selector_spec(SEL + "::test_order")
    assert not any("COMP-08" in e or "COMP-09" in e for e in mm.validate_spec(ok, "verification"))
    assert not any("COMP-08" in e for e in mm.validate_spec(selector_spec(SEL), "verification"))
    assert any("COMP-08" in e for e in mm.validate_spec(selector_spec("nextest::-p other-crate"), "verification"))
    assert any("COMP-08" in e for e in mm.validate_spec(selector_spec(SEL + "-extra"), "verification"))
    assert any("COMP-09" in e and "outside" in e for e in mm.validate_spec(selector_spec(SEL, fakes={"reserve-inventory": "nextest::-p other"}), "verification"))


def test_selectors_are_not_paths_for_artifacts_and_are_reported_unchecked_by_drift(tmp_path):
    s = selector_spec(SEL + "::t")
    assert mm.check_artifacts(s, tmp_path) == []
    assert mm.check_drift(s, tmp_path)["unchecked"][0] == f"checkout: test selector '{SEL}'"


def cargo_repo(tmp_path, checkout_toml, dev=""):
    for rel, text in {"crates/checkout/Cargo.toml": checkout_toml, "crates/inventory/Cargo.toml": '[package]\nname = "inventory"\n',
                      "Cargo.toml": '[workspace]\nmembers = ["crates/*"]\n[workspace.dependencies]\ninventory = { path = "crates/inventory" }\n'}.items():
        (tmp_path / rel).parent.mkdir(parents=True, exist_ok=True); (tmp_path / rel).write_text(text)
    s = spec(); s.components[0].source_paths, s.components[1].source_paths = ["crates/checkout"], ["crates/inventory"]
    return s


@pytest.mark.parametrize("dep", ['inventory = { path = "../inventory" }', "inventory = { workspace = true }"])
def test_drift_cargo_path_dependency_matching_requires_conforms(tmp_path, dep):
    s = cargo_repo(tmp_path, f'[package]\nname = "checkout"\n[dependencies]\n{dep}\n')
    assert mm.check_drift(s, tmp_path) == {"errors": [], "unchecked": []}


def test_drift_cargo_undeclared_dependency_and_dev_dependency_are_reported(tmp_path):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n')
    (tmp_path / "crates/inventory/Cargo.toml").write_text('[package]\nname = "inventory"\n[dependencies]\ncheckout = { path = "../checkout" }\n')
    assert any("'inventory' depends on 'checkout'" in e and "crates/inventory/Cargo.toml" in e for e in mm.check_drift(s, tmp_path)["errors"])
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n[dev-dependencies]\ninventory = { path = "../inventory" }\n')
    assert any("tests of 'checkout' depend on 'inventory'" in e for e in mm.check_drift(s, tmp_path)["errors"])
    (tmp_path / "crates/checkout/Cargo.toml").write_text("[package\n")
    assert any("cannot parse" in e and "Cargo.toml" in e for e in mm.check_drift(s, tmp_path)["errors"])


def scope_spec():
    s = spec()
    s.components[0].source_paths, s.components[0].test_paths = ["src/checkout"], ["tests/checkout", "nextest::-p checkout"]
    s.components[1].source_paths = ["src/inventory"]
    s.data_models[1].schema_path = "schemas/order.json"
    return s


def test_scope_allows_only_the_components_own_paths():
    s = scope_spec()
    ok = ["src/checkout/app.py", "tests/checkout/fakes/x.py", "schemas/order.json", "src" + chr(92) + "checkout" + chr(92) + "b.py"]
    assert mm.check_scope(s, "checkout", ok) == []
    errors = mm.check_scope(s, "checkout", ["src/inventory/api.py", "spec.yaml", "src/checkoutx/a.py", "../x", "/abs"])
    assert len(errors) == 5 and all(e.startswith("scope:") for e in errors)
    assert mm.check_scope(s, "checkout", ["uv.lock"], allow=["uv.lock"]) == []
    assert mm.check_scope(s, "checkout", ["docs/a.md"], allow=["docs"]) == []


def test_scope_cli(tmp_path, capsys):
    s = scope_spec(); path = tmp_path / "spec.yaml"; path.write_text(json.dumps(s.model_dump(mode="json")))
    changed = tmp_path / "changed.txt"; changed.write_text("src/checkout/app.py\nsrc/inventory/api.py\n\n")
    assert mm.main([str(path), "--check-scope", "checkout", "--changed", str(changed), "--json"]) == 1
    assert "src/inventory/api.py" in json.loads(capsys.readouterr().out)["errors"][0]
    assert mm.main([str(path), "--check-scope", "ghost", "--changed", str(changed)]) == 1
    with pytest.raises(SystemExit) as e: mm.main([str(path), "--check-scope", "checkout"])
    assert e.value.code == 2


def test_plan_orders_providers_before_consumers():
    p = mm.change_plan(spec(), "reserve-inventory")
    assert p["waves"] == [["inventory"], ["checkout"]]
    assert p["components"]["checkout"]["requires"] == ["reserve-inventory"]
    assert p["components"]["inventory"]["source_paths"] == [] and "ev-order-test" in p["invalidated_evidence"]
    assert mm.change_plan(spec(), "order")["waves"] == [["inventory"], ["checkout"]]  # lineage: order-stock-projection is derived from order


def test_plan_cli(capsys):
    assert mm.main([str(EXAMPLE), "--plan", "reserve-inventory", "--json"]) == 0
    assert json.loads(capsys.readouterr().out)["waves"] == [["inventory"], ["checkout"]]
    assert mm.main([str(EXAMPLE), "--plan", "ghost"]) == 1


def faked_spec(contract=True, passed=True):
    s = spec(); s.components[0].fakes = {"reserve-inventory": "tests/checkout/fakes/reserve.py"}
    if contract:
        s.requirements.append(mm.Requirement(id="req-reserve-contract", version="1.0", status="verified", category="contract", target="reserve-inventory", criteria=["409 on insufficient_inventory"]))
        s.evidence.append(mm.Evidence(id="ev-reserve-contract", requirement_id="req-reserve-contract", requirement_version="1.0", method="test", passed=passed,
                                      provenance="pytest::test_reserve_contract", subject_revision="1.0", timestamp="2026-10-30", verifier="ci", environment="ci/ubuntu"))
    return s


def test_faked_capability_needs_a_verified_provider_contract_requirement():
    assert not any("COMP-10" in e for e in mm.validate_spec(faked_spec()))
    errors = [e for e in mm.validate_spec(faked_spec(contract=False)) if "COMP-10" in e]
    assert len(errors) == 1 and "reserve-inventory" in errors[0] and "checkout" in errors[0]
    assert any("COMP-10" in e for e in mm.validate_spec(faked_spec(passed=False)))
    assert not any("COMP-10" in e for e in mm.validate_spec(faked_spec(contract=False), "verification"))


def test_evidence_environment_is_optional():
    assert spec().evidence[0].environment is None


def split_spec(tmp_path, root_extra=None, fragments=None):
    data = yaml.safe_load(EXAMPLE.read_text(encoding="utf-8"))
    data.setdefault("data_models", []); [data.setdefault(k, {} if k == "subject_bindings" else []) for k in ("requirements", "risks", "evidence", "subject_bindings")]
    parts = tmp_path / "parts"; parts.mkdir(exist_ok=True)
    def dump(p, d): p.write_text(yaml.safe_dump(d))
    chk = {"components": [c for c in data["components"] if c["id"] == "checkout"], "capabilities": [c for c in data["capabilities"] if c["id"] == "place-order"]}
    inv = {"components": [c for c in data["components"] if c["id"] == "inventory"], "capabilities": [c for c in data["capabilities"] if c["id"] == "reserve-inventory"]}
    rest = {k: data[k] for k in ("data_models", "requirements", "risks", "evidence", "subject_bindings")}
    dump(parts / "checkout.yaml", chk); dump(parts / "inventory.yaml", inv); dump(parts / "rest.yaml", rest)
    for name, d in (fragments or {}).items(): dump(parts / name, d)
    root = {"system": data["system"], "capabilities": [], "components": [], "include": ["parts/*.yaml"], **(root_extra or {})}
    dump(tmp_path / "spec.yaml", root)
    return tmp_path / "spec.yaml"


def test_include_merges_fragments_into_the_same_spec(tmp_path):
    merged = mm.load_spec(split_spec(tmp_path))
    original = spec().model_dump()
    got = merged.model_dump()
    assert got.pop("include") == [] and original.pop("include") == []
    key = lambda xs: sorted(xs, key=lambda x: x["id"])
    for k in ("capabilities", "components", "data_models", "requirements", "risks", "evidence"): assert key(got.pop(k)) == key(original.pop(k)), k
    assert got == original
    assert mm.validate_spec(merged) == []


def test_include_rejects_bad_fragments_and_patterns(tmp_path):
    with pytest.raises(mm.IncludeError, match="bad.yaml"): mm.load_spec(split_spec(tmp_path, fragments={"bad.yaml": {"system": {"id": "x", "version": "1"}}}))
    (tmp_path / "parts" / "bad.yaml").unlink()
    with pytest.raises(mm.IncludeError, match="nest.yaml"): mm.load_spec(split_spec(tmp_path, fragments={"nest.yaml": {"include": ["x.yaml"]}}))
    (tmp_path / "parts" / "nest.yaml").unlink()
    for bad in ("../outside.yaml", "/abs.yaml", "C:/abs.yaml", "missing.yaml"):
        with pytest.raises(mm.IncludeError): mm.load_spec(split_spec(tmp_path, {"include": [bad]}))
    assert mm.load_spec(split_spec(tmp_path, {"include": ["parts/*.yaml", "none/*.yaml"]})).components


def test_include_duplicate_subject_binding_and_duplicate_ids(tmp_path):
    b = {"place-order": {"version": "1.0", "revision": "r", "artifacts": {}}}
    root = split_spec(tmp_path, fragments={"b1.yaml": {"subject_bindings": b}, "b2.yaml": {"subject_bindings": b}})
    with pytest.raises(mm.IncludeError, match="place-order"): mm.load_spec(root)
    (tmp_path / "parts/b2.yaml").unlink(); (tmp_path / "parts/b1.yaml").unlink()
    root = split_spec(tmp_path, fragments={"dup.yaml": {"components": [yaml.safe_load(EXAMPLE.read_text(encoding="utf-8"))["components"][0]]}})
    assert any("Duplicate component id" in e for e in mm.validate_spec(mm.load_spec(root)))


def test_include_cli_and_fragment_schema(tmp_path, capsys):
    root = split_spec(tmp_path)
    assert mm.main([str(root)]) == 0
    assert mm.main([str(split_spec(tmp_path, {"include": ["nope.yaml"]}))]) == 1 and "nope.yaml" in capsys.readouterr().out
    assert mm.main(["--fragment-schema"]) == 0 and json.loads(capsys.readouterr().out) == mm.fragment_json_schema()
    assert json.loads((Path(__file__).parent / "fragment.schema.json").read_text()) == mm.fragment_json_schema()
    assert "system" not in mm.fragment_json_schema()["properties"] and "include" not in mm.fragment_json_schema()["properties"]

def test_drift_integration_tests_external_components_and_via(tmp_path):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n[dev-dependencies]\ninventory = { path = "../inventory" }\n')
    s.components[0].test_scope = "integration"
    assert mm.check_drift(s, tmp_path) == {"errors": [], "unchecked": []}
    s.components[0].requires = []
    assert any("tests of 'checkout' depend on 'inventory'" in e and "without a required capability" in e for e in mm.check_drift(s, tmp_path)["errors"])
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n[dependencies]\ninventory = { path = "../inventory" }\n')
    (tmp_path / "crates/inventory/Cargo.toml").write_text('[package]\nname = "inventory"\n[dev-dependencies]\ncheckout = { path = "../checkout" }\n')
    s.components[1].external, s.components[0].via = True, {"reserve-inventory": "x"}
    assert mm.check_drift(s, tmp_path) == {"errors": [], "unchecked": ["inventory: external, own sources not scanned", "checkout: 'reserve-inventory' via 'x' (runtime-mediated)"]}


def test_external_components_provide_only():
    s = spec(); c = s.components[1]; c.external, c.layer = True, None
    assert mm.validate_spec(s, "architecture") == []
    c.requires, c.test_paths = ["reserve-inventory"], ["tests"]
    assert any("COMP-11" in e and "requires, test_paths" in e for e in mm.validate_spec(s, "architecture"))


def js_repo(tmp_path, files):
    for rel, text in files.items():
        (tmp_path / rel).parent.mkdir(parents=True, exist_ok=True); (tmp_path / rel).write_text(text)
    s = spec(); s.components[0].source_paths, s.components[1].source_paths = ["src/checkout"], ["src/inventory"]
    return s


def test_drift_js_ts_relative_imports_follow_requires(tmp_path):
    s = js_repo(tmp_path, {"src/checkout/app.ts": "import { reserve } from '../inventory/api';\nexport * from '../inventory/api'\n", "src/inventory/api.ts": "export const reserve = 1;\n"})
    assert mm.check_drift(s, tmp_path)["errors"] == []
    (tmp_path / "src/inventory/api.ts").write_text("const c = require('../checkout/app');\n")
    (tmp_path / "src/checkout/app.ts").write_text("export const x = 1;\n")
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert len(errors) == 1 and "'inventory' imports 'checkout'" in errors[0] and "src/inventory/api.ts -> src/checkout/app.ts" in errors[0]


def test_drift_js_ts_package_names_dynamic_imports_and_comments(tmp_path):
    s = js_repo(tmp_path, {
        "src/checkout/package.json": '{"name": "@acme/checkout"}', "src/inventory/package.json": '{"name": "@acme/inventory"}',
        "src/inventory/api.ts": "// import x from '@acme/checkout'\n/* import y from '@acme/checkout' */\nconst z = await import('@acme/checkout/lib');\nimport 'left-pad';\n"})
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert len(errors) == 1 and "'inventory' imports 'checkout'" in errors[0] and "@acme/checkout" not in "".join(e for e in errors if "left-pad" in e)
    (tmp_path / "src/inventory/api.ts").write_text("// import x from '@acme/checkout'\nimport 'left-pad';\n")
    assert mm.check_drift(s, tmp_path)["errors"] == []


def test_drift_js_ts_tests_and_node_modules(tmp_path):
    s = js_repo(tmp_path, {"src/checkout/app.ts": "export const a = 1;\n", "src/inventory/api.ts": "export const b = 1;\n",
                           "src/checkout/node_modules/dep/index.js": "require('../../../inventory/api');\n",
                           "tests/checkout/app.test.ts": "import { b } from '../../src/inventory/api';\n"})
    s.components[0].test_paths = ["tests/checkout"]
    errors = mm.check_drift(s, tmp_path)["errors"]
    assert len(errors) == 1 and "tests of 'checkout' import 'inventory'" in errors[0] and "app.test.ts" in errors[0]


CHECKOUT_REQS = ["req-order", "req-order-contract", "req-order-serial", "req-resilience", "req-security"]


def handoff(**over):
    base = {"component": "checkout", "base": "abc123",
            "results": [{"requirement": r, "requirement_version": "1.0", "method": "test", "passed": True, "provenance": "tests/checkout/test_x.py::t", "environment": "ci"} for r in CHECKOUT_REQS],
            "changed": ["src/checkout/app.py"], "findings": [{"kind": "assumption", "text": "inventory is idempotent"}]}
    return {**base, **over}


def ret_spec():
    s = spec(); s.components[0].source_paths, s.components[0].test_paths = ["src/checkout"], ["tests/checkout"]
    return s


def check(s, data, **kw): return mm.check_return(s, mm.HandoffReturn.model_validate(data), **kw)


def test_return_complete_and_in_scope_passes():
    assert check(ret_spec(), handoff()) == []


def test_return_reports_dropped_unknown_stale_and_unverifiable_results():
    s = ret_spec()
    dropped = handoff(results=handoff()["results"][1:])
    assert any("req-order" in e and "neither a result nor unmet" in e for e in check(s, dropped))
    assert any("ghost" in e for e in check(s, handoff(results=[*handoff()["results"], {**handoff()["results"][0], "requirement": "ghost"}])))
    stale = handoff(); stale["results"][0]["requirement_version"] = "0.9"
    assert any("version" in e and "req-order" in e for e in check(s, stale))
    skipped = handoff(); skipped["results"][0]["skipped"] = ["flaky"]
    assert any("skipped" in e for e in check(s, skipped))
    elsewhere = handoff(); elsewhere["results"][0]["provenance"] = "tests/inventory/test_x.py::t"
    assert any("outside test_paths" in e for e in check(s, elsewhere))
    assert any("req-latency" in e and "does not apply" in e for e in check(s, handoff(results=[*handoff()["results"], {**handoff()["results"][0], "requirement": "req-latency"}])))


def test_return_failures_must_be_listed_unmet_and_changes_stay_in_scope():
    s = ret_spec()
    failing = handoff(); failing["results"][0]["passed"] = False
    assert any("failing" in e and "unmet" in e for e in check(s, failing))
    failing["unmet"] = [{"requirement": "req-order", "reason": "409 path not implemented"}]
    assert check(s, failing) == []
    assert any("src/inventory/api.py" in e for e in check(s, handoff(changed=["src/inventory/api.py"])))
    assert check(s, handoff(changed=["uv.lock"]), allow=["uv.lock"]) == []
    assert any("unknown component" in e for e in check(s, handoff(component="ghost")))
    with pytest.raises(Exception): mm.HandoffReturn.model_validate(handoff(unmet=[{"requirement": "req-order", "reason": ""}]))


def test_return_cli_and_schema(tmp_path, capsys):
    s = ret_spec(); path = tmp_path / "spec.yaml"; path.write_text(json.dumps(s.model_dump(mode="json")))
    good = tmp_path / "return.yaml"; good.write_text(yaml.safe_dump(handoff()))
    assert mm.main([str(path), "--check-return", str(good), "--json"]) == 0
    assert json.loads(capsys.readouterr().out)["findings"][0]["kind"] == "assumption"
    bad = tmp_path / "bad.yaml"; bad.write_text(yaml.safe_dump(handoff(changed=["README.md"])))
    assert mm.main([str(path), "--check-return", str(bad)]) == 1
    bad.write_text("component: checkout\nnope: 1\n")
    assert mm.main([str(path), "--check-return", str(bad)]) == 1
    capsys.readouterr()
    assert mm.main(["--return-schema"]) == 0 and json.loads(capsys.readouterr().out) == mm.return_json_schema()
    assert json.loads((Path(__file__).parent / "handoff-return.schema.json").read_text()) == mm.return_json_schema()


def criteria_spec(*records):
    """req-order with two criteria; its single example evidence replaced by per-criterion records (criteria, passed)."""
    s = spec()
    r = next(x for x in s.requirements if x.id == "req-order"); r.criteria = ["a", "b"]
    base = next(e for e in s.evidence if e.requirement_id == "req-order")
    s.evidence = [e for e in s.evidence if e is not base]
    for i, (crit, passed) in enumerate(records):
        s.evidence.append(base.model_copy(update={"id": f"ev-order-{i}", "criteria": crit, "passed": passed}))
    return s


def test_evidence_criteria_must_cover_every_requirement_criterion():
    assert not any("req-order" in e for e in mm.validate_spec(criteria_spec((["a"], True), (["b"], True))))
    errs = [e for e in mm.validate_spec(criteria_spec((["a"], True))) if "[REQ-05]" in e and "req-order@" in e]
    assert len(errs) == 1 and "'b'" in errs[0] and "'a'" not in errs[0]
    assert not any("req-order@" in e for e in mm.validate_spec(criteria_spec(([], True))))
    assert not any("req-order@" in e for e in mm.validate_spec(criteria_spec(([], True), (["b"], True))))


def test_failing_criterion_is_named_and_stays_blocking():
    errs = [e for e in mm.validate_spec(criteria_spec((["a"], True), (["b"], False))) if "req-order@" in e]
    assert len(errs) == 1 and "[REQ-04]" in errs[0] and "'b'" in errs[0] and "'a'" not in errs[0]


def test_evidence_criteria_must_name_requirement_criteria():
    errs = mm.validate_spec(criteria_spec((["a", "zzz"], True), (["b"], True)), "inventory")
    assert any("[EV-06]" in e and "zzz" in e for e in errs)
    assert not any("[EV-06]" in e for e in mm.validate_spec(criteria_spec((["a"], True), (["b"], True)), "inventory"))


def test_return_results_can_cover_criteria_individually():
    s = ret_spec(); next(r for r in s.requirements if r.id == "req-order").criteria = ["a", "b"]
    data = handoff(); first = data["results"][0]
    data["results"] = [{**first, "criteria": ["a"]}, {**first, "criteria": ["b"]}, *data["results"][1:]]
    assert check(s, data) == []
    data["results"] = [{**first, "criteria": ["a"]}, *handoff()["results"][1:]]
    assert any("req-order" in e and "'b'" in e and "not covered" in e for e in check(s, data))
    data["unmet"] = [{"requirement": "req-order", "reason": "b pending"}]
    assert check(s, data) == []
    dup = handoff(); dup["results"] = [{**first, "criteria": ["a"]}, {**first, "criteria": ["a"]}, *handoff()["results"][1:]]
    assert any("duplicate" in e for e in check(s, dup))
    bad = handoff(); bad["results"][0]["criteria"] = ["zzz"]
    assert any("zzz" in e for e in check(s, bad))


def import_spec():
    """example.yaml with an external platform component that provides platform:run-module, pinned from another spec."""
    s = spec()
    s.system.imports = [mm.SpecImport(id="platform", path="platform/spec.yaml", version="2.1", capabilities={"run-module": "2.0"})]
    s.components.append(mm.Component(id="platform", version="2.1", status="verified", external=True, provides=["platform:run-module"]))
    s.components[0].requires.append("platform:run-module@^2.0")
    return s


def test_imported_capabilities_resolve_without_copying_contracts():
    s = import_spec()
    assert not [e for e in mm.validate_spec(s, "architecture") if "platform" in e]
    assert mm.resolve_capabilities(s)["checkout"]["platform:run-module"] == "platform"
    s.system.imports[0].capabilities["run-module"] = "1.0"
    assert any("unresolved capability 'platform:run-module@^2.0'" in e for e in mm.validate_spec(s, "architecture"))


def test_imports_must_be_unique_and_qualified_references_must_resolve():
    s = import_spec(); s.system.imports.append(s.system.imports[0].model_copy())
    assert any("[IMP-01]" in e and "Duplicate import" in e for e in mm.validate_spec(s, "inventory"))
    s = import_spec(); s.components[0].requires.append("nowhere:thing@^1.0")
    assert any("[IMP-01]" in e and "nowhere:thing" in e for e in mm.validate_spec(s, "inventory"))
    s = import_spec(); s.components[1].provides.append("platform:run-module")
    assert any("[IMP-01]" in e and "external" in e for e in mm.validate_spec(s, "inventory"))
    s = import_spec(); s.components[-1].provides.append("platform:missing")
    assert any("[IMP-01]" in e and "platform:missing" in e for e in mm.validate_spec(s, "inventory"))


def test_check_imports_verifies_pins_against_the_imported_spec(tmp_path):
    other = spec().model_dump(mode="json")
    other["system"].update(id="platform", version="2.1")
    other["capabilities"][0].update(id="run-module", version="2.0"); other["components"][0]["provides"] = ["run-module"]
    (tmp_path / "platform").mkdir(); p = tmp_path / "platform" / "spec.yaml"; p.write_text(yaml.safe_dump(other))
    s = import_spec()
    assert mm.check_imports(s, tmp_path, tmp_path) == []
    s.system.imports[0].version = "3.0"
    assert any("version" in e and "3.0" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    s = import_spec(); s.system.imports[0].capabilities = {"run-module": "9.9", "gone": "1.0"}
    errors = mm.check_imports(s, tmp_path, tmp_path)
    assert any("run-module" in e and "9.9" in e for e in errors) and any("gone" in e for e in errors)
    s = import_spec(); s.system.imports[0].sha256 = "0" * 64
    assert any("sha256" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    s.system.imports[0].sha256 = __import__("hashlib").sha256(p.read_bytes()).hexdigest()
    assert mm.check_imports(s, tmp_path, tmp_path) == []
    s = import_spec(); s.system.imports[0].path = "../outside.yaml"
    assert any("outside" in e or "unsafe" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    s = import_spec(); s.system.imports[0].path = "absent/spec.yaml"
    assert any("cannot load" in e for e in mm.check_imports(s, tmp_path, tmp_path))


def test_check_imports_cli_requires_root(tmp_path):
    with pytest.raises(SystemExit) as e: mm.main([str(EXAMPLE), "--check-imports"])
    assert e.value.code == 2


def test_backstage_export_maps_components_apis_resources_and_dependencies():
    docs = mm.export_backstage(spec(), owner="team-a")
    by = {(d["kind"], d["metadata"]["name"]): d for d in docs}
    assert by[("System", "commerce-system")]["spec"]["owner"] == "team-a"
    chk = by[("Component", "checkout")]["spec"]
    assert chk["system"] == "commerce-system" and chk["owner"] == "team-a" and chk["lifecycle"] == "production"
    assert chk["providesApis"] == ["place-order"] and chk["consumesApis"] == ["reserve-inventory"] and chk["dependsOn"] == ["component:inventory"]
    assert by[("API", "place-order")]["spec"]["type"] == "openapi" and by[("API", "reserve-inventory")]["spec"]["owner"] == "team-a"
    assert by[("Resource", "order")]["spec"]["owner"] == "team-a" and by[("Resource", "order")]["spec"]["dependsOn"] == []
    assert by[("Resource", "order-stock-projection")]["spec"]["dependsOn"] == ["resource:order", "resource:stock-item"]
    assert yaml.safe_load_all(yaml.safe_dump_all(docs)) is not None
    assert mm.export_backstage(spec())[0]["spec"]["owner"] == "unknown"


def test_backstage_external_and_lifecycle():
    s = import_spec(); s.components[0].status = "draft"
    docs = {(d["kind"], d["metadata"]["name"]): d for d in mm.export_backstage(s)}
    assert docs[("Component", "platform")]["spec"]["type"] == "external" and docs[("Component", "checkout")]["spec"]["lifecycle"] == "experimental"
    assert ("API", "platform-run-module") in docs and docs[("Component", "checkout")]["spec"]["consumesApis"][-1] == "platform-run-module"


def test_structurizr_export_has_containers_externals_and_relationships():
    dsl = mm.export_structurizr(import_spec())
    assert dsl.startswith("workspace {") and dsl.rstrip().endswith("}")
    assert 'softwareSystem "commerce-system"' in dsl and 'container "checkout"' in dsl and '"application"' in dsl
    assert 'softwareSystem "platform"' in dsl and "External" in dsl
    assert "checkout -> inventory" in dsl and "checkout -> platform" in dsl and "reserve-inventory" in dsl
    assert dsl.count("{") == dsl.count("}")


def test_export_cli(capsys):
    assert mm.main([str(EXAMPLE), "--export", "backstage", "--owner", "team-a"]) == 0
    assert "kind: Component" in capsys.readouterr().out
    assert mm.main([str(EXAMPLE), "--export", "structurizr"]) == 0
    assert capsys.readouterr().out.startswith("workspace {")
    with pytest.raises(SystemExit) as e: mm.main([str(EXAMPLE), "--export", "nope"])
    assert e.value.code == 2
