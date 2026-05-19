from __future__ import annotations

import json
import random
import uuid
from datetime import datetime

from seed.world import Operation, Service, Tenant, apply_stories, sample_duration_ms

SPAN_COLS = [
    "tenant_id", "trace_id", "span_id", "parent_span_id",
    "service_name", "service_namespace", "service_version",
    "operation_name", "span_kind",
    "start_time_unix_nano", "end_time_unix_nano", "duration_ns",
    "status_code", "status_message",
    "attributes", "resource_attributes",
    "environment", "host_id", "workload", "deployment_id",
]

EVENT_COLS = [
    "tenant_id", "trace_id", "span_id", "event_index",
    "name", "timestamp_unix_nano", "attributes",
]

_SPAN_KIND = {"api": "SERVER", "frontend": "SERVER", "worker": "INTERNAL", "consumer": "CONSUMER"}


def generate_trace(
    tenant: Tenant,
    service: Service,
    operation: Operation,
    dt: datetime,
    rng: random.Random,
) -> tuple[list[list], list[list], str, str, bool, float]:
    """
    Generate span rows and span_event rows for one request.

    Returns (span_rows, event_rows, trace_id, root_span_id, is_error, root_duration_ms).
    """
    latency_mult, error_rate, version_override = apply_stories(
        tenant, dt, service.service_name, operation.error_rate
    )
    version = version_override or service.service_version
    is_error = rng.random() < error_rate

    trace_id = uuid.uuid4().hex + uuid.uuid4().hex  # 32 hex chars, no dashes
    root_span_id = uuid.uuid4().hex[:16]
    dt_ns = int(dt.timestamp() * 1e9)
    host = f"host-{rng.randint(1, 10)}"

    root_duration_ms = sample_duration_ms(operation.p50_ms, operation.p99_ms, rng) * latency_mult
    root_duration_ns = int(root_duration_ms * 1e6)

    span_rows: list[list] = []
    event_rows: list[list] = []

    # Root span
    span_rows.append([
        tenant.tenant_id,
        trace_id,
        root_span_id,
        None,
        service.service_name,
        "",
        version,
        operation.name,
        _SPAN_KIND.get(service.service_type, "INTERNAL"),
        dt_ns,
        dt_ns + root_duration_ns,
        root_duration_ns,
        "ERROR" if is_error else "OK",
        "request failed" if is_error else "",
        json.dumps({"http.method": operation.name.split()[0] if " " in operation.name else operation.name}),
        json.dumps({"service.name": service.service_name, "service.version": version}),
        tenant.environment,
        host,
        service.service_name,
        "",
    ])

    if is_error:
        event_rows.append([
            tenant.tenant_id,
            trace_id,
            root_span_id,
            0,
            "exception",
            dt_ns + root_duration_ns // 2,
            json.dumps({
                "exception.type": "RuntimeError",
                "exception.message": f"{operation.name} failed",
            }),
        ])

    # Child spans for downstream services
    child_offset_ns = rng.randint(1_000_000, 5_000_000)  # 1–5 ms processing before first call
    service_map = {s.service_name: s for s in tenant.services}

    for i, downstream_name in enumerate(service.downstream):
        down_svc = service_map.get(downstream_name)
        if down_svc is None:
            continue
        child_op = rng.choice(down_svc.operations)
        child_duration_ms = sample_duration_ms(child_op.p50_ms, child_op.p99_ms, rng)
        child_duration_ns = int(child_duration_ms * 1e6)
        child_start_ns = dt_ns + child_offset_ns
        child_span_id = uuid.uuid4().hex[:16]
        child_is_error = is_error and rng.random() < 0.5

        span_rows.append([
            tenant.tenant_id,
            trace_id,
            child_span_id,
            root_span_id,
            down_svc.service_name,
            "",
            down_svc.service_version,
            child_op.name,
            "CLIENT",
            child_start_ns,
            child_start_ns + child_duration_ns,
            child_duration_ns,
            "ERROR" if child_is_error else "OK",
            "downstream failure" if child_is_error else "",
            json.dumps({}),
            json.dumps({"service.name": down_svc.service_name}),
            tenant.environment,
            host,
            service.service_name,
            "",
        ])

        child_offset_ns += child_duration_ns + rng.randint(500_000, 2_000_000)

    return span_rows, event_rows, trace_id, root_span_id, is_error, root_duration_ms
