# Signal Support Matrix

This document describes which telemetry signals Observable supports, their ingestion paths,
and known limitations as of the 0.1 release.

## Ingestion protocols

| Protocol | Signals | Status |
|----------|---------|--------|
| OTLP/gRPC (port 4317) | Traces, Logs, Metrics | Supported |
| OTLP/HTTP (port 4318) | Traces, Logs, Metrics | Supported |
| Prometheus Remote Write (port 4318) | Metrics | Supported |


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
| ExponentialHistogram | Modeled | Not ingested | Present in domain model but silently dropped during ingestion |
| Summary | Modeled | Not ingested | Present in domain model but silently dropped during ingestion |

### Histogram fidelity

OTLP explicit-bucket histograms are fully supported. Each bucket's `explicitBounds`,
`bucketCounts`, `count`, `sum`, `min`, and `max` fields are preserved through ingestion
and stored in ClickHouse. Querying returns the original bucket structure.

**ExponentialHistogram** and **Summary** metric types have Rust domain-model structs but
no conversion path in either the OTLP/gRPC or OTLP/HTTP ingest pipelines. Data points
of these types are silently dropped — no error is returned to the sender and no data is
stored. This is a known data-loss scenario (see below).

### Prometheus Remote Write

Prometheus timeseries are converted to Observable's metric model on ingestion. Histogram
buckets (distinguished by the `le` label) are grouped into a single histogram series with
explicit bounds. The `+Inf` bucket is required; timeseries without it are emitted as
individual gauge series instead. Native histograms are not yet supported.

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
