# Seed Generator Design

**Date:** 2026-05-18  
**Status:** Approved  
**Scope:** `scripts/seed/` — bulk historical telemetry seed for ClickHouse + Postgres

---

## 1. Purpose

The seed generator inserts a large, realistic corpus of historical telemetry directly into ClickHouse (spans, logs, metrics) and Postgres (tenants, api_keys). It serves four goals simultaneously:

1. **UI/feature development** — realistic multi-tenant, multi-service data across time so dashboards, trace waterfalls, and service maps render with real content.
2. **Performance/stress testing** — configurable volume up to hundreds of millions of rows to measure query latency, ClickHouse compression, and index behavior under load.
3. **Tenant isolation QA** — many distinct tenants with overlapping service names to verify cross-tenant data cannot bleed through in queries.
4. **Demo/showcase** — time-bounded "stories" (latency spikes, error bursts, deployments) produce compelling narratives visible in the UI.

---

## 2. Architecture

```
scripts/
  seed.py                  # thin entry point
  seed/
    __main__.py            # argparse CLI + top-level orchestration
    world.py               # world model: tenants, service graphs, stories, profiles
    pg_seeder.py           # postgres: INSERT tenants + api_keys
    traces.py              # span + span_events generator
    logs.py                # log record generator
    metrics.py             # metric_series + metric_points generator
    inserter.py            # clickhouse batch inserter (clickhouse-connect)
    profiles/
      small.json
      medium.json
      large.json
    requirements.txt
Dockerfile.seed
```

Five components:

| Component | Responsibility |
|-----------|---------------|
| **World model** | Builds topology first; all signals are derived from it |
| **Postgres seeder** | Writes tenants + api_keys before any ClickHouse data |
| **Signal generators** | Derive spans, logs, metrics from world model with cross-signal coherence |
| **ClickHouse inserter** | Bulk-inserts in configurable batches via `clickhouse-connect` |
| **Docker Compose service** | `seed` profile service; one-command entry point |

---

## 3. World Model

The world model defines the complete synthetic universe before any rows are generated. It is serializable to JSON so any run can be replayed deterministically via `--seed <N>`.

### 3.1 Tenants

Each tenant has:
- `tenant_id` (UUID, deterministically derived from seed + index)
- `name` (e.g., `acme-corp`, `globex-inc`)
- `environment` (`production` or `staging`)
- `size` (`large` | `medium` | `small`) — controls per-tenant RPS and service count
- `api_key` — written to Postgres `api_keys` table

### 3.2 Service Graphs

Each tenant owns a directed dependency graph of services. Service types are drawn from a catalog:

| Type | Examples |
|------|---------|
| HTTP API | `order-api`, `product-api`, `user-api` |
| Frontend BFF | `web-frontend`, `mobile-bff` |
| Background worker | `order-worker`, `notification-worker` |
| Queue consumer | `event-consumer`, `audit-consumer` |
| Database client | implicit — represented as a downstream leaf span |

Each service has:
- `service_name`, `service_version`
- `operation_catalog`: list of named operations, each with `p50_ms`, `p99_ms`, `error_rate`
- `downstream`: list of service names it calls (defines the call chain for trace generation)

Latency for each span is sampled from a log-normal distribution parameterized by `(p50_ms, p99_ms)`.

### 3.3 Stories

Stories are time-bounded events that override baseline behavior during their window:

| Story type | Effect |
|------------|--------|
| `DeploymentStory` | Service version bump at `T + Xd`; brief error spike (2× error_rate for 30 min) |
| `LatencySpikeStory` | Operation p99 multiplied by factor F for Y hours (simulates slow DB or upstream) |
| `ErrorBurstStory` | Error rate elevated to E% for Z hours (simulates bad deploy or dependency outage) |
| `ColdStartStory` | Service starts from zero traffic, ramps to normal over 1 hour |

Stories are declared per-tenant in the world model JSON and are visible in the UI as anomalies.

---

## 4. Profiles

Profiles set defaults that CLI flags override.

| Profile | Tenants | Services/tenant | Days | ~Total rows |
|---------|---------|-----------------|------|-------------|
| `small` | 3 | 5 | 7 | ~500K |
| `medium` | 15 | 20 | 30 | ~20M |
| `large` | 50 | 40 | 90 | ~300M |

All counts are overridable:
```
python -m seed --profile medium --tenants 30 --days 60
```

---

## 5. Time Distribution

Traffic is not uniform. A per-bucket RPS multiplier is applied:

| Window | Multiplier |
|--------|-----------|
| Business hours 09:00–18:00 weekday | 1.0× |
| Evenings 18:00–23:00 | 0.4× |
| Nights 23:00–07:00 | 0.1× |
| Weekends | 0.2× of weekday equivalent |

Generation walks time forward in 1-minute buckets. Each bucket draws a Poisson-distributed request count per service (mean = `service_rps × 60s × multiplier`). Stories override the multiplier in their window.

---

## 6. Signal Generation

### 6.1 Traces (spans + span_events)

One "request" produces one trace:
- A root span for the entry-point service
- One child span per downstream hop in the call chain
- All spans share `trace_id`; parent-child links set via `parent_span_id`
- Duration: log-normal sampled; child durations sum into root duration
- Error: drawn against `operation.error_rate`; error spans get `status_code = ERROR`
- `span_events`: error spans emit an `exception` event (`exception.type`, `exception.message`, `exception.stacktrace`)

### 6.2 Logs

Each request produces one correlated log record:
- Same `trace_id` and `span_id` as the root span
- Severity: INFO (normal), WARN (duration > p99), ERROR (span errored)
- Body: realistic template messages per operation (e.g., `"POST /orders 201 142ms"`)

Each service also emits background logs at ~0.05 Hz (heartbeats, GC events) unlinked from traces.

### 6.3 Metrics

On a fixed 15s cadence per service:
- `metric_series` row inserted once per `(tenant_id, service_name, metric_name, attributes)` — idempotent via `ReplacingMergeTree`
- `metric_points` per cadence tick:

| Metric name | Type | Description |
|-------------|------|-------------|
| `http.server.request_count` | counter | Total requests since service start |
| `http.server.duration` | histogram | Request duration distribution |
| `process.memory.usage` | gauge | Simulated memory usage (random walk) |
| `http.server.error_count` | counter | Total errors since service start |

Metric values are derived from the same Poisson/log-normal model as traces, so metrics and traces stay coherent during story windows.

---

## 7. Insertion Strategy

**Order:** Postgres first, then ClickHouse: `metric_series` → `spans` + `span_events` → `logs` → `metric_points`.

This ordering lets the platform serve partial results via query-api immediately as each table fills.

**Batching:** Rows are accumulated in memory and flushed at 10,000 rows (configurable via `--batch-size`). Uses `clickhouse-connect` (HTTP/8123) for compatibility with the existing stack.

**Progress:** `tqdm` progress bar per signal type, with estimated time remaining.

**Resumability:** `--resume` skips any tenant where `SELECT count() FROM observable.spans WHERE tenant_id = ?` already returns rows.

---

## 8. CLI

```
python scripts/seed.py [options]

Options:
  --profile    small|medium|large      Profile preset (default: small)
  --tenants    N                        Override tenant count
  --services   N                        Override services per tenant
  --days       N                        Override history length in days
  --seed       N                        Random seed for deterministic world (default: 42)
  --batch-size N                        Rows per ClickHouse insert (default: 10000)
  --resume                              Skip tenants that already have data
  --no-postgres                         Skip Postgres seeding (ClickHouse only)
  --dry-run                             Print world model summary, insert nothing
  --clickhouse-url  URL                 (default: http://localhost:8123)
  --clickhouse-user USER                (default: default)
  --clickhouse-password PASSWORD
  --postgres-url    URL                 (default: postgresql://observable:observable@localhost:5432/observable)
```

---

## 9. Docker Compose Integration

A `seed` profile service is added to `docker-compose.yml`. It never starts unless explicitly requested.

```bash
# One-command seed with default profile
docker compose --profile seed run --rm seed

# Override profile
SEED_PROFILE=medium docker compose --profile seed run --rm seed
```

Environment variables `CLICKHOUSE_URL`, `CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`, `POSTGRES_URL`, and `SEED_PROFILE` are pre-wired to the local stack defaults in the Compose service definition.

---

## 10. Constraints & Non-Goals

- The generator does **not** go through the ingest-gateway or Redpanda pipeline — it inserts directly into ClickHouse. This is intentional: it decouples seed volume from pipeline throughput limits.
- It does **not** seed OpenFGA / authorization data. Tenant isolation tests using this data should use the dev API key or manually issue tokens.
- It does **not** produce RUM (`session_id`, `user_hash`) data in this version.
- Profiles are the intended parameterization surface. Adding new signal types (profiles, deployment markers) follows the same pattern: add a generator module, register it in `__main__.py`.
