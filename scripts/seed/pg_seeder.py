from __future__ import annotations

import psycopg

from seed.world import Tenant, hash_key


def _hash_key(plaintext: str) -> str:
    return hash_key(plaintext)


# Google SRE-style default multi-window burn rate thresholds, matching the
# baseline SLO seeded for the dev tenant in migrations/postgres/021_create_slo_definitions.sql.
DEFAULT_SLO_TARGET = 0.995
DEFAULT_SLO_WINDOW_DAYS = 30
DEFAULT_SLO_BURN_RATE_FAST_THRESHOLD = 14.4
DEFAULT_SLO_BURN_RATE_SLOW_THRESHOLD = 1.0


def seed_postgres(tenants: list[Tenant], pg_url: str, dry_run: bool = False) -> None:
    """Insert tenants, api_keys, and baseline SLOs into Postgres. Idempotent via ON CONFLICT DO NOTHING."""
    if dry_run:
        service_count = sum(len(tenant.services) for tenant in tenants)
        print(
            f"[dry-run] Would insert {len(tenants)} tenants + {len(tenants)} api_keys "
            f"+ {service_count} SLOs"
        )
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
                for service in tenant.services:
                    cur.execute(
                        """
                        INSERT INTO slo_definitions (
                            tenant_id, service_name, environment, sli_type, target,
                            window_days, burn_rate_fast_threshold, burn_rate_slow_threshold,
                            description
                        )
                        VALUES (%s, %s, %s, 'availability', %s, %s, %s, %s, %s)
                        ON CONFLICT (tenant_id, service_name, environment, sli_type) DO NOTHING
                        """,
                        (
                            tenant.tenant_id,
                            service.service_name,
                            tenant.environment,
                            DEFAULT_SLO_TARGET,
                            DEFAULT_SLO_WINDOW_DAYS,
                            DEFAULT_SLO_BURN_RATE_FAST_THRESHOLD,
                            DEFAULT_SLO_BURN_RATE_SLOW_THRESHOLD,
                            f"{service.service_name} availability SLO",
                        ),
                    )
        conn.commit()
