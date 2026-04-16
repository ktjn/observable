# ADR-003: ClickHouse Adoption Boundary

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

ClickHouse is a high-performance, columnar OLAP database that is highly effective for logs and traces. However, it is not a general-purpose relational database and has specific characteristics (e.g., eventual consistency, limited support for large joins, primary key design) that must be managed.

## Decision

ClickHouse is the **primary engine for all high-volume telemetry data: logs, traces, and metrics**.

- It will NOT be used for transactional metadata, user accounts, or fine-grained configuration (those belong in PostgreSQL).
- We adopt a "ClickHouse-first" approach for data plane services and maintain a strict service boundary so the storage engine can be swapped or augmented if necessary.
- Metrics use the `MetricSeries` + `MetricPoint` table design defined in `spec/14-domain-model.md`. Rollups are implemented as ClickHouse materialized views.

**Revisit condition for metrics:** If Phase 2 or Phase 3 cardinality testing shows ClickHouse cannot meet the P50 < 1 s query latency target under production-representative label cardinality, open a new ADR to evaluate a dedicated TSDB (e.g., VictoriaMetrics). The query facade already abstracts storage engines from clients.

## Consequences

**Easier:** 
- Extremely fast query performance on large datasets.
- Efficient storage through columnar compression.
- Familiar toolset for observability engineering.

**Harder:** 
- Schema migrations and primary key changes are complex.
- Requires careful handling of "deletes" (TTL or lightweight deletes).
- Must manage "small insert" problems using buffering (Redpanda/Kafka).

**Constrained:** 
- The query layer must be designed to work within ClickHouse's execution model (e.g., avoiding massive table-to-table joins).

## Alternatives Considered

### Option A: Elasticsearch/OpenSearch
Rejected due to higher resource consumption (heap management, indexing overhead) and lower compression ratios compared to ClickHouse for typical observability workloads.

### Option B: Managed Cloud Logs (e.g., CloudWatch, Stackdriver)
Rejected to maintain platform portability and avoid high egress/ingest costs.

## Related

- `spec/03-storage.md` (Storage Strategy)
- `spec/14-domain-model.md` (MetricSeries and MetricPoint schemas)
- `spec/adr/ADR-002-polyglot-storage.md` (Polyglot Storage vs Single Engine)
