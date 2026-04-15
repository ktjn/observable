# Risks and Roadmap

## 22. Risks

1. **Single-engine fantasy** — One database for all signals usually becomes a constraint.
2. **Cardinality collapse** — Without budgets and controls, costs explode.
3. **Weak tenant isolation** — Fatal for enterprise adoption.
4. **Agent sprawl** — Too many collectors/agents create support debt.
5. **Custom query DSL too early** — Start with minimal semantics and evolve.
6. **AI-first roadmap** — Wrong order. Reliability first.
7. **No cost model** — Observability products fail economically before technically.

---

## 23. Initial Deliverables

Produce and review these before implementation begins:

1. product requirements document with target users, non-goals, and success metrics
2. system context and container diagrams
3. canonical domain model with OTel attribute mapping
4. tenancy, authn, authz, and data-isolation model
5. ingestion, storage, queue, retention, and schema-governance ADRs
6. query API spec, including initial trace/log/metric semantics and versioning policy
7. frontend architecture spec and first user workflows
8. deployment architecture, environment topology, and release strategy
9. security architecture, supply-chain baseline, and audit requirements
10. test strategy with synthetic, replay, malformed, and high-cardinality datasets
11. platform SLOs and non-functional acceptance targets
12. phased roadmap with staffing, ownership, and phase gates

---

## 24. Suggested First Release Scope

### 24.1 Internal MVP Scope

Internal MVP is for dogfooding and validating the core architecture. It is not the externally supported v1.

- OTLP ingest for traces and logs
- basic metrics ingest
- tenant auth with API keys and workload identity
- durable queue before transform/storage
- ClickHouse-backed traces/logs and initial metrics storage
- trace, log, metric, and configuration query APIs
- React UI for trace search, log search, metric exploration, and one dashboard workflow
- internal platform telemetry, health dashboards, and synthetic test telemetry
- hot retention policy prototype
- basic threshold alerts
- internal dogfooding

### 24.2 External v1 Scope

Include in externally supported v1:

- OTLP ingest
- logs + traces + basic metrics
- React UI
- service catalog
- dashboards
- threshold + burn-rate alerts
- RBAC
- SSO/OIDC for target customer environments
- hot/warm retention
- audit logs
- tenant-aware rate limits, quotas, and cardinality diagnostics
- trace-log correlation and RED metrics
- k8s deployment
- canary releases
- rollback and restore runbooks
- load, chaos, tenant-isolation, and upgrade/rollback test evidence

Do not block v1 on:

- session replay
- mobile SDK
- full profiling
- deep AI features
- billing perfection
- multi-region active-active
- regional residency controls unless required by the first target customer
- tenant-isolated deployment packaging unless required by the first target customer

### 24.3 Near-Term Execution Order

1. Ratify Phase 0 specs and ADRs.
2. Build the ingest-to-query slice for traces/logs.
3. Add tenant auth, durable buffering, and ClickHouse writes.
4. Add query APIs and the minimal UI.
5. Add basic metrics.
6. Add platform telemetry and dogfood dashboards.
7. Add tenant isolation tests, rate limits, quotas, cardinality budgets, retention, and audit logs.
8. Add RBAC and basic threshold alerts.
9. Add Kubernetes/GitOps/canary delivery.
10. Add correlation, service catalog, RED metrics, SLOs, burn-rate alerts, and warm retention for v1.

---

## 25. Final Recommendation

Recommended technology shape:

| Concern | Choice |
|---------|--------|
| External contract | OpenTelemetry |
| Data plane language | Rust |
| Query substrate | Arrow + DataFusion |
| Logs/traces store | ClickHouse |
| Frontend | React 19 + TypeScript + Vite 8 + TanStack Query |
| Authorization | RBAC + OpenFGA-style fine-grained model |
| Runtime | Kubernetes |
| Delivery | GitOps + progressive rollout |
| Process | trunk-based, ADR-driven, test-heavy, telemetry-contract aware |

This is a credible path to a production-ready observability platform that stays aligned with current ecosystem direction without copying Dynatrace/New Relic internals blindly.
