# ADR-005: Arrow/DataFusion Query Layer

**Date:** 2026-04-15 (updated 2026-04-22)  
**Status:** Accepted (Implementation Deferred)  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-22  

## Context

The platform requires a high-performance, extensible query layer capable of federating queries across multiple storage engines (ClickHouse, object storage, TSDB). It must handle complex analytical queries with low latency and support custom operators for observability-specific logic.

## Decision

The **query substrate will be built using Apache Arrow as the in-memory format and Apache DataFusion as the query engine**. DataFusion, a Rust-native, extensible query engine, provides a strong foundation for building a unified, multi-signal query API.

### Implementation Status Update (2026-04-22)

As of P3-S7b, the project has introduced a `QueryPlanner` abstraction in `query-api` to encapsulate SQL generation. While DataFusion remains the target for Phase 4 (v1 production readiness) and federated query support, the current implementation continues to use direct ClickHouse SQL to minimize immediate complexity during Phase 3. The `QueryPlanner` provides the injection point for DataFusion when needed.

## Consequences

**Easier:** 
- Federated queries across multiple engines.
- Unified, columnar in-memory format (Arrow).
- Extensible via custom physical and logical plans.
- Built-in SQL support and query optimization.

**Harder:** 
- Requires significant custom implementation for observability-specific operators (e.g., trace waterfall reconstruction, percentile rollups).
- Integration with external storage engines (ClickHouse) needs a custom connector.

**Constrained:** 
- The query API's execution model is tied to DataFusion's capabilities and architecture.

## Alternatives Considered

### Option A: Custom-built Query Engine
Rejected due to high development effort and risk compared to leveraging the well-tested DataFusion core.

### Option B: DuckDB
Rejected because DataFusion offers better extensibility and is natively built in Rust, aligning with ADR-004.

## Related

- `spec/03-storage.md` (Query Engine and Compute)
- `spec/adr/ADR-004: Rust for Data Plane Services`
