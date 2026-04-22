# Gap Analysis and Strategic Recommendations
Date: 2026-04-19
Last reviewed: 2026-04-22
Status: Updated Audit Report

## 1. Overview

This document compares the current specifications in `spec/` with the implementation in `services/`, `apps/`, `migrations/`, and `charts/`. It supersedes the initial 2026-04-19 snapshot where later Phase 2 and Phase 3 work has closed or narrowed several gaps.

Current implementation baseline:
- Phase 2 is complete in `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`.
- Query paths are tenant-scoped and perform fail-closed tenant validation for traces, logs, and metrics.
- ClickHouse spans, logs, and metric points are partitioned or ordered by `tenant_id`.
- Query-read and credential-validation audit logs exist.
- Rate limiting, cardinality budget observation, hot trace retention, threshold alert evaluation, Helm render/rollback, canary promotion, and perf smoke baselines exist.
- Frontend Phase 3 work has started: app shell, theme persistence, service catalog summaries, service detail overview, and service-scoped Logs/Metrics/Traces tabs are present.

## 2. Architectural Gaps

### 2.1 Arrow/DataFusion Query Substrate

- **Requirement**: `spec/03-storage.md` and `spec/13-risks-roadmap.md` identify Arrow as the in-memory format and DataFusion as the strategic query substrate for multi-source planning, columnar execution, custom operators, schema-aware query behavior, correlation, and SLO burn-rate windows.
- **Current State**: `services/query-api` still uses direct ClickHouse and PostgreSQL clients. There is no `datafusion` or `arrow` dependency and no federated query planning layer. The current implementation is acceptable for Phase 1 through active Phase 3 slices but remains below the strategic storage/query architecture.
- **Impact**: Cross-source features will keep accumulating ad hoc SQL and application-side joins unless a substrate slice is introduced before broad topology, dashboard, SLO, and NL-query work depends on richer query semantics.
- **Recommendation**: Do not rewrite query-api immediately. Add a bounded Phase 3 substrate spike that wraps one read path behind an internal planner interface and proves Arrow/DataFusion value on a specific feature, such as field facets or topology rollups.

### 2.2 Warm/Object Storage Tier

- **Requirement**: `spec/03-storage.md` requires object storage for warm/cold telemetry, profile blobs, replay payloads, exports, and backups.
- **Current State**: Hot ClickHouse storage exists with TTLs. `storage-writer` also has an explicit hot trace retention worker. No MinIO/S3-compatible service exists in `docker-compose.yml`, no object-store client is present, and no warm-tier movement path is implemented.
- **Impact**: Phase 4 warm retention, backup/restore, profile blobs, and long-retention economics are blocked.
- **Recommendation**: Keep object storage out of the Phase 3 critical path. Add it as the first Phase 4 production-readiness slice: local MinIO, a minimal object layout, one export/movement path, and query semantics for that one dataset.

### 2.3 Multi-Tenancy Enforcement

- **Requirement**: `spec/04-tenancy-security.md` requires `tenant_id` isolation at every layer.
- **Current State**: This gap is mostly closed for the implemented signal paths. Query middleware requires tenant context; trace, log, metric-series, and metric-point reads bind `tenant_id` and validate returned rows fail-closed. Stream processing stamps the envelope tenant into signal rows. ClickHouse spans/logs/metric_points partition by `tenant_id`; metric_series orders by `tenant_id`.
- **Remaining Risk**: ClickHouse does not provide database-native row-level security in this implementation. Isolation relies on query construction, partitioning, and validation. New endpoints can regress unless tenant tests remain mandatory.
- **Recommendation**: Track this as a continuing test/review obligation, not a missing Phase 2 feature. Every new query, ingest, deployment marker, dashboard, SLO, and infrastructure endpoint must include same-tenant and cross-tenant tests.

## 3. Feature Gaps

### 3.1 Field Faceting and Explorer Statistics

- **Requirement**: `spec/05-frontend.md` and `spec/09-api.md` require Log and Trace explorer facet statistics for common fields.
- **Current State**: The active plan already identifies this as the next slice, P3-S7. Current query responses do not include a `facets` object, and the frontend does not render facet sidebars.
- **Recommendation**: Make P3-S7 the immediate next implementation slice. Start with bounded Top N facets for service name, status code, and severity/log level; defer arbitrary high-cardinality analytics.

### 3.2 Service Topology and Infrastructure Correlation

- **Requirement**: `spec/05-frontend.md` and `spec/09-api.md` require service topology from trace-derived relationships and infrastructure inventory/correlation from OTel resource attributes.
- **Current State**: Service catalog and service detail summaries exist. Service-scoped signal tabs exist. There is no topology endpoint, graph UI, infrastructure inventory, or resource-attribute correlation panel.
- **Recommendation**: Keep the current plan sequence: P3-S8 topology, P3-S9 infrastructure inventory, and P3-S10 infrastructure correlation. Add tenant tests and performance checks to each API slice.

### 3.3 Deployment Markers

- **Requirement**: `spec/18-deployment-markers.md` defines deployment marker lifecycle APIs, retention, authorization, and timeline overlays.
- **Current State**: Span rows carry `deployment_id`, canary promotion tooling exists, and service summaries expose `latest_deployment` as a placeholder. There is no deployment marker table, ingest/update API, query API, or timeline overlay.
- **Recommendation**: Keep P3-S11, but split it into small sub-slices: data model/API, ingestion or CI helper, query/list path, and one timeline overlay.

### 3.4 SLO Burn-Rate Workflow

- **Requirement**: `spec/07-alerting-slo.md` requires SLO entities, error-budget semantics, and multi-window burn-rate alerting.
- **Current State**: `alert-evaluator` implements one threshold alert path. The schema anticipates `slo_burn_rate`, but burn-rate evaluation and SLO definition workflows are not implemented.
- **Recommendation**: Keep full SLO burn-rate work in Phase 4 (P4-S5). Add a prerequisite to P3 dashboard/schema work only if the frontend needs to display SLO placeholders before the burn-rate engine exists.

### 3.5 Profiling Telemetry

- **Requirement**: `spec/03-storage.md`, `spec/06-agents.md`, and `spec/13-risks-roadmap.md` identify profiling as an advanced signal and differentiator.
- **Current State**: No profile ingest route, stream payload, ClickHouse index table, object-storage blob writer, or profiling explorer exists.
- **Recommendation**: Keep profiling in Phase 6. It should depend on Phase 4 object storage because profile blobs are specified to live in object storage.

### 3.6 Browser RUM

- **Requirement**: `spec/03-storage.md` defines `session_id`; `spec/06-agents.md` identifies a Browser SDK for RUM traces, Web Vitals, session context, and JS errors.
- **Current State**: Generic trace/log/metric ingest exists, but there is no browser SDK, RUM event schema, session model, web-vitals endpoint, or RUM UI.
- **Recommendation**: Keep RUM in Phase 6. The first slice should use standard OTel browser-compatible trace/log payloads plus explicit session attributes before introducing a custom RUM endpoint.

### 3.7 eBPF Instrumentation

- **Requirement**: `spec/06-agents.md` describes an eBPF sensor as an optional privileged DaemonSet with strict security constraints.
- **Current State**: The platform relies on OTel collectors, manual/SDK instrumentation, and the Collectable legacy-ingest tool. No eBPF sensor or integration exists.
- **Recommendation**: Keep eBPF as an optional Phase 6 slice. Require a written security review before any privileged DaemonSet is added.

## 4. Implementation Gaps

### 4.1 Frontend Maturity

- **Previous Gap**: Frontend was described as a skeleton.
- **Current State**: This is no longer accurate. The frontend has a navigation shell, theme persistence, service catalog/summary integration, service detail overview, trace detail, log context/live-tail components, and service-scoped signal tabs.
- **Remaining Gap**: The frontend still lacks facets, topology map, infrastructure views, deployment timeline overlay, dashboard promotion/import-export, Alerting & SLO management, and profiling/RUM experiences.
- **Recommendation**: Continue Phase 3 in the current order. Do not jump to advanced signals before completing the service/infrastructure/correlation foundation.

### 4.2 Alerting Engine Capabilities

- **Previous Gap**: Alert evaluator was minimal.
- **Current State**: One threshold alert evaluation path exists and writes alert firings. This satisfies the Phase 2 foundation but not the SLO product.
- **Remaining Gap**: Pending-to-active debounce, notifications, SLO definitions, burn-rate windows, incident timelines, and routing are missing.
- **Recommendation**: Keep threshold alerting as the reusable base. Add SLO burn-rate in P4-S5, then notification/routing/incident workflows in Phase 5.

### 4.3 Schema Registry and Semantic Annotations

- **Requirement**: `spec/03-storage.md` defines a Schema Registry with semantic annotations, and the current plan added P3-S14 as the prerequisite for the Phase 8 NL query layer.
- **Current State**: No Schema Registry storage, API, annotation model, or frontend surface exists.
- **Recommendation**: Keep P3-S14 after dashboard shape stabilization. It should store semantic annotations separately from structural field metadata so annotations can evolve without rewriting schema history.

## 5. Plan Updates Required

The current plan should be updated with the following closure steps:

1. Keep P3-S7 as the immediate next slice and make it explicitly close the field-faceting gap.
2. Add a Phase 3 bounded query-substrate spike after facets and before broad topology/dashboard work, unless P3-S7 proves direct SQL is sufficient for the next three slices.
3. Expand P3-S11 into deployment-marker sub-slices: storage/API, CI helper or ingest integration, query/list endpoint, and one timeline overlay.
4. Add object storage and warm-retention prerequisites to P4-S1 and P4-S2.
5. Make P4-S5 explicitly reuse the threshold evaluator foundation for SLO burn-rate alerts.
6. Make P6 profiling depend on object storage from Phase 4.
7. Make P6 eBPF explicitly require a security review before implementation.

## 6. ADR/Spec Sync

No ADR or spec change is required for this audit update. The implementation gaps remain within already specified architecture and roadmap scope. The plan document is updated in the same iteration to reflect closure steps.
