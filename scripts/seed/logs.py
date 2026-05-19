from __future__ import annotations

import json
import random
import uuid
from datetime import datetime

from seed.world import Service, Tenant

LOG_COLS = [
    "tenant_id", "log_id", "timestamp_unix_nano", "observed_timestamp_unix_nano",
    "severity_number", "severity_text", "body",
    "trace_id", "span_id",
    "attributes", "resource_attributes",
    "service_name", "environment", "host_id", "fingerprint",
]

_SLOW_THRESHOLD_MS = 500.0

_BACKGROUND_MESSAGES = [
    "health check OK",
    "connection pool size 10",
    "GC pause {}ms",
    "cache hit ratio {}%",
    "metrics flushed",
    "keepalive sent",
    "config reloaded",
]


def generate_request_log(
    tenant: Tenant,
    service: Service,
    operation,
    trace_id: str,
    span_id: str,
    status_code: str,
    duration_ms: float,
    dt: datetime,
    rng: random.Random,
) -> list:
    """Generate one log record correlated to a root span."""
    ts_ns = int(dt.timestamp() * 1e9)
    is_error = status_code == "ERROR"
    is_slow = not is_error and duration_ms > _SLOW_THRESHOLD_MS

    if is_error:
        sev_num, sev_text = 17, "ERROR"
    elif is_slow:
        sev_num, sev_text = 13, "WARN"
    else:
        sev_num, sev_text = 9, "INFO"

    method = operation.name.split()[0] if " " in operation.name else operation.name
    path = operation.name.split()[-1] if " " in operation.name else ""
    code = "500" if is_error else "200"
    body = f"{method} {path} {code} {duration_ms:.0f}ms".strip()

    return [
        tenant.tenant_id,
        str(uuid.uuid4()),
        ts_ns,
        ts_ns + rng.randint(0, 1_000_000),
        sev_num,
        sev_text,
        body,
        trace_id,
        span_id,
        json.dumps({"operation": operation.name}),
        json.dumps({"service.name": service.service_name, "service.version": service.service_version}),
        service.service_name,
        tenant.environment,
        f"host-{rng.randint(1, 10)}",
        None,
    ]


def generate_background_log(
    tenant: Tenant,
    service: Service,
    dt: datetime,
    rng: random.Random,
) -> list:
    """Generate one unlinked background log (heartbeat, GC, etc.)."""
    ts_ns = int(dt.timestamp() * 1e9)
    tmpl = rng.choice(_BACKGROUND_MESSAGES)
    body = tmpl.format(rng.randint(2, 30)) if "{}" in tmpl else tmpl

    return [
        tenant.tenant_id,
        str(uuid.uuid4()),
        ts_ns,
        ts_ns,
        9,
        "INFO",
        body,
        None,
        None,
        json.dumps({}),
        json.dumps({"service.name": service.service_name}),
        service.service_name,
        tenant.environment,
        f"host-{rng.randint(1, 10)}",
        None,
    ]
