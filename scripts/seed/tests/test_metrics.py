import json
import random
from datetime import datetime, timezone

from seed.world import Operation, Service, Tenant
from seed.metrics import SERIES_COLS, POINT_COLS, make_series_rows, generate_metric_points


def _tenant() -> Tenant:
    op = Operation("POST /orders", 80.0, 400.0, 0.02)
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    return Tenant("tid-001", "acme", "production", "key", [svc])


def test_series_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    for row in rows:
        assert len(row) == len(SERIES_COLS)


def test_series_has_four_metrics_per_service():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    assert len(rows) == 4


def test_series_tenant_id_stamped():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    for row in rows:
        assert row[SERIES_COLS.index("tenant_id")] == "tid-001"


def test_point_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    series_rows = make_series_rows(tenant, svc)
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    points = generate_metric_points(tenant, svc, series_rows, dt, 100, random.Random(0))
    for row in points:
        assert len(row) == len(POINT_COLS)


def test_four_points_per_tick():
    tenant = _tenant()
    svc = tenant.services[0]
    series_rows = make_series_rows(tenant, svc)
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    points = generate_metric_points(tenant, svc, series_rows, dt, 100, random.Random(0))
    assert len(points) == 4


def test_request_count_non_negative():
    tenant = _tenant()
    svc = tenant.services[0]
    series_rows = make_series_rows(tenant, svc)
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    points = generate_metric_points(tenant, svc, series_rows, dt, 50, random.Random(0))
    req_metric = next(p for p in points if p[POINT_COLS.index("metric_name")] == "http.server.request_count")
    assert req_metric[POINT_COLS.index("value_int")] >= 0


def test_series_ids_are_unique():
    tenant = _tenant()
    svc = tenant.services[0]
    rows = make_series_rows(tenant, svc)
    ids = [row[SERIES_COLS.index("metric_series_id")] for row in rows]
    assert len(ids) == len(set(ids))
