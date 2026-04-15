# ADR-012: Retention and Tiering

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

Observability data has a high temporal value decay. Recent data is queried frequently for real-time alerting and troubleshooting, while older data is mostly used for trend analysis or compliance. Storing all data on expensive "hot" storage is not cost-effective.

## Decision

The platform will implement a **multi-tiered retention strategy**:
1.  **Hot Tier (3–14 days):** Stored in ClickHouse on NVMe/SSD for full-fidelity, low-latency querying.
2.  **Warm Tier (15–60 days):** Stored in ClickHouse using disk-based storage or object-storage-backed tables with partial rollups.
3.  **Cold Tier (2–12 months):** Compressed archives stored in S3/Object Storage.
4.  **Archive Tier (Long-term):** Exported as parquet/raw files for compliance, requiring re-ingestion for querying.

## Consequences

**Easier:** 
- Significant reduction in storage costs.
- Better performance for real-time queries (hot data is smaller and faster).
- Complies with data retention policies.

**Harder:** 
- Increased complexity in managing data movement between tiers.
- Queries across tiers may have varying performance and consistency.

**Constrained:** 
- The query layer must handle cross-tier federation and potentially slower "cold" data access.

## Alternatives Considered

### Option A: Single Tier (SSD for everything)
Rejected as economically unsustainable for large-scale data volumes over long periods.

### Option B: Automatic Cloud Tiering (e.g., S3 Intelligent Tiering)
Rejected because it doesn't provide enough control over query semantics and application-level rollups.

## Related

- `spec/03-storage.md` (Retention Tiers)
- `spec/10-process.md` (ADR list)
- `spec/13-risks-roadmap.md` (Risk 7: No cost model)
