# Development Process

## 15. Architecture Decision Records

Maintain ADRs from day 1.

### Required ADR Set

1. OTel as external contract
2. polyglot storage vs single engine
3. ClickHouse adoption boundary
4. Rust for data plane services
5. Arrow/DataFusion query layer
6. React/Vite frontend
7. multi-tenant isolation strategy
8. authorization model
9. queue/stream backbone
10. deployment model: k8s-first or hybrid
11. sampling strategy
12. retention and tiering
13. schema governance
14. AI feature boundaries
15. build vs buy decisions for incidenting/auth/billing

---

## 16. Development Process Specification

### 16.1 Repo Strategy

Monorepo preferred when:
- shared protobuf/schema packages
- shared UI packages
- shared infra modules
- many internal APIs evolving together

Polyrepo acceptable if org scale requires it.

### 16.2 Branching

- trunk-based
- short-lived feature branches
- feature flags for incomplete work
- protected main
- release branches only when necessary

### 16.3 Definition of Ready

- requirement with acceptance criteria
- domain owner assigned
- telemetry impact identified
- data retention impact identified
- auth impact identified
- runbook/docs impact identified

### 16.4 Definition of Done

- tests pass
- telemetry emitted
- dashboards/alerts updated if relevant
- security review complete if boundary changed
- load test impact assessed if hot path changed
- ADR updated if architecture changed
- migration path documented
- rollback path documented

### 16.5 Engineering Standards

- API-first
- protobuf or OpenAPI contracts required
- backward compatibility policy enforced
- no breaking query semantics without versioning
- every service exports health, metrics, traces, and logs
- every persistent schema change uses migrations

---

## 17. Project Plan: Small Steps to Production

### Phase 0 — Foundations

1. Define product scope and success metrics.
2. Write system context diagram.
3. Define tenancy model.
4. Define telemetry domain model.
5. Create ADR template.
6. Create repo/release conventions.
7. Define security baseline.
8. Define environments.

### Phase 1 — Minimal Viable Platform

1. OTLP ingest gateway.
2. Tenant auth + API keys/workload identity.
3. Durable queue.
4. Basic trace/log storage.
5. Basic metric storage.
6. Query APIs.
7. Simple UI: trace explorer, logs, metrics.
8. Basic dashboards.
9. Basic threshold alerts.
10. Internal dogfooding.

### Phase 2 — Correlation

1. Trace-log correlation.
2. Service catalog.
3. Service map.
4. Deployment events.
5. Derived RED metrics.
6. K8s metadata enrichment.
7. Search and filtering improvements.

### Phase 3 — Production Hardening

1. Rate limits.
2. Quotas.
3. Retention tiers.
4. Backup/restore.
5. Audit logs.
6. RBAC/ReBAC.
7. SSO/SCIM.
8. Disaster recovery.
9. Cost controls.
10. Multi-region strategy.

### Phase 4 — Reliability Product

1. SLOs.
2. Burn-rate alerts.
3. Incidents.
4. On-call integrations.
5. Runbook workflows.
6. Topology-aware impact.

### Phase 5 — Advanced Telemetry

1. Profiling.
2. RUM.
3. Mobile observability.
4. Synthetics.
5. eBPF enrichment.
6. Session replay (if desired).

### Phase 6 — Enterprise Readiness

1. Regional residency.
2. BYOK.
3. Tenant-isolated deployments.
4. Compliance reporting.
5. Billing/metering.
6. Marketplace/private deployment packaging.

### Phase 7 — Intelligence

1. Anomaly models.
2. Query recommendations.
3. Incident summarization.
4. Capacity forecasting.
5. Remediation hooks with approval controls.
