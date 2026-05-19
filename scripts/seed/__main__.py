from __future__ import annotations

import argparse
import math
import random
import sys
from datetime import datetime, timedelta, timezone

from seed.inserter import Inserter
from seed.logs import LOG_COLS, generate_background_log, generate_request_log
from seed.metrics import POINT_COLS, SERIES_COLS, generate_metric_points, make_series_rows
from seed.pg_seeder import seed_postgres
from seed.traces import EVENT_COLS, SPAN_COLS, generate_trace
from seed.world import WorldModel, build_world, load_profile, traffic_multiplier

# How often to emit background logs (per service, per minute bucket)
_BACKGROUND_LOG_PROB = 0.05
# Metrics cadence: emit one tick per N seconds of simulated time
_METRIC_CADENCE_S = 15


def _parse(argv: list[str] | None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Seed Observable ClickHouse + Postgres with historical telemetry")
    p.add_argument("--profile", default="small", choices=["small", "medium", "large"])
    p.add_argument("--tenants", type=int, default=None)
    p.add_argument("--services", type=int, default=None, dest="services_per_tenant")
    p.add_argument("--days", type=int, default=None)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--batch-size", type=int, default=10_000)
    p.add_argument("--resume", action="store_true", help="Skip tenants that already have span data")
    p.add_argument("--no-postgres", action="store_true", help="Skip Postgres seeding")
    p.add_argument("--dry-run", action="store_true", help="Print world summary, insert nothing")
    p.add_argument("--clickhouse-host", default="localhost")
    p.add_argument("--clickhouse-port", type=int, default=8123)
    p.add_argument("--clickhouse-user", default="default")
    p.add_argument("--clickhouse-password", default="observable")
    p.add_argument("--postgres-url", default="postgresql://observable:observable@localhost:5432/observable")
    return p.parse_args(argv)


def run(argv: list[str] | None = None) -> None:
    args = _parse(argv)

    overrides = {k: v for k, v in {
        "tenants": args.tenants,
        "services_per_tenant": args.services_per_tenant,
        "days": args.days,
    }.items() if v is not None}

    world = load_profile(args.profile, seed=args.seed, **overrides)

    print(f"World: {len(world.tenants)} tenants, "
          f"{sum(len(t.services) for t in world.tenants)} total services, "
          f"{(world.end_time - world.start_time).days} days")
    print(f"Window: {world.start_time.isoformat()} -> {world.end_time.isoformat()}")

    if not args.no_postgres:
        seed_postgres(world.tenants, args.postgres_url, dry_run=args.dry_run)

    if args.dry_run:
        print("[dry-run] Skipping ClickHouse inserts.")
        return

    inserter = Inserter(
        host=args.clickhouse_host,
        port=args.clickhouse_port,
        user=args.clickhouse_user,
        password=args.clickhouse_password,
        batch_size=args.batch_size,
    )

    try:
        _run_inserts(world, inserter, args.resume)
    finally:
        inserter.close()


def _run_inserts(world: WorldModel, inserter: Inserter, resume: bool) -> None:
    # Build per-tenant series rows upfront (idempotent for ReplacingMergeTree)
    all_series: list[list] = []
    for tenant in world.tenants:
        for svc in tenant.services:
            all_series.extend(make_series_rows(tenant, svc))
    inserter.insert_in_batches("observable.metric_series", SERIES_COLS, all_series, desc="metric_series")

    # Time walk: 1-minute buckets
    span_buf: list[list] = []
    event_buf: list[list] = []
    log_buf: list[list] = []
    point_buf: list[list] = []

    # Pre-build series_rows lookup: (tenant_id, service_name) -> list[series_row]
    series_lookup: dict[tuple[str, str], list[list]] = {}
    for tenant in world.tenants:
        for svc in tenant.services:
            series_lookup[(tenant.tenant_id, svc.service_name)] = make_series_rows(tenant, svc)

    # Determine tenants to skip (--resume)
    skip_tenant_ids: set[str] = set()
    if resume:
        for tenant in world.tenants:
            count = inserter.row_count("observable.spans", tenant.tenant_id)
            if count > 0:
                print(f"[resume] Skipping tenant {tenant.name} ({count} spans exist)")
                skip_tenant_ids.add(tenant.tenant_id)

    active_tenants = [t for t in world.tenants if t.tenant_id not in skip_tenant_ids]
    if not active_tenants:
        print("Nothing to insert (all tenants already have data).")
        return

    total_minutes = int((world.end_time - world.start_time).total_seconds() / 60)
    print(f"Walking {total_minutes:,} minute-buckets × {len(active_tenants)} tenants...")

    metric_tick_counter: dict[tuple[str, str], int] = {}

    current = world.start_time
    while current < world.end_time:
        multiplier = traffic_multiplier(current)
        bucket_seed = int(current.timestamp()) ^ world.seed
        rng = random.Random(bucket_seed)

        for tenant in active_tenants:
            for svc in tenant.services:
                expected = svc.rps * 60 * multiplier
                count = int(expected) + (1 if rng.random() < (expected % 1) else 0)

                for _ in range(max(0, count)):
                    op = rng.choice(svc.operations)
                    offset_s = rng.uniform(0, 60)
                    dt = current + timedelta(seconds=offset_s)

                    sp_rows, ev_rows, trace_id, root_sid, is_error, dur_ms = generate_trace(
                        tenant, svc, op, dt, rng
                    )
                    log_row = generate_request_log(
                        tenant, svc, op, trace_id, root_sid,
                        "ERROR" if is_error else "OK", dur_ms, dt, rng
                    )
                    span_buf.extend(sp_rows)
                    event_buf.extend(ev_rows)
                    log_buf.append(log_row)

                # Background log (low rate)
                if rng.random() < _BACKGROUND_LOG_PROB:
                    log_buf.append(generate_background_log(tenant, svc, current, rng))

                # Metric tick every _METRIC_CADENCE_S seconds of simulated time
                key = (tenant.tenant_id, svc.service_name)
                tick = metric_tick_counter.get(key, 0)
                elapsed_s = int((current - world.start_time).total_seconds())
                new_tick = elapsed_s // _METRIC_CADENCE_S
                if new_tick > tick:
                    metric_tick_counter[key] = new_tick
                    s_rows = series_lookup[(tenant.tenant_id, svc.service_name)]
                    point_buf.extend(generate_metric_points(tenant, svc, s_rows, current, count, rng))

        current += timedelta(minutes=1)

        # Flush when buffers get large to avoid OOM
        if len(span_buf) >= 50_000:
            inserter.insert_in_batches("observable.spans", SPAN_COLS, span_buf, desc="spans")
            inserter.insert_in_batches("observable.span_events", EVENT_COLS, event_buf, desc="span_events")
            inserter.insert_in_batches("observable.logs", LOG_COLS, log_buf, desc="logs")
            inserter.insert_in_batches("observable.metric_points", POINT_COLS, point_buf, desc="metric_points")
            span_buf.clear(); event_buf.clear(); log_buf.clear(); point_buf.clear()

    # Final flush
    if span_buf:
        inserter.insert_in_batches("observable.spans", SPAN_COLS, span_buf, desc="spans (final)")
    if event_buf:
        inserter.insert_in_batches("observable.span_events", EVENT_COLS, event_buf, desc="span_events (final)")
    if log_buf:
        inserter.insert_in_batches("observable.logs", LOG_COLS, log_buf, desc="logs (final)")
    if point_buf:
        inserter.insert_in_batches("observable.metric_points", POINT_COLS, point_buf, desc="metric_points (final)")

    print("Done.")


def main() -> None:
    run(sys.argv[1:])


if __name__ == "__main__":
    main()
