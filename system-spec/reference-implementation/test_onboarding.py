import datetime
import hashlib
import json
import re
import shutil
import subprocess
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
    assert mm.check_artifacts(s, tmp_path)[0].endswith(f"result.txt' (evidence '{s.evidence[0].id}')")
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


def ws_tree(tmp_path, links=("https://git.example/platform", "https://git.example/platform")):
    plat = spec().model_dump(mode="json"); plat["system"].update(id="platform", version="2.1")
    plat["capabilities"][0].update(id="run-module", version="2.0"); plat["components"][0]["provides"] = ["run-module"]
    plat["requirements"] = plat["risks"] = plat["evidence"] = []
    app = import_spec(); app.system.imports[0].link = links[0]
    for n, d in (("platform", plat), ("app", app.model_dump(mode="json"))):
        (tmp_path / n).mkdir(); (tmp_path / n / "spec.yaml").write_text(yaml.safe_dump(d))
    w = {"id": "ws", "members": [{"id": "platform", "path": "platform/spec.yaml", "link": links[1]}, {"id": "app", "path": "app/spec.yaml"}]}
    (tmp_path / "workspace.yaml").write_text(yaml.safe_dump(w)); return tmp_path / "workspace.yaml", w


def test_workspace_checks_members_imports_links_and_cycles(tmp_path):
    f, w = ws_tree(tmp_path)
    app = tmp_path / "app" / "spec.yaml"; d = yaml.safe_load(app.read_text()); d["system"]["imports"][0]["path"] = "../platform/spec.yaml"; app.write_text(yaml.safe_dump(d))
    assert mm.check_workspace(f, "inventory") == []
    d["system"]["imports"][0]["link"] = "https://git.example/other.git"; app.write_text(yaml.safe_dump(d))
    assert any("link" in e for e in mm.check_workspace(f, "inventory"))
    d["system"]["imports"][0]["link"] = "https://git.example/platform.git/"; app.write_text(yaml.safe_dump(d))
    assert mm.check_workspace(f, "inventory") == []
    d["system"]["imports"][0]["path"] = "../elsewhere/spec.yaml"; app.write_text(yaml.safe_dump(d))
    assert any("not a workspace member" in e or "cannot load" in e for e in mm.check_workspace(f, "inventory"))
    d["system"]["imports"][0]["path"] = "../platform/spec.yaml"; d["system"]["id"] = "platform"; app.write_text(yaml.safe_dump(d))
    assert any("more than one member" in e for e in mm.check_workspace(f, "inventory"))
    plat = tmp_path / "platform" / "spec.yaml"; p = yaml.safe_load(plat.read_text()); d["system"]["id"] = "app"; app.write_text(yaml.safe_dump(d))
    p["system"]["imports"] = [{"id": "app", "path": "../app/spec.yaml", "version": d["system"]["version"]}]; plat.write_text(yaml.safe_dump(p))
    assert any("cycle" in e for e in mm.check_workspace(f, "inventory"))
    w["members"][0]["path"] = "gone/spec.yaml"; f.write_text(yaml.safe_dump(w))
    assert any("clone https://git.example/platform" in e for e in mm.check_workspace(f, "inventory"))
    w["members"][0]["path"] = "../x.yaml"; f.write_text(yaml.safe_dump(w))
    assert any("unsafe" in e for e in mm.check_workspace(f, "inventory"))


def test_workspace_cli(tmp_path, capsys):
    f, _ = ws_tree(tmp_path)
    app = tmp_path / "app" / "spec.yaml"; d = yaml.safe_load(app.read_text()); d["system"]["imports"][0]["path"] = "../platform/spec.yaml"; app.write_text(yaml.safe_dump(d))
    assert mm.main([str(f), "--workspace", "--stage", "inventory"]) == 0 and capsys.readouterr().out.strip() == "VALID"
    d["system"]["imports"][0]["version"] = "9.9"; app.write_text(yaml.safe_dump(d))
    assert mm.main([str(f), "--workspace", "--stage", "inventory", "--json"]) == 1 and "app:" in capsys.readouterr().out


def test_hierarchical_subsystem_realized_by_exports_and_requirements(tmp_path):
    # Single-spec validation of exports, realized_by, and cross-spec requirements
    s = spec()
    s.system.exports = ["place-order", "ghost-cap"]
    assert any("[REF-03]" in e and "ghost-cap" in e for e in mm.validate_spec(s, "inventory"))
    s.system.exports = ["place-order"]
    s.components[0].realized_by = "missing-imp"
    assert any("[IMP-01]" in e and "missing-imp" in e for e in mm.validate_spec(s, "inventory"))
    s.system.imports = [mm.SpecImport(id="checkout-sub", path="sub/spec.yaml", version="1.0")]
    s.components[0].realized_by = "checkout-sub"
    s.components[0].external = True
    assert any("[COMP-11]" in e and "realized_by" in e for e in mm.validate_spec(s, "architecture"))
    s.components[0].external = False
    s.requirements[0].depends_on = ["ghost:req-x"]
    assert any("[REF-06]" in e and "ghost:req-x" in e for e in mm.validate_spec(s, "inventory"))
    s.requirements[0].depends_on = ["checkout-sub:req-order"]
    assert mm.validate_spec(s, "release") == []
    assert "subsystem" in mm.diagram(s)

    # Child spec realizing checkout component
    child = spec()
    child.system.id = "checkout-subsystem"
    for x in [*child.requirements, *child.risks]:
        if x.target == "commerce-system": x.target = "checkout-subsystem"
    child.system.exports = ["place-order"]
    # In child, reserve-inventory is an external dependency provided by an external component
    inv_comp = next(c for c in child.components if c.id == "inventory")
    inv_comp.external = True; inv_comp.layer = inv_comp.domain = None; inv_comp.owns_data = []; inv_comp.requires = []
    for d in child.data_models:
        if d.owner == "inventory": d.owner = "checkout"; child.components[0].owns_data.append(d.id)
    (tmp_path / "sub").mkdir()
    sub_path = tmp_path / "sub" / "spec.yaml"
    sub_path.write_text(yaml.safe_dump(child.model_dump(mode="json")))
    assert mm.check_imports(s, tmp_path, tmp_path) == []

    # Export surface violation
    child.system.exports = ["reserve-inventory"]
    sub_path.write_text(yaml.safe_dump(child.model_dump(mode="json")))
    assert any("does not provide and export capability 'place-order'" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    child.system.exports = ["place-order"]

    # Undeclared external requirement in child (child requires extra external capability not in parent component.requires)
    child.capabilities.append(child.capabilities[0].model_copy(update={"id": "secret-dep"}))
    inv_comp.provides.append("secret-dep")
    child.components[0].requires.append("secret-dep@^1.0")
    sub_path.write_text(yaml.safe_dump(child.model_dump(mode="json")))
    assert any("requires external capability 'secret-dep'" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    inv_comp.provides.pop(); child.components[0].requires.pop(); child.capabilities.pop()

    # Contract & data compatibility across parent-child boundary
    child.capabilities[0].preconditions.append("extra_precondition")
    sub_path.write_text(yaml.safe_dump(child.model_dump(mode="json")))
    assert any("[COMPAT-01]" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    child.capabilities[0].preconditions.pop()
    child.data_models[1].fields.pop("status")
    sub_path.write_text(yaml.safe_dump(child.model_dump(mode="json")))
    assert any("[COMPAT-05]" in e for e in mm.check_imports(s, tmp_path, tmp_path))
    child.data_models[1].fields["status"] = "string"

    # Cross-level requirement verification & child release maturity
    child.evidence = [e for e in child.evidence if e.requirement_id != "req-order"]
    sub_path.write_text(yaml.safe_dump(child.model_dump(mode="json")))
    errs = mm.check_imports(s, tmp_path, tmp_path)
    assert any("requirement 'req-order'" in e and "not verified" in e for e in errs)
    assert any("not release-verified" in e for e in errs)
    # Deferring the parent requirement/component via release_scope allows unverified child
    s.requirements[0].priority = "P1"; s.system.release_scope = ["P0"]
    for r in s.requirements:
        if r.target in ("checkout", "place-order", "order"): r.priority = "P1"
    assert mm.check_imports(s, tmp_path, tmp_path) == []

    # Workspace per-member stage override
    (tmp_path / "parent.yaml").write_text(yaml.safe_dump(s.model_dump(mode="json")))
    ws = tmp_path / "workspace.yaml"
    ws.write_text(yaml.safe_dump({"id": "ws", "members": [{"id": "parent", "path": "parent.yaml"}, {"id": "sub", "path": "sub/spec.yaml", "stage": "verification"}]}))
    assert mm.check_workspace(ws, "release") == []



def test_drift_cargo_path_dependency_on_unowned_path_is_unchecked(tmp_path):
    s = drift_repo(tmp_path, "x = 1\n")
    (tmp_path / "src/checkout/Cargo.toml").write_text('[dependencies]\nlib = { path = "../lib" }\n')
    r = mm.check_drift(s, tmp_path)
    assert r["errors"] == [] and "checkout: 'src/lib' not owned by any component" in r["unchecked"]

def test_drift_dev_dependency_repeating_a_normal_dependency_is_not_a_test_edge(tmp_path):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n[dependencies]\ninventory = { path = "../inventory" }\n[dev-dependencies]\ninventory = { path = "../inventory" }\n')
    assert mm.check_drift(s, tmp_path)["errors"] == []

def test_test_requires_satisfies_drift_for_tests_only_and_skips_architecture_rules(tmp_path):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n[dev-dependencies]\ninventory = { path = "../inventory" }\n')
    c = s.components[0]
    c.test_scope, c.requires, c.test_requires = "integration", [], ["reserve-inventory"]
    assert mm.check_drift(s, tmp_path)["errors"] == []
    assert "inventory" not in mm.resolve_capabilities(s)["checkout"].values()
    assert not any("COMP-0" in e and "checkout" in e for e in mm.validate_spec(s, "architecture"))
    c.test_requires = ["nope"]
    assert any("COMP-04" in e and "test_requires" in e for e in mm.validate_spec(s, "architecture"))


def test_ambiguous_target_names_the_kinds():
    s = spec(); s.data_models[0].id = s.components[0].id; s.requirements[0].target = s.components[0].id
    assert any("[REF-02]" in e and "component, data model" in e and "rename one" in e for e in mm.validate_spec(s, "inventory"))


def test_diagram_modes_and_reports_are_stable_and_complete(capsys):
    s = spec(); a = mm.diagram(s)
    s.components.reverse(); s.data_models.reverse(); s.capabilities.reverse()
    assert mm.diagram(s) == a
    h = mm.diagram(s, "hierarchy")
    assert all(f'"{c.id}<br>' in h for c in s.components) and 'commerce-system<br>3 req' in h
    r = mm.diagram(s, "risks")
    assert "risk_oversell -.->|mitigated_by| r_req_reserve" in r
    assert "| risk-oversell | inventory | mitigation | req-reserve |" in mm.report(s, "risks")
    cov = mm.report(s, "coverage")
    assert "| checkout |" in cov and "| **Total** |" in cov and "| place-order |" not in cov and cov.splitlines()[2].startswith("| Component |")
    assert cov.startswith("### By category") and "### By priority" not in cov and "### By mandatory" not in cov and 'checkout<br>5 req incl.' in h and "a_place_order" in h.splitlines()[-2]
    assert mm.main([str(EXAMPLE), "--diagram", "hierarchy"]) == 0 and "req" in capsys.readouterr().out
    assert mm.main([str(EXAMPLE), "--report", "risks"]) == 0 and "| Risk |" in capsys.readouterr().out
    assert mm.main([str(EXAMPLE), "--report", "coverage", "--json"]) == 0 and json.loads(capsys.readouterr().out)["category"]["checkout"]["Total"] == 5
    s.requirements[0].priority = "P0"; assert "### By priority" in mm.report(s, "coverage") and list(mm.report_data(s, "coverage")) == ["category", "priority"]
    s.requirements[0].mandatory = False; assert list(mm.report_data(s, "coverage")) == ["category", "priority", "mandatory"]


def test_proposed_requirements_pass_verification_but_are_deferred_at_release(capsys):
    s = spec(); r = s.requirements[0]
    s.evidence = [e for e in s.evidence if e.requirement_id != r.id]; r.status = "specified"
    assert any(r.id in e for e in mm.validate_spec(s, "release"))
    r.proposed = True
    assert mm.validate_spec(s, "verification") == [] and r.id in mm.deferred_scope(s)["requirements"]
    assert not any(f"'{r.id}'" in e for e in mm.validate_spec(s, "release"))
    assert mm.main([str(EXAMPLE), "--json"]) == 0 and json.loads(capsys.readouterr().out)["proposed"] == []


def sysreq(**kw):
    return mm.Requirement(id="req-validate", version="1.0", target="commerce-system", status="verified", category="security", kind="non-functional",
                          criteria=["rejects invalid input"], applies_to=mm.Selector(kind="capability", has_interface=True), **kw)


def subj_ev(cap, ok=True, n=None):
    return mm.Evidence(id=f"ev-validate-{n or cap}", requirement_id="req-validate", requirement_version="1.0", method="test", passed=ok, provenance=f"pytest::test_validate[{cap}]", subject_id=cap)


def test_applies_to_requires_evidence_per_selected_entity():
    s = spec(); r = sysreq(); s.requirements.append(r)
    sel = mm._applies(s, r)
    assert sel == sorted(c.id for c in s.capabilities if c.interface) and len(sel) >= 2
    key = ("req-validate", "1.0")
    assert mm._evidence_state(s)[key] == (False, [], sel)
    s.evidence += [subj_ev(c) for c in sel[:-1]]
    ok, failing, unc = mm._evidence_state(s)[key]
    assert not ok and unc == [sel[-1]]
    s.evidence.append(subj_ev(sel[-1])); assert mm._evidence_state(s)[key][0]
    s.evidence.append(subj_ev(sel[0], ok=False, n="bad")); assert not mm._evidence_state(s)[key][0]
    rep = next(x for x in mm.evidence_report(s) if x["requirement"] == "req-validate")
    assert rep["applies_to"] == sel and not rep["verified"]


def test_applies_to_new_entity_is_uncovered_and_filters_and_vacuity():
    s = spec(); r = sysreq(); s.requirements.append(r); sel = mm._applies(s, r)
    s.evidence += [subj_ev(c) for c in sel]; assert mm._evidence_state(s)[("req-validate", "1.0")][0]
    extra = s.capabilities[0].model_copy(update={"id": "new-cap"}); s.capabilities.append(extra)
    assert mm._evidence_state(s)[("req-validate", "1.0")][2] == ["new-cap"]
    r.applies_to = mm.Selector(kind="component", external=True)
    assert mm._applies(s, r) == [] and mm._evidence_state(s)[("req-validate", "1.0")][0]
    r.applies_to = mm.Selector(kind="component", layer="domain"); assert mm._applies(s, r) == ["inventory"]
    r.applies_to = mm.Selector(kind="capability", layer="domain"); assert mm._applies(s, r) == ["reserve-inventory"]


def test_applies_to_validity_rules():
    ids = lambda errs: {e[1:e.index("]")] for e in errs}
    s = spec(); r = sysreq(); r.target = "checkout"; s.requirements.append(r)
    assert "REQ-11" in ids(mm.validate_spec(s, "inventory"))
    r.target = "commerce-system"; r.applies_to = mm.Selector(kind="component", has_interface=True)
    assert "REQ-11" in ids(mm.validate_spec(s, "inventory"))
    r.applies_to = mm.Selector(kind="capability", has_interface=True); assert "REQ-11" not in ids(mm.validate_spec(s, "inventory"))


def test_drift_dependency_graph_file(tmp_path, capsys):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n')
    g = tmp_path / "deps.yaml"
    graph = {"packages": [{"name": "checkout", "path": "crates/checkout"}, {"name": "inventory", "path": "crates/inventory"}, {"name": "tool", "path": "tools/x"}],
             "edges": [{"from": "checkout", "to": "inventory"}, {"from": "checkout", "to": "ghost"}]}
    g.write_text(yaml.safe_dump(graph))
    d = mm.check_drift(s, tmp_path, g)
    assert d["errors"] == [] and any("package 'tool'" in u for u in d["unchecked"]) and any("unknown package" in u for u in d["unchecked"])
    s.components[0].requires = []
    assert any("component 'checkout' depends on 'inventory' (checkout -> inventory)" in e for e in mm.check_drift(s, tmp_path, g)["errors"])
    graph["edges"] = [{"from": "checkout", "to": "inventory", "kind": "dev"}]; g.write_text(yaml.safe_dump(graph))
    assert any("tests of 'checkout' depend on 'inventory'" in e for e in mm.check_drift(s, tmp_path, g)["errors"])
    s.components[0].test_requires = ["reserve-inventory"]; s.components[0].test_scope = "integration"
    assert mm.check_drift(s, tmp_path, g)["errors"] == []
    g.write_text("edges: [{from: a, to: b, kind: weird}]")
    assert any("cannot read dependency graph" in e for e in mm.check_drift(s, tmp_path, g)["errors"])
    assert mm.main([str(EXAMPLE), "--check-drift", "--root", str(tmp_path), "--deps", str(g)]) == 1


def test_check_ownership_flags_unowned_and_doubly_owned_packages(tmp_path, capsys):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n')
    for n in ("b", "x-spike"): (tmp_path / f"crates/{n}").mkdir(parents=True); (tmp_path / f"crates/{n}/Cargo.toml").write_text(f'[package]\nname = "{n}"\n')
    (tmp_path / "web").mkdir(); (tmp_path / "web/package.json").write_text('{"name": "web"}')
    (tmp_path / "target/y").mkdir(parents=True); (tmp_path / "target/y/Cargo.toml").write_text('[package]\nname = "built"\n')
    (tmp_path / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\n')
    assert mm.check_drift(s, tmp_path)["errors"] == []
    errs = mm.check_drift(s, tmp_path, ownership=True)["errors"]
    assert sorted(e.split("'")[1] for e in errs) == ["b", "web", "x-spike"] and all("owned by no component" in e and "checkout, inventory" in e for e in errs) and "built" not in "".join(errs)
    assert [e.split("'")[1] for e in mm.check_drift(s, tmp_path, ownership=True, ignore=["crates/*-spike", "web"])["errors"]] == ["b"]
    assert [e.split("'")[1] for e in mm.check_drift(s, tmp_path, ownership=True, ignore=["x-spike", "b"])["errors"]] == ["web"]
    ignore = ["b", "web", "x-spike"]
    s.components[1].source_paths = [*s.components[1].source_paths, "crates/checkout"]
    assert mm.check_drift(s, tmp_path, ownership=True, ignore=ignore)["errors"] == ["drift: package 'checkout' (crates/checkout) is owned by several components: checkout, inventory"]
    s.components[0].source_paths = ["crates"]  # a nested path is more specific: inventory owns crates/inventory and crates/checkout
    assert mm.check_drift(s, tmp_path, ownership=True, ignore=ignore)["errors"] == []


def test_check_ownership_counts_source_paths_not_test_paths(tmp_path):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n')
    s.components[0].test_paths, s.components[1].source_paths = ["crates/inventory"], []
    errs = mm.check_drift(s, tmp_path, ownership=True)["errors"]
    assert [e.split("'")[1] for e in errs] == ["inventory"] and "owned by no component" in errs[0] and "appears only in the test_paths of: checkout; test_paths do not confer ownership" in errs[0]
    s.components[1].source_paths = ["crates/inventory"]; s.components[0].test_paths = ["crates/inventory"]
    assert mm.check_drift(s, tmp_path, ownership=True)["errors"] == []


def test_check_ownership_across_several_specs(tmp_path, capsys):
    for n in ("orders", "payments", "shared-money", "new"): (tmp_path / f"crates/{n}").mkdir(parents=True); (tmp_path / f"crates/{n}/Cargo.toml").write_text(f'[package]\nname = "{n}"\n')
    mk = lambda name, *paths: (tmp_path / f"{name}.yaml").write_text(yaml.safe_dump({"system": {"id": f"{name}-system", "version": "1.0", "layers": ["domain"]}, "capabilities": [], "components": [{"id": name, "version": "1.0", "layer": "domain", "source_paths": list(paths)}]}))
    mk("orders", "crates/orders", "crates/shared-money"); mk("payments", "crates/payments", "crates/shared-money")
    root = ["--check-drift", "--root", str(tmp_path), "--check-ownership", "--json"]
    run = lambda *a: (mm.main([str(tmp_path / a[0]), *root, *a[1:]]), json.loads(capsys.readouterr().out)["errors"])
    rc, errs = run("orders.yaml"); assert rc == 1 and any("package 'payments' (crates/payments) is owned by no component" in e and "one of: orders" in e for e in errs)  # alone, the other spec's package is unowned
    rc, errs = run("orders.yaml", "--ownership-spec", str(tmp_path / "payments.yaml"))
    assert rc == 1 and any("package 'shared-money' (crates/shared-money) is owned by several components: orders:orders, payments:payments" in e for e in errs)
    assert sorted(e.split("'")[1] for e in errs if "owned by" in e) == ["new", "shared-money"]  # each spec's own packages are owned across specs
    assert any("package 'new' (crates/new) is owned by no component" in e and "one of: orders:orders, payments:payments" in e for e in errs)
    mk("orders", "crates/orders"); mk("payments", "crates/payments", "crates/new", "crates/shared-money")
    assert run("orders.yaml", "--ownership-spec", str(tmp_path / "payments.yaml"))[0] == 0 and run("payments.yaml", "--ownership-spec", str(tmp_path / "orders.yaml"))[0] == 0
    rc, errs = run("orders.yaml", "--ownership-spec", str(tmp_path / "nope.yaml")); assert rc == 1 and any("cannot load ownership spec" in e for e in errs)
    for argv in (["--ownership-spec", "x.yaml"], ["--check-drift", "--root", str(tmp_path), "--ownership-spec", "x.yaml"]):
        with pytest.raises(SystemExit): mm.main([str(tmp_path / "orders.yaml"), *argv])


def test_check_ownership_flags_paths_inside_other_packages_and_ignored_ones(tmp_path, capsys):
    for n in ("examples/app", "crates/ops", "crates/spike-x"): (tmp_path / n / "src").mkdir(parents=True); (tmp_path / n / "Cargo.toml").write_text(f'[package]\nname = "{n.split("/")[1]}"\n')
    mk = lambda name, *paths: (tmp_path / f"{name}.yaml").write_text(yaml.safe_dump({"system": {"id": f"{name}-system", "version": "1.0", "layers": ["domain"]}, "capabilities": [], "components": [{"id": name, "version": "1.0", "layer": "domain", "source_paths": list(paths)}]}))
    root = ["--check-drift", "--root", str(tmp_path), "--check-ownership", "--json", "--ownership-ignore", "crates/spike-x"]
    run = lambda spec, other: (mm.main([str(tmp_path / spec), *root, "--ownership-spec", str(tmp_path / other)]), json.loads(capsys.readouterr().out)["errors"])
    mk("app", "examples/app"); mk("framework", "crates/ops")
    assert run("framework.yaml", "app.yaml")[0] == 0 and run("app.yaml", "framework.yaml")[0] == 0
    mk("framework", "crates/ops", "examples/app/src")  # 1. a path inside another spec's package
    want = "source path 'examples/app/src' of 'framework:framework' lies inside package 'app' (examples/app), owned by app:app: overlapping ownership"
    assert [e for e in run("framework.yaml", "app.yaml")[1] if "overlapping" in e] == [f"drift: {want}"]
    assert [e.split("'")[1] for e in run("app.yaml", "framework.yaml")[1] if "overlapping" in e] == ["examples/app/src"]
    mk("framework", "crates/ops", "crates/spike-x/src")  # 2. a path inside an ignored package
    errs = run("framework.yaml", "app.yaml")[1]
    assert errs == ["drift: source path 'crates/spike-x/src' of 'framework:framework' lies inside package 'spike-x' (crates/spike-x), which --ownership-ignore exempts: a claim on an ignored package"]
    mk("framework", "crates/ops", "crates/ops/src"); assert run("framework.yaml", "app.yaml")[0] == 0  # inside a package the component itself owns is fine


def test_check_ownership_exempted_root_package_covers_only_its_own_directory(tmp_path, capsys):
    for n, f in (("crates/types", "Cargo.toml"), (".", "package.json")): (tmp_path / n).mkdir(parents=True, exist_ok=True); (tmp_path / n / f).write_text('[package]\nname = "types"\n' if f == "Cargo.toml" else '{"name": "docs-tools"}')
    (tmp_path / "models").mkdir()
    s = spec(); c = s.components[0]; c.source_paths = ["crates/types", "models"]; s.components[1].source_paths = []
    assert mm.check_drift(s, tmp_path, ownership=True, ignore=["."])["errors"] == []  # models/ is a plain directory outside every crate
    c.source_paths = ["crates/types", "."]
    assert mm.check_drift(s, tmp_path, ownership=True, ignore=["."])["errors"] == ["drift: source path '.' of 'checkout' lies inside package 'docs-tools' (.), which --ownership-ignore exempts: a claim on an ignored package"]
    c.source_paths = ["crates/types", "models"]
    assert any("package 'docs-tools' (.) is owned by no component" in e for e in mm.check_drift(s, tmp_path, ownership=True)["errors"])  # unexempted, it is still unowned
    (tmp_path / "crates/spike").mkdir(); (tmp_path / "crates/spike/Cargo.toml").write_text('[package]\nname = "spike"\n'); c.source_paths = ["crates/types", "crates/spike/src"]
    assert any("'crates/spike/src'" in e and "ignored package" in e for e in mm.check_drift(s, tmp_path, ownership=True, ignore=[".", "crates/spike"])["errors"])  # a deeper exempted package still covers its subtree


def test_check_ownership_covers_dependency_graph_packages_and_the_cli(tmp_path, capsys):
    s = cargo_repo(tmp_path, '[package]\nname = "checkout"\n')
    g = tmp_path / "deps.yaml"; g.write_text(yaml.safe_dump({"packages": [{"name": "checkout", "path": "crates/checkout"}, {"name": "b", "path": "crates/b"}], "edges": [{"from": "checkout", "to": "b"}]}))
    assert mm.check_drift(s, tmp_path, g)["errors"] == []
    assert [e.split("'")[1] for e in mm.check_drift(s, tmp_path, g, True)["errors"]] == ["b"]
    p = tmp_path / "spec.yaml"; p.write_text(json.dumps(s.model_dump(mode="json")))
    assert mm.main([str(p), "--check-drift", "--root", str(tmp_path), "--deps", str(g), "--check-ownership"]) == 1
    assert mm.main([str(p), "--check-drift", "--root", str(tmp_path), "--deps", str(g), "--check-ownership", "--ownership-ignore", "b"]) == 0
    for argv in (["--check-ownership"], ["--check-drift", "--root", str(tmp_path), "--ownership-ignore", "b"]):
        with pytest.raises(SystemExit): mm.main([str(p), *argv])


def nextest_repo(tmp_path):
    w = lambda p, t: ((tmp_path / p).parent.mkdir(parents=True, exist_ok=True), (tmp_path / p).write_text(t))
    w("crates/store/src/lib.rs", "// mod ghost;\npub mod kv;\nmod util;\n#[path = \"odd/place.rs\"]\nmod moved;\n#[cfg(test)]\nmod tests {\n    mod deep {}\n}\n")
    w("crates/store/src/kv.rs", "mod inner;\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn round_trips() {}\n}\n"); w("crates/store/src/kv/inner.rs", "#[test]\nfn deep() {}\n")
    w("crates/store/src/util/mod.rs", ""); w("crates/store/src/odd/place.rs", ""); w("crates/store/tests/it.rs", "mod common;\n#[test]\nfn roundtrip() {}\n"); w("crates/store/src/main.rs", "fn main() {}\n")
    w("crates/ops/src/lib.rs", "mod run;\n"); w("crates/ops/src/run.rs", "#[test]\nfn go() {}\n")  # a crate outside the workspace: its own metadata, its own workspace_root
    tgt = lambda n, k, f, r: {"name": n, "kind": [k], "src_path": str(tmp_path / r / f)}
    for name, root, pkg, targets in (("meta.json", ".", "store", [tgt("store", "lib", "src/lib.rs", "crates/store"), tgt("it", "test", "tests/it.rs", "crates/store"), tgt("store-cli", "bin", "src/main.rs", "crates/store")]),
                                     ("ops.json", "crates/ops", "ops", [tgt("ops", "lib", "src/lib.rs", "crates/ops")])):
        (tmp_path / name).write_text(json.dumps({"workspace_root": str(tmp_path / root), "packages": [{"name": pkg, "targets": targets}]}))
    xml = lambda cases: "<testsuites><testsuite>" + "".join(f'<testcase classname="{c}" name="{n}"/>' for c, n in cases) + "</testsuite></testsuites>"
    (tmp_path / "n.xml").write_text(xml([("store", "kv::tests::round_trips"), ("store", "kv::inner::deep"), ("store", "tests::mine"), ("store", "moved::x"), ("store", "ghost::y"), ("store::it", "roundtrip"), ("store::bin/store-cli", "main_test"), ("other", "z")]))
    (tmp_path / "ops.xml").write_text(xml([("ops", "run::go")]))
    s = spec(); ev = s.evidence[0]
    s.evidence = [ev.model_copy(update={"id": "ev-file", "provenance": "crates/store/src/kv.rs"}),
                  ev.model_copy(update={"id": "ev-test", "provenance": "crates/store/src/lib.rs::tests::mine", "requirement_id": "req-reserve", "requirement_version": "1.0"}),
                  ev.model_copy(update={"id": "ev-sel", "provenance": "nextest::-p ops", "requirement_id": "req-projection", "requirement_version": "1.0"})]
    s.components[0].source_paths, s.components[0].test_paths = ["crates/store"], ["nextest::-p ops"]
    return s


def test_nextest_test_map_resolves_binaries_modules_and_requirements(tmp_path, capsys):
    s = nextest_repo(tmp_path)
    load = lambda n: json.loads((tmp_path / n).read_text())
    ev = s.evidence[0]; file_ref = f"{ev.requirement_id}@{ev.requirement_version}"
    tm, bad = mm.nextest_test_map(s, [([str(tmp_path / "n.xml")], load("meta.json")), ([str(tmp_path / "ops.xml")], load("ops.json"))], tmp_path)
    assert tm == {"store::kv::tests::round_trips": {"test": "crates/store/src/kv.rs::tests::round_trips", "verifies": [file_ref]},  # the file's own module (kv) is not repeated
                  "store::kv::inner::deep": {"test": "crates/store/src/kv/inner.rs::deep"},
                  "store::tests::mine": {"test": "crates/store/src/lib.rs::tests::mine", "verifies": ["req-reserve@1.0"]},  # a cited test
                  "store::moved::x": {"test": "crates/store/src/odd/place.rs::x"},
                  "store::it::roundtrip": {"test": "crates/store/tests/it.rs::roundtrip"},
                  "store::bin/store-cli::main_test": {"test": "crates/store/src/main.rs::main_test"},
                  "ops::run::go": {"test": "crates/ops/src/run.rs::go", "verifies": ["req-projection@1.0"]}}  # a runner selector; paths relative to --root, not the crate's workspace
    assert bad == ["store::ghost::y", "other::z"]
    p = tmp_path / "s.yaml"; p.write_text(json.dumps(s.model_dump(mode="json")))
    args = [str(p), "--root", str(tmp_path), "--emit-test-map", str(tmp_path / "n.xml"), "--emit-test-map", str(tmp_path / "ops.xml"), "--cargo-metadata", str(tmp_path / "meta.json"), "--cargo-metadata", str(tmp_path / "ops.json")]
    assert mm.main(args) == 0
    cap = capsys.readouterr(); assert yaml.safe_load(cap.out) == tm and "2 reported tests not resolved" in cap.err
    (tmp_path / "map.yaml").write_text(cap.out)
    rc = mm.main([str(p), "--check-report", str(tmp_path / "n.xml"), "--check-report", str(tmp_path / "ops.xml"), "--report-scope", "evidence", "--test-map", str(tmp_path / "map.yaml"), "--root", str(tmp_path), "--json"])
    out = json.loads(capsys.readouterr().out)
    assert rc == 0 and out["unbound_tests"] == 6 and out["unbound_by_component"] == {"checkout": 4, "(unowned)": 2}  # 9 reported, 3 named by evidence (file, test, selector) through the map
    assert mm.main([str(p), "--root", str(tmp_path), "--emit-test-map", str(tmp_path / "n.xml"), "--emit-test-map", str(tmp_path / "ops.xml"), "--cargo-metadata", str(tmp_path / "meta.json")]) == 0  # one metadata serves all
    capsys.readouterr()
    for argv in (["--emit-test-map", "x.xml"], ["--cargo-metadata", str(tmp_path / "meta.json")], ["--emit-test-map", "a.xml", "--emit-test-map", "b.xml", "--emit-test-map", "c.xml", "--cargo-metadata", "m1", "--cargo-metadata", "m2"]):
        with pytest.raises(SystemExit): mm.main([str(p), *argv])
    (tmp_path / "meta.json").write_text("{bad"); assert mm.main([str(p), "--emit-test-map", str(tmp_path / "n.xml"), "--cargo-metadata", str(tmp_path / "meta.json")]) == 1


def test_render_docs_is_idempotent_checkable_and_escapes_cells(tmp_path, capsys):
    f = tmp_path / "d.md"
    f.write_text("# Intro\n<!-- system-spec: capabilities -->\nold\n<!-- /system-spec -->\nkeep me\n<!-- system-spec: diagram:risks -->\n<!-- /system-spec -->\n<!-- system-spec: report:coverage -->\n<!-- /system-spec -->\n")
    s = spec()
    s.capabilities[0].interface.operation = "A|B\nC"
    assert mm.render_docs(s, [f], check=True) and f.read_text().count("old") == 1
    assert mm.render_docs(s, [f]) == []
    text = f.read_text()
    assert text.startswith("# Intro\n") and "\nkeep me\n" in text and "| A\\|B<br>C |" in text and "```mermaid\ngraph LR" in text and "### By category" in text
    assert mm.render_docs(s, [f]) == [] and f.read_text() == text and mm.render_docs(s, [f], check=True) == []
    s.capabilities[0].version = "9.9"
    assert mm.render_docs(s, [f], check=True) and f.read_text() == text
    assert mm.render_docs(s, [f]) == [] and "9.9" in f.read_text()
    for k in ("components", "contracts", "data-models", "requirements", "risks", "evidence-summary", "report:risks", "diagram:runtime"): assert mm.render_block(s, k)
    f.write_text("<!-- system-spec: nope -->\n<!-- /system-spec -->\n")
    assert any("unknown block kind" in e for e in mm.render_docs(s, [f]))
    assert mm.main([str(EXAMPLE), "--render-docs", str(f)]) == 1 and "unknown block kind" in capsys.readouterr().out


def test_impact_paths_maps_changes_to_components_tests_and_unmapped(capsys):
    s = spec(); s.components[0].source_paths, s.components[0].test_paths = ["src/checkout"], ["tests/checkout", "nextest::-p checkout"]
    s.components[1].source_paths, s.components[1].test_paths = ["src/inventory"], ["tests/inventory"]
    r = mm.impact_paths(s, ["./src/inventory/a.py", "src\\inventory\\b.py", "src/inventoryx/c.py", "docs/x.md"])
    assert r["changed_components"] == ["inventory"] and r["unmapped_paths"] == ["src/inventoryx/c.py", "docs/x.md"]
    assert {"inventory", "checkout"} <= set(r["components"]) and "ev-order-test" in r["invalidated_evidence"]
    assert set(r["tests"]) == {"tests/checkout", "nextest::-p checkout", "tests/inventory"}
    r = mm.impact_paths(s, ["tests/checkout/test_a.py"]); assert r["changed_components"] == ["checkout"] and "nextest::-p checkout" in r["tests"]
    assert mm.impact_paths(s, ["x"])["components"] == [] and mm.impact_analysis(s, "inventory")["tests"]
    p = Path(__file__).parent / "example.yaml"
    assert mm.main([str(p), "--impact-paths", "nowhere/x.py", "--json"]) == 0 and json.loads(capsys.readouterr().out)["unmapped_paths"] == ["nowhere/x.py"]


def test_binding_hash_mismatch_names_stale_subject_requirements_and_evidence(tmp_path, capsys):
    s = bound_spec()
    (tmp_path / "source.py").write_bytes(b"source")
    s.subject_bindings["place-order"].artifacts = {"source.py": hashlib.sha256(b"source").hexdigest()}
    s.components[0].source_paths = ["source.py"]
    stale = []; assert mm.check_artifacts(s, tmp_path, stale) == [] and stale == []
    (tmp_path / "source.py").write_bytes(b"changed")
    errs = mm.check_artifacts(s, tmp_path, stale)
    reqs = sorted(r.id for r in s.requirements if r.target == "place-order" and r.status != "deprecated")
    assert len(errs) == 1 and "'source.py' (subject_bindings.place-order): stale requirements " + ", ".join(reqs) in errs[0] and "stale evidence " in errs[0]
    assert stale[0]["subject"] == "place-order" and stale[0]["path"] == "source.py" and sorted(stale[0]["requirements"]) == reqs and stale[0]["evidence"]
    p = tmp_path / "spec.yaml"; p.write_text(yaml.safe_dump(s.model_dump(mode="json", exclude_none=True)))
    assert mm.main([str(p), "--check-artifacts", "--root", str(tmp_path), "--json"]) == 1
    out = json.loads(capsys.readouterr().out)
    assert out["stale"][0]["subject"] == "place-order" and "stale requirements" in out["errors"][0]


@pytest.mark.skipif(not __import__("shutil").which("git"), reason="needs git")
def test_bind_generates_subject_bindings_and_refuses_dirty_trees(tmp_path, capsys):
    import subprocess
    g = lambda *a: subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *a], cwd=tmp_path, check=True, capture_output=True, text=True).stdout.strip()
    (tmp_path / "src").mkdir(); (tmp_path / "src/a.py").write_bytes(b"a"); (tmp_path / "src/b.txt").write_bytes(b"b"); (tmp_path / "src/untracked.py").write_bytes(b"u")
    g("init", "-q"); g("add", "src/a.py", "src/b.txt"); g("commit", "-qm", "x"); rev = g("rev-parse", "HEAD")
    s = bound_spec(); s.components[0].source_paths = ["src"]; s.evidence[0].artifacts = {"result.txt": "0" * 64}; (tmp_path / "result.txt").write_bytes(b"ok")
    r = mm.bind_subjects(s, tmp_path, rev)
    assert r["subject_bindings"] == {"place-order": {"version": "1.0", "revision": rev, "artifacts": {"src/a.py": hashlib.sha256(b"a").hexdigest(), "src/b.txt": hashlib.sha256(b"b").hexdigest()}}}
    assert list(r["evidence_artifacts"].values()) == [{"result.txt": hashlib.sha256(b"ok").hexdigest()}] and "run_artifacts" not in r
    # a record inheriting its artifacts from a run: the digests belong to the run, once
    s.runs = [mm.Run(id="gate", artifacts={"result.txt": "0" * 64, "log.txt": "1" * 64})]; s.evidence[0].run = "gate"; s.evidence[0].artifacts = {"result.txt": "0" * 64, "log.txt": "1" * 64, "own.txt": "2" * 64}
    (tmp_path / "log.txt").write_bytes(b"log"); (tmp_path / "own.txt").write_bytes(b"own")
    r2 = mm.bind_subjects(s, tmp_path, rev)
    assert r2["evidence_artifacts"] == {s.evidence[0].id: {"own.txt": hashlib.sha256(b"own").hexdigest()}} and r2["run_artifacts"] == {"gate": {"log.txt": hashlib.sha256(b"log").hexdigest(), "result.txt": hashlib.sha256(b"ok").hexdigest()}}
    s.evidence[0].artifacts = {"result.txt": "0" * 64, "log.txt": "1" * 64}; r3 = mm.bind_subjects(s, tmp_path, rev)
    assert "evidence_artifacts" not in r3 and list(r3["run_artifacts"]) == ["gate"]  # nothing of its own to refresh
    s.runs, s.evidence[0].run, s.evidence[0].artifacts = [], None, {"result.txt": "0" * 64}
    assert list(mm.bind_subjects(s, tmp_path, rev, ["place-order"], ["*.py"])["subject_bindings"]["place-order"]["artifacts"]) == ["src/a.py"]
    s.subject_bindings["place-order"] = mm.SubjectBinding(**mm.bind_subjects(s, tmp_path, rev)["subject_bindings"]["place-order"]); s.evidence[0].artifacts = {}
    assert mm.check_artifacts(s, tmp_path) == []
    for args in (("nope",), (rev, ["ghost"]), (rev, [], ["*.md"])):
        with pytest.raises(ValueError): mm.bind_subjects(s, tmp_path, *args)
    (tmp_path / "src/a.py").write_bytes(b"changed")
    with pytest.raises(ValueError, match="differ from revision"): mm.bind_subjects(s, tmp_path, rev)
    p = tmp_path / "spec.yaml"; p.write_text(yaml.safe_dump(s.model_dump(mode="json", exclude_none=True)))
    assert mm.main([str(p), "--bind", "--revision", rev, "--root", str(tmp_path)]) == 1 and "differ from revision" in capsys.readouterr().out
    g("commit", "-qam", "y"); assert mm.main([str(p), "--bind", "--revision", "HEAD", "--root", str(tmp_path), "--json"]) == 0 and "src/a.py" in capsys.readouterr().out
    s.runs = [mm.Run(id="gate", artifacts={"result.txt": "0" * 64})]; s.evidence[0].run = "gate"; s.evidence[0].artifacts = {"result.txt": "0" * 64}
    p.write_text(yaml.safe_dump(s.model_dump(mode="json", exclude_none=True)))
    assert mm.main([str(p), "--bind", "--revision", "HEAD", "--root", str(tmp_path)]) == 0
    out = capsys.readouterr().out; assert set(yaml.safe_load(out)) == {"subject_bindings"} and "# refreshed digests, update each run's `artifacts` by hand:\n#   gate: " in out  # pasteable whole
    pasted = yaml.safe_load(p.read_text()) | yaml.safe_load(out); mm.Spec.model_validate(pasted)
    assert mm.main([str(p), "--bind", "--revision", "HEAD", "--root", str(tmp_path), "--json"]) == 0 and "run_artifacts" in json.loads(capsys.readouterr().out)


def test_check_test_names_finds_renamed_or_deleted_tests(tmp_path, capsys):
    s = spec(); e = s.evidence[0]
    (tmp_path / "t").mkdir(); (tmp_path / "t/test_o.py").write_text("def test_place_order(): pass\nclass T:\n    def test_nested(self): pass\n")
    for prov in ("t/test_o.py::test_place_order", "t/test_o.py::T::test_nested", "t/test_o.py::test_place_order[param-1]"):
        e.provenance = prov; assert mm.check_artifacts(s, tmp_path, test_names=True) == []
    e.provenance = "t/test_o.py::test_place_order_RENAMED"
    assert mm.check_artifacts(s, tmp_path) == []
    assert mm.check_artifacts(s, tmp_path, test_names=True) == [f"artifacts: evidence '{e.id}' cites 'test_place_order_RENAMED', not found in t/test_o.py"]
    e.provenance = "t/test_o.py::order"; assert mm.check_artifacts(s, tmp_path, test_names=True) != []
    e.provenance = "pytest::test_x"; assert mm.check_artifacts(s, tmp_path, test_names=True) == []


def test_impact_and_plan_accept_imported_capability_ids(capsys):
    s = import_spec()
    s.components.append(mm.Component(id="tester", version="1.0", status="verified", layer=s.components[0].layer, test_requires=["platform:run-module@^2.0"]))
    imp = mm.impact_analysis(s, "platform:run-module")
    assert {"checkout", "platform", "tester"} <= set(imp["components"]) and "req-order" in imp["requirements"]
    assert "tester" not in mm.impact_analysis(s, "reserve-inventory")["components"]
    p = mm.change_plan(s, "platform:run-module"); assert p["target"] == "platform:run-module" and "checkout" in p["components"]
    f = Path(__file__).parent / "example.yaml"
    assert mm.main([str(f), "--impact", "pay:nothing"]) == 1 and "Unknown --impact ID" in capsys.readouterr().out


def test_check_imports_override_and_vendored_copy(tmp_path, capsys):
    other = spec().model_dump(mode="json")
    other["system"].update(id="platform", version="2.1")
    other["capabilities"][0].update(id="run-module", version="2.0"); other["components"][0]["provides"] = ["run-module"]
    elsewhere = tmp_path / "elsewhere"; elsewhere.mkdir(); (elsewhere / "p.yaml").write_text(yaml.safe_dump(other))
    repo = tmp_path / "repo"; repo.mkdir()
    s = import_spec(); s.system.imports[0].path = "missing/spec.yaml"
    assert any("cannot load" in e for e in mm.check_imports(s, repo, repo))
    assert mm.check_imports(s, repo, repo, {"platform": elsewhere / "p.yaml"}) == []
    s.system.imports[0].sha256 = "0" * 64
    assert any("sha256 mismatch" in e for e in mm.check_imports(s, repo, repo, {"platform": elsewhere / "p.yaml"}))
    s.system.imports[0].sha256 = hashlib.sha256((elsewhere / "p.yaml").read_bytes()).hexdigest()
    assert mm.check_imports(s, repo, repo, {"platform": elsewhere / "p.yaml"}) == []
    (repo / "imports").mkdir(); (repo / "imports/platform.yaml").write_bytes((elsewhere / "p.yaml").read_bytes())
    assert mm.check_imports(s, repo, repo) == []
    (repo / "imports/platform.yaml").write_text(yaml.safe_dump({**other, "system": {**other["system"], "version": "9.0"}}))
    assert any("sha256 mismatch" in e or "pins version" in e for e in mm.check_imports(s, repo, repo))
    sp = repo / "spec.yaml"; sp.write_text(yaml.safe_dump(s.model_dump(mode="json", exclude_none=True)))
    assert mm.main([str(sp), "--check-imports", "--root", str(repo), "--import-path", f"platform={elsewhere / 'p.yaml'}"]) == 0
    with pytest.raises(SystemExit): mm.main([str(sp), "--import-path", "x=y"])


def test_unquoted_numeric_digest_gets_a_targeted_error(tmp_path, capsys):
    data = bound_spec().model_dump(mode="json", exclude_none=True)
    p = tmp_path / "s.yaml"; p.write_text(yaml.safe_dump(data).replace("a" * 64, "0" * 64))
    assert yaml.safe_load(p.read_text())["subject_bindings"]["place-order"]["artifacts"]["source.py"] == 0
    with pytest.raises(Exception, match="SHA-256 must be a quoted string"): mm.load_spec(p)
    assert mm.main([str(p)]) == 1 and "SHA-256 must be a quoted string" in capsys.readouterr().out
    for model, kw in ((mm.Evidence, dict(id="e", requirement_id="r", requirement_version="1", method="test", passed=True, provenance="p")), (mm.SpecImport, dict(id="i", path="p", version="1"))):
        with pytest.raises(Exception, match="quoted string"): model(**kw, **({"artifacts": {"f": 123}} if model is mm.Evidence else {"sha256": 123}))
    assert mm.SpecImport(id="i", path="p", version="1", sha256="0" * 64).sha256 == "0" * 64


def test_runs_supply_fields_evidence_leaves_unset():
    d = spec().model_dump(mode="json", exclude_none=True)
    run = {"id": "run-1", "subject_revision": "rev1", "timestamp": "2026-01-10", "verifier": "pytest", "environment": "ci", "artifacts": {"log.txt": "a" * 64}}
    d["runs"] = [run]
    for e in d["evidence"][:2]: e.pop("timestamp", None); e.pop("verifier", None); e.pop("subject_revision", None); e.pop("artifacts", None); e["run"] = "run-1"
    d["evidence"][1]["verifier"] = "own"; d["evidence"][1]["artifacts"] = {}
    s = mm.Spec.model_validate(d)
    a, b = s.evidence[0], s.evidence[1]
    assert (a.subject_revision, str(a.timestamp)[:10], a.verifier, a.environment, a.artifacts) == ("rev1", "2026-01-10", "pytest", "ci", {"log.txt": "a" * 64})
    assert b.verifier == "own" and b.artifacts == {} and b.environment == "ci" and s.evidence[2].run is None
    assert not [e for e in mm.validate_spec(s) if "REF-15" in e or "REF-01" in e]
    d["evidence"][0]["run"] = "ghost"; assert any("REF-15" in e for e in mm.validate_spec(mm.Spec.model_validate(d)))
    d["runs"] = [run, run]; assert any("REF-01" in e and "run" in e for e in mm.validate_spec(mm.Spec.model_validate(d)))


def test_measurements_must_agree_with_criterion_thresholds():
    def build(measure=None, passed=True, covers=True):
        d = spec().model_dump(mode="json", exclude_none=True)
        q = next(r for r in d["requirements"] if r["id"] == "req-latency"); q["criteria"] = [{"id": "c1", "text": "p95", "threshold": {"metric": "p95_ms", "op": "<", "value": 150, "unit": "ms"}}]
        e = next(x for x in d["evidence"] if x["id"] == "ev-latency-bench"); e["passed"] = passed; e["criteria"] = ["c1"] if covers else []
        if measure is not None: e["measurements"] = {"p95_ms": measure}
        return mm.Spec.model_validate(d)
    ev09 = lambda s: [e for e in mm.validate_spec(s, "verification") if "EV-09" in e]
    assert ev09(build(121.4)) == []
    assert any("violates 'c1' (< 150)" in e for e in ev09(build(150.0)))
    assert any("without measuring 'p95_ms'" in e for e in ev09(build()))
    assert any("without measuring" in e for e in ev09(build(covers=False)))
    assert ev09(build(passed=False)) == []
    assert any("failed but every threshold" in e for e in ev09(build(121.4, passed=False)))
    assert ev09(build(200, passed=False)) == []
    assert any(r.get("measurements") == {"ev-latency-bench": {"p95_ms": 121.4}} for r in mm.evidence_report(build(121.4)))


def test_impact_invalidates_applies_to_evidence_only_for_reached_subjects():
    d = spec().model_dump(mode="json", exclude_none=True)
    d["components"].append({"id": "auditor", "version": "1.0", "status": "verified", "layer": d["components"][0]["layer"], "owns_data": ["audit"]})
    d["data_models"].append({"id": "audit", "version": "1.0", "owner": "auditor", "classification": "internal", "fields": {"id": "uuid"}})
    d["requirements"].append({"id": "req-enc", "version": "1.0", "status": "verified", "category": "serialization", "target": "commerce-system", "applies_to": {"kind": "data_model"}, "criteria": ["round-trips"]})
    ev = lambda i, subj: {"id": i, "requirement_id": "req-enc", "requirement_version": "1.0", "method": "test", "passed": True, "subject_id": subj, "subject_revision": "1.0", "provenance": "t.py"}
    d["evidence"] += [ev("ev-enc-order", "order"), ev("ev-enc-audit", "audit")]
    s = mm.Spec.model_validate(d)
    inv = lambda t: set(mm.impact_analysis(s, t)["invalidated_evidence"])
    assert "req-enc" in mm.impact_analysis(s, "place-order")["requirements"] and "ev-enc-order" in inv("place-order") and "ev-enc-audit" not in inv("place-order")
    assert {"ev-enc-order", "ev-enc-audit"} <= inv("req-enc") and inv("auditor") >= {"ev-enc-audit"} and "ev-enc-order" not in inv("auditor")


def test_render_docs_block_selects_its_own_spec(tmp_path, capsys):
    other = spec().model_dump(mode="json", exclude_none=True)
    other["capabilities"][0]["id"] = "other-cap"; other["components"][0]["provides"] = ["other-cap"]
    other["requirements"] = [r for r in other["requirements"] if r["target"] != "place-order"]; other["evidence"] = [e for e in other["evidence"] if e["requirement_id"] in {r["id"] for r in other["requirements"]}]
    (tmp_path / "sub").mkdir(); (tmp_path / "sub/other.yaml").write_text(yaml.safe_dump(other))
    f = tmp_path / "doc.md"
    f.write_text("<!-- system-spec: capabilities -->\n<!-- /system-spec -->\n<!-- system-spec: capabilities spec=sub/other.yaml -->\n<!-- /system-spec -->\n")
    assert mm.render_docs(spec(), [f], check=True) and mm.render_docs(spec(), [f]) == []
    a, b = f.read_text().split("<!-- system-spec: capabilities spec=sub/other.yaml -->")
    assert "| place-order |" in a and "other-cap" not in a and "| other-cap |" in b and "| place-order |" not in b
    assert mm.render_docs(spec(), [f], check=True) == [] and mm.render_docs(spec(), [f]) == []
    assert mm.main([str(EXAMPLE), "--render-docs", str(f), "--check"]) == 0
    f.write_text("<!-- system-spec: capabilities spec=nope.yaml -->\n<!-- /system-spec -->\n")
    assert any("cannot load spec 'nope.yaml'" in e for e in mm.render_docs(spec(), [f]))
    (tmp_path / "bad.yaml").write_text(yaml.safe_dump({**other, "components": []})); f.write_text("<!-- system-spec: components spec=bad.yaml -->\n<!-- /system-spec -->\n")
    assert any("is invalid" in e for e in mm.render_docs(spec(), [f]))


def test_vendored_copy_covers_a_path_outside_root_and_messages_name_the_file_read(tmp_path):
    other = spec().model_dump(mode="json")
    other["system"].update(id="platform", version="2.1")
    other["capabilities"][0].update(id="run-module", version="2.0"); other["components"][0]["provides"] = ["run-module"]
    text = yaml.safe_dump(other)
    ws = tmp_path / "ws"; (ws / "payments").mkdir(parents=True); (ws / "orders/spec/imports").mkdir(parents=True)
    s = import_spec(); s.system.imports[0].path = "../../payments/spec.yaml"; s.system.imports[0].sha256 = hashlib.sha256(text.encode()).hexdigest()
    root, base = ws / "orders", ws / "orders/spec"
    assert any("unsafe or outside-root path '../../payments/spec.yaml'" in e for e in mm.check_imports(s, root, base))
    (base / "imports/platform.yaml").write_text(text)
    assert mm.check_imports(s, root, base) == []
    (base / "imports/platform.yaml").write_text(text + "# edited\n")
    errs = mm.check_imports(s, root, base)
    assert errs == [f"imports: 'platform' sha256 mismatch for spec/imports/platform.yaml (vendored copy; declared path ../../payments/spec.yaml)"]
    (ws / "payments/spec.yaml").write_text(text)
    assert mm.check_imports(s, root, base) == errs  # outside root: the vendored copy still wins
    ov = tmp_path / "elsewhere.yaml"; ov.write_text(text + "# other\n")
    assert mm.check_imports(s, root, base, {"platform": ov}) == [f"imports: 'platform' sha256 mismatch for {ov} (--import-path override; declared path ../../payments/spec.yaml)"]


def test_check_flags_without_stage_run_only_the_checks(tmp_path, capsys):
    s = bound_spec(); s.system.status = "draft"
    (tmp_path / "source.py").write_bytes(b"source"); s.subject_bindings["place-order"].artifacts = {"source.py": hashlib.sha256(b"source").hexdigest()}
    p = tmp_path / "spec.yaml"; p.write_text(yaml.safe_dump(s.model_dump(mode="json", exclude_none=True)))
    run = lambda *a: (mm.main([str(p), *a, "--root", str(tmp_path), "--json"]), json.loads(capsys.readouterr().out))
    rc, out = run("--check-artifacts"); assert rc == 0 and out == {"valid": True, "errors": []}
    rc, out = run("--check-artifacts", "--check-drift"); assert rc == 0 and "stage" not in out
    rc, out = run("--check-artifacts", "--stage", "release"); assert rc == 1 and out["stage"] == "release" and any("STAT-04" in e for e in out["errors"])
    (tmp_path / "source.py").write_bytes(b"changed")
    rc, out = run("--check-artifacts"); assert rc == 1 and all(e.startswith("artifacts:") for e in out["errors"]) and "stage" not in out
    s.requirements = [q for q in s.requirements if q.target != "place-order"]; s.evidence = [e for e in s.evidence if e.requirement_id in {q.id for q in s.requirements}]
    assert mm.check_artifacts(s, tmp_path) == ["artifacts: hash mismatch for 'source.py' (subject_bindings.place-order)"]


def test_render_blocks_show_status_applies_to_proposed_evidence_and_summary():
    d = spec().model_dump(mode="json", exclude_none=True)
    d["requirements"] += [{"id": "req-enc", "version": "1.0", "status": "verified", "category": "serialization", "target": "commerce-system", "applies_to": {"kind": "data_model", "domain": "sales"}, "criteria": ["rt"]},
                          {"id": "req-split", "version": "1.0", "status": "specified", "proposed": True, "category": "semantic", "target": "place-order", "criteria": ["splits"]}]
    s = mm.Spec.model_validate(d)
    rows = {l.split("|")[1].strip(): l for l in mm.render_block(s, "requirements").splitlines()[2:]}
    assert "| specified (proposed) |" in rows["req-split"] and "| every data model (domain=sales) |" in rows["req-enc"] and "| verified |" in rows["req-order"]
    ev = mm.render_block(s, "evidence").splitlines()
    assert ev[0].startswith("| Evidence | Requirement | Subject | Criteria | Method | Provenance | Run | Verifier | Result |") and any("req-order@1.0" in l and "pytest::test_checkout" in l and l.endswith("| passed |") for l in ev)
    st = dict(l.strip("| ").split(" | ") for l in mm.render_block(s, "status").splitlines()[2:])
    assert st["Requirements verified"] == "11 of 12" and st["Open requirements"] == "req-split" and st["Evidence records (passed)"] == "10 (10)" and st["Runs"] == "0"
    assert "Provider" in mm.render_block(s, "capabilities").splitlines()[0] and "| checkout |" in mm.render_block(s, "capabilities")
    assert all(c in mm.render_block(s, "components").splitlines()[0] for c in ("Owns data", "Source paths", "Test requires"))
    assert all(c in mm.render_block(s, "data-models").splitlines()[0] for c in ("Derived from", "Schema"))


def test_render_layers_and_domains_follow_declared_order_and_escape():
    s = spec(); s.system.layers = ["interface", *s.system.layers] if "interface" not in s.system.layers else s.system.layers
    s.system.domains = [mm.Domain(id="sales", description="orders | checkout", uses=["stock"]), mm.Domain(id="stock")]
    s.components[0].domain, s.components[1].domain = "sales", "stock"
    lay = mm.render_block(s, "layers").splitlines()
    assert lay[0] == "| Layer | Components | Source paths |" and [l.split("|")[1].strip() for l in lay[2:]] == s.system.layers
    assert any(f"| {s.components[0].layer} |" in l and s.components[0].id in l for l in lay[2:]) and "| interface | - | - |" in lay
    dom = mm.render_block(s, "domains").splitlines()
    assert dom[2] == f"| sales | orders \\| checkout | stock | {s.components[0].id} |" and dom[3] == f"| stock | - | - | {s.components[1].id} |"
    s.components[1].external = True; assert s.components[1].id not in mm.render_block(s, "domains").splitlines()[3]


def test_render_requirements_show_criterion_text_and_risks_show_owner_description_waiver():
    d = spec().model_dump(mode="json", exclude_none=True)
    q = next(r for r in d["requirements"] if r["id"] == "req-order"); q["criteria"] = [{"id": "c1", "text": "get returns | the last value"}, "a missing key reads as absent"]
    k = d["risks"][0]; k.update(description="port is open", owner="platform team", waives="COMP-05", expires="2027-01-31")
    s = mm.Spec.model_validate(d)
    row = next(l for l in mm.render_block(s, "requirements").splitlines() if l.startswith("| req-order |"))
    assert "c1: get returns \\| the last value; a missing key reads as absent" in row
    risks = mm.render_block(s, "risks").splitlines()
    assert risks[0] == "| Risk | Target | Description | Owner | Treatment | Mitigated by | Residual | Transferred to | Waives |"
    assert any(l.startswith(f"| {k['id']} |") and "| port is open | platform team |" in l and l.endswith("| COMP-05 until 2027-01-31 |") for l in risks)
    assert mm.report(s, "risks").splitlines()[0] == "| Risk | Target | Treatment | Mitigated by | Residual | Transferred to |"


def test_check_test_names_requires_module_segments_in_order(tmp_path):
    s = spec(); e = s.evidence[0]
    (tmp_path / "src").mkdir()
    (tmp_path / "src/grant.rs").write_text("#[cfg(test)]\nmod signature_tests {\n    mod inner {\n        #[test]\n        fn verify_covers_the_matrix() {}\n    }\n}\n")
    (tmp_path / "src/a.py").write_text("class TestGrant:\n    def test_verify(self): pass\n")
    (tmp_path / "src/a.ts").write_text("describe('grant', () => { it('verify', () => {}) })\n")
    def chk(prov):
        e.provenance = prov
        return [x for x in mm.check_artifacts(s, tmp_path, test_names=True) if "cites" in x]
    assert chk("src/grant.rs::signature_tests::verify_covers_the_matrix") == []
    assert chk("src/grant.rs::signature_tests::inner::verify_covers_the_matrix") == []
    assert chk("src/grant.rs::verify_covers_the_matrix") == []
    assert chk("src/grant.rs::tests::verify_covers_the_matrix") == [f"artifacts: evidence '{e.id}' cites module 'tests', not declared in src/grant.rs"]
    assert "module 'signature_tests'" in chk("src/grant.rs::inner::signature_tests::verify_covers_the_matrix")[0]  # wrong order
    assert chk("src/a.py::TestGrant::test_verify") == [] and "class 'Other'" in chk("src/a.py::Other::test_verify")[0]
    assert chk("src/a.ts::grant::verify") == [] and "scope 'nope'" in chk("src/a.ts::nope::verify")[0]
    assert "'no_such'" in chk("src/grant.rs::signature_tests::no_such")[0]


def test_render_contracts_show_each_failure_mode_with_its_error():
    s = spec(); c = s.capabilities[0]
    c.failure_modes = ["card_declined", "gateway_timeout", "duplicate_request", "odd|mode"]
    c.interface.errors = {"card_declined": "charge.declined", "gateway_timeout": 504, "odd|mode": "a|b"}
    row = next(l for l in mm.render_block(s, "contracts").splitlines() if l.startswith(f"| {c.id} |"))
    assert row.endswith("| card_declined → charge.declined; gateway_timeout → 504; duplicate_request; odd\\|mode → a\\|b |")
    c.interface = None; assert next(l for l in mm.render_block(s, "contracts").splitlines() if l.startswith(f"| {c.id} |")).endswith("| card_declined; gateway_timeout; duplicate_request; odd\\|mode |")


def test_render_docs_markers_must_stand_alone_outside_fences(tmp_path):
    f = tmp_path / "doc.md"
    doc = ("# Orders\n\nWrite `<!-- system-spec: capabilities -->` to get a table, or `<!-- system-spec: KIND -->`.\n\n## Hand-written section\n\nKept by hand.\n\n"
           "```markdown\n<!-- system-spec: components -->\n<!-- /system-spec -->\n```\n\n~~~\n<!-- system-spec: nope -->\n~~~\n\n"
           "<!-- system-spec: components -->\nold\n<!-- /system-spec -->\n\nTail text.\n")
    f.write_text(doc)
    assert mm.render_docs(spec(), [f]) == []
    new = f.read_text()
    assert new.startswith(doc.split("<!-- system-spec: components -->\nold")[0]) and "Kept by hand." in new and new.endswith("\nTail text.\n")
    assert new.count("| Component |") == 1 and "```markdown\n<!-- system-spec: components -->\n<!-- /system-spec -->\n```" in new
    assert mm.render_docs(spec(), [f], check=True) == []
    f.write_text("<!--  system-spec: components -->\ntext\n"); assert mm.render_docs(spec(), [f]) == [] and f.read_text() == "<!--  system-spec: components -->\ntext\n"  # not a marker: plain text
    f.write_text("  <!-- system-spec: components -->\n  <!-- /system-spec -->\n"); assert mm.render_docs(spec(), [f]) == [] and "| Component |" in f.read_text()


def test_render_docs_reports_damaged_markers_by_line_without_writing(tmp_path):
    f = tmp_path / "doc.md"
    for text, msg in (("a\n<!-- system-spec: components -->\nstill here\n", "line 2: block 'components' has no closing marker"),
                      ("<!-- system-spec: components -->\nx\n<!-- system-spec: capabilities -->\n<!-- /system-spec -->\n", "line 3: opening marker inside the block opened at line 1; is its closing marker missing?"),
                      ("text\n<!-- /system-spec -->\n", "line 2: closing marker without an opening marker")):
        f.write_text(text)
        assert mm.render_docs(spec(), [f]) == [f"docs: {f}: {msg}"]
        assert f.read_text() == text


def test_render_docs_keeps_crlf_line_endings(tmp_path):
    f = tmp_path / "doc.md"; f.write_bytes(b"# T\r\n<!-- system-spec: layers -->\r\nold\r\n<!-- /system-spec -->\r\nend\r\n")
    assert mm.render_docs(spec(), [f]) == []
    b = f.read_bytes(); assert b.count(b"\n") == b.count(b"\r\n") and b.endswith(b"<!-- /system-spec -->\r\nend\r\n") and b"| Layer |" in b


def test_editing_a_model_behind_a_passing_model_check_record_names_the_record(tmp_path):
    s = bound_spec(); e = s.evidence[0]
    e.method, e.bounds, e.provenance = "model-check", {"products": 2}, "tla+::Inventory.tla"
    s.subject_bindings, s.components[0].source_paths, s.data_models[0].schema_path = {}, [], None
    files = {"Inventory.tla": b"MODULE Inventory", "Inventory.cfg": b"CONSTANT n = 2", "tlc.out": b"No error has been found."}
    for n, b in files.items(): (tmp_path / n).write_bytes(b)
    e.artifacts = {n: hashlib.sha256(b).hexdigest() for n, b in files.items()}
    assert mm.check_artifacts(s, tmp_path) == []
    (tmp_path / "Inventory.cfg").write_bytes(b"CONSTANT n = 3")
    assert mm.check_artifacts(s, tmp_path) == [f"artifacts: hash mismatch for 'Inventory.cfg' (evidence '{e.id}')"]


def formal_spec(**fa):  # the example's `ev-reserve-test` is a passing model-check on inventory's capability
    s = mm.load_spec(EXAMPLE); s.components[1].formal_assessment = mm.FormalAssessment(**fa) if fa else None
    return s


def test_formal_assessment_lists_unassessed_components_and_missing_evidence():
    s = formal_spec(); assert mm.formal_assessment_gaps(s) == {"unassessed": ["checkout", "inventory"], "no_evidence": [], "accepted_gaps": {}}
    s = formal_spec(applies=True, indicators=["concurrency"]); assert mm.formal_assessment_gaps(s) == {"unassessed": ["checkout"], "no_evidence": [], "accepted_gaps": {}}
    model = next(e for e in s.evidence if e.id == "ev-reserve-test"); model.passed = False
    assert mm.formal_assessment_gaps(s)["no_evidence"] == ["inventory"]
    model.passed = True; model.requirement_id = "req-order"  # now on checkout's capability, not inventory's
    assert mm.formal_assessment_gaps(s)["no_evidence"] == ["inventory"]
    s.components[1].formal_assessment.evidence = ["ev-reserve-test"]; assert mm.formal_assessment_gaps(s)["no_evidence"] == []  # cited by the assessment
    s = formal_spec(applies=False, rationale="stateless lookup"); assert mm.formal_assessment_gaps(s) == {"unassessed": ["checkout"], "no_evidence": [], "accepted_gaps": {}}
    s.components[0].external = True; s.components[0].layer = s.components[0].domain = None; assert mm.formal_assessment_gaps(s)["unassessed"] == []  # external: not assessed here


def test_formal_assessment_must_be_consistent_comp_12():
    codes = lambda s: [e for e in mm.validate_spec(s, "verification") if "[COMP-12]" in e]
    assert codes(formal_spec(applies=True, indicators=["protocol"], evidence=["ev-reserve-test"])) == []
    assert codes(formal_spec(applies=False, rationale="no shared state")) == []
    assert "no indicators" in codes(formal_spec(applies=True))[0]
    assert "no rationale" in codes(formal_spec(applies=False))[0]
    assert "unknown evidence 'nope'" in codes(formal_spec(applies=True, indicators=["protocol"], evidence=["nope"]))[0]
    assert "not model-check or proof" in codes(formal_spec(applies=True, indicators=["protocol"], evidence=["ev-order-test"]))[0]
    assert not [e for e in mm.validate_spec(formal_spec(applies=True), "architecture") if "COMP-12" in e]


def test_formal_assessment_cli_notes_at_verification_and_release_only(tmp_path, capsys):
    s = formal_spec(applies=True, indicators=["concurrency"]); next(e for e in s.evidence if e.id == "ev-reserve-test").passed = False
    f = tmp_path / "s.yaml"; f.write_text(yaml.safe_dump(json.loads(s.model_dump_json(exclude_defaults=True))))
    mm.main([str(f), "--stage", "verification"]); out = capsys.readouterr().out
    assert "have no formal verification assessment (formal_assessment): checkout" in out and "no passing model-check or proof evidence: inventory" in out
    mm.main([str(f), "--stage", "architecture"]); assert "formal" not in capsys.readouterr().out
    mm.main([str(f), "--stage", "architecture", "--json"]); j = json.loads(capsys.readouterr().out)
    assert j["formal_unassessed"] == ["checkout"] and j["formal_no_evidence"] == ["inventory"]


def accepted_spec(**kw):
    s = mm.load_spec(EXAMPLE)
    s.system.acceptance = mm.Acceptance(**{"accepted_by": "release-manager", "accepted_on": "2026-11-01", "revision": "abc123", "scope": "P0", **kw})
    return s


def test_acceptance_is_recorded_apart_from_passing_the_stage():
    s = mm.load_spec(EXAMPLE); assert mm.acceptance_report(s) is None and mm.validate_spec(s, "release") == []  # passing, never accepted
    s = accepted_spec(); r = mm.acceptance_report(s)
    assert r["passes"] is True and r["accepted_by"] == "release-manager" and r["stage"] == "release" and r["accepted_on"] == "2026-11-01"
    assert r["stale"] == []  # evidence is dated 2026-10-30
    s.evidence[0].timestamp = datetime.date(2026, 11, 5); s.subject_bindings["place-order"] = mm.SubjectBinding(version="1.0", revision="def456", artifacts={"a.py": "0" * 64})
    assert mm.acceptance_report(s)["stale"] == [f"evidence '{s.evidence[0].id}' dated 2026-11-05, after acceptance", "subject 'place-order' bound at revision def456, accepted abc123"]
    s.evidence = s.evidence[1:]; assert mm.acceptance_report(s)["passes"] is False  # a requirement lost its evidence


def test_acceptance_in_cli_and_status_block(tmp_path, capsys):
    s = accepted_spec(); f = tmp_path / "s.yaml"; f.write_text(yaml.safe_dump(json.loads(s.model_dump_json(exclude_defaults=True))))
    mm.main([str(f)]); out = capsys.readouterr().out
    assert "note: accepted for release by release-manager on 2026-11-01 at abc123; --stage release passes now" in out and "stale" not in out
    mm.main([str(f), "--json"]); assert json.loads(capsys.readouterr().out)["acceptance"]["passes"] is True
    status = mm.render_block(mm.load_spec(f), "status")
    assert "| Acceptance | release accepted by release-manager on 2026-11-01 at abc123 (P0) |" in status and "| Passes --stage release | yes |" in status and "| Acceptance current | yes (code not checked) |" in status
    assert "Acceptance" not in mm.render_block(mm.load_spec(EXAMPLE), "status")


def test_formal_assessment_cited_evidence_of_another_component_counts_and_comp_12_accepts_it():
    s = formal_spec()  # `ev-reserve-test` is a passing model-check on inventory's capability
    s.components[0].formal_assessment = mm.FormalAssessment(applies=True, indicators=["protocol"], rationale="shared protocol", evidence=["ev-reserve-test"])
    assert mm.formal_assessment_gaps(s)["no_evidence"] == []
    assert not [e for e in mm.validate_spec(s, "verification") if "COMP-12" in e]
    s.components[0].formal_assessment.evidence = []  # without the citation checkout has no model evidence of its own
    assert mm.formal_assessment_gaps(s)["no_evidence"] == ["checkout"]


def test_comp_13_applicable_formal_assessment_needs_evidence_or_an_accepted_risk_at_release():
    s = formal_spec(applies=True, indicators=["protocol"])
    assert not [e for e in mm.validate_spec(s, "release") if "COMP-13" in e]  # inventory's capability has a passing model-check
    s = formal_spec(applies=True, indicators=["protocol"]); s.components[0].formal_assessment = mm.FormalAssessment(applies=True, indicators=["protocol"])  # checkout: none
    errs = [e for e in mm.validate_spec(s, "release") if "COMP-13" in e]; assert len(errs) == 1 and "'checkout'" in errs[0]
    assert not [e for e in mm.validate_spec(s, "verification") if "COMP-13" in e]  # release only
    def with_risk(**kw):
        s2 = formal_spec(applies=True, indicators=["protocol"]); s2.components[0].formal_assessment = mm.FormalAssessment(applies=True, indicators=["protocol"], risk="risk-no-model")
        s2.risks.append(mm.Risk(**{"id": "risk-no-model", "target": "checkout", "treatment": "acceptance", "residual_accepted": True, "owner": "o", **kw})); return s2
    assert [e for e in mm.validate_spec(with_risk(), "release") if "COMP-1" in e and "COMP-10" not in e and "COMP-11" not in e] == []
    assert any("COMP-12" in e and "must target" in e for e in mm.validate_spec(with_risk(residual_accepted=False), "verification"))
    assert any("COMP-12" in e and "must target" in e for e in mm.validate_spec(with_risk(target="inventory"), "verification"))
    s3 = with_risk(); s3.components[0].formal_assessment.risk = "ghost"; assert any("unknown risk 'ghost'" in e for e in mm.validate_spec(s3, "verification"))


def test_accepted_formal_gaps_are_listed_apart_from_unaccepted_ones(tmp_path, capsys):
    s = formal_spec(applies=True, indicators=["protocol"]); next(e for e in s.evidence if e.id == "ev-reserve-test").passed = False
    s.components[0].formal_assessment = mm.FormalAssessment(applies=True, indicators=["protocol"], risk="risk-a")
    s.risks.append(mm.Risk(id="risk-a", target="checkout", treatment="acceptance", residual_accepted=True, owner="o"))
    assert (g := mm.formal_assessment_gaps(s))["no_evidence"] == ["inventory"] and g["accepted_gaps"] == {"checkout": "risk-a"}
    f = tmp_path / "s.yaml"; f.write_text(yaml.safe_dump(json.loads(s.model_dump_json(exclude_defaults=True))))
    mm.main([str(f), "--stage", "verification"]); out = capsys.readouterr().out
    assert "no passing model-check or proof evidence: inventory\n" in out and "gaps accepted by a risk (formal_assessment.risk): checkout (risk-a)" in out
    mm.main([str(f), "--stage", "verification", "--json"]); j = json.loads(capsys.readouterr().out)
    assert j["formal_no_evidence"] == ["inventory"] and j["formal_accepted_gaps"] == {"checkout": "risk-a"}


def test_check_flags_keep_the_stage_reports(tmp_path, capsys):
    s = formal_spec(applies=True, indicators=["concurrency"]); s.system.acceptance = mm.Acceptance(accepted_by="rm", accepted_on="2026-11-01", revision="abc")
    f = tmp_path / "s.yaml"; f.write_text(yaml.safe_dump(json.loads(s.model_dump_json(exclude_defaults=True))))
    args = [str(f), "--stage", "verification", "--root", str(tmp_path)]
    mm.main(args); plain = capsys.readouterr().out
    mm.main([*args, "--check-artifacts"]); out = capsys.readouterr().out
    assert [l for l in plain.splitlines() if l.startswith("note:")] and all(l in out for l in plain.splitlines() if l.startswith("note:"))
    mm.main([*args, "--check-artifacts", "--json"]); j = json.loads(capsys.readouterr().out)
    assert j["stage"] == "verification" and j["acceptance"]["revision"] == "abc" and j["formal_unassessed"] == ["checkout"] and "residual_risks" in j
    mm.main([str(f), "--root", str(tmp_path), "--check-artifacts", "--json"]); assert "acceptance" not in json.loads(capsys.readouterr().out)  # no stage validated


@pytest.fixture
def accepted_repo(tmp_path):
    if not shutil.which("git"): pytest.skip("needs git")
    g = lambda *a: subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *a], cwd=tmp_path, check=True, capture_output=True, text=True).stdout.strip()
    g("init", "-q"); (tmp_path / "checkout").mkdir(); (tmp_path / "inventory").mkdir()
    (tmp_path / "checkout/a.py").write_text("a"); (tmp_path / "inventory/b.py").write_text("b"); g("add", "."); g("commit", "-qm", "accepted")
    s = accepted_spec(revision=g("rev-parse", "HEAD")); s.components[0].source_paths, s.components[1].source_paths = ["checkout"], ["inventory"]
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(json.loads(s.model_dump_json(exclude_defaults=True)))); g("add", "."); g("commit", "-qm", "spec")
    return tmp_path, g, s


def test_acceptance_names_components_whose_source_changed_since_the_accepted_revision(accepted_repo):
    root, g, s = accepted_repo  # no subject_bindings at all
    r = mm.acceptance_report(s, root, root / "spec.yaml")
    assert r["code_checked"] is True and r["stale"] == [] and r["changed_since_acceptance"] == {}  # the spec commit itself is the acceptance
    (root / "inventory/b.py").write_text("changed"); g("commit", "-qam", "change"); sha = g("rev-parse", "--short", "HEAD")
    r = mm.acceptance_report(s, root, root / "spec.yaml")
    assert r["changed_since_acceptance"] == {"inventory": [sha]} and r["stale"] == [f"component 'inventory' changed in 1 commit since acceptance ({sha})"]
    (root / "spec.yaml").write_text((root / "spec.yaml").read_text() + "\n"); g("commit", "-qam", "edit spec")
    assert set(mm.acceptance_report(s, root, root / "spec.yaml")["changed_since_acceptance"]) == {"inventory", "spec"}
    r = mm.acceptance_report(s); assert r["code_checked"] is False and r["stale"] == []  # no --root: not checked, said so


def test_acceptance_anchor_follows_any_acceptance_field_and_never_precedes_the_revision(accepted_repo):
    root, g, s = accepted_repo; f = root / "spec.yaml"; spec_commits = lambda: mm.acceptance_report(s, root, f)["changed_since_acceptance"].get("spec")
    f.write_text(f.read_text() + "# b\n"); g("commit", "-qam", "spec change B"); b = g("rev-parse", "HEAD")
    assert len(spec_commits()) == 1
    d = yaml.safe_load(f.read_text()); d["system"]["acceptance"]["revision"] = b; f.write_text(yaml.safe_dump(d)); g("commit", "-qam", "re-accept at B, same day")  # accepted_on unchanged
    s.system.acceptance.revision = b; assert spec_commits() is None and mm.acceptance_report(s, root, f)["stale"] == []
    f.write_text(f.read_text() + "# e\n"); g("commit", "-qam", "spec change E"); e = g("rev-parse", "--short", "HEAD")
    f.write_text(re.sub(r"(accepted_on: .*)", r"\1  # reformatted", f.read_text())); g("commit", "-qam", "reformat accepted_on"); fmt = g("rev-parse", "--short", "HEAD")
    assert spec_commits() == [fmt, e]  # the reformat is no re-acceptance, so E still counts


def test_render_docs_status_runs_the_git_check_with_root(accepted_repo):
    root, g, s = accepted_repo; md = root / "d.md"; md.write_text("<!-- system-spec: status -->\n<!-- /system-spec -->\n"); cur = lambda: dict(l.strip("| ").split(" | ") for l in md.read_text().splitlines()[3:-1])["Acceptance current"]
    assert mm.render_docs(s, [md], root=root, spec_file=root / "spec.yaml") == [] and cur() == "yes"  # git consulted, nothing changed
    assert mm.render_docs(s, [md]) == [] and cur() == "yes (code not checked)"
    (root / "inventory/b.py").write_text("changed"); g("commit", "-qam", "change")
    assert mm.render_docs(s, [md], check=True, root=root, spec_file=root / "spec.yaml") == [f"docs: '{md}' is stale; run --render-docs"]  # was "yes (code not checked)"
    mm.render_docs(s, [md], root=root, spec_file=root / "spec.yaml"); assert cur() == "no: changed since acceptance: inventory"
    (root / "inventory/b.py").write_text("changed again"); g("commit", "-qam", "second commit, as a branch before a squash merge")
    assert mm.render_docs(s, [md], check=True, root=root, spec_file=root / "spec.yaml") == []  # no hash or count in the row, so squashing the two commits keeps `--check` passing
    assert "1 commit" not in cur() and "2 commits" in mm.acceptance_report(s, root, root / "spec.yaml")["stale"][0]  # validation still names them
    s.system.acceptance.revision = "f" * 40; mm.render_docs(s, [md], root=root); assert "is not a known commit" in cur()


def test_acceptance_reports_unknown_and_non_ancestor_revisions(accepted_repo):
    root, g, s = accepted_repo
    s.system.acceptance.revision = "f" * 40
    assert mm.acceptance_report(s, root)["stale"] == [f"revision {'f' * 40} is not a known commit in {root.resolve()}"]
    g("checkout", "-q", "-b", "side"); (root / "checkout/a.py").write_text("x"); g("commit", "-qam", "side"); side = g("rev-parse", "HEAD"); g("checkout", "-q", "-")
    s.system.acceptance.revision = side
    assert "not an ancestor of HEAD" in mm.acceptance_report(s, root)["stale"][0]
    s.system.acceptance.revision = g("rev-parse", "HEAD"); assert mm.acceptance_report(s, root / "checkout")["code_checked"] is True  # a subdirectory of the checkout


def test_acceptance_without_git_and_cli_notes(tmp_path, capsys):
    s = accepted_spec(); assert mm.acceptance_report(s, tmp_path)["code_checked"] is False  # --root outside a git work tree
    f = tmp_path / "s.yaml"; f.write_text(yaml.safe_dump(json.loads(s.model_dump_json(exclude_defaults=True))))
    mm.main([str(f)]); assert "code changes since the accepted revision were not checked" in capsys.readouterr().out


def test_acceptance_numeric_revision_fails_with_a_clear_message():
    with pytest.raises(Exception, match="revision must be a quoted string"): mm.Acceptance.model_validate({"accepted_by": "x", "accepted_on": "2026-11-01", "revision": 0})
    assert mm.Acceptance.model_validate({"accepted_by": "x", "accepted_on": "2026-11-01", "revision": "0000000"}).revision == "0000000"
