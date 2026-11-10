"""Regression tests for scripts/health-worker-credential.py (stdlib only, fake psql)."""

from pathlib import Path
import hashlib
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "health-worker-credential.py"
TENANT = "00000000-0000-0000-0000-000000000001"
OTHER = "00000000-0000-0000-0000-000000000002"
URL = "postgres://owner:s3cr%40t@db.internal:5433/observable?sslmode=require"

FAKE_PSQL = """#!{python}
import json, os, sys
log = os.environ["FAKE_PSQL_LOG"]
with open(log, "a") as f:
    f.write(json.dumps({{"argv": sys.argv[1:], "stdin": sys.stdin.read(),
        "env": {{k: v for k, v in os.environ.items() if k.startswith("PG")}}}}) + "\\n")
sys.exit(int(os.environ.get("FAKE_PSQL_EXIT", "0")))
"""


@unittest.skipIf(os.name == "nt", "fake psql shim needs a POSIX shebang")
class HealthWorkerCredentialTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        psql = bin_dir / "psql"
        psql.write_text(FAKE_PSQL.format(python=sys.executable))
        psql.chmod(0o755)
        self.log = self.tmp / "psql.log"
        self.env = {
            "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
            "FAKE_PSQL_LOG": str(self.log),
            "HEALTH_WORKER_ADMIN_DATABASE_URL": URL,
        }

    def run_script(self, *args, **env):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            cwd=ROOT,
            capture_output=True,
            text=True,
            env={**self.env, **env},
        )

    def calls(self):
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_issue_registers_only_the_digest_and_writes_a_private_file(self):
        creds = self.tmp / "secret" / "workers.json"
        res = self.run_script("issue", "--tenant", TENANT, "--file", str(creds), "--ttl-days", "7")
        self.assertEqual(res.returncode, 0, res.stderr)
        token = json.loads(creds.read_text())[TENANT]
        self.assertRegex(token, r"^ohw_[0-9a-f]{64}$")
        self.assertEqual(stat.S_IMODE(creds.stat().st_mode), 0o600)
        self.assertNotIn(token, res.stdout + res.stderr)
        (call,) = self.calls()
        self.assertIn(f"digest={hashlib.sha256(token.encode()).hexdigest()}", call["argv"])
        self.assertIn(f"tenant={TENANT}", call["argv"])
        self.assertIn("days=7", call["argv"])
        self.assertNotIn(token, json.dumps(call))
        self.assertIn("INSERT INTO auth.health_worker_credentials", call["stdin"])
        self.assertEqual(call["env"]["PGHOST"], "db.internal")
        self.assertEqual(call["env"]["PGPORT"], "5433")
        self.assertEqual(call["env"]["PGPASSWORD"], "s3cr@t")
        self.assertEqual(call["env"]["PGSSLMODE"], "require")
        self.assertFalse(any("s3cr" in a for a in call["argv"]))

    def test_issue_keeps_other_tenants_and_replaces_the_tenant_token(self):
        creds = self.tmp / "workers.json"
        creds.write_text(json.dumps({OTHER: "ohw_" + "a" * 64, TENANT: "ohw_" + "b" * 64}))
        res = self.run_script("issue", "--tenant", TENANT, "--file", str(creds))
        self.assertEqual(res.returncode, 0, res.stderr)
        data = json.loads(creds.read_text())
        self.assertEqual(data[OTHER], "ohw_" + "a" * 64)
        self.assertNotEqual(data[TENANT], "ohw_" + "b" * 64)
        self.assertIn("days=30", self.calls()[0]["argv"])

    def test_failed_registration_writes_nothing(self):
        creds = self.tmp / "workers.json"
        res = self.run_script("issue", "--tenant", TENANT, "--file", str(creds), FAKE_PSQL_EXIT="3")
        self.assertEqual(res.returncode, 1)
        self.assertIn("nothing was written", res.stderr)
        self.assertFalse(creds.exists())

    def test_invalid_inputs_are_refused_before_any_write(self):
        creds = self.tmp / "workers.json"
        cases = [
            (("issue", "--tenant", "dev-tenant", "--file", str(creds)), {}),
            (("issue", "--tenant", TENANT, "--file", str(creds), "--ttl-days", "0"), {}),
            (("issue", "--tenant", TENANT, "--file", str(creds), "--ttl-days", "366"), {}),
            (("issue", "--tenant", TENANT, "--file", str(creds)), {"HEALTH_WORKER_ADMIN_DATABASE_URL": ""}),
            (("issue", "--tenant", TENANT, "--file", str(creds)), {"HEALTH_WORKER_ADMIN_DATABASE_URL": "mysql://x/y"}),
        ]
        for args, env in cases:
            with self.subTest(args=args, env=env):
                self.assertNotEqual(self.run_script(*args, **env).returncode, 0)
        bad = self.tmp / "bad.json"
        bad.write_text(json.dumps({TENANT: "plaintext"}))
        self.assertEqual(self.run_script("issue", "--tenant", TENANT, "--file", str(bad)).returncode, 1)
        self.assertEqual(self.calls(), [])
        self.assertFalse(creds.exists())

    def test_revoke_keeps_the_held_credential(self):
        held = "ohw_" + "c" * 64
        creds = self.tmp / "workers.json"
        creds.write_text(json.dumps({TENANT: held}))
        res = self.run_script("revoke", "--tenant", TENANT, "--keep-file", str(creds))
        self.assertEqual(res.returncode, 0, res.stderr)
        (call,) = self.calls()
        self.assertIn(f"keep={hashlib.sha256(held.encode()).hexdigest()}", call["argv"])
        self.assertIn("SET revoked_at = now()", call["stdin"])

    def test_revoke_without_keep_revokes_all_and_refuses_a_file_without_the_tenant(self):
        self.assertEqual(self.run_script("revoke", "--tenant", TENANT).returncode, 0)
        self.assertIn("keep=", self.calls()[0]["argv"])
        creds = self.tmp / "workers.json"
        creds.write_text(json.dumps({OTHER: "ohw_" + "a" * 64}))
        res = self.run_script("revoke", "--tenant", TENANT, "--keep-file", str(creds))
        self.assertEqual(res.returncode, 1)
        self.assertEqual(len(self.calls()), 1)


if __name__ == "__main__":
    unittest.main()
