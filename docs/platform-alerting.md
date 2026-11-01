# Platform Health & Alerting Guide

This guide describes the platform-level health monitoring, Prometheus alerting rules, ServiceMonitors, and Grafana dashboards for the Observable platform in Kubernetes.

For service-level runbooks, see [docs/runbooks/README.md](runbooks/README.md).

---

## Overview

Observable components expose standard Prometheus `/metrics` endpoints and readiness/liveness `/health` endpoints. The platform provides out-of-the-box integration with the [Prometheus Operator](https://prometheus-operator.dev/) and `kube-prometheus-stack` via:

- **`ServiceMonitor`**: Automatically discovers and scrapes all Observable services on their respective HTTP and platform ports.
- **`PrometheusRule`**: Codified alert definitions targeting critical failure modes across the ingest, stream processing, storage, query, and identity paths.
- **Grafana Dashboard**: A unified overview of platform service status, ingestion throughput, pipeline errors, and request latencies (`dashboards/observable-platform-health.json`).

---

## Component Metrics & Scrape Ports

| Component | Port Name | Port | Path | Key Metrics |
|-----------|-----------|------|------|-------------|
| `ingest-gateway` | `platform` | 4321 | `/metrics` | `ingest_gateway_requests_total`, `ingest_gateway_rejections_total`, `ingest_gateway_spans_received_total` |
| `stream-processor` | `http` | 4323 | `/metrics` | `stream_processor_batches_processed_total`, `stream_processor_flush_errors_total` |
| `storage-writer` | `http` | 4320 | `/metrics` | `storage_writer_http_requests_total`, `storage_writer_http_request_duration_seconds` |
| `query-api` | `http` | 8090 | `/metrics` | `query_api_http_requests_total`, `query_api_http_request_duration_seconds` |
| `auth-service` | `http` | 4319 | `/metrics` | `auth_service_http_requests_total`, `auth_service_http_request_duration_seconds` |
| `admin-service` | `http` | 4324 | `/metrics` | `admin_service_http_requests_total`, `admin_service_http_request_duration_seconds` |
| `alert-evaluator` | `http` | 4322 | `/metrics` | `alert_evaluator_http_requests_total`, `alert_evaluator_evaluations_total` |

---

## Critical Path Alert Definitions

The `PrometheusRule` resource defines alerts across 6 critical operational domains. Each alert includes direct links to operational runbooks for immediate triage.

### 1. Ingestion Pipeline (`observable.ingest.alerts`)

| Alert Name | Severity | Condition | Description | Runbook |
|------------|----------|-----------|-------------|---------|
| `ObservableIngestGatewayDown` | `critical` | `up{app_kubernetes_io_component="ingest-gateway"} == 0` for 2m | Ingest gateway pods are unresponsive; telemetry ingress is down. | [ingest-queue-failure.md](runbooks/ingest-queue-failure.md) |
| `ObservableIngestHighRejectionRate` | `warning` | Rejection rate exceeds 5% of total ingress requests for 5m | Ingestion requests rejected due to rate limits, tenant limits, or invalid auth. | [ingest-queue-failure.md](runbooks/ingest-queue-failure.md) |

### 2. Stream Processing (`observable.stream.alerts`)

| Alert Name | Severity | Condition | Description | Runbook |
|------------|----------|-----------|-------------|---------|
| `ObservableStreamProcessorDown` | `critical` | `up{app_kubernetes_io_component="stream-processor"} == 0` for 2m | Stream processor is down; raw telemetry accumulating in Redpanda without normalization. | [ingest-queue-failure.md](runbooks/ingest-queue-failure.md) |
| `ObservableStreamProcessorFlushErrors` | `warning` | `rate(stream_processor_flush_errors_total[5m]) > 0` for 5m | Errors encountered flushing span metrics to the normalized stream. | [ingest-queue-failure.md](runbooks/ingest-queue-failure.md) |

### 3. Storage & Persistence (`observable.storage.alerts`)

| Alert Name | Severity | Condition | Description | Runbook |
|------------|----------|-----------|-------------|---------|
| `ObservableStorageWriterDown` | `critical` | `up{app_kubernetes_io_component="storage-writer"} == 0` for 2m | Storage writer is down; normalized telemetry is not persisting to ClickHouse. | [storage-recovery.md](runbooks/storage-recovery.md) |
| `ObservableStorageWriterHighErrorRate` | `critical` | 5xx error rate exceeds 1% of write batches for 5m | Storage writer failing to persist batches to ClickHouse. | [storage-recovery.md](runbooks/storage-recovery.md) |

### 4. Query & Analytics (`observable.query.alerts`)

| Alert Name | Severity | Condition | Description | Runbook |
|------------|----------|-----------|-------------|---------|
| `ObservableQueryApiDown` | `critical` | `up{app_kubernetes_io_component="query-api"} == 0` for 2m | Query API is down; dashboard and telemetry reads are unavailable. | [dependency-outage.md](runbooks/dependency-outage.md) |
| `ObservableQueryApiHighErrorRate` | `warning` | 5xx error rate exceeds 5% of queries for 5m | Query API returning internal errors on queries. | [dependency-outage.md](runbooks/dependency-outage.md) |

### 5. Identity & Authorization (`observable.auth.alerts`)

| Alert Name | Severity | Condition | Description | Runbook |
|------------|----------|-----------|-------------|---------|
| `ObservableAuthServiceDown` | `critical` | `up{app_kubernetes_io_component="auth-service"} == 0` for 2m | Auth service is down; user sessions and ingestion key checks fail closed. | [auth-identity-failure.md](runbooks/auth-identity-failure.md) |
| `ObservableAuthServiceHighErrorRate` | `critical` | 5xx error rate exceeds 2% for 5m | Auth service encountering errors validating sessions or keys. | [auth-identity-failure.md](runbooks/auth-identity-failure.md) |

### 6. Control Plane & Alerting (`observable.control.alerts`)

| Alert Name | Severity | Condition | Description | Runbook |
|------------|----------|-----------|-------------|---------|
| `ObservableAdminServiceDown` | `warning` | `up{app_kubernetes_io_component="admin-service"} == 0` for 2m | Admin service is down; tenant configuration changes are blocked. | [dependency-outage.md](runbooks/dependency-outage.md) |
| `ObservableAlertEvaluatorDown` | `warning` | `up{app_kubernetes_io_component="alert-evaluator"} == 0` for 2m | Alert evaluator is down; alert condition evaluation is suspended. | [dependency-outage.md](runbooks/dependency-outage.md) |

---

## Enabling Monitoring in Helm

To deploy `ServiceMonitor`, `PrometheusRule`, and Grafana dashboard resources with your release, enable them in your `values.yaml`:

```yaml
monitoring:
  prometheusRule:
    enabled: true
    additionalLabels:
      release: prometheus-stack  # Set to match your prometheus-operator ruleSelector
  serviceMonitor:
    enabled: true
    interval: "15s"
    scrapeTimeout: "10s"
    additionalLabels:
      release: prometheus-stack  # Set to match your prometheus-operator serviceMonitorSelector
  grafanaDashboard:
    enabled: true
    additionalLabels:
      grafana_dashboard: "1"     # Picked up automatically by Grafana sidecar
```

### Manual Dashboard Import

If not using the Grafana sidecar, import [dashboards/observable-platform-health.json](../dashboards/observable-platform-health.json) directly in the Grafana UI (**Dashboards** → **New** → **Import**).
