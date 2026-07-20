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

## 23. Roadmap Authority

[`ROADMAP.md`](../ROADMAP.md) is the sole authority for release sequence, outcomes, dependencies,
exit evidence, and non-goals. This document records durable product risks and the stable-core scope;
it does not maintain a parallel execution order.

Roadmap capabilities are delivered as tiny vertical slices under [the development
process](10-process.md). Existing UI and API surfaces count as shipped capability, but do not count
as operational maturity until the roadmap's release evidence is satisfied.

---

## 24. Stable Self-Hosted Scope

### 24.1 Shipped Evaluation Baseline

`0.1.0` provides the initial ingest, storage, query, visualization, alerting, tenancy, Compose, and
Helm surfaces for evaluation and small non-critical deployments. Published-artifact verification and
operational maturity remain separate evidence requirements; see the roadmap's `0.1` baseline.

### 24.2 Path to 1.0

The stable self-hosted contract is built in dependency order:

1. dependable Docker Compose evaluation from published artifacts;
2. one operator-ready Kubernetes topology;
3. enforceable governance for shared team adoption;
4. a complete service-reliability workflow; and
5. verified compatibility, security, performance, recovery, and support boundaries for `1.0.0`.

Every stage must close its documented evidence gates before the dependent stability claim is made.
Kubernetes remains the production target under [ADR-010](adr/ADR-010-deployment-model.md); Compose is
the easiest evaluation path, not the production topology.

### 24.3 Not Required for 1.0

The stable core does not require browser RUM, mobile observability, session replay, continuous
profiling, synthetics, advanced AI, billing, regional residency, tenant-isolated packaging, or
multi-region active-active operation. These remain unordered post-1.0 themes unless the authoritative
roadmap is explicitly revised.

---

## 25. Final Recommendation

Recommended technology shape:

| Concern             | Choice                                                        |
| ------------------- | ------------------------------------------------------------- |
| External contract   | OpenTelemetry                                                 |
| Data plane language | Rust                                                          |
| Query substrate     | Arrow + DataFusion                                            |
| Logs/traces store   | ClickHouse                                                    |
| Frontend            | React 19 + TypeScript + Vite 8 + TanStack Query               |
| Authorization       | RBAC + OpenFGA-style fine-grained model                       |
| Runtime             | Kubernetes                                                    |
| Delivery            | GitOps + progressive rollout                                  |
| Process             | trunk-based, ADR-driven, test-heavy, telemetry-contract aware |

This is a credible path to a production-ready observability platform that stays aligned with current ecosystem direction without copying Dynatrace/New Relic internals blindly.
