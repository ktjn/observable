import hashlib
import json
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
    assert mm.check_drift(s, tmp_path) == {"errors": [], "unchecked": ["stock.sql"]}


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
    assert mm.check_drift(s, root) == {"errors": [], "unchecked": []}
