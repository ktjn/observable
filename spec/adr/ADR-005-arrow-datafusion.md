# ADR-005: ClickHouse-first query execution; DataFusion conditional

**Original decision:** 2026-04-15  
**Revised:** 2026-11-07  
**Status:** Accepted — DataFusion is optional, not a 2.0 prerequisite

## Context

Observable currently generates and executes scoped ClickHouse SQL through a Rust query facade. Adding a second general-purpose execution engine before demonstrated cross-source needs increases query-plan complexity, latency risk, maintenance and duplicated optimization work. Business capabilities and experience contracts do not require DataFusion.

## Decision

- Keep the Rust query API, semantic query/planner abstraction and storage-independent public contracts.
- Use ClickHouse as the default execution and aggregation engine for logs, traces, metrics and business evidence in 2.0.
- Push predicate, time range, tenant scope, aggregation and limits to the storage engine wherever supported.
- Apache Arrow remains an **optional** interchange/in-memory representation when it provides measured value; it is not mandatory for every query path.
- Introduce Apache DataFusion only through a separate design/ADR supported by workload evidence: unavoidable cross-source federation, reusable custom operators or local columnar computation that a single engine cannot efficiently perform.
- Keep the source of the authorization decision outside the generated SQL. Mandatory tenant/purpose enforcement and query-budget validation must hold regardless of optimizer or storage adapter.
- Do not introduce DataFusion just to implement MetricsQL, LogsQL, dashboards or trace-log correlation.

## Adoption test

Prove the need with realistic traces, logs, metric series and business evidence. Measure P50/P95 latency, memory, query correctness, tenant isolation, planner complexity and operational cost against direct query execution. Include negative authorization and failure cases. The implementation must preserve the existing capability contracts and UX scenarios.

## Consequences

The 2.0 execution model stays simple. Federation may later use DataFusion, but is not a committed milestone. A future metric/log store adapter may use a native query language without a system-wide DataFusion dependency.

## Related

- [Storage engine evaluation](ADR-045-victoria-storage-evaluation.md)
- [Storage specification](../03-storage.md)
- [Business verification plan](../../docs/plans/phase0-assurance-hardening.md)
