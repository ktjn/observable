"""Consistency tests for spec/system.yaml against distribution/manifest.yaml,
source_paths, and models/*.mdl schema_path references (stdlib only).
"""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = ROOT / "spec/system.yaml"
MANIFEST = ROOT / "distribution/manifest.yaml"
VERSION_FILE = ROOT / "VERSION"

MANIFEST_TO_SPEC_COMPONENT = {
    "auth": "observable-auth",
    "control": "observable-control",
    "ingest": "observable-ingest",
    "process": "observable-process",
    "storeClickhouse": "observable-store-clickhouse",
    "query": "observable-query",
    "alerting": "observable-alerting",
    "catalog": "observable-catalog",
    "web": "observable-web",
}


def parse_manifest_versions(text: str) -> tuple[str, dict[str, str]]:
    m = re.search(r"(?m)^productVersion:\s*(\S+)", text)
    product_ver = m.group(1) if m else ""
    comp_vers: dict[str, str] = {}
    in_comps = False
    for line in text.splitlines():
        if line.startswith("components:"):
            in_comps = True
            continue
        if in_comps:
            if line and not line[0].isspace() and not line.startswith("#"):
                break
            cm = re.match(r"^\s{2}([A-Za-z0-9_]+):\s*(\S+)", line)
            if cm:
                comp_vers[cm.group(1)] = cm.group(2)
    return product_ver, comp_vers


def parse_spec_blocks(text: str, section: str) -> list[str]:
    lines = text.splitlines()
    blocks: list[list[str]] = []
    in_sec = False
    cur: list[str] = []
    for line in lines:
        if re.match(rf"^{section}:\s*$", line):
            in_sec = True
            continue
        if in_sec:
            if line and not line[0].isspace() and not line.startswith("#"):
                break
            if line.startswith("  - id:"):
                if cur:
                    blocks.append(cur)
                cur = [line]
            elif cur:
                cur.append(line)
    if cur:
        blocks.append(cur)
    return ["\n".join(b) for b in blocks]


def defines_test(path: Path, selector: str) -> bool:
    """True if `selector` (test name; `Class::method` for Python) is defined in `path`."""
    name = selector.split("::")[-1]
    text = path.read_text(encoding="utf-8")
    if path.suffix in (".ts", ".tsx"):
        title = re.escape(name)
        return re.search(rf"""\b(?:test|it)\(\s*["'`]{title}["'`]""", text) is not None
    pattern = rf"^\s*(?:async\s+)?fn\s+{re.escape(name)}\s*[<(]" if path.suffix == ".rs" \
        else rf"^\s*def\s+{re.escape(name)}\s*\("
    return re.search(pattern, text, re.M) is not None


class SystemSpecConsistencyTests(unittest.TestCase):
    def test_system_spec_exists_and_matches_root_version(self):
        self.assertTrue(SPEC.is_file(), "spec/system.yaml must exist")
        spec_text = SPEC.read_text(encoding="utf-8")
        root_ver = VERSION_FILE.read_text(encoding="utf-8").strip()
        sys_match = re.search(
            r'(?ms)^system:\n.*?^\s+id:\s*(\S+).*?^\s+version:\s*"?([^"\n]+)"?',
            spec_text,
        )
        self.assertIsNotNone(sys_match, "system block must declare id and version")
        self.assertEqual(sys_match.group(1), "observable-platform")
        self.assertEqual(sys_match.group(2), root_ver)

    def test_components_match_distribution_manifest_and_source_paths_exist(self):
        spec_text = SPEC.read_text(encoding="utf-8")
        _, manifest_comps = parse_manifest_versions(MANIFEST.read_text(encoding="utf-8"))
        blocks = parse_spec_blocks(spec_text, "components")
        spec_comps: dict[str, tuple[str, list[str]]] = {}
        for block in blocks:
            cid = re.search(r"^\s*-\s*id:\s*(\S+)", block, re.M).group(1)
            cver = re.search(r'^\s*version:\s*"?([^"\n]+)"?', block, re.M).group(1)
            sp_tail = block.partition("source_paths:\n")[2]
            sp_lines = []
            for sline in sp_tail.splitlines():
                if sline.startswith("      - "):
                    sp_lines.append(sline.removeprefix("      - ").strip())
                else:
                    break
            spec_comps[cid] = (cver, sp_lines)

        self.assertEqual(
            set(spec_comps.keys()),
            set(MANIFEST_TO_SPEC_COMPONENT.values()),
            "spec/system.yaml components must match runtime components in distribution/manifest.yaml",
        )
        for mkey, cid in MANIFEST_TO_SPEC_COMPONENT.items():
            cver, paths = spec_comps[cid]
            self.assertEqual(
                cver,
                manifest_comps[mkey],
                f"{cid} version '{cver}' must match distribution/manifest.yaml '{mkey}: {manifest_comps[mkey]}'",
            )
            self.assertTrue(paths, f"{cid} must declare non-empty source_paths")
            for rel in paths:
                self.assertTrue(
                    (ROOT / rel).exists(),
                    f"{cid} source_path '{rel}' does not exist in repository",
                )

    def test_data_model_schema_paths_and_evidence_provenance_exist(self):
        spec_text = SPEC.read_text(encoding="utf-8")
        for block in parse_spec_blocks(spec_text, "data_models"):
            did = re.search(r"^\s*-\s*id:\s*(\S+)", block, re.M).group(1)
            sm = re.search(r"^\s*schema_path:\s*(\S+)", block, re.M)
            self.assertIsNotNone(sm, f"DataModel '{did}' must declare schema_path")
            schema_rel = sm.group(1)
            self.assertTrue(
                (ROOT / schema_rel).is_file(),
                f"DataModel '{did}' schema_path '{schema_rel}' does not exist",
            )

    def test_component_dag_and_layer_ordering_comp05_comp06(self):
        spec_text = SPEC.read_text(encoding="utf-8")
        layers = ["experience", "application", "domain", "infrastructure"]
        layer_rank = {name: idx for idx, name in enumerate(layers)}

        comp_layer: dict[str, str] = {}
        cap_provider: dict[str, str] = {}
        comp_requires: dict[str, list[str]] = {}

        for block in parse_spec_blocks(spec_text, "components"):
            cid = re.search(r"^\s*-\s*id:\s*(\S+)", block, re.M).group(1)
            layer = re.search(r"^\s*layer:\s*(\S+)", block, re.M).group(1)
            comp_layer[cid] = layer

            prov_tail = block.partition("provides:\n")[2]
            for line in prov_tail.splitlines():
                if line.startswith("      - "):
                    cap_provider[line.removeprefix("      - ").strip()] = cid
                else:
                    break

            req_tail = block.partition("requires:\n")[2]
            reqs: list[str] = []
            for line in req_tail.splitlines():
                if line.startswith("      - "):
                    reqs.append(line.removeprefix("      - ").strip().split("@")[0])
                else:
                    break
            comp_requires[cid] = reqs

        for cid, req_caps in comp_requires.items():
            for cap in req_caps:
                target = cap_provider[cap]
                self.assertLessEqual(
                    layer_rank[comp_layer[cid]],
                    layer_rank[comp_layer[target]],
                    f"COMP-05 violation: {cid} ({comp_layer[cid]}) cannot depend on {target} ({comp_layer[target]})",
                )

        self.assertIn("evaluate-alerts", comp_requires["observable-query"])
        self.assertIn("store-telemetry", comp_requires["observable-alerting"])
        self.assertNotIn("query-telemetry", comp_requires["observable-alerting"])
        self.assertTrue(
            (ROOT / "services/query-api/clickhouse-schema-support.yaml").is_file()
        )
        self.assertTrue(
            (ROOT / "services/alert-evaluator/clickhouse-schema-support.yaml").is_file()
        )

    def test_evidence_provenance_paths_and_timestamps(self):
        spec_text = SPEC.read_text(encoding="utf-8")
        evidence_blocks = parse_spec_blocks(spec_text, "evidence")
        self.assertGreater(len(evidence_blocks), 0, "No evidence records found")

        iso_regex = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")

        for block in evidence_blocks:
            eid = re.search(r"^\s*-\s*id:\s*(\S+)", block, re.M).group(1)
            pm = re.search(r"^\s*provenance:\s*(.+?)\s*$", block, re.M)
            self.assertIsNotNone(pm, f"Evidence '{eid}' must declare provenance")
            prov = pm.group(1).strip("\"'")
            prov_rel, _, selector = prov.partition("::")
            self.assertTrue(
                (ROOT / prov_rel).is_file(),
                f"Evidence '{eid}' provenance file '{prov_rel}' does not exist",
            )
            mm = re.search(r"^\s*method:\s*(\S+)", block, re.M)
            if mm and mm.group(1) == "test":
                self.assertTrue(
                    selector,
                    f"Evidence '{eid}' (method: test) must name a test as path::test_name",
                )
                self.assertTrue(
                    defines_test(ROOT / prov_rel, selector),
                    f"Evidence '{eid}' test '{selector}' is not defined in '{prov_rel}'",
                )
            tm = re.search(r"^\s*timestamp:\s*\"([^\"]+)\"", block, re.M)
            self.assertIsNotNone(tm, f"Evidence '{eid}' must declare timestamp")
            self.assertTrue(
                iso_regex.match(tm.group(1)),
                f"Evidence '{eid}' timestamp '{tm.group(1)}' is not valid ISO 8601 UTC",
            )

    def test_system_spec_verification_stage_validation(self):
        """2.0 gate (docs/plans/business-spec-adoption.md Q7): the spec validates at
        verification stage until the target release. The stage runs on its own
        because the CLI skips stage invariants when --check-artifacts/--check-drift pass."""
        import shutil
        import subprocess

        uv_bin = shutil.which("uv")
        system_spec_bin = shutil.which("system-spec")
        if uv_bin:
            base = [uv_bin, "run", "--project", str(ROOT / "models"), "system-spec"]
        elif system_spec_bin:
            base = [system_spec_bin]
        else:
            self.fail("Neither uv nor system-spec is available in PATH")

        for flags in (
            [str(SPEC), "--stage", "verification"],
            [str(SPEC), "--stage", "verification", "--check-artifacts", "--check-drift", "--root", str(ROOT)],
        ):
            res = subprocess.run(base + flags, capture_output=True, text=True, cwd=str(ROOT))
            self.assertEqual(
                res.returncode,
                0,
                f"system-spec {' '.join(flags[1:])} failed with exit {res.returncode}:\n{res.stderr}\n{res.stdout}",
            )
            self.assertIn("VALID", res.stdout)


if __name__ == "__main__":
    unittest.main()
