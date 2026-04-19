# Gap Analysis and Strategic Recommendations
Date: 2026-04-19
Status: Audit Report

## 1. Overview
This document lists the discrepancies between the platform specifications (under `spec/`) and the current implementation (under `services/` and `apps/`). It also suggests architectural improvements and missing features to align with the Phase 3/4 roadmap.

## 2. Architectural Gaps

### 2.1 Arrow/DataFusion Query Substrate
- **Requirement**: `spec/03-storage.md` and `ADR-005` mandate an Arrow-native query layer using Apache DataFusion for federated query planning and custom operators.
- **Current State**: `services/query-api` uses direct ClickHouse and PostgreSQL clients with standard SQL. There is no DataFusion integration or federated query logic.
- **Recommendation**: Integrate `datafusion` crate into `query-api`. Implement a query planner that can join data across ClickHouse (telemetry) and Postgres (metadata/tenants).

### 2.2 S3-Compatible Object Storage
- **Requirement**: `spec/03-storage.md` mentions Object Storage for warm/cold data retention and archiving.
- **Current State**: No object storage service (e.g., MinIO) is present in `docker-compose.yml`. No storage writer implementation for S3/Blob storage exists.
- **Recommendation**: Add MinIO to the local development stack. Implement an archiver service or extend `storage-writer` to offload older ClickHouse parts to S3.

### 2.3 Comprehensive Multi-Tenancy Enforcement
- **Requirement**: `spec/04-tenancy-security.md` requires `tenant_id` isolation at every layer.
- **Current State**: Gateways enforce tenant headers, but ClickHouse schemas and some query paths rely on application-level filtering rather than Row-Level Security (RLS) or strictly isolated tables.
- **Recommendation**: Audit all ClickHouse tables in `migrations/clickhouse/` to ensure `tenant_id` is a primary key or partition key where appropriate.

## 3. Feature Gaps

### 3.1 Profiling Telemetry
- **Requirement**: Strategic roadmap (`spec/13-risks-roadmap.md`) identifies Profiling as a key differentiator.
- **Current State**: No ingest routes, stream processing, or storage schema for profiles exist.
- **Recommendation**: Define a profile storage schema (likely using ClickHouse `Map` or specialized columnar formats) and add OTLP profiling support to `ingest-gateway`.

### 3.2 Real User Monitoring (RUM)
- **Requirement**: `spec/05-frontend.md` describes a RUM correlation UX.
- **Current State**: `ingest-gateway` handles backend traces but lacks specialized handling for RUM events (session tracking, web vitals).
- **Recommendation**: Add a RUM-specific ingest endpoint and session-state management in the `stream-processor`.

### 3.3 eBPF Instrumentation
- **Requirement**: `spec/06-agents.md` lists eBPF as the preferred method for zero-touch instrumentation.
- **Current State**: The platform currently depends entirely on external OTel collectors or manual SDK instrumentation.
- **Recommendation**: Begin prototyping a small eBPF-based collector (or use `beyla`/`kepler` integration) to provide out-of-the-box visibility for k8s workloads.

## 4. Implementation Gaps

### 4.1 Frontend Maturity (Phase 1/2)
- **Gap**: The `apps/frontend` implementation is currently a skeleton compared to the rich UX described in `spec/05-frontend.md`.
- **Missing**: Trace waterfall visualizations, alerting configuration UI, and advanced correlation navigation.
- **Recommendation**: Prioritize the Trace Waterfall component and the Alerting management UI to fulfill Phase 1 Internal MVP goals.

### 4.2 Alerting Engine Capabilities
- **Gap**: Current `alert-evaluator` is minimal and may not support complex "burn-rate" alerts described in `spec/07-alerting-slo.md`.
- **Recommendation**: Implement SLO-based alerting logic and integrate with the (missing) DataFusion layer for more complex thresholding.
