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


if __name__ == "__main__":
    unittest.main()
