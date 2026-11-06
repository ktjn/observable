import sys
from pathlib import Path

import pytest

from metamodel import MockBinding, check_isolation, load_spec, main


ROOT = Path(__file__).resolve().parent.parent / "examples" / "components"
SPEC = ROOT / "spec.yaml"


def test_full_isolation_example():
    spec = load_spec(SPEC)
    assert check_isolation(spec, ROOT) == []
    assert main([str(SPEC), "--check-isolation", "--root", str(ROOT)]) == 0


def test_isolation_is_explicit_opt_in():
    spec = load_spec(SPEC)
    assert spec.components[0].provided_mocks
    assert all(c.standalone_harness for c in spec.components)
    with pytest.raises(SystemExit) as exc:
        main([str(SPEC), "--check-isolation"])
    assert exc.value.code == 2


def test_missing_dependency_substitute_and_published_mock():
    spec = load_spec(SPEC)
    checkout = next(c for c in spec.components if c.id == "checkout")
    checkout.fakes = {}
    checkout.provided_mocks = {}
    errors = check_isolation(spec, ROOT)
    assert any("dependency substitute" in e for e in errors)
    assert any("missing provided mock" in e for e in errors)


def test_missing_harness_and_mock_suite():
    spec = load_spec(SPEC)
    inventory = next(c for c in spec.components if c.id == "inventory")
    inventory.standalone_harness = []
    inventory.provided_mocks["reserve-inventory"].contract_suite = []
    errors = check_isolation(spec, ROOT)
    assert any("missing standalone_harness" in e for e in errors)
    assert any("missing contract_suite" in e for e in errors)


def test_contract_failure_is_detected(tmp_path):
    spec = load_spec(SPEC)
    spec.components = [c for c in spec.components if c.id == "inventory"]
    component = spec.components[0]
    script = tmp_path / "suite.py"
    script.write_text("import os,sys\nsys.exit(3 if os.getenv('SYSTEM_SPEC_MODE') == 'mock' else 0)\n")
    mock = tmp_path / "provider.py"
    mock.write_text("x = 1\n")
    component.standalone_harness = [sys.executable, "suite.py"]
    component.provided_mocks = {
        "reserve-inventory": MockBinding(path="provider.py", contract_suite=[sys.executable, "suite.py"])
    }
    errors = check_isolation(spec, tmp_path)
    assert any("mock contract: exit 3" in e for e in errors)
    assert not any("real contract:" in e for e in errors)


def test_missing_mock_path_and_traversal():
    spec = load_spec(SPEC)
    inventory = next(c for c in spec.components if c.id == "inventory")
    inventory.provided_mocks["reserve-inventory"].path = "../outside.py"
    errors = check_isolation(spec, ROOT)
    assert any("invalid mock path" in e for e in errors)


def test_timeout_is_reported(tmp_path):
    spec = load_spec(SPEC)
    spec.components = [c for c in spec.components if c.id == "inventory"]
    component = spec.components[0]
    (tmp_path / "provider.py").write_text("x = 1\n")
    component.standalone_harness = [sys.executable, "-c", "import time; time.sleep(1)"]
    component.provided_mocks = {
        "reserve-inventory": MockBinding(path="provider.py", contract_suite=[sys.executable, "-c", "pass"])
    }
    errors = check_isolation(spec, tmp_path, timeout=0.05)
    assert any("standalone" in e and "timed out" in e for e in errors)
