# ADR-013: Schema Governance

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

Observability platforms must handle diverse and evolving telemetry schemas while maintaining consistency for querying and alerting. Rigid schemas stifle development agility, while completely schema-less designs lead to data quality issues and query performance degradation.

## Decision

The platform will adopt a **multi-tiered schema governance strategy**:
1.  **Standard Contracts:** Core telemetry fields (e.g., `tenant_id`, `service_name`, `trace_id`) are governed by Protobuf/OTel definitions and strictly enforced at ingest.
2.  **Schema-on-Write (High Velocity):** Frequently used attributes and resources are automatically discovered and indexed during ingestion.
3.  **Schema-on-Read (Low Velocity):** Rare or highly dynamic fields are stored in a flexible format (e.g., ClickHouse JSON/Map) and parsed at query time.
4.  **Schema Registry:** A centralized registry will track and version schemas for telemetry types and custom instrumentation.

## Consequences

**Easier:** 
- Consistency for critical dimensions across all signals.
- Automatic discovery of new telemetry attributes.
- High performance for common query patterns through optimized indexing.

**Harder:** 
- More complex ingest logic to handle schema discovery and enforcement.
- Schema evolution (breaking changes) requires careful management and versioning.

**Constrained:** 
- Core telemetry dimensions must always follow the standard contract.

## Alternatives Considered

### Option A: Strict Schema-Only
Rejected because it prevents the flexible and rapid instrumentation common in modern observability.

### Option B: Schema-less/JSON-Only
Rejected due to poor query performance and lack of data quality guarantees for critical platform features.

## Related

- `spec/10-process.md` (Engineering Standards: API-first)
- `spec/03-storage.md` (Logical Data Model)
- `spec/14-domain-model.md` (Authoritative domain model and metric/log/span contracts)
- `spec/adr/ADR-001: OpenTelemetry as External Contract`
