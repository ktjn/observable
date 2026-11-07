# ADR-045: Evaluate VictoriaMetrics and VictoriaLogs behind signal-specific storage contracts

**Date:** 2026-11-07  
**Status:** Accepted evaluation decision — **no production storage migration authorized**

## Context

Observable currently uses ClickHouse for logs, traces, metrics and analytical evidence; PostgreSQL for transactional control state; Redpanda for durable buffering. VictoriaMetrics provides a metrics-oriented query and storage model (MetricsQL, PromQL-compatible queries). VictoriaLogs provides log-oriented full-text queries and exploration (LogsQL, facets, tail). Specialized engines may offer better metrics semantics, compression, query latency or operational cost, but introduce additional query languages, failure modes and lifecycle overhead.

## Decision

1. **Keep ClickHouse** as the primary engine for all three signals during the 2.0 baseline; preserve PostgreSQL and Redpanda.
2. **Benchmark VictoriaMetrics first** against ClickHouse for real metric ingestion, high-cardinality series, rate/increase, histograms, alerts and SLO queries.
3. **Benchmark VictoriaLogs separately** against ClickHouse for structured/unstructured search, fields/facets, live tail, correlations and incident workflows.
4. Include a **hybrid** candidate: VictoriaMetrics for metrics + ClickHouse for logs, traces and business analytics. Evaluate specialized VictoriaLogs only if its measured improvement justifies another engine.
5. Keep raw telemetry storage behind separately versioned metrics/logs/traces capability adapters and test against the same accepted query and authorization contracts. Do **not** weaken business requirements to match backend-specific capabilities.
6. Cross-signal correlation is an Observable responsibility. Every engine must preserve stable tenant/environment/service/trace/span and business reference semantics, evidence provenance, explicit sampling/freshness and authorization scope.
7. Do not require DataFusion or an additional tracing engine to evaluate or adopt one signal-specific backend.
8. Treat single-node and cluster tenancy differently. Prove tenant isolation at the authoritative query/API boundary for every topology; label-based tenant separation is not sufficient by itself for hostile multi-tenant security.

## Benchmark protocol

Run all candidates against an identical synthetic/replayed workload representative of target deployments, with equal retention, replication, hardware, resource limits and recovery assumptions.

| Dimension | Evidence |
|---|---|
| Ingestion | sustained samples/log events per second, tail latency, rejected/backpressured events, replay correctness |
| Storage | disk bytes per million events and per retention horizon, compression, compaction overhead |
| Metrics queries | P50/P95 and correctness for rate/increase, histograms, range queries, SLO burn rate, cardinality |
| Logs queries | P50/P95 for filter, facet, high-cardinality fields, live tail, time-neighbor context |
| Correlation | UX-02/03/04 evidence drill-down across metrics, logs, traces, incidents and business references |
| Security | tenant escape, purpose denial, protected existence, injection and audit tests |
| Recovery | lost/slow node, broker replay, backup/restore, delayed/stale evidence and failure visibility |
| Operations | resource cost, component count, upgrades, backups, diagnostics and required operator work |

Record raw benchmark commands, fixtures, deployment manifests, exact versions, measurements and negative tests. Acceptance thresholds must be business-approved before comparison; no arbitrary winning percentage is prescribed.

## Adoption gate

A technology-change ADR and compatibility plan are required before switching the authoritative backend. The candidate must have contract parity for the affected requirements, passing security/negative tests, independent mocks, rollback/recovery plan and a meaningful measured benefit after operational cost. A single good vendor benchmark is not sufficient.

The first experiment should be **VictoriaMetrics vs ClickHouse for metrics**. VictoriaLogs is the second, independently reversible experiment. No changes to ingestion schemas or current deployment are mandated by this ADR.

## References

- [VictoriaMetrics MetricsQL](https://docs.victoriametrics.com/metricsql/)
- [VictoriaLogs LogsQL](https://docs.victoriametrics.com/victorialogs/logsql/)
- [VictoriaLogs ingest](https://docs.victoriametrics.com/victorialogs/data-ingestion/)
- [VictoriaMetrics OpenTelemetry integration](https://docs.victoriametrics.com/opentelemetry/)
- [Storage strategy](../03-storage.md)
