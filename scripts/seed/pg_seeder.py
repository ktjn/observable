from __future__ import annotations

import psycopg

from seed.world import Tenant, hash_key


def _hash_key(plaintext: str) -> str:
    return hash_key(plaintext)


def seed_postgres(tenants: list[Tenant], pg_url: str, dry_run: bool = False) -> None:
    """Insert tenants and api_keys into Postgres. Idempotent via ON CONFLICT DO NOTHING."""
    if dry_run:
        print(f"[dry-run] Would insert {len(tenants)} tenants + {len(tenants)} api_keys")
        return

    with psycopg.connect(pg_url) as conn:
        with conn.cursor() as cur:
            for tenant in tenants:
                cur.execute(
                    "INSERT INTO tenants (id, name) VALUES (%s, %s) ON CONFLICT (id) DO NOTHING",
                    (tenant.tenant_id, tenant.name),
                )
                cur.execute(
                    """
                    INSERT INTO api_keys (tenant_id, key_hash, name, role, environment)
                    VALUES (%s, %s, %s, %s, %s)
                    ON CONFLICT (key_hash) DO NOTHING
                    """,
                    (
                        tenant.tenant_id,
                        hash_key(tenant.api_key_plaintext),
                        f"seed-key-{tenant.name}",
                        "member",
                        tenant.environment,
                    ),
                )
        conn.commit()
