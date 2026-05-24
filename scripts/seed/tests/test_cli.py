import sys
from importlib import import_module
from unittest.mock import MagicMock, patch


def test_cli_module_imports_without_optional_clients(monkeypatch):
    monkeypatch.delitem(sys.modules, "seed.__main__", raising=False)
    monkeypatch.delitem(sys.modules, "seed.inserter", raising=False)
    monkeypatch.delitem(sys.modules, "seed.pg_seeder", raising=False)
    monkeypatch.setitem(sys.modules, "clickhouse_connect", None)
    monkeypatch.setitem(sys.modules, "psycopg", None)

    module = import_module("seed.__main__")
    assert callable(module.run)


def test_dry_run_exits_zero():
    with patch("seed.pg_seeder.seed_postgres") as mock_pg, \
         patch("seed.inserter.clickhouse_connect.get_client", return_value=MagicMock()):
        from seed.__main__ import run
        run([
            "--profile", "small",
            "--dry-run",
            "--clickhouse-host", "localhost",
            "--clickhouse-port", "8123",
            "--clickhouse-user", "default",
            "--clickhouse-password", "pw",
            "--postgres-url", "postgresql://x/y",
        ])
        mock_pg.assert_called_once()


def test_no_postgres_flag_skips_pg_seeder():
    with patch("seed.pg_seeder.seed_postgres") as mock_pg, \
         patch("seed.inserter.clickhouse_connect.get_client", return_value=MagicMock()):
        from seed.__main__ import run
        run([
            "--profile", "small",
            "--dry-run",
            "--no-postgres",
            "--clickhouse-host", "localhost",
            "--clickhouse-port", "8123",
            "--clickhouse-user", "default",
            "--clickhouse-password", "pw",
            "--postgres-url", "postgresql://x/y",
        ])
        mock_pg.assert_not_called()
