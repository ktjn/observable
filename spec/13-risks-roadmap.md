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

Produce these documents first:

1. product requirements document
2. system context and container diagrams
3. domain model
4. tenancy and auth model
5. ingestion and storage ADRs
6. query language/API spec
7. frontend architecture spec
8. deployment architecture spec
9. security architecture
10. test strategy
11. SLOs for the platform itself
12. phased roadmap with staffing assumptions

---

## 24. Suggested First Release Scope

**Include in v1:**
- OTLP ingest
- logs + traces + basic metrics
- React UI
- service catalog
- dashboards
- threshold + burn-rate alerts
- RBAC
- SSO
- hot/warm retention
- audit logs
- k8s deployment
- canary releases
- internal dogfooding

**Do not block v1 on:**
- session replay
- mobile SDK
- full profiling
- deep AI features
- billing perfection
- multi-region active-active

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
