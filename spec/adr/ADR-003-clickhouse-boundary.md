# ADR-003: ClickHouse Adoption Boundary

**Date:** 2026-04-15  
**Status:** Proposed  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

ClickHouse is a high-performance, columnar OLAP database that is highly effective for logs and traces. However, it is not a general-purpose relational database and has specific characteristics (e.g., eventual consistency, limited support for large joins, primary key design) that must be managed.

## Decision

ClickHouse will be the **primary engine for all high-volume telemetry data (logs, traces, and initially metrics)**. 
- It will NOT be used for transactional metadata, user accounts, or fine-grained configuration (which belong in PostgreSQL).
- We will adopt a "ClickHouse-first" approach for data plane services but maintain a strict service boundary so that the storage engine can be swapped or augmented if necessary.

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
- `spec/adr/ADR-002: Polyglot Storage vs Single Engine`
