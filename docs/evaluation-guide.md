# Ten-Minute Evaluation Guide

Get Observable running and receiving telemetry in under ten minutes.

## Prerequisites

- Docker 20.10+ and Docker Compose v2
- 8 GB free memory (4 GB minimum)
- 20 GB free disk
- A terminal with `curl`

## 1. Start the platform

```bash
git clone https://github.com/ktjn/observable.git
cd observable
docker compose --profile evaluation up -d
```

Wait for all services to become healthy:

```bash
docker compose --profile evaluation ps
```

All containers should show `healthy` within two minutes. The frontend is
available at **http://localhost:3000** and the OTLP endpoint at
**localhost:4317** (gRPC) / **localhost:4318** (HTTP).

## 2. Create an API key

Open the browser at http://localhost:3000 and sign in through the Zitadel OIDC
flow. The default evaluation credentials are shown in the Zitadel admin console
at http://localhost:8080.

Once signed in, navigate to **Getting Started** (or `/getting-started`). The
onboarding wizard walks through language selection, API key creation, and first
signal detection.

Alternatively, create an API key via the API:

```bash
# Replace the session cookie with your own after logging in
curl -s http://localhost:4324/v1/tenants/<tenant-id>/tokens \
  -H 'Content-Type: application/json' \
  -H 'Cookie: session=<your-session-cookie>' \
  -d '{"name": "eval-key"}' | jq .
```

Save the `plaintext` value — it is shown only once.

## 3. Send traces, logs, and metrics

### Using the OpenTelemetry Collector

Create `otel-collector-config.yaml`:

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4327
      http:
        endpoint: 0.0.0.0:4328

exporters:
  otlphttp:
    endpoint: http://localhost:4318
    headers:
      Authorization: "Bearer <your-api-key>"

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlphttp]
    logs:
      receivers: [otlp]
      exporters: [otlphttp]
    metrics:
      receivers: [otlp]
      exporters: [otlphttp]
```

Run the collector:

```bash
docker run --rm --network host \
  -v "$(pwd)/otel-collector-config.yaml:/etc/otelcol/config.yaml" \
  otel/opentelemetry-collector-contrib:0.115.0
```

### Using curl (OTLP HTTP)

Send a single test trace:

```bash
curl -X POST http://localhost:4318/v1/traces \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer <your-api-key>' \
  -d '{
    "resourceSpans": [{
      "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "eval-test"}}]},
      "scopeSpans": [{
        "spans": [{
          "traceId": "aaaaaaaaaaaaaaaabbbbbbbbbbbbbbbb",
          "spanId": "ccccccccdddddddd",
          "name": "hello-observable",
          "kind": 2,
          "startTimeUnixNano": "1700000000000000000",
          "endTimeUnixNano": "1700000000050000000",
          "status": {"code": 1}
        }]
      }]
    }]
  }'
```

### Using Prometheus Remote Write

```bash
# Point your Prometheus instance at Observable
# prometheus.yml
remote_write:
  - url: http://localhost:4318/api/v1/write
    headers:
      Authorization: Bearer <your-api-key>
```

## 4. Explore your data

Open http://localhost:3000 and navigate to:

- **Traces** — search by service name, duration, or status code. Click a trace
  to see the waterfall view and correlated logs.
- **Logs** — filter by severity, service, or full-text search.
- **Metrics** — browse available metric names, view time series charts.
- **Dashboards** — create custom dashboards with metric, log, and trace panels.
- **Alerts** — define threshold rules with webhook notification channels.

## 5. Cross-signal correlation

Observable automatically correlates signals using trace and span IDs:

1. Open a trace in the **Traces** view.
2. The **Trace-correlated logs** panel shows all logs sharing that trace ID.
3. Click a span to filter logs to that exact span.
4. From any log entry with a trace ID, click through to the trace view.

## 6. Self-observability

Observable monitors itself. Import the self-health dashboard:

```bash
curl -X POST http://localhost:4324/v1/tenants/<tenant-id>/dashboards/import \
  -H 'Content-Type: application/json' \
  -H 'Cookie: session=<your-session-cookie>' \
  -d @dashboards/observable-self-health.json
```

This dashboard shows per-service traces, error rates, and platform health.

## 7. Clean up

```bash
docker compose --profile evaluation down -v
```

The `-v` flag removes all data volumes. Omit it to preserve data between
restarts.

## What to evaluate

- **Ingestion**: Can your applications send OTLP traces, logs, and metrics?
- **Query latency**: Are trace and log searches responsive for your volume?
- **Correlation**: Does trace-to-log navigation work with your instrumentation?
- **Alerting**: Do threshold alerts fire and deliver webhooks correctly?
- **Multi-tenancy**: Are tenants isolated in queries and administration?

## Next steps

- See [Deployment Guide](deployment.md) for production-like Helm configuration.
- See [Resource Requirements](resource-requirements.md) for capacity planning.
- See [Capacity and Recovery](capacity-recovery.md) for operational limits.
- See [Backup and Restore](backup-restore.md) for data protection.
- See [Architecture](architecture.md) for component details and troubleshooting.
- See [Signal Support Matrix](signal-support.md) for supported data types.
