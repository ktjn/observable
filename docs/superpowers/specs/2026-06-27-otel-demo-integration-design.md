# OTel Demo Integration Design

**Date:** 2026-06-27
**Status:** Approved

## Overview

Replace the custom testbench shop with the official [OpenTelemetry Demo](https://github.com/open-telemetry/opentelemetry-demo) as the primary signal-generating demo in the Observable docker-compose setup. The OTel demo is a ~20-service polyglot microservices application (astronomy shop theme) that produces rich, realistic traces, metrics, and logs — a much more compelling signal source than the custom Python shop.

## Goals

- Add the full OTel demo as a default-on service set (starts with `docker compose up`, same as `crypto-demo`)
- Route all OTel demo telemetry through Observable's `ingest-gateway` (not Jaeger/Prometheus/Grafana)
- Remove the custom testbench shop and its infrastructure entirely
- Keep `crypto-demo` untouched

## Non-Goals

- Keeping any of the OTel demo's observability backends (Jaeger, Prometheus, Grafana, OpenSearch)
- Using a git submodule — images are pulled from `ghcr.io/open-telemetry/demo:*`
- Wrapping services in a Docker Compose profile

## File Changes

### New: `demos/otel-demo/docker-compose.yml`

Vendored from the OTel demo's published docker-compose, with these modifications:
- `jaeger`, `prometheus`, `grafana`, `opensearch`, `opensearch-data-prepper` services removed
- `otelcol` service's config volume replaced with our custom `otelcol-config.yml` (see below)
- `otelcol` gets `depends_on: ingest-gateway: { condition: service_healthy }` so it starts only once the platform is ready
- All services reference `ingest-gateway` on the Observable default Docker network

### New: `demos/otel-demo/otelcol-config.yml`

A minimal OpenTelemetry Collector config that:

```yaml
receivers:
  otlp:
    protocols:
      grpc: { endpoint: "0.0.0.0:4317" }
      http: { endpoint: "0.0.0.0:4318" }

exporters:
  otlp/observable:
    endpoint: "ingest-gateway:4317"
    headers:
      authorization: "Bearer otel-demo-api-key-0000"
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlp/observable]
    metrics:
      receivers: [otlp]
      exporters: [otlp/observable]
    logs:
      receivers: [otlp]
      exporters: [otlp/observable]
```

This replaces the OTel demo's default collector config entirely, stripping out all Jaeger/Prometheus/OpenSearch routing.

### New: `migrations/postgres/034_add_otel_demo_tenant.sql`

Follows the pattern of `033_add_crypto_demo_tenant.sql`:

- Tenant: `id = '00000000-0000-0000-0000-000000000004'`, `name = 'otel-demo'`
- API key: plaintext `otel-demo-api-key-0000`, SHA-256 stored in `api_keys` table, role `member`, environment `otel-demo`

### Modified: `docker-compose.yml`

Two changes:
1. Add `include:` block at the top referencing `demos/otel-demo/docker-compose.yml`
2. Remove shop testbench services: `shop-db`, `shop-queue`, `shop-api`, `shop-worker`, `shop-loadgen`
3. Remove `shop_db_data` volume

### Deleted: `testbench/`

The entire `testbench/` directory is removed:
- `testbench/api/` — Python FastAPI shop
- `testbench/worker/` — Python RabbitMQ consumer
- `testbench/loadgen/` — Python load generator
- `testbench/frontend/` — Node.js frontend
- `testbench/db/init.sql` — shop schema

## Network Topology

The OTel demo services communicate internally on their own network. The `otelcol` service is the only one that needs to reach `ingest-gateway`. Docker Compose `include:` shares the default network, so `ingest-gateway` is reachable by hostname from `otelcol`.

## API Key

| Field | Value |
|---|---|
| Plaintext | `otel-demo-api-key-0000` |
| Tenant | `otel-demo` (UUID `00000000-0000-0000-0000-000000000004`) |
| Role | `member` |
| Environment | `otel-demo` |

The SHA-256 of `otel-demo-api-key-0000` must be computed and inserted in the migration (same pattern as crypto-demo).

## Startup Order

```
postgres/clickhouse/redpanda healthy
  → *-setup jobs complete
    → ingest-gateway healthy
      → otelcol starts (depends_on ingest-gateway)
        → OTel demo microservices start (depend on otelcol)
```

## Testing / Verification

After `docker compose up`:
1. All OTel demo services reach healthy/running state
2. The astronomy shop frontend is reachable (default port `8080`)
3. Traces appear in Observable's trace explorer within ~30 seconds of the load generator starting
4. `shop-*` services are absent from `docker compose ps` output
