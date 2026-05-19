import random
from datetime import datetime, timezone

from seed.world import Operation, Service, Tenant
from seed.logs import LOG_COLS, generate_request_log, generate_background_log


def _tenant() -> Tenant:
    op = Operation("POST /orders", 80.0, 400.0, 0.02)
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    return Tenant("tid-001", "acme", "production", "key", [svc])


def test_request_log_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "OK", 80.0, dt, random.Random(0))
    assert len(row) == len(LOG_COLS)


def test_background_log_row_length():
    tenant = _tenant()
    svc = tenant.services[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_background_log(tenant, svc, dt, random.Random(0))
    assert len(row) == len(LOG_COLS)


def test_error_span_produces_error_log():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "ERROR", 80.0, dt, random.Random(0))
    sev_num = row[LOG_COLS.index("severity_number")]
    sev_text = row[LOG_COLS.index("severity_text")]
    assert sev_num == 17
    assert sev_text == "ERROR"


def test_ok_fast_span_produces_info_log():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "OK", 30.0, dt, random.Random(0))
    assert row[LOG_COLS.index("severity_number")] == 9
    assert row[LOG_COLS.index("severity_text")] == "INFO"


def test_ok_slow_span_produces_warn_log():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "trace-abc", "span-xyz", "OK", 600.0, dt, random.Random(0))
    assert row[LOG_COLS.index("severity_number")] == 13
    assert row[LOG_COLS.index("severity_text")] == "WARN"


def test_request_log_links_trace_and_span():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "mytrace", "myspan", "OK", 50.0, dt, random.Random(0))
    assert row[LOG_COLS.index("trace_id")] == "mytrace"
    assert row[LOG_COLS.index("span_id")] == "myspan"


def test_background_log_has_no_trace_link():
    tenant = _tenant()
    svc = tenant.services[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_background_log(tenant, svc, dt, random.Random(0))
    assert row[LOG_COLS.index("trace_id")] is None
    assert row[LOG_COLS.index("span_id")] is None


def test_tenant_id_stamped():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    row = generate_request_log(tenant, svc, op, "t", "s", "OK", 50.0, dt, random.Random(0))
    assert row[LOG_COLS.index("tenant_id")] == "tid-001"
