# Signal Support Matrix

This document describes which telemetry signals Observable supports, their ingestion paths,
and known limitations as of the 0.1 release.

## Ingestion protocols

| Protocol | Signals | Status |
|----------|---------|--------|
| OTLP/gRPC (port 4317) | Traces, Logs, Metrics | Supported |
| OTLP/HTTP (port 4318) | Traces, Logs, Metrics | Supported |
| Prometheus Remote Write (port 4318) | Metrics | Supported |

Other sources (syslog, log4j2, MQTT, webhooks) are handled by the standalone
[Collectable](https://github.com/ktjn/collectable) edge-pipeline tool, which emits OTLP.

## Trace support

| Feature | Status |
|---------|--------|
| Span ingestion and storage | Supported |
| Span attributes, events, links | Supported |
| Resource and scope attributes | Supported |
| Trace/span ID correlation | Supported |
| Service map / topology | Supported |

## Log support

| Feature | Status |
|---------|--------|
| Log record ingestion and storage | Supported |
| Severity, body, attributes | Supported |
| Trace/span ID correlation | Supported |
| Resource and scope attributes | Supported |

## Metric support

| Metric type | Ingestion | Storage | Known limitations |
|-------------|-----------|---------|-------------------|
| Gauge | Supported | Supported | |
| Sum (counter) | Supported | Supported | |
| Histogram | Supported | Supported | Bucket boundaries preserved |
| ExponentialHistogram | Modeled | Not ingested | Present in domain model but no ingest-path conversion yet |
| Summary | Modeled | Not ingested | Present in domain model but no ingest-path conversion yet |

### Prometheus Remote Write

Prometheus timeseries are converted to Observable's metric model on ingestion. Histogram
buckets are grouped and preserved. Native histograms are not yet supported.

## Alerting

| Alert type | Status |
|------------|--------|
| Threshold | Supported |
| Composite | Supported |
| SLO burn-rate | Supported |

Notification channel: webhook. Additional channels (email, Slack, PagerDuty) are deferred
beyond 0.1.

## Known data-loss scenarios

These are known situations where data may be lost or degraded:

1. **ExponentialHistogram and Summary metrics** are accepted by the OTLP endpoint without
   error but are not persisted — they are silently dropped during ingestion conversion.
2. **Redpanda unavailability** during ingestion causes data to be rejected (fail-closed).
   The ingest gateway does not buffer or retry.
3. **ClickHouse unavailability** causes the storage writer to stop consuming from the queue.
   Data remains in Redpanda until retention limits are reached.
4. **Retention** is governed by ClickHouse TTL settings. No built-in tiered storage or
   archival is provided in 0.1.

## Cross-signal correlation

Traces, logs, and metrics are correlated through shared identities:
- Tenant ID and environment (scoped per ingestion token)
- Service name (from resource attributes)
- Trace ID and span ID (for trace-log correlation)
