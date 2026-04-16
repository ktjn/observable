# Design: Metrics Storage Decision and Local Dev Story

**Date:** 2026-04-16
**Status:** Approved
**Scope:** Resolves two open gaps identified in spec review pass

---

## Problem

Two gaps were blocking Phase 1 execution:

1. **Metrics storage**: `spec/03-storage.md §5.1` listed two valid options (ClickHouse vs. dedicated TSDB) without committing to one. Phase 1 step 7 (basic metrics ingestion and storage) cannot start without a decision.

2. **Local dev story**: No spec defined how to run the stack locally. With Rust services + ClickHouse + Kafka/Redpanda + Postgres + OpenFGA, contributors had no canonical starting point.

---

## Decisions

### 1. Metrics Storage: ClickHouse (Phase 1)

Use ClickHouse for metrics in Phase 1 and until a concrete performance or cardinality constraint justifies a dedicated TSDB.

**Rationale:**
- ClickHouse is already chosen for logs and traces. Adding metrics reuses the same engine, operational runbook, retention model, and query layer.
- The domain model already defines `MetricSeries` and `MetricPoint` table shapes that map cleanly to ClickHouse column families.
- Phase 1 is about proving the ingest-to-query path with minimum operational complexity. A second storage engine before a single tenant is running is premature.
- Multiple production observability platforms (Signoz, Quickwit-adjacent stacks) use ClickHouse for metrics at scale.

**Revisit condition:** If Phase 2 or Phase 3 cardinality testing reveals that ClickHouse cannot meet the P50 < 1s query target for high-cardinality metric workloads, open a new ADR to evaluate VictoriaMetrics or Thanos as a metrics-specific tier. The query facade already abstracts storage engines from clients, so a later migration is contained.

**Spec changes required:**
- `spec/03-storage.md §5.1`: Replace "Two valid options" with committed ClickHouse decision and revisit condition.
- `spec/adr/ADR-003-clickhouse-boundary.md`: Extend ClickHouse boundary explicitly to metrics (currently scoped to logs and traces).

---

### 2. Local Dev: Docker Compose + native services

**Approach:** Docker Compose starts all external dependencies. Rust services and the React frontend run natively.

**Developer workflow:**
```
make dev          # starts Docker Compose dependency stack
cargo run -p <service>   # run a specific Rust service
npm run dev       # run the React frontend
```

**Docker Compose services (dependency stack):**

| Service       | Image                        | Port  | Purpose                        |
|---------------|------------------------------|-------|-------------------------------|
| clickhouse    | clickhouse/clickhouse-server | 8123, 9000 | Telemetry store          |
| redpanda      | redpandadata/redpanda        | 9092, 9644 | Durable queue/stream     |
| postgres      | postgres:16                  | 5432  | Control plane metadata store  |
| openfga       | openfga/openfga              | 8080  | Fine-grained auth store       |

**Rules:**
- Local env uses fixture API keys and seeded tenant data — no production secrets needed.
- Each Rust service reads config from environment variables with local defaults in `.env.local` at the repo root (gitignored; `.env.local.example` is committed as a template).
- ClickHouse schema migrations run automatically on service startup in local mode; in CI and production they are explicit migration steps.
- The Compose stack must start cleanly from scratch with `docker compose up` — no manual seed steps except those in `make dev`.

**Spec changes required:**
- `spec/12-deployment.md`: Add `§19.6 Local Development` section documenting the above.

---

## What this does NOT change

- Technology choices for Rust, React, Kafka/Redpanda, Postgres, OpenFGA remain as specified.
- The queue/stream decision (gap #2 from the review) is a separate ADR ratification and is not addressed here.
- Phase 0 exit gate status is not adjudicated here.
- All other open gaps from the review pass are deferred.

---

## Affected files

| File | Change |
|------|--------|
| `spec/03-storage.md` | Commit metrics to ClickHouse; remove hedge |
| `spec/adr/ADR-003-clickhouse-boundary.md` | Extend boundary to include metrics |
| `spec/12-deployment.md` | Add §19.6 Local Development |
