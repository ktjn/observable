# Customer IT Cost and Lifecycle Governance

> **Audience:** Platform engineers, architects, procurement teams, and budget owners evaluating or operating Observable.
>
> **Purpose:** Provide a structured model for calculating total cost of ownership and planning the full operational lifecycle of an Observable deployment. Covers self-hosted and managed-cloud deployment modes.

---

## 1. Total Cost of Ownership (TCO)

TCO quantifies all costs incurred from initial evaluation through end-of-life. For Observable, the model has four cost categories.

```
TCO
├── 1.1 CAPEX            — one-time investment costs
├── 1.2 OPEX             — recurring operational costs
├── 1.3 Risk costs       — cost of incidents, downtime, and vendor lock-in
└── 1.4 End-of-life costs — decommission, migration, data export
```

### 1.1 CAPEX — Capital Expenditure

One-time or infrequent investment costs incurred before and during deployment.

| Cost Item | Observable | SaaS alternatives (Datadog, New Relic, Dynatrace) |
|---|---|---|
| **Platform infrastructure setup** | Kubernetes cluster provisioning, storage volumes, networking. Typically absorbed into existing cloud spend if a k8s platform already exists. | None — managed by vendor. |
| **Instrumentation / migration** | Low. OpenTelemetry SDK adoption is the only instrumentation investment. If workloads are already OTel-instrumented, migration cost is near zero. | High. Proprietary agents require per-language SDK adoption. Switching vendors requires full re-instrumentation. |
| **Integration work** | Moderate. Wire up OTLP exporters, configure collectors, deploy the k8s operator (Phase 6). Existing Prometheus/Grafana stacks can forward without code changes once Prometheus remote_write support ships (ADR-017). | Low — agents auto-detect most frameworks. |
| **Training** | Platform engineering team must understand k8s operations, ClickHouse administration basics, and the Observable query and config APIs. | Minimal initial training; deep product knowledge has ongoing hidden cost. |
| **Evaluation / PoC** | `docker compose up -d` provides a full local stack in minutes. No vendor engagement required. | Vendor-managed trials are free but require a sales engagement. |

**Observable CAPEX advantage:** Because OpenTelemetry is the only instrumentation contract, the investment in SDK adoption is portable. Instrumentation done for Observable works with any OTel-compatible backend. The CAPEX is spent on standards, not on a vendor.

---

### 1.2 OPEX — Operational Expenditure

Recurring costs per billing period (monthly or annually).

#### Infrastructure OPEX (self-hosted)

| Component | Sizing guidance | Notes |
|---|---|---|
| **Kubernetes cluster** | 3–6 nodes for a small production deployment; scales horizontally with ingest volume | Use a managed k8s service (EKS, GKE, AKS) to reduce ops burden |
| **ClickHouse** | CPU-bound for queries; storage-bound for retention. Start with 3-node cluster (6 vCPU, 24 GB RAM each). | Hot tier data drives storage cost. Tune retention windows to control cost (see §2.3). |
| **Redpanda** | 3-node cluster; CPU and network-bound. Scale with ingest throughput. | Replace with a managed Kafka/Redpanda service to reduce ops burden. |
| **PostgreSQL** | Minimal resources for control-plane metadata. A single primary with replica is sufficient for most deployments. | Use a managed database service (RDS, Cloud SQL) for reliability without SRE overhead. |
| **Object storage** | Low cost per GB. Scales with cold-tier retention volume. | S3-compatible; at cloud provider marginal rates. |

**Predictability advantage:** Observable's infrastructure costs scale with data volume, not with user seats or host count. There are no per-host, per-user, or per-feature-line billing surprises. Cost modeling is straightforward: ingest volume × retention duration = storage, query volume × cluster size = compute.

#### Staffing OPEX

| Role | Time allocation | Notes |
|---|---|---|
| **Platform engineer / SRE** | 0.25–0.5 FTE for a team of 10–50 engineers | Manages upgrades, retention tuning, cardinality budgets, and capacity planning. Scales sublinearly. |
| **Security / compliance** | Periodic review | Audit log reviews, RBAC policy updates, key rotation. |
| **On-call operations** | Low-overhead if Kubernetes and managed cloud services handle infrastructure reliability | Platform SLOs and health dashboards surface issues proactively. |

#### Licensing OPEX

Observable is a build-not-buy platform. There is no per-seat or per-host license fee for the core platform. Third-party components used by Observable carry their own licenses:

| Component | License | Notes |
|---|---|---|
| ClickHouse | Apache-2.0 | Free to self-host |
| Redpanda Community | BSL 1.1 → Apache-2.0 (4 years) | Check current Redpanda license terms for commercial use |
| PostgreSQL | PostgreSQL License (permissive) | Free |
| OpenFGA | Apache-2.0 | Free |
| `@grafana/ui` | Apache-2.0 | Visualization components only; not Grafana itself |

**Licensing risk note:** Redpanda's BSL license converts to Apache-2.0 after four years. For regulated or risk-averse procurement, evaluate whether a Kafka-compatible managed service (Amazon MSK, Confluent Cloud) is preferable.

---

### 1.3 Risk Costs

Risk costs are the financial exposure from incidents, data loss, and strategic lock-in. They are often invisible in initial TCO models but can dwarf CAPEX and OPEX over a 3–5 year horizon.

#### Vendor lock-in risk

| Risk | Observable | SaaS alternatives |
|---|---|---|
| **Instrumentation lock-in** | Negligible. OTel SDK instrumentation is portable to any OTel-compatible backend. | High. Proprietary agents require full re-instrumentation to switch vendors. |
| **Query language lock-in** | Low. Arrow/DataFusion uses standard SQL-compatible semantics. No proprietary DSL. | High. Datadog Query Language (DQL), NRQL, and DynaQL are not transferable. |
| **Data portability** | High. Export APIs (CSV, JSON, OTLP) are planned (see `00-market-analysis.md §3`). | Low. Data is generally inaccessible outside the vendor's UI/API. |
| **Dashboard portability** | High. Every dashboard is a JSON artifact in git; CI/CD-reviewable and version-controlled. | Low. Dashboard definitions are proprietary formats stored in vendor systems. |

#### Operational risk

| Risk | Mitigation in Observable |
|---|---|
| **Platform outage** | Kubernetes rolling deployments, canary gates, and automated rollback (`scripts/canary-promote.sh`). Platform self-telemetry surfaces health proactively. |
| **Data loss** | Durable Redpanda queue before storage writes. At-least-once delivery with idempotent write design. Retention policies prevent silent data expiry. |
| **Cardinality explosion** | Per-tenant cardinality budgets enforced at ingest. Budget exhaustion emits a warning before any data is dropped. |
| **Auth boundary failure** | Tenant isolation enforced at query layer, storage layer, and API layer. Cross-tenant tests are part of the CI gate. |
| **Ingest overload** | Rate limiting (per-tenant token bucket) and backpressure propagation to agents. Graceful degradation order: debug logs first, error spans last. |

#### Security and compliance risk

| Risk | Observable posture |
|---|---|
| **Data residency** | Self-hosted deployment gives full control over data location. Regional residency controls are planned (Phase 7). |
| **PII exposure** | Dual-layer PII scrubbing: agent-side (rules delivered via OpAMP remote config) and server-side pipeline. |
| **Audit trail** | Credential validation, query reads, and (planned) config changes emit immutable audit records. |
| **Supply chain** | Signed artifacts, SBOM generation, and provenance attestations on every release. |

---

### 1.4 End-of-Life Costs

Costs incurred when decommissioning an Observable deployment or migrating to a different platform.

| Cost Item | Observable | Notes |
|---|---|---|
| **Data export** | Export APIs planned for CSV, JSON, and OTLP formats. Cold-tier data in object storage is directly accessible (standard S3 format). | Requires implementing the planned export API endpoints (see `00-market-analysis.md §3`). |
| **Instrumentation migration** | Zero. OTel SDK instrumentation works unchanged with any OTel-compatible successor platform. | This is the single most significant EOL cost reduction compared to proprietary-agent platforms. |
| **Dashboard migration** | Dashboard JSON artifacts are serializable and version-controlled. An export-to-Grafana adapter is achievable. | Depends on target platform format. |
| **Knowledge transfer** | Platform team retains transferable knowledge: Kubernetes operations, ClickHouse SQL, Redpanda Kafka APIs, OTel semantics. These are all standard skills. | No vendor-specific certification cliff. |
| **Infrastructure decommission** | Standard cloud resource teardown. No vendor-specific decommission process. | |

---

## 2. Lifecycle Management (LCM)

LCM defines how an Observable deployment is managed across its full operational life — from procurement through decommission.

```
LCM
├── 2.1 Acquisition planning
├── 2.2 Deployment and integration
├── 2.3 Operations and maintenance
├── 2.4 Upgrade strategy
└── 2.5 Decommission
```

### 2.1 Acquisition Planning

**Evaluation checklist for Observable:**
- [ ] Kubernetes cluster available (EKS, GKE, AKS, or self-managed)
- [ ] Object storage available (S3 or S3-compatible)
- [ ] Workloads instrumented with OTel SDKs, or an OTel Collector can be added as a local hop
- [ ] Platform engineering team can commit 0.25–0.5 FTE to operate the platform
- [ ] Security team has reviewed the [tenancy and security spec](04-tenancy-security.md)
- [ ] Data residency requirements are understood (self-hosted deployment gives full control)

**Total cost comparison inputs (3-year horizon, 500 engineers, 200 services):**

| Vendor model | Estimated 3-year cost driver |
|---|---|
| Datadog | ~$240–480k/yr at production scale (per-host + per-product-line pricing) |
| New Relic | ~$120–300k/yr (data ingest + user seats) |
| Dynatrace | ~$180–400k/yr (DDU consumption, typically opaque until in production) |
| Observable (self-hosted) | Infrastructure cost (est. $30–80k/yr cloud spend) + 0.25–0.5 FTE SRE time |

> **Note:** Vendor pricing varies significantly by negotiation, scale, and contract terms. These ranges are directional, not contractual. Run a full PoC with realistic ingest volumes before committing.

**Procurement cycle fit:**
- Observable requires no vendor contract. Procurement is a cloud infrastructure purchase.
- Renewal risk is zero — no annual subscription renewal cliff.
- Budget predictability is high — costs scale with ingest volume and infrastructure, not with team headcount.

---

### 2.2 Deployment and Integration

**Deployment path:**

```
1. Provision Kubernetes cluster and object storage
2. Deploy Observable via Helm chart:
     helm install observable charts/observable -f values.yaml
3. Run database migrations (automatic via Helm pre-install Job hook)
4. Verify with smoke test:
     docker compose up smoke-test --abort-on-container-exit
5. Configure OTel Collectors or agents to point at the ingest gateway
6. Validate first signal in the UI
```

**Integration with existing infrastructure:**
- **Prometheus-based metrics:** Prometheus remote_write receiver (ADR-017; planned, not yet built)
- **Existing Grafana dashboards:** Observable supports Grafana as an optional visualization layer via datasource plugins (ADR-016). Existing dashboards can continue to work during migration.
- **CI/CD and deployment events:** Deploy events are ingested via the config API (Phase 3 P3-S5).
- **Identity providers:** SSO/OIDC integration is planned (Phase 4 P4-S3). API key authentication is available from day one.

**Migration from an existing observability platform:**
1. Deploy Observable alongside the existing platform (parallel run, not cutover)
2. Configure OTel Collectors to dual-export to both platforms
3. Validate Observable coverage matches existing platform for critical services
4. Shift alert rule definitions to Observable
5. Decommission the previous platform after one retention window (14 days for hot tier)

---

### 2.3 Operations and Maintenance

**Retention tuning:**

Retention windows directly control OPEX. Tune them to match actual investigation behavior:

| Tier | Default | Adjust when... |
|---|---|---|
| Hot (full fidelity) | 14 days | Reduce to 7 days if storage cost is a concern; increase to 14 days if post-incident reviews regularly exceed one week |
| Warm (indexed + partial rollups) | 30–60 days | Extend for compliance or SLO trend analysis |
| Cold (compressed, object-backed) | 2–12 months | Extend for regulated retention requirements |
| Archive | Compliance-defined | Configured per tenant or project |

**Cardinality budget management:**
- Monitor `METRIC_SERIES_BUDGET_PER_TENANT` alerts (emitted as structured warnings by the ingest gateway)
- Increase the budget or identify and remove high-cardinality label sources before the budget is exhausted
- Use the planned Cardinality Inspector UI ([spec/05 §9.5](05-frontend.md)) to identify top offenders

**Capacity planning signals:**
- Redpanda consumer lag → ingest pipeline is falling behind; scale stream-processor replicas
- ClickHouse query latency P95 > 1s → either query complexity, data volume, or cluster under-provisioned
- Storage writer batch size → indicates ingest throughput; baseline and alert on sudden change

---

### 2.4 Upgrade Strategy

**Observable follows a Helm-based rolling upgrade model with canary gates:**

```
1. New image published with immutable tag (commit SHA + version)
2. Canary deployment: 10% of traffic routed to new version
3. Automated analysis: query latency P95, ingest error rate, alert latency
4. If SLOs hold for the canary window: promote to 100%
5. If SLO regression detected: automatic rollback to previous revision
      helm rollback observable <previous-revision>
```

**Schema migration policy ([ADR-013](adr/ADR-013-schema-governance.md)):**
- Migrations are **forward-only**. There is no rollback for applied schema changes.
- The expand–migrate–contract pattern is required for any migration that would break the previous service version.
- Migration compatibility with the N−1 service version is a release gate.

**Agent upgrade cadence:**
- Agents support `stable`, `preview`, and `lts` channels
- `stable` agents remain compatible with the ingest gateway for at least N−1 minor versions
- The platform accepts telemetry from agents up to two major versions behind

**Customer upgrade recommendation:**
- Track the `stable` channel for production; use `preview` in staging to get early signal on breaking changes
- Do not run more than one major version behind; security patches are not backported beyond N−1
- Test upgrades in a staging environment that mirrors production retention and ingest volume

---

### 2.5 Decommission

**Graceful decommission steps:**

1. **Export telemetry data** — Use the export API (planned) to extract any data needed for compliance or historical reference. Cold-tier data in object storage is directly accessible in standard formats.
2. **Export dashboard and alert definitions** — All dashboards and alert rules are JSON artifacts in git. They are already exported by design.
3. **Redirect instrumentation** — Update OTel Collector export targets to point at the successor platform. No SDK changes are required.
4. **Stop ingest** — Scale ingest gateway replicas to zero. Allow the stream-processor to drain the Redpanda queue.
5. **Decommission storage** — Drop ClickHouse tables and delete object storage buckets after confirming all required data has been exported.
6. **Remove Helm release** — `helm uninstall observable`; delete Kubernetes namespaces and PVCs.

**Decommission timeline:** A typical decommission can be completed within one sprint (two weeks) for a team that has prepared the data export and successor configuration in advance.
