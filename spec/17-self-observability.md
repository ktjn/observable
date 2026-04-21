# Self-Observability Specification

## 1. Overview

Self-observability is the platform's ability to monitor its own health, performance, and reliability. This is a critical requirement for an observability platform — we must be more reliable than the services we monitor.

## 2. Telemetry Strategy

The platform employs a dual-path self-monitoring strategy:

### 2.1 Recursive (In-Band) Monitoring
Every platform service is instrumented as a "first-class citizen" of the platform.
- **Traces:** Services emit OTLP traces to the `ingest-gateway`.
- **Metrics:** Services emit OTLP metrics to the `ingest-gateway`.
- **Logs:** Services emit OTLP logs to the `ingest-gateway`.
- **Target:** Telemetry is stored in a dedicated `system` or `platform` tenant.
- **Value:** This dogfoods the entire pipeline and provides deep visibility into complex requests (e.g., a query spanning multiple services).

### 2.2 Independent (Out-of-Band) Monitoring
To detect and debug failures in the platform itself (e.g., the `ingest-gateway` is down), a secondary, independent path is required.
- **Health Checks:** Every service exposes a `/health` or `/livez` / `/readyz` endpoint.
- **Prometheus Metrics:** Services expose a `/metrics` endpoint in Prometheus format, containing core operational metrics (e.g., process memory, CPU, request counts, queue lag).
- **External Probes:** An independent monitoring agent (e.g., standard Prometheus or a separate synthetic checker) scrapes these endpoints.
- **Value:** Provides visibility when the main telemetry pipeline is congested or failing.

## 3. Core Service-Level Indicators (SLIs)

| SLI | Measurement Point | Definition |
|-----|-------------------|------------|
| Ingest Availability | `ingest-gateway` | Success ratio (2xx/3xx) of OTLP export requests. |
| Ingest Latency | E2E | Time from `ingest-gateway` receive to `storage-writer` success. |
| Query Availability | `query-api` | Success ratio of telemetry queries. |
| Query Latency | `query-api` | P95/P99 latency of telemetry queries by signal type. |
| Queue Health | `redpanda` | Consumer group lag for `stream-processor` and `storage-writer`. |
| Write Latency | `storage-writer` | Batch write duration to ClickHouse. |
| Alert Latency | `alert-evaluator` | Time from threshold breach to notification delivery. |

## 4. Standard Instrumentation Requirements

Every platform service MUST:
1. Initialize the OpenTelemetry SDK with `service_name`, `service_version`, and `environment`.
2. Wrap all HTTP/gRPC handlers with tracing middleware.
3. Export metrics for:
   - Request rate, error rate, and duration (RED).
   - Concurrent request counts.
   - External dependency call duration and status.
   - Internal queue/buffer sizes.
4. Export structured logs with `trace_id` correlation.
5. Provide a `/health` endpoint returning 200 OK.
6. Provide a `/ready` endpoint returning 200 OK only when dependencies (DB, Queue) are connected.

## 5. Standard Dashboards

The following dashboards MUST be available in the `system` tenant:

1. **Platform Overview:** High-level health of all services and SLI status.
2. **Ingest Pipeline:** Ingest rate, gateway errors, queue lag, and storage write throughput.
3. **Query Performance:** Query rate, P99 latency, and ClickHouse resource usage.
4. **Dependency Health:** PostgreSQL connection pool, Redpanda throughput, and ClickHouse cluster status.

## 6. Alerting and SLOs

### 6.1 Internal SLOs
The platform team maintains internal SLOs (e.g., 99.9% Ingest Availability) that are more stringent than customer-facing SLAs.

### 6.2 Critical Self-Monitoring Alerts
- **Pipeline Stalled:** No new data written to ClickHouse for > 5 minutes.
- **Queue Backlog:** Consumer lag exceeds a defined threshold (e.g., 1 million messages).
- **Service Restart Loops:** High frequency of service restarts in the last hour.
- **Storage Full:** ClickHouse or Object Storage approaching quota.

## 7. Infrastructure Monitoring

Monitoring the underlying infrastructure (K8s nodes, VMs) is handled by standard infrastructure agents (e.g., `node_exporter`), but their telemetry is also ingested into the `system` tenant.
