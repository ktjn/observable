import json
import random
from datetime import datetime, timezone

from seed.world import Operation, Service, Story, Tenant
from seed.traces import SPAN_COLS, EVENT_COLS, generate_trace


def _tenant() -> Tenant:
    op = Operation("POST /orders", 80.0, 400.0, 0.0)  # error_rate=0 for predictable tests
    downstream_op = Operation("GET /products", 20.0, 100.0, 0.0)
    root_svc = Service("order-api", "1.0.0", "api", [op], ["product-api"], 10.0)
    down_svc = Service("product-api", "2.0.0", "api", [downstream_op], [], 20.0)
    return Tenant("tid-001", "acme", "production", "key", [root_svc, down_svc])


def test_span_row_length_matches_cols():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    rng = random.Random(42)
    span_rows, event_rows, trace_id, root_id, is_error, dur = generate_trace(
        tenant, svc, op, dt, rng
    )
    for row in span_rows:
        assert len(row) == len(SPAN_COLS), f"span row has {len(row)} fields, expected {len(SPAN_COLS)}"


def test_event_row_length_matches_cols():
    tenant = _tenant()
    # Force an error span
    op = Operation("POST /orders", 80.0, 400.0, 1.0)  # error_rate=1.0
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    tenant.services[0] = svc
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    rng = random.Random(42)
    span_rows, event_rows, *_ = generate_trace(tenant, svc, op, dt, rng)
    for row in event_rows:
        assert len(row) == len(EVENT_COLS)


def test_all_spans_share_trace_id():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, _, trace_id, *_ = generate_trace(tenant, svc, op, dt, random.Random(1))
    for row in span_rows:
        col_idx = SPAN_COLS.index("trace_id")
        assert row[col_idx] == trace_id


def test_root_span_has_no_parent():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, _, _, root_id, *_ = generate_trace(tenant, svc, op, dt, random.Random(2))
    root_row = next(r for r in span_rows if r[SPAN_COLS.index("span_id")] == root_id)
    assert root_row[SPAN_COLS.index("parent_span_id")] is None


def test_child_spans_reference_root_as_parent():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, _, _, root_id, *_ = generate_trace(tenant, svc, op, dt, random.Random(3))
    children = [r for r in span_rows if r[SPAN_COLS.index("span_id")] != root_id]
    for child in children:
        assert child[SPAN_COLS.index("parent_span_id")] == root_id


def test_error_span_has_status_code_error():
    op = Operation("POST /orders", 80.0, 400.0, 1.0)
    svc = Service("order-api", "1.0.0", "api", [op], [], 10.0)
    tenant = Tenant("tid", "acme", "production", "key", [svc])
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, event_rows, _, root_id, is_error, _ = generate_trace(
        tenant, svc, op, dt, random.Random(0)
    )
    assert is_error
    root_row = next(r for r in span_rows if r[SPAN_COLS.index("span_id")] == root_id)
    assert root_row[SPAN_COLS.index("status_code")] == "ERROR"
    assert len(event_rows) >= 1


def test_ok_span_produces_no_events():
    op = Operation("GET /health", 5.0, 20.0, 0.0)
    svc = Service("health-api", "1.0.0", "api", [op], [], 2.0)
    tenant = Tenant("tid", "acme", "production", "key", [svc])
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    _, event_rows, *_ = generate_trace(tenant, svc, op, dt, random.Random(0))
    assert len(event_rows) == 0


def test_tenant_id_stamped_on_all_rows():
    tenant = _tenant()
    svc = tenant.services[0]
    op = svc.operations[0]
    dt = datetime(2024, 1, 10, 12, 0, tzinfo=timezone.utc)
    span_rows, event_rows, *_ = generate_trace(tenant, svc, op, dt, random.Random(9))
    for row in span_rows:
        assert row[SPAN_COLS.index("tenant_id")] == "tid-001"
    for row in event_rows:
        assert row[EVENT_COLS.index("tenant_id")] == "tid-001"
