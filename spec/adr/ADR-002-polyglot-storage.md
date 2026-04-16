# ADR-002: Polyglot Storage vs Single Engine

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

Different telemetry signals (logs, traces, metrics, profiles) have distinct access patterns, cardinality characteristics, and compression requirements. While a single-engine approach (e.g., everything in a traditional RDBMS) simplifies operations, it often fails to scale efficiently or provide the necessary query performance for high-volume observability data.

## Decision

The platform will adopt a **polyglot storage strategy**. Instead of a single engine, we will use specialized storage engines optimized for each signal type:
- **Logs and Traces:** ClickHouse (Columnar OLAP).
- **Metrics:** ClickHouse (Phase 1 and beyond; revisit if cardinality testing shows P50 > 1 s — see `ADR-003-clickhouse-boundary.md` for the revisit condition).
- **Profiles:** Modular storage (initially object storage + specialized indexing).
- **Metadata/Configuration:** Relational database (e.g., PostgreSQL).
- **Long-term/Cold Storage:** Object storage (S3-compatible).

## Consequences

**Easier:** 
- Optimized performance and compression for each signal type.
- Ability to scale storage components independently.
- Better cost management through tiering.

**Harder:** 
- Increased operational complexity (managing multiple database systems).
- Cross-signal queries require a federated query layer (e.g., DataFusion).

**Constrained:** 
- Data consistency across different engines must be managed at the application/query layer.

## Alternatives Considered

### Option A: Single Database (e.g., PostgreSQL or ClickHouse only)
Rejected because no single engine excels at all telemetry workloads (especially high-cardinality metrics vs. large-volume logs).

### Option B: Everything in Object Storage (Data Lake approach)
Rejected due to high query latency and complexity in providing real-time alerting/dashboards.

## Related

- `spec/03-storage.md` (Storage Strategy)
- `spec/13-risks-roadmap.md` (Risk 1: Single-engine fantasy)
