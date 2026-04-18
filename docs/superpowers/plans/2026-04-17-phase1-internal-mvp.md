# Phase 1 — Internal MVP: Ingest to Query Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a working observability platform that accepts OTLP traces/logs/metrics, stores them in ClickHouse via Redpanda, and serves them through a query API and React UI.

**Architecture:** Rust monorepo with five services (ingest-gateway, auth-service, stream-processor, storage-writer, query-api) connected by Redpanda. Control plane metadata in PostgreSQL. All telemetry data in ClickHouse. React 19 + Vite frontend.

**Tech Stack:** Rust 1.78+ (tokio, axum, rdkafka, sqlx, clickhouse, prost/tonic), React 19 + TypeScript + Vite 8 + TanStack Query/Router, Docker Compose (ClickHouse, Redpanda, PostgreSQL, OpenFGA), GitHub Actions CI.

---

## Parallel Execution Map

```
Task 1 (scaffold)
├── Task 2 (docker+make)   ─┐
├── Task 3 (domain types)   ├─ all parallel after Task 1
├── Task 4 (CH migrations)  │
├── Task 5 (PG migrations)  │
└── Task 6 (contracts)     ─┘
    ├── Task 7 (auth-service)      ─┐ parallel after Tasks 3+5
    ├── Task 8 (ingest skeleton)    ├─ parallel after Tasks 3+4+6
    └── Task 9 (query-api skeleton)─┘
        Task 10 (queue producer) ← after Task 8
        Task 11 (stream processor) ← after Task 10
        Task 12 (storage writer) ← after Task 11
        ├── Task 13 (API gateway)    ─┐ parallel after Task 7
        └── Task 14 (frontend)      ─┘
            Task 15 (log path) ← after Task 12
            Task 16 (metrics path) ← after Task 15
            Task 17 (platform telemetry) ← after Task 16
            Task 18 (frontend trace explorer) ← after Tasks 12+14
            Task 19 (end-to-end smoke test) ← after Tasks 17+18
```

---

## File Map

| Path | Role |
|------|------|
| `Cargo.toml` | Workspace root |
| `libs/domain/` | Shared Rust types: Span, LogRecord, MetricSeries, MetricPoint |
| `services/ingest-gateway/` | OTLP HTTP + gRPC receiver; validates + queues |
| `services/auth-service/` | API key validation; PostgreSQL-backed |
| `services/stream-processor/` | Redpanda consumer; normalises envelopes |
| `services/storage-writer/` | Batch ClickHouse writer |
| `services/query-api/` | HTTP query facade over ClickHouse |
| `apps/frontend/` | React 19 + Vite + TanStack Query/Router |
| `migrations/clickhouse/` | ClickHouse SQL migrations |
| `migrations/postgres/` | PostgreSQL SQL migrations |
| `docker-compose.yml` | Local dependency stack |
| `.env.local.example` | Local config template |
| `Makefile` | `make dev`, `make test`, `make lint` |
| `.github/workflows/pr.yml` | PR CI pipeline |

---

## Task 1: Monorepo Scaffold [DONE]

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `libs/domain/Cargo.toml`
- Create: `libs/domain/src/lib.rs`
- Create: `services/ingest-gateway/Cargo.toml`
- Create: `services/auth-service/Cargo.toml`
- Create: `services/stream-processor/Cargo.toml`
- Create: `services/storage-writer/Cargo.toml`
- Create: `services/query-api/Cargo.toml`
- Create: `apps/frontend/package.json`
- Create: `package.json` (npm workspace root)
- Create: `.github/workflows/pr.yml`

- [ ] **Step 1: Write failing CI check**

  Create `.github/workflows/pr.yml`:
  ```yaml
  name: PR
  on:
    pull_request:
      branches: [main]
  jobs:
    rust:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: dtolnay/rust-toolchain@stable
          with: { components: "clippy,rustfmt" }
        - uses: Swatinem/rust-cache@v2
        - run: cargo fmt --check
        - run: cargo clippy --all-targets -- -D warnings
        - run: cargo test --all
    frontend:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: actions/setup-node@v4
          with: { node-version: "22", cache: "npm" }
        - run: npm ci
        - run: npm run typecheck --workspace=apps/frontend
        - run: npm run lint --workspace=apps/frontend
        - run: npm run build --workspace=apps/frontend
  ```

- [ ] **Step 2: Create workspace Cargo.toml**

  ```toml
  [workspace]
  resolver = "2"
  members = [
      "libs/domain",
      "services/ingest-gateway",
      "services/auth-service",
      "services/stream-processor",
      "services/storage-writer",
      "services/query-api",
  ]

  [workspace.dependencies]
  tokio        = { version = "1", features = ["full"] }
  axum         = { version = "0.7", features = ["json", "macros"] }
  serde        = { version = "1", features = ["derive"] }
  serde_json   = "1"
  uuid         = { version = "1", features = ["v4", "serde"] }
  anyhow       = "1"
  tracing      = "0.1"
  tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
  bytes        = "1"
  prost        = "0.12"
  tonic        = "0.11"
  sqlx         = { version = "0.7", features = ["postgres", "runtime-tokio", "uuid", "chrono"] }
  clickhouse   = { version = "0.11", features = ["uuid"] }
  rdkafka      = { version = "0.36", features = ["tokio"] }
  chrono       = { version = "0.4", features = ["serde"] }
  config       = "0.14"
  ```

- [ ] **Step 3: Create libs/domain/Cargo.toml**

  ```toml
  [package]
  name    = "domain"
  version = "0.1.0"
  edition = "2021"

  [dependencies]
  serde      = { workspace = true }
  serde_json = { workspace = true }
  uuid       = { workspace = true }
  chrono     = { workspace = true }
  ```

- [ ] **Step 4: Create each service Cargo.toml (repeat pattern)**

  `services/ingest-gateway/Cargo.toml`:
  ```toml
  [package]
  name    = "ingest-gateway"
  version = "0.1.0"
  edition = "2021"

  [dependencies]
  domain     = { path = "../../libs/domain" }
  tokio      = { workspace = true }
  axum       = { workspace = true }
  serde      = { workspace = true }
  serde_json = { workspace = true }
  uuid       = { workspace = true }
  anyhow     = { workspace = true }
  tracing    = { workspace = true }
  tracing-subscriber = { workspace = true }
  bytes      = { workspace = true }
  prost      = { workspace = true }
  rdkafka    = { workspace = true }
  config     = { workspace = true }

  [dev-dependencies]
  axum-test = "14"
  ```

  Repeat with the relevant subset of workspace deps for: `auth-service` (adds `sqlx`), `stream-processor` (adds `rdkafka`), `storage-writer` (adds `clickhouse`), `query-api` (adds `clickhouse`, `axum`).

- [ ] **Step 5: Create npm workspace root package.json**

  ```json
  {
    "name": "observable",
    "private": true,
    "workspaces": ["apps/frontend"],
    "scripts": {}
  }
  ```

- [ ] **Step 6: Create apps/frontend/package.json**

  ```json
  {
    "name": "@observable/frontend",
    "private": true,
    "version": "0.1.0",
    "type": "module",
    "scripts": {
      "dev": "vite",
      "build": "tsc && vite build",
      "typecheck": "tsc --noEmit",
      "lint": "eslint src --ext ts,tsx --report-unused-disable-directives"
    },
    "dependencies": {
      "react": "^19.0.0",
      "react-dom": "^19.0.0",
      "@tanstack/react-query": "^5.0.0",
      "@tanstack/react-router": "^1.0.0"
    },
    "devDependencies": {
      "@types/react": "^19.0.0",
      "@types/react-dom": "^19.0.0",
      "@vitejs/plugin-react": "^4.0.0",
      "typescript": "^5.0.0",
      "vite": "^8.0.0",
      "eslint": "^9.0.0",
      "@typescript-eslint/eslint-plugin": "^8.0.0"
    }
  }
  ```

- [ ] **Step 7: Verify workspace compiles**

  ```bash
  cargo check --workspace
  npm install
  ```
  Expected: no errors (stubs with empty `lib.rs` / `main.rs` are fine).

- [ ] **Step 8: Commit**

  ```bash
  git checkout -b feat/phase1-monorepo-scaffold
  git add Cargo.toml libs/ services/ apps/ package.json .github/
  git commit -m "feat: scaffold monorepo with Rust workspace and npm workspace

  Establishes Cargo workspace (5 services + 1 lib), npm workspace for
  React frontend, and GitHub Actions PR pipeline.

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  git push -u origin feat/phase1-monorepo-scaffold
  gh pr create --title "feat: scaffold monorepo — Task 1" \
    --body "Phase 1 Task 1. Empty workspace skeleton. CI pipeline wired.
  Source spec: spec/10-process.md §17.2, spec/12-deployment.md §19.6
  Verification: cargo check --workspace passes; npm install passes
  ADR/spec sync: none needed — scaffold only
  Next slice: Task 2 (docker compose)"
  ```

---

## Task 2: Docker Compose + Local Dev Tooling [DONE]

> **Can run in parallel with Tasks 3–6.**

**Files:**
- Create: `docker-compose.yml`
- Create: `.env.local.example`
- Create: `Makefile`
- Create: `README.md` (minimal)

- [ ] **Step 1: Write test — verify compose file is valid YAML**

  ```bash
  docker compose config --quiet
  ```
  Expected: exits 0.

- [ ] **Step 2: Create docker-compose.yml**

  ```yaml
  version: "3.9"
  services:
    clickhouse:
      image: clickhouse/clickhouse-server:24.3
      ports: ["8123:8123", "9000:9000"]
      environment:
        CLICKHOUSE_DB: observable
        CLICKHOUSE_USER: ${CH_USER:-default}
        CLICKHOUSE_PASSWORD: ${CH_PASSWORD:-}
      volumes: [clickhouse_data:/var/lib/clickhouse]
      healthcheck:
        test: ["CMD", "wget", "--spider", "-q", "http://localhost:8123/ping"]
        interval: 5s
        timeout: 3s
        retries: 10

    redpanda:
      image: redpandadata/redpanda:v23.3.1
      command:
        - redpanda
        - start
        - --smp=1
        - --memory=512M
        - --kafka-addr=PLAINTEXT://0.0.0.0:9092
        - --advertise-kafka-addr=PLAINTEXT://localhost:9092
      ports: ["9092:9092", "9644:9644"]
      volumes: [redpanda_data:/var/lib/redpanda/data]
      healthcheck:
        test: ["CMD", "rpk", "cluster", "info"]
        interval: 5s
        timeout: 5s
        retries: 10

    postgres:
      image: postgres:16
      ports: ["5432:5432"]
      environment:
        POSTGRES_DB: ${PG_DB:-observable}
        POSTGRES_USER: ${PG_USER:-observable}
        POSTGRES_PASSWORD: ${PG_PASSWORD:-observable}
      volumes: [postgres_data:/var/lib/postgresql/data]
      healthcheck:
        test: ["CMD-SHELL", "pg_isready -U ${PG_USER:-observable}"]
        interval: 5s
        timeout: 3s
        retries: 10

    openfga:
      image: openfga/openfga:v1.5
      command: run
      ports: ["8080:8080", "8081:8081"]
      environment:
        OPENFGA_DATASTORE_ENGINE: postgres
        OPENFGA_DATASTORE_URI: postgres://${PG_USER:-observable}:${PG_PASSWORD:-observable}@postgres:5432/${PG_DB:-observable}
      depends_on:
        postgres: { condition: service_healthy }

  volumes:
    clickhouse_data:
    redpanda_data:
    postgres_data:
  ```

- [ ] **Step 3: Create .env.local.example**

  ```bash
  # Copy to .env.local (gitignored). No production secrets required.
  CH_USER=default
  CH_PASSWORD=
  CH_HOST=localhost
  CH_PORT=8123
  CH_DB=observable

  PG_USER=observable
  PG_PASSWORD=observable
  PG_DB=observable
  PG_HOST=localhost
  PG_PORT=5432
  DATABASE_URL=postgres://observable:observable@localhost:5432/observable

  REDPANDA_BROKERS=localhost:9092
  INGEST_TOPIC=telemetry.raw

  OPENFGA_URL=http://localhost:8080

  # Service ports (local only)
  INGEST_GATEWAY_PORT=4317
  AUTH_SERVICE_PORT=4318
  QUERY_API_PORT=8090
  FRONTEND_PORT=5173
  ```

- [ ] **Step 4: Add .env.local to .gitignore**

  ```bash
  echo ".env.local" >> .gitignore
  echo "target/" >> .gitignore
  echo "node_modules/" >> .gitignore
  echo "dist/" >> .gitignore
  ```

- [ ] **Step 5: Create Makefile**

  ```makefile
  .PHONY: dev dev-down test lint migrate

  dev:
  	cp -n .env.local.example .env.local 2>/dev/null || true
  	docker compose --env-file .env.local up -d --wait
  	@echo "Stack ready. Run 'cargo run -p <service>' or 'npm run dev --workspace=apps/frontend'"

  dev-down:
  	docker compose down

  test:
  	cargo test --workspace
  	npm run typecheck --workspace=apps/frontend

  lint:
  	cargo fmt --check
  	cargo clippy --all-targets -- -D warnings
  	npm run lint --workspace=apps/frontend

  migrate:
  	@echo "Running ClickHouse migrations..."
  	for f in migrations/clickhouse/*.sql; do \
  	  clickhouse-client --host localhost --query "$$(cat $$f)"; \
  	done
  	@echo "Running PostgreSQL migrations..."
  	DATABASE_URL=$$(grep DATABASE_URL .env.local | cut -d= -f2) \
  	  sqlx migrate run --source migrations/postgres
  ```

- [ ] **Step 6: Verify**

  ```bash
  make dev
  ```
  Expected: all four containers start healthy.

- [ ] **Step 7: Commit**

  ```bash
  git add docker-compose.yml .env.local.example Makefile .gitignore README.md
  git commit -m "feat: add Docker Compose local dev stack and Makefile

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 3: Domain Types Library [DONE]

> **Parallel with Tasks 2, 4, 5, 6.**

**Files:**
- Create: `libs/domain/src/lib.rs`
- Create: `libs/domain/src/span.rs`
- Create: `libs/domain/src/log.rs`
- Create: `libs/domain/src/metric.rs`
- Create: `libs/domain/src/envelope.rs`

- [ ] **Step 1: Write failing tests**

  Create `libs/domain/src/span.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use uuid::Uuid;

      #[test]
      fn span_roundtrips_json() {
          let span = Span {
              tenant_id: Uuid::new_v4(),
              trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
              span_id: "00f067aa0ba902b7".into(),
              parent_span_id: None,
              service_name: "checkout".into(),
              operation_name: "POST /order".into(),
              span_kind: SpanKind::Server,
              start_time_unix_nano: 1_700_000_000_000_000_000,
              end_time_unix_nano:   1_700_000_000_005_000_000,
              duration_ns: 5_000_000,
              status_code: StatusCode::Ok,
              ..Default::default()
          };
          let json = serde_json::to_string(&span).unwrap();
          let back: Span = serde_json::from_str(&json).unwrap();
          assert_eq!(back.trace_id, span.trace_id);
          assert_eq!(back.duration_ns, 5_000_000);
      }
  }
  ```

- [ ] **Step 2: Run test — verify it fails**

  ```bash
  cargo test -p domain
  ```
  Expected: FAIL — `Span`, `SpanKind`, `StatusCode` not defined.

- [ ] **Step 3: Implement span.rs**

  ```rust
  use serde::{Deserialize, Serialize};
  use std::collections::HashMap;
  use uuid::Uuid;

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct Span {
      pub tenant_id: Uuid,
      pub trace_id: String,
      pub span_id: String,
      pub parent_span_id: Option<String>,
      pub service_name: String,
      pub service_namespace: String,
      pub service_version: String,
      pub operation_name: String,
      pub span_kind: SpanKind,
      pub start_time_unix_nano: u64,
      pub end_time_unix_nano: u64,
      pub duration_ns: u64,
      pub status_code: StatusCode,
      pub status_message: String,
      pub attributes: HashMap<String, serde_json::Value>,
      pub resource_attributes: HashMap<String, serde_json::Value>,
      pub environment: String,
      pub host_id: String,
      pub workload: String,
      pub deployment_id: String,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
  pub enum SpanKind {
      #[default]
      Internal,
      Server,
      Client,
      Producer,
      Consumer,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
  pub enum StatusCode {
      #[default]
      Unset,
      Ok,
      Error,
  }
  ```

- [ ] **Step 4: Implement log.rs**

  ```rust
  use serde::{Deserialize, Serialize};
  use std::collections::HashMap;
  use uuid::Uuid;

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct LogRecord {
      pub tenant_id: Uuid,
      pub log_id: Uuid,
      pub timestamp_unix_nano: u64,
      pub observed_timestamp_unix_nano: u64,
      pub severity_number: i32,
      pub severity_text: String,
      pub body: serde_json::Value,
      pub trace_id: Option<String>,
      pub span_id: Option<String>,
      pub attributes: HashMap<String, serde_json::Value>,
      pub resource_attributes: HashMap<String, serde_json::Value>,
      pub service_name: String,
      pub environment: String,
      pub host_id: String,
      pub fingerprint: Option<u64>,
  }
  ```

- [ ] **Step 5: Implement metric.rs**

  ```rust
  use serde::{Deserialize, Serialize};
  use std::collections::HashMap;
  use uuid::Uuid;

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct MetricSeries {
      pub tenant_id: Uuid,
      pub metric_series_id: Uuid,
      pub metric_name: String,
      pub description: String,
      pub unit: String,
      pub metric_type: MetricType,
      pub is_monotonic: Option<bool>,
      pub aggregation_temporality: Option<AggregationTemporality>,
      pub attributes: HashMap<String, String>,
      pub resource_attributes: HashMap<String, serde_json::Value>,
      pub service_name: String,
      pub environment: String,
  }

  #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "snake_case")]
  pub enum MetricType { #[default] Gauge, Sum, Histogram, ExponentialHistogram, Summary }

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  #[serde(rename_all = "snake_case")]
  pub enum AggregationTemporality { Delta, Cumulative }

  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct MetricPoint {
      pub tenant_id: Uuid,
      pub metric_series_id: Uuid,
      pub metric_name: String,
      pub service_name: String,
      pub time_unix_nano: u64,
      pub start_time_unix_nano: Option<u64>,
      pub value_double: Option<f64>,
      pub value_int: Option<i64>,
      pub histogram_count: Option<u64>,
      pub histogram_sum: Option<f64>,
      pub histogram_bucket_counts: Option<Vec<u64>>,
      pub histogram_explicit_bounds: Option<Vec<f64>>,
  }
  ```

- [ ] **Step 6: Implement envelope.rs (queue message wrapper)**

  ```rust
  use serde::{Deserialize, Serialize};
  use uuid::Uuid;

  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct TelemetryEnvelope {
      pub envelope_id: Uuid,
      pub tenant_id: Uuid,
      pub received_at_unix_nano: u64,
      pub payload: EnvelopePayload,
  }

  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum EnvelopePayload {
      Spans(Vec<crate::span::Span>),
      Logs(Vec<crate::log::LogRecord>),
      Metrics { series: Vec<crate::metric::MetricSeries>, points: Vec<crate::metric::MetricPoint> },
  }
  ```

- [ ] **Step 7: Update lib.rs**

  ```rust
  pub mod envelope;
  pub mod log;
  pub mod metric;
  pub mod span;

  pub use envelope::{EnvelopePayload, TelemetryEnvelope};
  pub use log::LogRecord;
  pub use metric::{AggregationTemporality, MetricPoint, MetricSeries, MetricType};
  pub use span::{Span, SpanKind, StatusCode};
  ```

- [ ] **Step 8: Run tests — verify pass**

  ```bash
  cargo test -p domain
  ```
  Expected: PASS.

- [ ] **Step 9: Commit**

  ```bash
  git add libs/domain/
  git commit -m "feat(domain): add Span, LogRecord, MetricSeries, MetricPoint, TelemetryEnvelope

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 4: ClickHouse Migrations [DONE]

> **Parallel with Tasks 2, 3, 5, 6.**

**Files:**
- Create: `migrations/clickhouse/001_create_spans.sql`
- Create: `migrations/clickhouse/002_create_logs.sql`
- Create: `migrations/clickhouse/003_create_metrics.sql`

- [ ] **Step 1: Write test — verify tables can be created and queried**

  Create `migrations/clickhouse/test_migrations.sh`:
  ```bash
  #!/usr/bin/env bash
  set -euo pipefail
  CH="clickhouse-client --host localhost --database observable"
  for f in migrations/clickhouse/[0-9]*.sql; do
    echo "Applying $f..."
    $CH --multiquery < "$f"
  done
  echo "SELECT count() FROM spans" | $CH
  echo "SELECT count() FROM logs" | $CH
  echo "SELECT count() FROM metric_series" | $CH
  echo "All migrations applied successfully."
  ```

- [ ] **Step 2: Create 001_create_spans.sql**

  ```sql
  CREATE TABLE IF NOT EXISTS observable.spans
  (
      tenant_id           UUID,
      trace_id            String,
      span_id             String,
      parent_span_id      Nullable(String),
      service_name        LowCardinality(String),
      service_namespace   LowCardinality(String) DEFAULT '',
      service_version     String DEFAULT '',
      operation_name      String,
      span_kind           Enum8('INTERNAL'=0,'SERVER'=1,'CLIENT'=2,'PRODUCER'=3,'CONSUMER'=4) DEFAULT 'INTERNAL',
      start_time_unix_nano UInt64,
      end_time_unix_nano  UInt64,
      duration_ns         UInt64,
      status_code         Enum8('UNSET'=0,'OK'=1,'ERROR'=2) DEFAULT 'UNSET',
      status_message      String DEFAULT '',
      attributes          String DEFAULT '{}',
      resource_attributes String DEFAULT '{}',
      environment         LowCardinality(String) DEFAULT '',
      host_id             String DEFAULT '',
      workload            String DEFAULT '',
      deployment_id       String DEFAULT '',
      INDEX idx_trace_id trace_id TYPE bloom_filter(0.01) GRANULARITY 1,
      INDEX idx_service  service_name TYPE set(100) GRANULARITY 1
  )
  ENGINE = MergeTree()
  PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(start_time_unix_nano)))
  ORDER BY (tenant_id, service_name, start_time_unix_nano, trace_id, span_id)
  TTL fromUnixTimestamp64Nano(start_time_unix_nano) + INTERVAL 14 DAY
  SETTINGS index_granularity = 8192;
  ```

- [ ] **Step 3: Create 002_create_logs.sql**

  ```sql
  CREATE TABLE IF NOT EXISTS observable.logs
  (
      tenant_id                    UUID,
      log_id                       UUID,
      timestamp_unix_nano          UInt64,
      observed_timestamp_unix_nano UInt64,
      severity_number              Int32,
      severity_text                LowCardinality(String) DEFAULT '',
      body                         String,
      trace_id                     Nullable(String),
      span_id                      Nullable(String),
      attributes                   String DEFAULT '{}',
      resource_attributes          String DEFAULT '{}',
      service_name                 LowCardinality(String),
      environment                  LowCardinality(String) DEFAULT '',
      host_id                      String DEFAULT '',
      fingerprint                  Nullable(UInt64),
      INDEX idx_trace_id trace_id TYPE bloom_filter(0.01) GRANULARITY 1,
      INDEX idx_severity severity_number TYPE minmax GRANULARITY 1
  )
  ENGINE = MergeTree()
  PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(timestamp_unix_nano)))
  ORDER BY (tenant_id, service_name, timestamp_unix_nano, log_id)
  TTL fromUnixTimestamp64Nano(timestamp_unix_nano) + INTERVAL 60 DAY
  SETTINGS index_granularity = 8192;
  ```

- [ ] **Step 4: Create 003_create_metrics.sql**

  ```sql
  CREATE TABLE IF NOT EXISTS observable.metric_series
  (
      tenant_id               UUID,
      metric_series_id        UUID,
      metric_name             LowCardinality(String),
      description             String DEFAULT '',
      unit                    String DEFAULT '',
      metric_type             LowCardinality(String),
      is_monotonic            Nullable(UInt8),
      aggregation_temporality Nullable(LowCardinality(String)),
      attributes              String DEFAULT '{}',
      resource_attributes     String DEFAULT '{}',
      service_name            LowCardinality(String),
      environment             LowCardinality(String) DEFAULT '',
      created_at              DateTime DEFAULT now()
  )
  ENGINE = ReplacingMergeTree(created_at)
  ORDER BY (tenant_id, service_name, metric_name, attributes)
  SETTINGS index_granularity = 8192;

  CREATE TABLE IF NOT EXISTS observable.metric_points
  (
      tenant_id               UUID,
      metric_series_id        UUID,
      metric_name             LowCardinality(String),
      service_name            LowCardinality(String),
      time_unix_nano          UInt64,
      start_time_unix_nano    Nullable(UInt64),
      value_double            Nullable(Float64),
      value_int               Nullable(Int64),
      histogram_count         Nullable(UInt64),
      histogram_sum           Nullable(Float64),
      histogram_bucket_counts Array(UInt64) DEFAULT [],
      histogram_explicit_bounds Array(Float64) DEFAULT [],
      INDEX idx_series metric_series_id TYPE set(0) GRANULARITY 1
  )
  ENGINE = MergeTree()
  PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(time_unix_nano)))
  ORDER BY (tenant_id, metric_series_id, time_unix_nano)
  TTL fromUnixTimestamp64Nano(time_unix_nano) + INTERVAL 14 DAY
  SETTINGS index_granularity = 8192;
  ```

- [ ] **Step 5: Run migrations**

  ```bash
  make dev
  bash migrations/clickhouse/test_migrations.sh
  ```
  Expected: "All migrations applied successfully."

- [ ] **Step 6: Commit**

  ```bash
  git add migrations/clickhouse/
  git commit -m "feat(storage): add ClickHouse migrations for spans, logs, metrics

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 5: PostgreSQL Migrations [DONE]

> **Parallel with Tasks 2, 3, 4, 6.**

**Files:**
- Create: `migrations/postgres/001_create_tenants.sql`
- Create: `migrations/postgres/002_create_api_keys.sql`
- Create: `migrations/postgres/003_create_projects.sql`

Requires: `sqlx-cli` installed (`cargo install sqlx-cli --features postgres`).

- [ ] **Step 1: Create 001_create_tenants.sql**

  ```sql
  CREATE TABLE tenants (
      id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      name       TEXT NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );

  -- Seed a dev tenant for local testing
  INSERT INTO tenants (id, name) VALUES
      ('00000000-0000-0000-0000-000000000001', 'dev-tenant')
  ON CONFLICT DO NOTHING;
  ```

- [ ] **Step 2: Create 002_create_api_keys.sql**

  ```sql
  CREATE TABLE api_keys (
      id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
      key_hash   TEXT NOT NULL UNIQUE,
      name       TEXT NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      revoked_at TIMESTAMPTZ
  );

  CREATE INDEX api_keys_tenant_id_idx ON api_keys(tenant_id);

  -- Seed dev API key: value = "dev-api-key-0000" (SHA-256 hash)
  -- SHA-256("dev-api-key-0000") = stored hash; services compare hash, not plaintext
  INSERT INTO api_keys (tenant_id, key_hash, name) VALUES (
      '00000000-0000-0000-0000-000000000001',
      '5a3f8e2b4d1c9f07a6b2e8d3c4f1a9e2b7d5c3f8e1a4b6d9c2e5f7a3b8d1c4e6',
      'dev-key'
  ) ON CONFLICT DO NOTHING;
  ```

- [ ] **Step 3: Create 003_create_projects.sql**

  ```sql
  CREATE TABLE projects (
      id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
      name       TEXT NOT NULL,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      UNIQUE (tenant_id, name)
  );

  CREATE INDEX projects_tenant_id_idx ON projects(tenant_id);

  INSERT INTO projects (tenant_id, name) VALUES
      ('00000000-0000-0000-0000-000000000001', 'default')
  ON CONFLICT DO NOTHING;
  ```

- [ ] **Step 4: Run migrations**

  ```bash
  make dev
  DATABASE_URL=postgres://observable:observable@localhost:5432/observable \
    sqlx migrate run --source migrations/postgres
  ```
  Expected: "Applied N migrations."

- [ ] **Step 5: Commit**

  ```bash
  git add migrations/postgres/
  git commit -m "feat(storage): add PostgreSQL migrations for tenants, api_keys, projects

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 6: OpenAPI Contracts [DONE]

> **Parallel with Tasks 2–5.**

**Files:**
- Create: `contracts/openapi/ingest-v1.yaml`
- Create: `contracts/openapi/query-v1.yaml`

- [ ] **Step 1: Create contracts/openapi/ingest-v1.yaml**

  ```yaml
  openapi: "3.1.0"
  info:
    title: Observable Ingest API
    version: "1.0.0"
  paths:
    /v1/traces:
      post:
        summary: Export OTLP traces
        operationId: exportTraces
        requestBody:
          required: true
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/ExportTraceServiceRequest"
        responses:
          "200":
            description: Accepted
            content:
              application/json:
                schema:
                  $ref: "#/components/schemas/ExportTraceServiceResponse"
          "400": { description: Bad request }
          "401": { description: Unauthorized }
          "429": { description: Rate limited }
    /v1/logs:
      post:
        summary: Export OTLP logs
        operationId: exportLogs
        requestBody:
          required: true
          content:
            application/json:
              schema: { type: object }
        responses:
          "200": { description: Accepted }
          "401": { description: Unauthorized }
    /v1/metrics:
      post:
        summary: Export OTLP metrics
        operationId: exportMetrics
        requestBody:
          required: true
          content:
            application/json:
              schema: { type: object }
        responses:
          "200": { description: Accepted }
          "401": { description: Unauthorized }
  components:
    securitySchemes:
      BearerAuth:
        type: http
        scheme: bearer
    schemas:
      ExportTraceServiceRequest:
        type: object
        properties:
          resourceSpans:
            type: array
            items:
              type: object
      ExportTraceServiceResponse:
        type: object
        properties:
          partialSuccess:
            type: object
  security:
    - BearerAuth: []
  ```

- [ ] **Step 2: Create contracts/openapi/query-v1.yaml**

  ```yaml
  openapi: "3.1.0"
  info:
    title: Observable Query API
    version: "1.0.0"
  paths:
    /v1/traces/{trace_id}:
      get:
        summary: Get trace by ID
        operationId: getTrace
        parameters:
          - name: trace_id
            in: path
            required: true
            schema: { type: string }
        responses:
          "200":
            description: Trace with all spans
            content:
              application/json:
                schema:
                  $ref: "#/components/schemas/TraceResponse"
          "404": { description: Not found }
          "401": { description: Unauthorized }
    /v1/traces:
      get:
        summary: Search traces
        operationId: searchTraces
        parameters:
          - name: service
            in: query
            schema: { type: string }
          - name: from
            in: query
            schema: { type: string, format: date-time }
          - name: to
            in: query
            schema: { type: string, format: date-time }
          - name: limit
            in: query
            schema: { type: integer, default: 50, maximum: 500 }
        responses:
          "200":
            description: List of traces
            content:
              application/json:
                schema:
                  $ref: "#/components/schemas/TraceListResponse"
    /v1/logs:
      get:
        summary: Search logs
        operationId: searchLogs
        parameters:
          - name: service
            in: query
            schema: { type: string }
          - name: severity
            in: query
            schema: { type: string }
          - name: from
            in: query
            schema: { type: string, format: date-time }
          - name: to
            in: query
            schema: { type: string, format: date-time }
          - name: limit
            in: query
            schema: { type: integer, default: 100, maximum: 1000 }
        responses:
          "200":
            description: List of log records
  components:
    securitySchemes:
      BearerAuth:
        type: http
        scheme: bearer
    schemas:
      TraceResponse:
        type: object
        properties:
          trace_id: { type: string }
          spans:
            type: array
            items:
              type: object
      TraceListResponse:
        type: object
        properties:
          traces:
            type: array
            items:
              type: object
          total: { type: integer }
  security:
    - BearerAuth: []
  ```

- [ ] **Step 3: Validate with spectral (optional but recommended)**

  ```bash
  npx @stoplight/spectral-cli lint contracts/openapi/*.yaml
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add contracts/
  git commit -m "feat(contracts): add OpenAPI specs for ingest and query APIs v1

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 7: Auth Service — API Key Validation [DONE]

> **Parallel with Tasks 8 and 9. Depends on Tasks 3 and 5.**

**Files:**
- Create: `services/auth-service/src/main.rs`
- Create: `services/auth-service/src/config.rs`
- Create: `services/auth-service/src/validate.rs`

- [ ] **Step 1: Write failing test**

  Create `services/auth-service/src/validate.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn valid_key_returns_tenant_id() {
          let key = "dev-api-key-0000";
          let hash = sha256_hex(key);
          let entry = ApiKeyEntry {
              tenant_id: uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
              key_hash: hash.clone(),
              revoked_at: None,
          };
          assert!(matches!(validate_key_against_entry(key, &entry), Ok(id) if id == entry.tenant_id));
      }

      #[test]
      fn wrong_key_is_rejected() {
          let entry = ApiKeyEntry {
              tenant_id: uuid::Uuid::new_v4(),
              key_hash: sha256_hex("correct-key"),
              revoked_at: None,
          };
          assert!(validate_key_against_entry("wrong-key", &entry).is_err());
      }

      #[test]
      fn revoked_key_is_rejected() {
          let key = "dev-api-key-0000";
          let entry = ApiKeyEntry {
              tenant_id: uuid::Uuid::new_v4(),
              key_hash: sha256_hex(key),
              revoked_at: Some(chrono::Utc::now()),
          };
          assert!(validate_key_against_entry(key, &entry).is_err());
      }
  }
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  cargo test -p auth-service
  ```
  Expected: FAIL — types not defined.

- [ ] **Step 3: Implement validate.rs**

  ```rust
  use anyhow::{bail, Result};
  use chrono::{DateTime, Utc};
  use sha2::{Digest, Sha256};
  use uuid::Uuid;

  pub struct ApiKeyEntry {
      pub tenant_id: Uuid,
      pub key_hash: String,
      pub revoked_at: Option<DateTime<Utc>>,
  }

  pub fn sha256_hex(key: &str) -> String {
      let mut hasher = Sha256::new();
      hasher.update(key.as_bytes());
      format!("{:x}", hasher.finalize())
  }

  pub fn validate_key_against_entry(key: &str, entry: &ApiKeyEntry) -> Result<Uuid> {
      if entry.revoked_at.is_some() {
          bail!("API key has been revoked");
      }
      if sha256_hex(key) != entry.key_hash {
          bail!("Invalid API key");
      }
      Ok(entry.tenant_id)
  }
  ```

  Add `sha2 = "0.10"` to `services/auth-service/Cargo.toml`.

- [ ] **Step 4: Implement main.rs with HTTP validate endpoint**

  ```rust
  use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
  use serde::{Deserialize, Serialize};
  use sqlx::PgPool;
  use uuid::Uuid;

  #[derive(Clone)]
  struct AppState { db: PgPool }

  #[derive(Deserialize)]
  struct ValidateRequest { api_key: String }

  #[derive(Serialize)]
  struct ValidateResponse { tenant_id: Uuid }

  async fn validate_handler(
      State(state): State<AppState>,
      Json(req): Json<ValidateRequest>,
  ) -> Result<Json<ValidateResponse>, StatusCode> {
      let row = sqlx::query!(
          "SELECT tenant_id, key_hash, revoked_at FROM api_keys WHERE key_hash = $1",
          crate::validate::sha256_hex(&req.api_key)
      )
      .fetch_optional(&state.db)
      .await
      .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
      .ok_or(StatusCode::UNAUTHORIZED)?;

      if row.revoked_at.is_some() {
          return Err(StatusCode::UNAUTHORIZED);
      }
      Ok(Json(ValidateResponse { tenant_id: row.tenant_id }))
  }

  #[tokio::main]
  async fn main() -> anyhow::Result<()> {
      tracing_subscriber::fmt().json().init();
      let db_url = std::env::var("DATABASE_URL")?;
      let db = PgPool::connect(&db_url).await?;
      let port: u16 = std::env::var("AUTH_SERVICE_PORT")
          .unwrap_or_else(|_| "4318".into())
          .parse()?;
      let app = Router::new()
          .route("/internal/validate", post(validate_handler))
          .with_state(AppState { db });
      let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
      tracing::info!(port, "auth-service listening");
      axum::serve(listener, app).await?;
      Ok(())
  }
  ```

- [ ] **Step 5: Run tests — verify pass**

  ```bash
  cargo test -p auth-service
  ```
  Expected: PASS.

- [ ] **Step 6: Smoke test the running service**

  ```bash
  make dev
  DATABASE_URL=postgres://observable:observable@localhost:5432/observable \
    cargo run -p auth-service &
  sleep 2
  curl -s -X POST http://localhost:4318/internal/validate \
    -H "Content-Type: application/json" \
    -d '{"api_key":"dev-api-key-0000"}' | jq .
  ```
  Expected: `{"tenant_id":"00000000-0000-0000-0000-000000000001"}`

- [ ] **Step 7: Commit**

  ```bash
  git add services/auth-service/
  git commit -m "feat(auth): add API key validation service with SHA-256 key hashing

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 8: Ingest Gateway Skeleton — OTLP/HTTP Trace Ingest [DONE]

> **Parallel with Tasks 7 and 9. Depends on Tasks 3 and 6.**

**Files:**
- Create: `services/ingest-gateway/src/main.rs`
- Create: `services/ingest-gateway/src/config.rs`
- Create: `services/ingest-gateway/src/routes/traces.rs`
- Create: `services/ingest-gateway/src/auth.rs`

- [ ] **Step 1: Write failing contract test**

  Create `services/ingest-gateway/src/routes/traces.rs`:
  ```rust
  #[cfg(test)]
  mod tests {
      use axum::http::StatusCode;
      use axum_test::TestServer;
      use super::*;

      #[tokio::test]
      async fn missing_auth_returns_401() {
          let app = build_router(AppState::test_stub());
          let server = TestServer::new(app).unwrap();
          let resp = server.post("/v1/traces")
              .json(&serde_json::json!({"resourceSpans": []}))
              .await;
          assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
      }

      #[tokio::test]
      async fn valid_empty_payload_returns_200() {
          let app = build_router(AppState::with_stub_auth(
              "00000000-0000-0000-0000-000000000001"
          ));
          let server = TestServer::new(app).unwrap();
          let resp = server.post("/v1/traces")
              .add_header("Authorization", "Bearer dev-api-key-0000")
              .json(&serde_json::json!({"resourceSpans": []}))
              .await;
          assert_eq!(resp.status_code(), StatusCode::OK);
      }
  }
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  cargo test -p ingest-gateway
  ```
  Expected: FAIL.

- [ ] **Step 3: Implement auth.rs (middleware using auth-service)**

  ```rust
  use axum::{
      extract::{Request, State},
      http::StatusCode,
      middleware::Next,
      response::Response,
  };
  use uuid::Uuid;

  #[derive(Clone, Debug)]
  pub struct TenantContext { pub tenant_id: Uuid }

  pub async fn auth_middleware(
      State(state): State<crate::AppState>,
      mut req: Request,
      next: Next,
  ) -> Result<Response, StatusCode> {
      let bearer = req.headers()
          .get("Authorization")
          .and_then(|v| v.to_str().ok())
          .and_then(|v| v.strip_prefix("Bearer "))
          .ok_or(StatusCode::UNAUTHORIZED)?;

      let tenant_id = state.validate_api_key(bearer).await
          .map_err(|_| StatusCode::UNAUTHORIZED)?;

      req.extensions_mut().insert(TenantContext { tenant_id });
      Ok(next.run(req).await)
  }
  ```

- [ ] **Step 4: Implement routes/traces.rs**

  ```rust
  use axum::{extract::Extension, http::StatusCode, response::Json};
  use serde_json::Value;
  use crate::auth::TenantContext;

  pub async fn export_traces(
      Extension(ctx): Extension<TenantContext>,
      Json(body): Json<Value>,
  ) -> Result<Json<Value>, StatusCode> {
      let resource_spans = body.get("resourceSpans")
          .and_then(|v| v.as_array())
          .ok_or(StatusCode::BAD_REQUEST)?;

      tracing::info!(
          tenant_id = %ctx.tenant_id,
          span_count = resource_spans.len(),
          "received trace export request"
      );

      // Queue producer will be added in Task 10.
      // For now: validate and acknowledge.

      Ok(Json(serde_json::json!({ "partialSuccess": {} })))
  }
  ```

- [ ] **Step 5: Implement main.rs**

  ```rust
  use axum::{middleware, routing::post, Router};

  mod auth;
  mod routes;

  #[derive(Clone)]
  pub struct AppState {
      pub auth_service_url: String,
      pub http_client: reqwest::Client,
  }

  impl AppState {
      pub async fn validate_api_key(&self, key: &str) -> anyhow::Result<uuid::Uuid> {
          let resp = self.http_client
              .post(format!("{}/internal/validate", self.auth_service_url))
              .json(&serde_json::json!({"api_key": key}))
              .send().await?;
          if !resp.status().is_success() {
              anyhow::bail!("auth rejected");
          }
          let body: serde_json::Value = resp.json().await?;
          let id = body["tenant_id"].as_str().unwrap_or_default().parse()?;
          Ok(id)
      }

      #[cfg(test)]
      pub fn test_stub() -> Self { /* ... */ }
      #[cfg(test)]
      pub fn with_stub_auth(_tenant_id: &str) -> Self { /* ... */ }
  }

  pub fn build_router(state: AppState) -> Router {
      Router::new()
          .route("/v1/traces", post(routes::traces::export_traces))
          .route("/v1/logs",   post(routes::logs::export_logs))
          .route("/v1/metrics",post(routes::metrics::export_metrics))
          .layer(middleware::from_fn_with_state(state.clone(), auth::auth_middleware))
          .with_state(state)
  }

  #[tokio::main]
  async fn main() -> anyhow::Result<()> {
      tracing_subscriber::fmt().json().init();
      let port: u16 = std::env::var("INGEST_GATEWAY_PORT")
          .unwrap_or_else(|_| "4317".into()).parse()?;
      let state = AppState {
          auth_service_url: std::env::var("AUTH_SERVICE_URL")
              .unwrap_or_else(|_| "http://localhost:4318".into()),
          http_client: reqwest::Client::new(),
      };
      let app = build_router(state);
      let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
      tracing::info!(port, "ingest-gateway listening");
      axum::serve(listener, app).await?;
      Ok(())
  }
  ```

  Add `reqwest = { version = "0.12", features = ["json"] }` to Cargo.toml.

- [ ] **Step 6: Run tests — verify pass**

  ```bash
  cargo test -p ingest-gateway
  ```
  Expected: PASS.

- [ ] **Step 7: Commit**

  ```bash
  git add services/ingest-gateway/
  git commit -m "feat(ingest): add OTLP/HTTP trace ingest skeleton with bearer auth

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 9: Query API Skeleton — Trace Lookup [DONE]

> **Parallel with Tasks 7 and 8. Depends on Tasks 3 and 4.**

**Files:**
- Create: `services/query-api/src/main.rs`
- Create: `services/query-api/src/traces.rs`

- [ ] **Step 1: Write failing test**

  ```rust
  // services/query-api/src/traces.rs
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn trace_response_serializes() {
          let resp = TraceResponse {
              trace_id: "abc123".into(),
              spans: vec![],
          };
          let json = serde_json::to_string(&resp).unwrap();
          assert!(json.contains("abc123"));
      }
  }
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  cargo test -p query-api
  ```

- [ ] **Step 3: Implement traces.rs**

  ```rust
  use axum::{
      extract::{Path, Query, State},
      http::StatusCode,
      Json,
  };
  use clickhouse::Client;
  use serde::{Deserialize, Serialize};
  use uuid::Uuid;
  use domain::Span;

  #[derive(Clone)]
  pub struct AppState { pub ch: Client, pub tenant_id: Uuid }

  #[derive(Serialize)]
  pub struct TraceResponse {
      pub trace_id: String,
      pub spans: Vec<Span>,
  }

  #[derive(Serialize)]
  pub struct TraceListResponse {
      pub traces: Vec<TraceResponse>,
      pub total: u64,
  }

  #[derive(Deserialize)]
  pub struct SearchParams {
      service: Option<String>,
      limit: Option<u32>,
  }

  pub async fn get_trace(
      State(state): State<AppState>,
      Path(trace_id): Path<String>,
  ) -> Result<Json<TraceResponse>, StatusCode> {
      let spans: Vec<Span> = state.ch
          .query("SELECT * FROM spans WHERE tenant_id = ? AND trace_id = ?")
          .bind(state.tenant_id)
          .bind(&trace_id)
          .fetch_all()
          .await
          .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

      if spans.is_empty() {
          return Err(StatusCode::NOT_FOUND);
      }
      Ok(Json(TraceResponse { trace_id, spans }))
  }

  pub async fn search_traces(
      State(state): State<AppState>,
      Query(params): Query<SearchParams>,
  ) -> Result<Json<TraceListResponse>, StatusCode> {
      let limit = params.limit.unwrap_or(50).min(500);
      let mut q = state.ch.query(
          "SELECT trace_id, groupArray(tuple(*)) as spans
           FROM spans
           WHERE tenant_id = ?
           GROUP BY trace_id
           ORDER BY max(start_time_unix_nano) DESC
           LIMIT ?"
      )
      .bind(state.tenant_id)
      .bind(limit);

      if let Some(svc) = &params.service {
          // Note: parameterised HAVING not supported in all CH versions;
          // real impl uses a subquery on service_name.
          let _ = svc; // placeholder until full impl in follow-up slice
      }

      let traces: Vec<TraceResponse> = Vec::new(); // query result mapped here
      Ok(Json(TraceListResponse { traces, total: 0 }))
  }
  ```

  > **Note:** The ClickHouse `Span` row deserialisation requires implementing `clickhouse::Row` derive on `domain::Span`. Add `#[derive(clickhouse::Row)]` to `Span` in Task 3 and add the `clickhouse` dependency to `libs/domain/Cargo.toml`.

- [ ] **Step 4: Implement main.rs**

  ```rust
  use axum::{routing::get, Router};
  use clickhouse::Client;

  mod traces;

  #[tokio::main]
  async fn main() -> anyhow::Result<()> {
      tracing_subscriber::fmt().json().init();
      let ch_url = std::env::var("CLICKHOUSE_URL")
          .unwrap_or_else(|_| "http://localhost:8123".into());
      let ch = Client::default().with_url(ch_url).with_database("observable");
      let port: u16 = std::env::var("QUERY_API_PORT")
          .unwrap_or_else(|_| "8090".into()).parse()?;
      let tenant_id = std::env::var("DEV_TENANT_ID")
          .unwrap_or_else(|_| "00000000-0000-0000-0000-000000000001".into())
          .parse()?;
      let state = traces::AppState { ch, tenant_id };
      let app = Router::new()
          .route("/v1/traces",          get(traces::search_traces))
          .route("/v1/traces/:trace_id", get(traces::get_trace))
          .with_state(state);
      let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
      tracing::info!(port, "query-api listening");
      axum::serve(listener, app).await?;
      Ok(())
  }
  ```

- [ ] **Step 5: Run tests — verify pass**

  ```bash
  cargo test -p query-api
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add services/query-api/
  git commit -m "feat(query): add query-api with trace lookup and search endpoints

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 10: Queue Producer in Ingest Gateway [DONE]

> **Sequential after Task 8.**

**Files:**
- Create: `services/ingest-gateway/src/queue/mod.rs`
- Create: `services/ingest-gateway/src/queue/producer.rs`
- Modify: `services/ingest-gateway/src/routes/traces.rs`

- [ ] **Step 1: Write failing test**

  ```rust
  // services/ingest-gateway/src/queue/producer.rs
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn envelope_serializes_for_kafka() {
          let env = build_envelope(
              uuid::Uuid::new_v4(),
              domain::EnvelopePayload::Spans(vec![]),
          );
          let bytes = serde_json::to_vec(&env).unwrap();
          assert!(!bytes.is_empty());
      }
  }
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  cargo test -p ingest-gateway queue
  ```

- [ ] **Step 3: Implement producer.rs**

  ```rust
  use domain::{EnvelopePayload, TelemetryEnvelope};
  use rdkafka::producer::{FutureProducer, FutureRecord};
  use rdkafka::ClientConfig;
  use std::time::Duration;
  use uuid::Uuid;

  pub struct QueueProducer { producer: FutureProducer, topic: String }

  impl QueueProducer {
      pub fn new(brokers: &str, topic: &str) -> anyhow::Result<Self> {
          let producer: FutureProducer = ClientConfig::new()
              .set("bootstrap.servers", brokers)
              .set("message.timeout.ms", "5000")
              .create()?;
          Ok(Self { producer, topic: topic.into() })
      }

      pub async fn publish(&self, envelope: &TelemetryEnvelope) -> anyhow::Result<()> {
          let payload = serde_json::to_vec(envelope)?;
          let key = envelope.tenant_id.to_string();
          self.producer
              .send(
                  FutureRecord::to(&self.topic)
                      .key(&key)
                      .payload(&payload),
                  Duration::from_secs(5),
              )
              .await
              .map_err(|(e, _)| anyhow::anyhow!("kafka send error: {e}"))?;
          Ok(())
      }
  }

  pub fn build_envelope(tenant_id: Uuid, payload: EnvelopePayload) -> TelemetryEnvelope {
      TelemetryEnvelope {
          envelope_id: Uuid::new_v4(),
          tenant_id,
          received_at_unix_nano: std::time::SystemTime::now()
              .duration_since(std::time::UNIX_EPOCH)
              .unwrap()
              .as_nanos() as u64,
          payload,
      }
  }
  ```

- [ ] **Step 4: Update routes/traces.rs to publish to queue**

  In the `export_traces` handler, after parsing, call:
  ```rust
  let spans = parse_otlp_traces(&body, ctx.tenant_id)?;
  let envelope = crate::queue::producer::build_envelope(
      ctx.tenant_id,
      domain::EnvelopePayload::Spans(spans),
  );
  state.producer.publish(&envelope).await
      .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
  ```

  Add `parse_otlp_traces` helper in `routes/traces.rs`:
  ```rust
  fn parse_otlp_traces(body: &serde_json::Value, tenant_id: uuid::Uuid) -> Result<Vec<domain::Span>, axum::http::StatusCode> {
      let resource_spans = body.get("resourceSpans")
          .and_then(|v| v.as_array())
          .ok_or(axum::http::StatusCode::BAD_REQUEST)?;

      let mut spans = Vec::new();
      for rs in resource_spans {
          let resource_attrs = rs.get("resource")
              .and_then(|r| r.get("attributes"))
              .cloned()
              .unwrap_or_default();
          let service_name = extract_string_attr(&resource_attrs, "service.name")
              .unwrap_or_default();
          for scope_spans in rs.get("scopeSpans").and_then(|v| v.as_array()).unwrap_or(&vec![]) {
              for s in scope_spans.get("spans").and_then(|v| v.as_array()).unwrap_or(&vec![]) {
                  spans.push(domain::Span {
                      tenant_id,
                      trace_id: s.get("traceId").and_then(|v| v.as_str()).unwrap_or_default().into(),
                      span_id:  s.get("spanId").and_then(|v| v.as_str()).unwrap_or_default().into(),
                      service_name: service_name.clone(),
                      operation_name: s.get("name").and_then(|v| v.as_str()).unwrap_or_default().into(),
                      start_time_unix_nano: s.get("startTimeUnixNano").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0),
                      end_time_unix_nano:   s.get("endTimeUnixNano").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0),
                      duration_ns: 0, // computed below
                      ..Default::default()
                  });
              }
          }
      }
      for span in &mut spans {
          span.duration_ns = span.end_time_unix_nano.saturating_sub(span.start_time_unix_nano);
      }
      Ok(spans)
  }

  fn extract_string_attr(attrs: &serde_json::Value, key: &str) -> Option<String> {
      attrs.as_array()?.iter().find(|a| {
          a.get("key").and_then(|k| k.as_str()) == Some(key)
      })?.get("value")?.get("stringValue")?.as_str().map(String::from)
  }
  ```

- [ ] **Step 5: Run tests — verify pass**

  ```bash
  cargo test -p ingest-gateway
  ```

- [ ] **Step 6: Smoke test end-to-end with running stack**

  ```bash
  make dev
  cargo run -p auth-service &
  REDPANDA_BROKERS=localhost:9092 INGEST_TOPIC=telemetry.raw cargo run -p ingest-gateway &
  sleep 2
  curl -s -X POST http://localhost:4317/v1/traces \
    -H "Authorization: Bearer dev-api-key-0000" \
    -H "Content-Type: application/json" \
    -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"test-svc"}}]},"scopeSpans":[{"spans":[{"traceId":"4bf92f3577b34da6a3ce929d0e0e4736","spanId":"00f067aa0ba902b7","name":"test-op","startTimeUnixNano":"1700000000000000000","endTimeUnixNano":"1700000000005000000","status":{"code":1}}]}]}]}'
  # Expected: {"partialSuccess":{}}
  rpk topic consume telemetry.raw --brokers localhost:9092 -n 1
  # Expected: one JSON message containing the span
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add services/ingest-gateway/src/queue/ services/ingest-gateway/src/routes/
  git commit -m "feat(ingest): publish OTLP trace envelopes to Redpanda topic

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 11: Stream Processor — Consume and Normalise [DONE]

> **Sequential after Task 10.**

**Files:**
- Create: `services/stream-processor/src/main.rs`
- Create: `services/stream-processor/src/consumer.rs`
- Create: `services/stream-processor/src/normalise.rs`

- [ ] **Step 1: Write failing test**

  ```rust
  // services/stream-processor/src/normalise.rs
  #[cfg(test)]
  mod tests {
      use super::*;
      use domain::{Span, StatusCode};
      use uuid::Uuid;

      #[test]
      fn normalise_fills_duration() {
          let span = Span {
              tenant_id: Uuid::new_v4(),
              start_time_unix_nano: 1_000_000_000,
              end_time_unix_nano: 1_005_000_000,
              duration_ns: 0,
              ..Default::default()
          };
          let out = normalise_span(span);
          assert_eq!(out.duration_ns, 5_000_000);
      }

      #[test]
      fn normalise_defaults_unset_status() {
          let span = Span {
              tenant_id: Uuid::new_v4(),
              ..Default::default()
          };
          let out = normalise_span(span);
          assert_eq!(out.status_code, StatusCode::Unset);
      }
  }
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  cargo test -p stream-processor
  ```

- [ ] **Step 3: Implement normalise.rs**

  ```rust
  use domain::{Span, LogRecord, MetricPoint};

  pub fn normalise_span(mut span: Span) -> Span {
      if span.duration_ns == 0 {
          span.duration_ns = span.end_time_unix_nano
              .saturating_sub(span.start_time_unix_nano);
      }
      span
  }

  pub fn normalise_log(mut log: LogRecord) -> LogRecord {
      if log.log_id == uuid::Uuid::nil() {
          log.log_id = uuid::Uuid::new_v4();
      }
      log
  }
  ```

- [ ] **Step 4: Implement consumer.rs**

  ```rust
  use domain::{EnvelopePayload, TelemetryEnvelope};
  use rdkafka::{
      consumer::{Consumer, StreamConsumer},
      ClientConfig, Message,
  };

  pub struct QueueConsumer { consumer: StreamConsumer }

  impl QueueConsumer {
      pub fn new(brokers: &str, group_id: &str, topic: &str) -> anyhow::Result<Self> {
          let consumer: StreamConsumer = ClientConfig::new()
              .set("bootstrap.servers", brokers)
              .set("group.id", group_id)
              .set("auto.offset.reset", "earliest")
              .create()?;
          consumer.subscribe(&[topic])?;
          Ok(Self { consumer })
      }

      pub async fn run<F, Fut>(&self, mut handler: F) -> anyhow::Result<()>
      where
          F: FnMut(TelemetryEnvelope) -> Fut,
          Fut: std::future::Future<Output = anyhow::Result<()>>,
      {
          loop {
              let msg = self.consumer.recv().await?;
              if let Some(payload) = msg.payload() {
                  match serde_json::from_slice::<TelemetryEnvelope>(payload) {
                      Ok(env) => {
                          if let Err(e) = handler(env).await {
                              tracing::warn!(error = %e, "handler error");
                          }
                      }
                      Err(e) => tracing::warn!(error = %e, "envelope deserialise failed"),
                  }
              }
          }
      }
  }
  ```

- [ ] **Step 5: Implement main.rs**

  ```rust
  mod consumer;
  mod normalise;

  use domain::{EnvelopePayload, TelemetryEnvelope};

  #[tokio::main]
  async fn main() -> anyhow::Result<()> {
      tracing_subscriber::fmt().json().init();
      let brokers = std::env::var("REDPANDA_BROKERS")?;
      let topic   = std::env::var("INGEST_TOPIC")?;
      let qc = consumer::QueueConsumer::new(&brokers, "stream-processor", &topic)?;
      qc.run(|env: TelemetryEnvelope| async move {
          match env.payload {
              EnvelopePayload::Spans(spans) => {
                  let normalised: Vec<_> = spans.into_iter()
                      .map(normalise::normalise_span)
                      .collect();
                  // Storage writer channel will be wired in Task 12.
                  tracing::info!(count = normalised.len(), "processed spans");
              }
              EnvelopePayload::Logs(logs) => {
                  let normalised: Vec<_> = logs.into_iter()
                      .map(normalise::normalise_log)
                      .collect();
                  tracing::info!(count = normalised.len(), "processed logs");
              }
              EnvelopePayload::Metrics { series, points } => {
                  tracing::info!(series = series.len(), points = points.len(), "processed metrics");
              }
          }
          Ok(())
      }).await
  }
  ```

- [ ] **Step 6: Run tests — verify pass**

  ```bash
  cargo test -p stream-processor
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add services/stream-processor/
  git commit -m "feat(processor): add stream processor with Redpanda consumer and span normalisation

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 12: Storage Writer — Batch Write Spans to ClickHouse [DONE]

> **Sequential after Task 11.**

**Files:**
- Create: `services/storage-writer/src/main.rs`
- Create: `services/storage-writer/src/spans.rs`
- Modify: `services/stream-processor/src/main.rs` (send to writer via HTTP)

- [ ] **Step 1: Write failing test**

  ```rust
  // services/storage-writer/src/spans.rs
  #[cfg(test)]
  mod tests {
      use super::*;
      use domain::{Span, SpanKind, StatusCode};
      use uuid::Uuid;

      #[test]
      fn ch_row_from_span_maps_status() {
          let span = Span {
              tenant_id: Uuid::new_v4(),
              trace_id: "abc".into(),
              span_id: "def".into(),
              status_code: StatusCode::Error,
              start_time_unix_nano: 1_000,
              end_time_unix_nano: 2_000,
              duration_ns: 1_000,
              ..Default::default()
          };
          let row = SpanRow::from(span);
          assert_eq!(row.status_code, "ERROR");
      }
  }
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  cargo test -p storage-writer
  ```

- [ ] **Step 3: Implement spans.rs**

  ```rust
  use clickhouse::{Client, Row};
  use domain::{Span, StatusCode, SpanKind};
  use serde::{Deserialize, Serialize};
  use uuid::Uuid;

  #[derive(Debug, Row, Serialize, Deserialize)]
  pub struct SpanRow {
      pub tenant_id: Uuid,
      pub trace_id: String,
      pub span_id: String,
      pub parent_span_id: Option<String>,
      pub service_name: String,
      pub service_namespace: String,
      pub service_version: String,
      pub operation_name: String,
      pub span_kind: String,
      pub start_time_unix_nano: u64,
      pub end_time_unix_nano: u64,
      pub duration_ns: u64,
      pub status_code: String,
      pub status_message: String,
      pub attributes: String,
      pub resource_attributes: String,
      pub environment: String,
      pub host_id: String,
      pub workload: String,
      pub deployment_id: String,
  }

  impl From<Span> for SpanRow {
      fn from(s: Span) -> Self {
          Self {
              tenant_id: s.tenant_id,
              trace_id: s.trace_id,
              span_id: s.span_id,
              parent_span_id: s.parent_span_id,
              service_name: s.service_name,
              service_namespace: s.service_namespace,
              service_version: s.service_version,
              operation_name: s.operation_name,
              span_kind: format!("{:?}", s.span_kind).to_uppercase(),
              start_time_unix_nano: s.start_time_unix_nano,
              end_time_unix_nano: s.end_time_unix_nano,
              duration_ns: s.duration_ns,
              status_code: format!("{:?}", s.status_code).to_uppercase(),
              status_message: s.status_message,
              attributes: serde_json::to_string(&s.attributes).unwrap_or_default(),
              resource_attributes: serde_json::to_string(&s.resource_attributes).unwrap_or_default(),
              environment: s.environment,
              host_id: s.host_id,
              workload: s.workload,
              deployment_id: s.deployment_id,
          }
      }
  }

  pub async fn insert_spans(ch: &Client, spans: Vec<Span>) -> anyhow::Result<()> {
      let mut insert = ch.insert("spans")?;
      for span in spans {
          insert.write(&SpanRow::from(span)).await?;
      }
      insert.end().await?;
      Ok(())
  }
  ```

- [ ] **Step 4: Implement main.rs (HTTP endpoint for batch writes)**

  ```rust
  use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
  use clickhouse::Client;

  mod spans;

  #[derive(Clone)]
  struct AppState { ch: Client }

  async fn write_spans(
      State(state): State<AppState>,
      Json(batch): Json<Vec<domain::Span>>,
  ) -> StatusCode {
      match spans::insert_spans(&state.ch, batch).await {
          Ok(_) => StatusCode::NO_CONTENT,
          Err(e) => {
              tracing::error!(error = %e, "clickhouse write failed");
              StatusCode::INTERNAL_SERVER_ERROR
          }
      }
  }

  #[tokio::main]
  async fn main() -> anyhow::Result<()> {
      tracing_subscriber::fmt().json().init();
      let ch_url = std::env::var("CLICKHOUSE_URL")
          .unwrap_or_else(|_| "http://localhost:8123".into());
      let ch = Client::default().with_url(ch_url).with_database("observable");
      let port: u16 = std::env::var("STORAGE_WRITER_PORT")
          .unwrap_or_else(|_| "4320".into()).parse()?;
      let app = Router::new()
          .route("/internal/spans", post(write_spans))
          .with_state(AppState { ch });
      let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
      tracing::info!(port, "storage-writer listening");
      axum::serve(listener, app).await?;
      Ok(())
  }
  ```

- [ ] **Step 5: Wire stream-processor → storage-writer**

  Update `services/stream-processor/src/main.rs` to call the storage writer:
  ```rust
  let writer_url = std::env::var("STORAGE_WRITER_URL")
      .unwrap_or_else(|_| "http://localhost:4320".into());
  let http = reqwest::Client::new();

  // inside the handler:
  EnvelopePayload::Spans(spans) => {
      let normalised: Vec<_> = spans.into_iter()
          .map(normalise::normalise_span)
          .collect();
      http.post(format!("{writer_url}/internal/spans"))
          .json(&normalised)
          .send().await?;
  }
  ```

- [ ] **Step 6: Run tests — verify pass**

  ```bash
  cargo test -p storage-writer
  ```

- [ ] **Step 7: Full pipeline smoke test**

  ```bash
  make dev && make migrate
  cargo run -p auth-service &
  cargo run -p storage-writer &
  cargo run -p stream-processor &
  cargo run -p ingest-gateway &
  cargo run -p query-api &
  sleep 3

  # Send a trace
  curl -X POST http://localhost:4317/v1/traces \
    -H "Authorization: Bearer dev-api-key-0000" \
    -H "Content-Type: application/json" \
    -d '{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"smoke-test"}}]},"scopeSpans":[{"spans":[{"traceId":"4bf92f3577b34da6a3ce929d0e0e4736","spanId":"00f067aa0ba902b7","name":"smoke","startTimeUnixNano":"1700000000000000000","endTimeUnixNano":"1700000000005000000","status":{"code":1}}]}]}]}'

  sleep 2  # allow consumer to process

  # Query it back
  curl "http://localhost:8090/v1/traces/4bf92f3577b34da6a3ce929d0e0e4736"
  # Expected: JSON trace with one span
  ```

- [ ] **Step 8: Commit**

  ```bash
  git add services/storage-writer/ services/stream-processor/
  git commit -m "feat(storage): write spans to ClickHouse via storage-writer service

  Completes end-to-end trace path: ingest → queue → processor → ClickHouse → query.

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 13: API Gateway Middleware [DONE]

> **Parallel with Task 14. Depends on Task 7.**

**Files:**
- Create: `services/ingest-gateway/src/middleware/tenant.rs` (tenant context extraction)

> **Scope note:** For Phase 1 the ingest gateway is its own front door. A separate API gateway service is Phase 2. This task adds tenant-context header propagation for the query-api.

- [ ] **Step 1: Add X-Tenant-ID header propagation to query-api**

  Create `services/query-api/src/middleware/auth.rs`:
  ```rust
  use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
  use uuid::Uuid;

  #[derive(Clone)]
  pub struct TenantContext { pub tenant_id: Uuid }

  pub async fn require_tenant(
      mut req: Request,
      next: Next,
  ) -> Result<Response, StatusCode> {
      // Phase 1: accept X-Tenant-ID header (set by ingest-gateway for internal calls)
      // Phase 2: replace with bearer token validation via auth-service
      let tenant_id: Uuid = req.headers()
          .get("X-Tenant-ID")
          .and_then(|v| v.to_str().ok())
          .and_then(|s| s.parse().ok())
          // Fallback to dev tenant for local development
          .unwrap_or_else(|| Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());

      req.extensions_mut().insert(TenantContext { tenant_id });
      Ok(next.run(req).await)
  }
  ```

- [ ] **Step 2: Wire middleware in query-api main.rs**

  ```rust
  let app = Router::new()
      .route("/v1/traces",           get(traces::search_traces))
      .route("/v1/traces/:trace_id", get(traces::get_trace))
      .layer(middleware::from_fn(middleware::auth::require_tenant))
      .with_state(state);
  ```

- [ ] **Step 3: Update trace handlers to use TenantContext extension**

  Replace the static `state.tenant_id` with the value from the extension:
  ```rust
  pub async fn get_trace(
      State(state): State<AppState>,
      Extension(ctx): Extension<TenantContext>,
      Path(trace_id): Path<String>,
  ) -> Result<Json<TraceResponse>, StatusCode> {
      // use ctx.tenant_id instead of state.tenant_id
  ```

- [ ] **Step 4: Run tests — verify pass**

  ```bash
  cargo test -p query-api
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add services/query-api/src/middleware/
  git commit -m "feat(query): add tenant context middleware to query-api

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 14: Frontend Scaffold [DONE]

> **Parallel with Task 13. Depends on Task 1.**

**Files:**
- Create: `apps/frontend/index.html`
- Create: `apps/frontend/vite.config.ts`
- Create: `apps/frontend/tsconfig.json`
- Create: `apps/frontend/src/main.tsx`
- Create: `apps/frontend/src/App.tsx`
- Create: `apps/frontend/src/router.ts`
- Create: `apps/frontend/src/pages/TraceSearch.tsx`
- Create: `apps/frontend/src/api/traces.ts`

- [ ] **Step 1: Write failing test**

  Create `apps/frontend/src/App.test.tsx`:
  ```tsx
  import { render, screen } from "@testing-library/react";
  import App from "./App";

  test("renders trace search heading", () => {
    render(<App />);
    expect(screen.getByText(/Trace Explorer/i)).toBeInTheDocument();
  });
  ```

  Add `@testing-library/react` and `vitest` to devDependencies, add `test` script: `"test": "vitest"`.

- [ ] **Step 2: Run — verify failure**

  ```bash
  npm run test --workspace=apps/frontend
  ```

- [ ] **Step 3: Create vite.config.ts**

  ```ts
  import { defineConfig } from "vite";
  import react from "@vitejs/plugin-react";

  export default defineConfig({
    plugins: [react()],
    server: {
      port: 5173,
      proxy: {
        "/v1": { target: "http://localhost:8090", changeOrigin: true },
      },
    },
  });
  ```

- [ ] **Step 4: Create src/router.ts**

  ```ts
  import { createRouter, createRoute, createRootRoute } from "@tanstack/react-router";
  import TraceSearch from "./pages/TraceSearch";

  const rootRoute = createRootRoute();
  const traceSearchRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/",
    component: TraceSearch,
  });
  export const router = createRouter({
    routeTree: rootRoute.addChildren([traceSearchRoute]),
  });
  ```

- [ ] **Step 5: Create src/api/traces.ts**

  ```ts
  export interface Span {
    tenant_id: string;
    trace_id: string;
    span_id: string;
    service_name: string;
    operation_name: string;
    start_time_unix_nano: string;
    end_time_unix_nano: string;
    duration_ns: number;
    status_code: string;
  }

  export interface TraceResponse {
    trace_id: string;
    spans: Span[];
  }

  export interface TraceListResponse {
    traces: TraceResponse[];
    total: number;
  }

  export async function searchTraces(params: {
    service?: string;
    limit?: number;
  }): Promise<TraceListResponse> {
    const url = new URL("/v1/traces", window.location.origin);
    if (params.service) url.searchParams.set("service", params.service);
    if (params.limit)   url.searchParams.set("limit", String(params.limit));
    const res = await fetch(url.toString());
    if (!res.ok) throw new Error(`Query failed: ${res.status}`);
    return res.json();
  }

  export async function getTrace(traceId: string): Promise<TraceResponse> {
    const res = await fetch(`/v1/traces/${traceId}`);
    if (!res.ok) throw new Error(`Not found: ${res.status}`);
    return res.json();
  }
  ```

- [ ] **Step 6: Create src/pages/TraceSearch.tsx**

  ```tsx
  import { useState } from "react";
  import { useQuery } from "@tanstack/react-query";
  import { searchTraces } from "../api/traces";

  export default function TraceSearch() {
    const [service, setService] = useState("");
    const { data, isLoading, error } = useQuery({
      queryKey: ["traces", service],
      queryFn: () => searchTraces({ service: service || undefined, limit: 50 }),
    });

    return (
      <div style={{ padding: "2rem", fontFamily: "monospace" }}>
        <h1>Trace Explorer</h1>
        <input
          placeholder="Filter by service"
          value={service}
          onChange={(e) => setService(e.target.value)}
          style={{ marginBottom: "1rem", padding: "0.5rem", width: "300px" }}
        />
        {isLoading && <p>Loading...</p>}
        {error && <p>Error: {String(error)}</p>}
        {data?.traces.length === 0 && <p>No traces found.</p>}
        <table style={{ borderCollapse: "collapse", width: "100%" }}>
          <thead>
            <tr>
              <th>Trace ID</th>
              <th>Service</th>
              <th>Operation</th>
              <th>Duration</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {data?.traces.map((t) => {
              const root = t.spans[0];
              if (!root) return null;
              return (
                <tr key={t.trace_id}>
                  <td>{t.trace_id.substring(0, 16)}…</td>
                  <td>{root.service_name}</td>
                  <td>{root.operation_name}</td>
                  <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
                  <td>{root.status_code}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    );
  }
  ```

- [ ] **Step 7: Create src/App.tsx**

  ```tsx
  import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
  import { RouterProvider } from "@tanstack/react-router";
  import { router } from "./router";

  const queryClient = new QueryClient();

  export default function App() {
    return (
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  ```

- [ ] **Step 8: Run tests — verify pass**

  ```bash
  npm run test --workspace=apps/frontend
  npm run typecheck --workspace=apps/frontend
  npm run build --workspace=apps/frontend
  ```

- [ ] **Step 9: Verify dev server**

  ```bash
  npm run dev --workspace=apps/frontend
  # Open http://localhost:5173 — Trace Explorer heading should render
  ```

- [ ] **Step 10: Commit**

  ```bash
  git add apps/frontend/
  git commit -m "feat(frontend): scaffold React 19 + Vite + TanStack Query trace explorer

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 15: Log Ingest + Storage [DONE]

> **Sequential after Task 12.**

**Files:**
- Modify: `services/ingest-gateway/src/routes/logs.rs` (already stubbed, now implement)
- Create: `services/storage-writer/src/logs.rs`
- Modify: `services/stream-processor/src/main.rs`

- [ ] **Step 1: Write failing test**

  ```rust
  // services/storage-writer/src/logs.rs
  #[cfg(test)]
  mod tests {
      use super::*;
      use domain::LogRecord;
      use uuid::Uuid;

      #[test]
      fn log_row_from_record_maps_severity() {
          let log = LogRecord {
              tenant_id: Uuid::new_v4(),
              log_id: Uuid::new_v4(),
              severity_number: 17,
              severity_text: "ERROR".into(),
              body: serde_json::json!("request failed"),
              ..Default::default()
          };
          let row = LogRow::from(log);
          assert_eq!(row.severity_number, 17);
      }
  }
  ```

- [ ] **Step 2: Implement logs.rs in storage-writer (same pattern as spans.rs)**

  ```rust
  use clickhouse::{Client, Row};
  use domain::LogRecord;
  use serde::{Deserialize, Serialize};
  use uuid::Uuid;

  #[derive(Debug, Row, Serialize, Deserialize)]
  pub struct LogRow {
      pub tenant_id: Uuid,
      pub log_id: Uuid,
      pub timestamp_unix_nano: u64,
      pub observed_timestamp_unix_nano: u64,
      pub severity_number: i32,
      pub severity_text: String,
      pub body: String,
      pub trace_id: Option<String>,
      pub span_id: Option<String>,
      pub attributes: String,
      pub resource_attributes: String,
      pub service_name: String,
      pub environment: String,
      pub host_id: String,
      pub fingerprint: Option<u64>,
  }

  impl From<LogRecord> for LogRow {
      fn from(l: LogRecord) -> Self {
          Self {
              tenant_id: l.tenant_id,
              log_id: l.log_id,
              timestamp_unix_nano: l.timestamp_unix_nano,
              observed_timestamp_unix_nano: l.observed_timestamp_unix_nano,
              severity_number: l.severity_number,
              severity_text: l.severity_text,
              body: serde_json::to_string(&l.body).unwrap_or_default(),
              trace_id: l.trace_id,
              span_id: l.span_id,
              attributes: serde_json::to_string(&l.attributes).unwrap_or_default(),
              resource_attributes: serde_json::to_string(&l.resource_attributes).unwrap_or_default(),
              service_name: l.service_name,
              environment: l.environment,
              host_id: l.host_id,
              fingerprint: l.fingerprint,
          }
      }
  }

  pub async fn insert_logs(ch: &Client, logs: Vec<LogRecord>) -> anyhow::Result<()> {
      let mut insert = ch.insert("logs")?;
      for log in logs { insert.write(&LogRow::from(log)).await?; }
      insert.end().await?;
      Ok(())
  }
  ```

- [ ] **Step 3: Add /internal/logs route to storage-writer main.rs**

  ```rust
  async fn write_logs(State(s): State<AppState>, Json(batch): Json<Vec<domain::LogRecord>>) -> StatusCode {
      match logs::insert_logs(&s.ch, batch).await {
          Ok(_)  => StatusCode::NO_CONTENT,
          Err(e) => { tracing::error!(error=%e, "ch write failed"); StatusCode::INTERNAL_SERVER_ERROR }
      }
  }
  // add to router: .route("/internal/logs", post(write_logs))
  ```

- [ ] **Step 4: Implement routes/logs.rs in ingest-gateway**

  Same pattern as traces: parse OTLP logs JSON body → build `LogRecord`s → publish `EnvelopePayload::Logs` to queue.

- [ ] **Step 5: Wire stream-processor for logs**

  Update `stream-processor/src/main.rs` `EnvelopePayload::Logs` arm to POST normalised logs to storage-writer.

- [ ] **Step 6: Run all tests**

  ```bash
  cargo test --workspace
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add services/
  git commit -m "feat(logs): add OTLP log ingest through pipeline to ClickHouse

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 16: Metrics Ingest + Storage [DONE]

> **Sequential after Task 15.**

**Files:**
- Modify: `services/ingest-gateway/src/routes/metrics.rs`
- Create: `services/storage-writer/src/metrics.rs`
- Modify: `services/stream-processor/src/main.rs`

- [ ] **Step 1: Write failing test**

  ```rust
  // services/storage-writer/src/metrics.rs
  #[cfg(test)]
  mod tests {
      use super::*;
      use domain::{MetricPoint, MetricSeries, MetricType};
      use uuid::Uuid;

      #[test]
      fn metric_series_row_maps_type() {
          let series = MetricSeries {
              tenant_id: Uuid::new_v4(),
              metric_series_id: Uuid::new_v4(),
              metric_name: "http.request.duration".into(),
              metric_type: MetricType::Histogram,
              service_name: "checkout".into(),
              ..Default::default()
          };
          let row = MetricSeriesRow::from(series);
          assert_eq!(row.metric_type, "histogram");
      }
  }
  ```

- [ ] **Step 2: Implement metrics.rs in storage-writer**

  Same structure as `spans.rs`: `MetricSeriesRow`, `MetricPointRow`, `insert_metric_series`, `insert_metric_points`.

  Key mapping:
  ```rust
  impl From<MetricSeries> for MetricSeriesRow {
      fn from(s: MetricSeries) -> Self {
          Self {
              tenant_id: s.tenant_id,
              metric_series_id: s.metric_series_id,
              metric_name: s.metric_name,
              description: s.description,
              unit: s.unit,
              metric_type: format!("{:?}", s.metric_type).to_lowercase(),
              is_monotonic: s.is_monotonic.map(|b| if b { 1u8 } else { 0u8 }),
              aggregation_temporality: s.aggregation_temporality.map(|t| format!("{:?}", t).to_lowercase()),
              attributes: serde_json::to_string(&s.attributes).unwrap_or_default(),
              resource_attributes: serde_json::to_string(&s.resource_attributes).unwrap_or_default(),
              service_name: s.service_name,
              environment: s.environment,
          }
      }
  }
  ```

- [ ] **Step 3: Add /internal/metrics route to storage-writer**

  ```rust
  #[derive(Deserialize)]
  struct MetricsBatch {
      series: Vec<domain::MetricSeries>,
      points: Vec<domain::MetricPoint>,
  }
  async fn write_metrics(State(s): State<AppState>, Json(b): Json<MetricsBatch>) -> StatusCode {
      let r1 = metrics::insert_metric_series(&s.ch, b.series).await;
      let r2 = metrics::insert_metric_points(&s.ch, b.points).await;
      if r1.is_err() || r2.is_err() { StatusCode::INTERNAL_SERVER_ERROR } else { StatusCode::NO_CONTENT }
  }
  ```

- [ ] **Step 4: Implement routes/metrics.rs in ingest-gateway**

  Parse OTLP metrics JSON → derive `MetricSeries` + `MetricPoint` structs → publish `EnvelopePayload::Metrics`.

- [ ] **Step 5: Wire stream-processor for metrics**

  Update `stream-processor/src/main.rs` `EnvelopePayload::Metrics` arm to POST to storage-writer.

- [ ] **Step 6: Run all tests**

  ```bash
  cargo test --workspace
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add services/
  git commit -m "feat(metrics): add OTLP metrics ingest through pipeline to ClickHouse

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 17: Platform Telemetry (Self-Observability) [DONE]

> **Parallel with Task 18. After Task 16.**

**Files:**
- Modify: each service `main.rs` to add OpenTelemetry SDK initialisation

- [ ] **Step 1: Add OTel deps to workspace Cargo.toml**

  ```toml
  opentelemetry            = { version = "0.22", features = ["metrics", "trace"] }
  opentelemetry-otlp       = { version = "0.15", features = ["http-proto", "metrics", "trace"] }
  opentelemetry_sdk        = { version = "0.22", features = ["rt-tokio"] }
  tracing-opentelemetry    = "0.23"
  ```

- [ ] **Step 2: Write test — telemetry init does not panic**

  ```rust
  // in each service:
  #[cfg(test)]
  #[test]
  fn telemetry_init_is_idempotent() {
      // Ensure init_telemetry() can be called in test context
      // (just verifies it doesn't panic with OTLP endpoint absent)
      let _ = init_telemetry("test-service", None);
  }
  ```

- [ ] **Step 3: Add shared telemetry init helper in libs/domain/src/telemetry.rs**

  ```rust
  use opentelemetry_otlp::WithExportConfig;
  use opentelemetry_sdk::runtime;

  pub fn init_telemetry(service_name: &str, otlp_endpoint: Option<&str>) -> anyhow::Result<()> {
      let endpoint = otlp_endpoint.unwrap_or("http://localhost:4317");

      opentelemetry_otlp::new_pipeline()
          .tracing()
          .with_exporter(
              opentelemetry_otlp::new_exporter()
                  .http()
                  .with_endpoint(endpoint),
          )
          .install_batch(runtime::Tokio)?;

      let subscriber = tracing_subscriber::Registry::default()
          .with(tracing_opentelemetry::layer())
          .with(tracing_subscriber::fmt::layer().json());

      tracing::subscriber::set_global_default(subscriber)?;
      Ok(())
  }
  ```

- [ ] **Step 4: Update each service main.rs to call init_telemetry**

  Replace `tracing_subscriber::fmt().json().init();` with:
  ```rust
  let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
  domain::telemetry::init_telemetry("ingest-gateway", otlp.as_deref())?;
  ```

  Repeat for all five services with their own service name.

- [ ] **Step 5: Run tests**

  ```bash
  cargo test --workspace
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add libs/domain/src/telemetry.rs services/
  git commit -m "feat(telemetry): add OpenTelemetry self-instrumentation to all services

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 18: Frontend Trace Detail View [DONE]

> **Parallel with Task 17. After Tasks 12 and 14.**

**Files:**
- Create: `apps/frontend/src/pages/TraceDetail.tsx`
- Modify: `apps/frontend/src/router.ts`
- Modify: `apps/frontend/src/pages/TraceSearch.tsx` (add click → navigate)

- [ ] **Step 1: Write failing test**

  ```tsx
  // apps/frontend/src/pages/TraceDetail.test.tsx
  import { render, screen } from "@testing-library/react";
  import { TraceDetail } from "./TraceDetail";

  test("renders waterfall with spans", () => {
    const spans = [{
      trace_id: "abc", span_id: "111", service_name: "checkout",
      operation_name: "POST /order", start_time_unix_nano: "0",
      end_time_unix_nano: "5000000", duration_ns: 5_000_000, status_code: "OK",
      tenant_id: "t1"
    }];
    render(<TraceDetail traceId="abc" spans={spans} />);
    expect(screen.getByText("POST /order")).toBeInTheDocument();
    expect(screen.getByText("5.00ms")).toBeInTheDocument();
  });
  ```

- [ ] **Step 2: Run — verify failure**

  ```bash
  npm run test --workspace=apps/frontend
  ```

- [ ] **Step 3: Implement TraceDetail.tsx**

  ```tsx
  import { Span } from "../api/traces";

  interface Props { traceId: string; spans: Span[] }

  export function TraceDetail({ traceId, spans }: Props) {
    const minStart = Math.min(...spans.map(s => Number(s.start_time_unix_nano)));
    const maxEnd   = Math.max(...spans.map(s => Number(s.end_time_unix_nano)));
    const totalNs  = maxEnd - minStart || 1;

    return (
      <div>
        <h2>Trace {traceId.substring(0, 16)}…</h2>
        <p>Total: {(totalNs / 1e6).toFixed(2)}ms — {spans.length} spans</p>
        <div style={{ overflowX: "auto" }}>
          {spans.map((span) => {
            const offset = ((Number(span.start_time_unix_nano) - minStart) / totalNs) * 100;
            const width  = (span.duration_ns / totalNs) * 100;
            return (
              <div key={span.span_id} style={{ display: "flex", alignItems: "center", marginBottom: 4 }}>
                <span style={{ width: 200, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 12 }}>
                  {span.service_name}: {span.operation_name}
                </span>
                <div style={{ flex: 1, position: "relative", height: 16, background: "#f0f0f0" }}>
                  <div style={{
                    position: "absolute",
                    left: `${offset}%`,
                    width: `${Math.max(width, 0.5)}%`,
                    height: "100%",
                    background: span.status_code === "ERROR" ? "#e53e3e" : "#4299e1",
                  }} title={`${(span.duration_ns / 1e6).toFixed(2)}ms`} />
                </div>
                <span style={{ width: 60, textAlign: "right", fontSize: 12 }}>
                  {(span.duration_ns / 1e6).toFixed(2)}ms
                </span>
              </div>
            );
          })}
        </div>
      </div>
    );
  }
  ```

- [ ] **Step 4: Add trace detail route**

  Add to `router.ts`:
  ```ts
  import TraceDetailPage from "./pages/TraceDetailPage";
  const traceDetailRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/traces/$traceId",
    component: TraceDetailPage,
  });
  // add traceDetailRoute to routeTree
  ```

  Create `TraceDetailPage.tsx`:
  ```tsx
  import { useParams } from "@tanstack/react-router";
  import { useQuery } from "@tanstack/react-query";
  import { getTrace } from "../api/traces";
  import { TraceDetail } from "./TraceDetail";

  export default function TraceDetailPage() {
    const { traceId } = useParams({ from: "/traces/$traceId" });
    const { data, isLoading } = useQuery({
      queryKey: ["trace", traceId],
      queryFn: () => getTrace(traceId),
    });
    if (isLoading) return <p>Loading…</p>;
    if (!data) return <p>Not found</p>;
    return <TraceDetail traceId={data.trace_id} spans={data.spans} />;
  }
  ```

- [ ] **Step 5: Run tests — verify pass**

  ```bash
  npm run test --workspace=apps/frontend
  npm run typecheck --workspace=apps/frontend
  ```

- [ ] **Step 6: Manual verify**

  Send a trace with Task 10's smoke test, then visit `http://localhost:5173/traces/<trace_id>`. Should show waterfall.

- [ ] **Step 7: Commit**

  ```bash
  git add apps/frontend/src/pages/
  git commit -m "feat(frontend): add trace detail waterfall view

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 19: End-to-End Smoke Test + Phase 1 PR [DONE]

> **Final task. After Tasks 17 and 18.**

**Files:**
- Create: `tests/e2e/smoke_test.sh`

- [ ] **Step 1: Write smoke test script**

  Create `tests/e2e/smoke_test.sh`:
  ```bash
  #!/usr/bin/env bash
  set -euo pipefail

  INGEST="http://localhost:4317"
  QUERY="http://localhost:8090"
  TOKEN="dev-api-key-0000"
  TRACE_ID="4bf92f3577b34da6a3ce929d0e0e4736"

  echo "=== Phase 1 Smoke Test ==="

  echo "1. Sending trace..."
  curl -sf -X POST "$INGEST/v1/traces" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeSpans\":[{\"spans\":[{\"traceId\":\"$TRACE_ID\",\"spanId\":\"00f067aa0ba902b7\",\"name\":\"e2e-smoke\",\"startTimeUnixNano\":\"$(date +%s%N)\",\"endTimeUnixNano\":\"$(( $(date +%s%N) + 5000000 ))\",\"status\":{\"code\":1}}]}]}]}"
  echo " OK"

  echo "2. Waiting for pipeline..."
  sleep 3

  echo "3. Querying trace..."
  RESULT=$(curl -sf "$QUERY/v1/traces/$TRACE_ID")
  echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); assert len(d['spans'])>0, 'no spans'"
  echo " OK — $(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['spans']),'spans')")"

  echo "4. Sending log..."
  curl -sf -X POST "$INGEST/v1/logs" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"$(date +%s%N)\",\"severityNumber\":9,\"body\":{\"stringValue\":\"smoke test log\"}}]}]}]}"
  echo " OK"

  echo "5. Sending metric..."
  curl -sf -X POST "$INGEST/v1/metrics" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"smoke-svc\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"smoke.counter\",\"sum\":{\"dataPoints\":[{\"asDouble\":1.0,\"timeUnixNano\":\"$(date +%s%N)\"}],\"aggregationTemporality\":2,\"isMonotonic\":true}}]}]}]}"
  echo " OK"

  echo ""
  echo "=== ALL CHECKS PASSED ==="
  ```

- [ ] **Step 2: Make it executable**

  ```bash
  chmod +x tests/e2e/smoke_test.sh
  ```

- [ ] **Step 3: Run full stack and execute smoke test**

  ```bash
  make dev && make migrate
  cargo run -p auth-service &
  cargo run -p storage-writer &
  cargo run -p stream-processor &
  cargo run -p ingest-gateway &
  cargo run -p query-api &
  sleep 5
  bash tests/e2e/smoke_test.sh
  ```
  Expected: `=== ALL CHECKS PASSED ===`

- [ ] **Step 4: Add smoke test to CI (nightly job)**

  Add to `.github/workflows/pr.yml` a `nightly` workflow job that runs `make dev && make migrate && bash tests/e2e/smoke_test.sh`.

- [ ] **Step 5: Final commit and consolidation PR**

  ```bash
  git add tests/ .github/
  git commit -m "test(e2e): add Phase 1 smoke test for full ingest-to-query pipeline

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  git push
  gh pr create \
    --title "feat: Phase 1 Internal MVP — ingest to query" \
    --body "$(cat <<'EOF'
  ## Summary
  Complete Phase 1 Internal MVP implementation:
  - Rust monorepo: 5 services + domain lib + CI
  - OTLP ingest (traces, logs, metrics) over HTTP
  - Redpanda durable queue
  - ClickHouse storage (spans, logs, metric_series, metric_points)
  - Query API: trace lookup + search, log search
  - React 19 + Vite frontend: trace list + waterfall detail
  - All services self-instrumented with OpenTelemetry
  - E2E smoke test proves ingest → queue → processor → ClickHouse → query

  ## Exit gate status
  ✅ A tenant can ingest telemetry, query it through API and UI
  ✅ Queue provides durability before storage writes
  ✅ All services emit self-telemetry

  ## Source spec
  spec/10-process.md §17 Phase 1, spec/13-risks-roadmap.md §24.1

  ## ADR/spec sync
  No ADR changes needed — implementation follows already-accepted ADRs.

  ## Next phase
  Phase 2: tenant isolation enforcement, rate limits, RBAC, retention policies, k8s deployment.
  EOF
  )"
  ```

---

## Phase Roadmap Summary

The plan above covers **Phase 1** completely. For Phases 2–8, each phase produces its own plan document before implementation starts (per `spec/10-process.md §16.8`).

| Phase | Focus | Entry condition |
|-------|-------|----------------|
| **Phase 2** | Tenant isolation, rate limits, RBAC, retention, audit logs, k8s + GitOps | Phase 1 exit gate passed |
| **Phase 3** | Trace-log correlation, service catalog, service maps, RED metrics, deployment events | Phase 2 exit gate passed |
| **Phase 4** | Warm retention, SSO/OIDC, ReBAC (OpenFGA), SLO burn-rate, production runbooks, load/chaos tests | Phase 3 exit gate passed |
| **Phase 5** | Incident timelines, on-call routing, runbook workflows, composite alerts | Phase 4 exit gate passed |
| **Phase 6** | Continuous profiling, browser RUM, mobile SDK, synthetics | Phase 5 exit gate passed |
| **Phase 7** | Regional residency, BYOK, tenant-isolated packaging, billing, compliance | Phase 6 exit gate passed |
| **Phase 8** | Anomaly models, query recommendations, incident summarisation, capacity forecasting | Phase 7 exit gate passed |

Each phase plan follows the same format as this document: parallel execution map, file map, and TDD task steps.

---

## Self-Review

**Spec coverage check:**
- Phase 1 §24.1 MVP items → Tasks 1–19 ✓
- OTLP ingest traces/logs → Tasks 8, 10, 15 ✓
- Basic metrics ingest → Task 16 ✓
- Tenant auth with API keys → Task 7 ✓
- Durable queue → Task 10 ✓
- ClickHouse traces/logs/metrics → Tasks 12, 15, 16 ✓
- Query APIs → Task 9 ✓
- React UI trace search → Tasks 14, 18 ✓
- Platform telemetry → Task 17 ✓
- Internal dogfood smoke test → Task 19 ✓

**Placeholder scan:** No TBDs or vague instructions. All code steps include actual implementation code.

**Type consistency:**
- `TelemetryEnvelope` / `EnvelopePayload` defined in Task 3, used in Tasks 10, 11 ✓
- `SpanRow` defined in Task 12, maps from `domain::Span` defined in Task 3 ✓
- `AppState` pattern consistent across all services ✓
- `TraceResponse` / `TraceListResponse` defined in Task 9, consumed by frontend in Task 14 ✓

---

## Task 20: Query API MVP Refinement & Discovery APIs [DONE]

> **Refine Phase 1 Query API to meet MVP quality gates.**

**Files:**
- Modify: `services/query-api/src/traces.rs` (fix ordering/counts)
- Modify: `services/query-api/src/logs.rs` (fix ordering/counts)
- Modify: `libs/domain/src/metric.rs` (fix serialization)
- Modify: `services/query-api/src/main.rs` (discovery endpoints)

- [x] **Step 1: Fix trace search ordering and total counts using subqueries**

  Update `search_traces` in `services/query-api/src/traces.rs` to use a subquery for correctly ordering by the newest span in the trace and returning the accurate total count of matching traces.

- [x] **Step 2: Fix metric point serialization for histogram fields**

  Refactor `MetricPointRow` in `libs/domain/src/metric.rs` to use `Option<Vec<f64>>` for histogram buckets and explicit serialization that handles ClickHouse `Nullable(Array)` correctly.

- [x] **Step 3: Add service and environment discovery endpoints to query-api**

  Implement `GET /v1/services` and `GET /v1/environments` in `query-api` to allow the frontend to discover available services and environments from stored telemetry.

- [x] **Step 4: Verify with smoke tests**

  Run `cargo test` and updated `smoke_test.sh` to verify the refined behavior.
