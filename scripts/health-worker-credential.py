#!/usr/bin/env python3
"""Provision tenant-bound health worker credentials (docs/plans/phase1-health-provider.md).

issue   Generate an `ohw_` token from 32 random bytes, register only its SHA-256 digest,
        tenant and expiry in auth.health_worker_credentials, and merge the plaintext
        into the worker's credentials file (JSON tenant id -> token, mode 0600).
revoke  Revoke the tenant's active credentials, except the one held in --keep-file.

The database URL comes from HEALTH_WORKER_ADMIN_DATABASE_URL, never from argv, and
must be a role that may write auth.health_worker_credentials (the migration owner).
The token is never printed. Rotate with: issue, restart health-service, then
revoke --keep-file.

Usage:
  scripts/health-worker-credential.py issue --tenant UUID --file PATH [--ttl-days N]
  scripts/health-worker-credential.py revoke --tenant UUID [--keep-file PATH]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import secrets
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path
from urllib.parse import parse_qsl, unquote, urlsplit

URL_ENV = "HEALTH_WORKER_ADMIN_DATABASE_URL"
MAX_TTL_DAYS = 365
LIBPQ_PARAMS = {"sslmode": "PGSSLMODE", "sslrootcert": "PGSSLROOTCERT", "sslcert": "PGSSLCERT",
                "sslkey": "PGSSLKEY", "connect_timeout": "PGCONNECT_TIMEOUT"}

ISSUE_SQL = """\
INSERT INTO auth.health_worker_credentials (token_hash_sha256, tenant_id, expires_at)
VALUES (:'digest', CAST(:'tenant' AS uuid), now() + make_interval(days => CAST(:'days' AS int)));
"""

REVOKE_SQL = """\
UPDATE auth.health_worker_credentials SET revoked_at = now()
WHERE tenant_id = CAST(:'tenant' AS uuid) AND revoked_at IS NULL AND token_hash_sha256 <> :'keep';
"""


def fail(msg: str) -> int:
    print(f"error: {msg}", file=sys.stderr)
    return 1


def tenant_arg(value: str) -> str:
    try:
        return str(uuid.UUID(value))
    except ValueError:
        raise argparse.ArgumentTypeError("tenant must be a UUID") from None


def ttl_arg(value: str) -> int:
    days = int(value)
    if not 1 <= days <= MAX_TTL_DAYS:
        raise argparse.ArgumentTypeError(f"ttl must be 1..{MAX_TTL_DAYS} days")
    return days


def pg_env(url: str) -> dict[str, str]:
    """libpq environment for a postgres:// URL, so the password never appears in argv."""
    u = urlsplit(url)
    if u.scheme not in ("postgres", "postgresql") or not u.hostname or not u.path.strip("/"):
        raise ValueError(f"{URL_ENV} must be a postgres://user:password@host:port/database URL")
    env = {"PGHOST": u.hostname, "PGDATABASE": unquote(u.path.lstrip("/"))}
    if u.port:
        env["PGPORT"] = str(u.port)
    if u.username:
        env["PGUSER"] = unquote(u.username)
    if u.password:
        env["PGPASSWORD"] = unquote(u.password)
    for k, v in parse_qsl(u.query):
        if k in LIBPQ_PARAMS:
            env[LIBPQ_PARAMS[k]] = v
    return env


def psql(url: str, sql: str, **variables: str) -> None:
    args = ["psql", "--no-psqlrc", "--quiet", "-v", "ON_ERROR_STOP=1"]
    for k, v in variables.items():
        args += ["-v", f"{k}={v}"]
    subprocess.run(args, input=sql, text=True, check=True, env={**os.environ, **pg_env(url)})


def read_file(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or not all(isinstance(v, str) and v.startswith("ohw_") for v in data.values()):
        raise ValueError(f"{path} must map tenant ids to ohw_ tokens")
    return data


def write_file(path: Path, data: dict[str, str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(data, f, indent=2, sort_keys=True)
            f.write("\n")
        os.replace(tmp, path)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def digest(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    i = sub.add_parser("issue")
    i.add_argument("--tenant", required=True, type=tenant_arg)
    i.add_argument("--file", required=True, type=Path)
    i.add_argument("--ttl-days", type=ttl_arg, default=30)
    r = sub.add_parser("revoke")
    r.add_argument("--tenant", required=True, type=tenant_arg)
    r.add_argument("--keep-file", type=Path)
    a = ap.parse_args(argv)

    url = os.environ.get(URL_ENV, "").strip()
    if not url:
        return fail(f"{URL_ENV} must be set")

    try:
        if a.cmd == "issue":
            pg_env(url)
            current = read_file(a.file)
            token = "ohw_" + secrets.token_hex(32)
            psql(url, ISSUE_SQL, digest=digest(token), tenant=a.tenant, days=str(a.ttl_days))
            write_file(a.file, {**current, a.tenant: token})
            print(f"issued health worker credential for tenant {a.tenant}, expires in {a.ttl_days} days; wrote {a.file}")
        else:
            keep = ""
            if a.keep_file:
                held = read_file(a.keep_file).get(a.tenant)
                if not held:
                    return fail(f"{a.keep_file} holds no credential for tenant {a.tenant}")
                keep = digest(held)
            psql(url, REVOKE_SQL, tenant=a.tenant, keep=keep)
            print(f"revoked active health worker credentials for tenant {a.tenant}" + (" except the held one" if keep else ""))
    except (ValueError, json.JSONDecodeError) as e:
        return fail(str(e))
    except subprocess.CalledProcessError:
        return fail("psql failed; nothing was written")
    except FileNotFoundError:
        return fail("psql is required")
    return 0


if __name__ == "__main__":
    sys.exit(main())
