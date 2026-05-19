from __future__ import annotations

import json
import random
import uuid
from datetime import datetime, timezone

from seed.world import Service, Tenant

SERIES_COLS = [
    "tenant_id", "metric_series_id", "metric_name", "description", "unit",
    "metric_type", "is_monotonic", "aggregation_temporality",
    "attributes", "resource_attributes", "service_name", "environment", "created_at",
]

POINT_COLS = [
    "tenant_id", "metric_series_id", "metric_name", "service_name",
    "time_unix_nano", "start_time_unix_nano",
    "value_double", "value_int",
    "histogram_count", "histogram_sum",
    "histogram_bucket_counts", "histogram_explicit_bounds",
]

# Fixed metric definitions emitted by every service
_METRIC_DEFS = [
    ("http.server.request_count", "Total HTTP requests", "requests", "sum", 1, "DELTA"),
    ("http.server.error_count",   "Total HTTP errors",   "requests", "sum", 1, "DELTA"),
    ("http.server.duration",      "Request duration",    "ms",       "histogram", None, "DELTA"),
    ("process.memory.usage",      "Memory usage",        "bytes",    "gauge",     None, None),
]


def make_series_rows(tenant: Tenant, service: Service) -> list[list]:
    """Return one metric_series row per metric definition for this service.

    Series IDs are deterministic from tenant_id + service_name + metric_name
    so repeated calls produce the same UUIDs (idempotent for ReplacingMergeTree).
    """
    rows = []
    now_str = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
    for name, desc, unit, mtype, is_mono, temporality in _METRIC_DEFS:
        series_id = str(uuid.uuid5(
            uuid.NAMESPACE_OID,
            f"{tenant.tenant_id}:{service.service_name}:{name}",
        ))
        rows.append([
            tenant.tenant_id,
            series_id,
            name,
            desc,
            unit,
            mtype,
            is_mono,
            temporality,
            json.dumps({}),
            json.dumps({"service.name": service.service_name}),
            service.service_name,
            tenant.environment,
            now_str,
        ])
    return rows


def generate_metric_points(
    tenant: Tenant,
    service: Service,
    series_rows: list[list],
    dt: datetime,
    request_count: int,
    rng: random.Random,
) -> list[list]:
    """Return one metric_point row per series for the given 15-second tick."""
    ts_ns = int(dt.timestamp() * 1e9)
    error_count = max(0, int(request_count * rng.uniform(0.005, 0.05)))
    mem_bytes = rng.randint(64 * 1024 * 1024, 512 * 1024 * 1024)

    series_map = {r[SERIES_COLS.index("metric_name")]: r for r in series_rows}
    rows = []

    def _sid(name: str) -> str:
        return series_map[name][SERIES_COLS.index("metric_series_id")]

    # http.server.request_count — counter (value_int)
    rows.append([
        tenant.tenant_id, _sid("http.server.request_count"), "http.server.request_count", service.service_name,
        ts_ns, ts_ns, None, request_count, None, None, [], [],
    ])

    # http.server.error_count — counter (value_int)
    rows.append([
        tenant.tenant_id, _sid("http.server.error_count"), "http.server.error_count", service.service_name,
        ts_ns, ts_ns, None, error_count, None, None, [], [],
    ])

    # http.server.duration — histogram
    bounds = [5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0]
    bucket_counts = [rng.randint(0, max(1, request_count // 10)) for _ in range(len(bounds) + 1)]
    total_dur = sum(b * c for b, c in zip(bounds, bucket_counts[:-1]))
    rows.append([
        tenant.tenant_id, _sid("http.server.duration"), "http.server.duration", service.service_name,
        ts_ns, ts_ns, None, None,
        sum(bucket_counts), total_dur,
        bucket_counts, bounds,
    ])

    # process.memory.usage — gauge (value_int)
    rows.append([
        tenant.tenant_id, _sid("process.memory.usage"), "process.memory.usage", service.service_name,
        ts_ns, None, None, mem_bytes, None, None, [], [],
    ])

    return rows
