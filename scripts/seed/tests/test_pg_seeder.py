from unittest.mock import MagicMock, patch, call
from seed.world import Tenant, Service, Operation
from seed.pg_seeder import (
    seed_postgres,
    _hash_key,
    DEFAULT_SLO_TARGET,
    DEFAULT_SLO_WINDOW_DAYS,
    DEFAULT_SLO_BURN_RATE_FAST_THRESHOLD,
    DEFAULT_SLO_BURN_RATE_SLOW_THRESHOLD,
)
import hashlib


def _make_tenant(idx: int) -> Tenant:
    op = Operation("GET /health", 5.0, 50.0, 0.001)
    svc = Service("health-api", "1.0.0", "api", [op], [], 1.0)
    return Tenant(f"tid-{idx}", f"tenant-{idx}", "production", f"plaintext-key-{idx}", [svc])


def test_hash_key_is_sha256():
    expected = hashlib.sha256(b"hello").hexdigest()
    assert _hash_key("hello") == expected


def test_seed_postgres_dry_run_does_not_connect():
    tenants = [_make_tenant(0)]
    with patch("seed.pg_seeder.psycopg.connect") as mock_connect:
        seed_postgres(tenants, "postgresql://x/y", dry_run=True)
        mock_connect.assert_not_called()


def test_seed_postgres_inserts_tenant_and_api_key():
    tenants = [_make_tenant(0)]
    mock_conn = MagicMock()
    mock_cur = MagicMock()
    mock_conn.__enter__ = MagicMock(return_value=mock_conn)
    mock_conn.__exit__ = MagicMock(return_value=False)
    mock_conn.cursor.return_value.__enter__ = MagicMock(return_value=mock_cur)
    mock_conn.cursor.return_value.__exit__ = MagicMock(return_value=False)

    with patch("seed.pg_seeder.psycopg.connect", return_value=mock_conn):
        seed_postgres(tenants, "postgresql://x/y")

    calls = mock_cur.execute.call_args_list
    # First call inserts tenant
    assert "INSERT INTO tenants" in calls[0][0][0]
    assert "tid-0" in calls[0][0][1]
    # Second call inserts api_key
    assert "INSERT INTO api_keys" in calls[1][0][0]
    assert _hash_key("plaintext-key-0") in calls[1][0][1]


def test_seed_postgres_inserts_one_slo_per_service():
    op = Operation("GET /health", 5.0, 50.0, 0.001)
    tenant = Tenant(
        "tid-0",
        "tenant-0",
        "production",
        "plaintext-key-0",
        [
            Service("health-api", "1.0.0", "api", [op], [], 1.0),
            Service("worker", "1.0.0", "worker", [op], [], 1.0),
        ],
    )
    mock_conn = MagicMock()
    mock_cur = MagicMock()
    mock_conn.__enter__ = MagicMock(return_value=mock_conn)
    mock_conn.__exit__ = MagicMock(return_value=False)
    mock_conn.cursor.return_value.__enter__ = MagicMock(return_value=mock_cur)
    mock_conn.cursor.return_value.__exit__ = MagicMock(return_value=False)

    with patch("seed.pg_seeder.psycopg.connect", return_value=mock_conn):
        seed_postgres([tenant], "postgresql://x/y")

    calls = mock_cur.execute.call_args_list
    slo_calls = [c for c in calls if "INSERT INTO slo_definitions" in c[0][0]]
    assert len(slo_calls) == 2

    first_params = slo_calls[0][0][1]
    assert first_params == (
        "tid-0",
        "health-api",
        "production",
        DEFAULT_SLO_TARGET,
        DEFAULT_SLO_WINDOW_DAYS,
        DEFAULT_SLO_BURN_RATE_FAST_THRESHOLD,
        DEFAULT_SLO_BURN_RATE_SLOW_THRESHOLD,
        "health-api availability SLO",
    )
    assert 0.0 < DEFAULT_SLO_TARGET < 1.0


def test_seed_postgres_uses_on_conflict_do_nothing():
    tenants = [_make_tenant(0)]
    mock_conn = MagicMock()
    mock_cur = MagicMock()
    mock_conn.__enter__ = MagicMock(return_value=mock_conn)
    mock_conn.__exit__ = MagicMock(return_value=False)
    mock_conn.cursor.return_value.__enter__ = MagicMock(return_value=mock_cur)
    mock_conn.cursor.return_value.__exit__ = MagicMock(return_value=False)

    with patch("seed.pg_seeder.psycopg.connect", return_value=mock_conn):
        seed_postgres(tenants, "postgresql://x/y")

    for c in mock_cur.execute.call_args_list:
        assert "ON CONFLICT" in c[0][0]
