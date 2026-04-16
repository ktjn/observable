# ADR-017: Prometheus remote_write Compatibility

**Date:** 2026-04-16
**Status:** Accepted
**Authors:** Claude Code
**Deciders:** Project Stakeholders
**Review date:** 2026-10-16

## Context

`spec/02-architecture.md §4.1` lists `Prometheus remote_write receiver` as a required ingest interface alongside OTLP. The majority of existing infrastructure tooling (node_exporter, kube-state-metrics, Blackbox Exporter, and most Kubernetes operators) exposes metrics in Prometheus exposition format and writes to remote_write endpoints. Requiring customers to migrate to OTel-native metrics before they can use the platform creates significant onboarding friction.

ADR-001 establishes OTel as the primary external contract. This ADR defines how Prometheus `remote_write` fits alongside that contract without undermining the OTel-first principle.

## Decision

The platform will accept **Prometheus `remote_write` (v1 and v2) at the ingest gateway as a first-class input alongside OTLP**. Prometheus-originated metrics are translated into the platform's internal OTel-aligned `MetricSeries` + `MetricPoint` model at the ingest boundary before they enter the durable queue.

Specifically:

- The ingest gateway exposes a `/api/v1/write` endpoint compatible with Prometheus `remote_write` v1 and the `remote_write` v2 specification (content-type `application/x-protobuf`, snappy-compressed).
- Translation at the ingest boundary:
  - Prometheus metric name → `metric_name`
  - Prometheus label set → `MetricSeries.attributes`
  - Prometheus counter → `type = sum`, `is_monotonic = true`, `aggregation_temporality = cumulative`
  - Prometheus gauge → `type = gauge`
  - Prometheus histogram (classic) → `type = histogram`, explicit buckets
  - Prometheus native histogram → `type = exponential_histogram`
  - Prometheus summary → `type = summary`
  - `job` label → `service_name` (with fallback to `observable.service_name` label if present)
  - `instance` label → `workload` / `host_id` enrichment via instance-to-host mapping
- All translated signals carry `observable.ingest_source = prometheus_remote_write` as a resource attribute for provenance tracing.
- OTel semantic convention labels (e.g., `service.name`) are preserved as-is if present in the Prometheus label set.

**Boundary:** Prometheus `remote_write` is an ingest compatibility layer only. The query API, data model, and alerting evaluation are OTel-native throughout. No Prometheus-specific query DSL (PromQL) is exposed on the query path; however, the query facade may accept PromQL-compatible expressions as a query language option (a separate decision outside this ADR).

## Consequences

**Easier:**
- Zero migration friction for customers with Prometheus-based infrastructure monitoring
- Native support for kube-state-metrics, node_exporter, and operator-exported metrics without an OTel Collector translation hop
- Existing Grafana / alerting setups that push via remote_write can be pointed at the platform immediately

**Harder:**
- Prometheus label semantics do not map perfectly to OTel resource vs. metric attributes; the translation heuristic for `job`/`instance` must be documented and tested
- Prometheus `remote_write` v1 has no native support for resource attributes, so OTel resource context (cluster, namespace, pod) must be inferred from labels or injected via an OTel Collector enrichment hop upstream
- The platform must maintain a compatible `remote_write` endpoint through Prometheus specification revisions

**Constrained:**
- Prometheus `remote_write` is a compatibility surface; new platform features (exemplar-native queries, OTel temporality controls) are not accessible to pure remote_write producers without migrating to OTLP

## Alternatives Considered

### Option A: Require OTel Collector translation upstream
Reject native remote_write; require all Prometheus sources to use an OTel Collector with `prometheusreceiver` before reaching the platform.

**Rejected** because it adds an operational hop to every Prometheus-instrumented environment, increases onboarding complexity, and requires customers to maintain a Collector configuration just to forward existing metrics.

### Option B: Accept remote_write but store as a separate Prometheus-native signal
Maintain a separate storage path and query API for Prometheus-originated metrics.

**Rejected** because it splits the unified data model and creates two classes of metrics with different query semantics.

## Related

- `spec/02-architecture.md` §4.1 (Required ingest interfaces)
- `spec/14-domain-model.md` (MetricSeries and MetricPoint schemas)
- `ADR-001-otel-external-contract.md` — OTel remains the primary contract; remote_write is a compatibility layer
- `ADR-003-clickhouse-boundary.md` — translated MetricPoints write through the same ClickHouse path
