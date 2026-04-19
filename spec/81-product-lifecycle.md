# Product Cost Structure and Lifecycle Strategy

> **Audience:** Observable product team, engineering leadership, commercial planners, and support operations.
>
> **Purpose:** Define the cost structure for building and delivering Observable as a product, and establish the product lifecycle management (PLM) framework that governs roadmap versioning, support windows, end-of-life decisions, and upgrade incentive design.
>
> **Relationship to customer-facing document:** This document and `docs/91-customer-tco.md` are intentionally separate. The customer document describes what the customer expects to pay and operate. This document describes what Observable costs to build and deliver, and how Observable will manage its own product lifecycle. Keeping them separate prevents the product team from unconsciously biasing customer-facing cost models with manufacturer incentives — a common source of lifecycle mismatches and customer dissatisfaction.

---

## 1. Product Cost Structure

The cost structure captures what it takes to build, run, and support Observable as a product.

```
Product Cost Structure
├── 1.1 Product CAPEX         — engineering investment to build the platform
├── 1.2 Delivery OPEX         — cost to run the platform for customers
├── 1.3 Support and service costs
└── 1.4 Warranty and incident costs
```

### 1.1 Product CAPEX — Engineering Investment

The up-front engineering cost of building Observable. This is the cost the product team amortizes over the product lifecycle.

| Investment Area | Description | Primary Spec Reference |
|---|---|---|
| **Core data plane services** | Ingest gateway, stream processor, storage writer, query API, alert evaluator — all in Rust | [ADR-004](../spec/adr/ADR-004-rust-data-plane.md), [spec/02](../spec/02-architecture.md) |
| **Storage schema and migrations** | ClickHouse and PostgreSQL schema design, migration tooling, retention policy implementation | [ADR-002](../spec/adr/ADR-002-polyglot-storage.md), [ADR-013](../spec/adr/ADR-013-schema-governance.md) |
| **Query layer** | Arrow/DataFusion federated query execution, custom operators (trace waterfall, service graph rollup, histogram, SLO burn-rate) | [ADR-005](../spec/adr/ADR-005-arrow-datafusion.md), [spec/03 §6](../spec/03-storage.md) |
| **Frontend** | React 19 + Vite + TanStack Query/Router; entity-centric navigation; cross-signal correlation UX | [ADR-006](../spec/adr/ADR-006-react-vite-frontend.md), [spec/05](../spec/05-frontend.md) |
| **Auth and authorization** | OIDC integration, OpenFGA ReBAC model, API key management | [ADR-008](../spec/adr/ADR-008-authorization-model.md), [spec/04](../spec/04-tenancy-security.md) |
| **Agent ecosystem** | OTel Collector distribution, k8s operator, language auto-instrumentation, eBPF sensor, browser/mobile SDKs, OpAMP remote config | [spec/06](../spec/06-agents.md) |
| **Deployment and release tooling** | Helm charts, GitOps delivery, canary promotion scripts, kind integration test harness | [ADR-020](../spec/adr/ADR-020-helm-chart-strategy.md), [spec/12](../spec/12-deployment.md) |
| **CI/CD pipeline** | GitHub Actions; all non-trivial logic in `scripts/` and runnable locally | [ADR-019](../spec/adr/ADR-019-ci-scripts-runnable-locally.md) |

**Investment sequencing discipline:** Observable's phased roadmap ([spec/10-process.md §17](../spec/10-process.md)) requires that ingest, storage, query, and auth foundations are production-stable before advanced features (AI, profiling, session replay) receive investment. This prevents the common failure mode of investing in differentiating features before the commodity foundation is reliable.

**Amortization model:** Product CAPEX is amortized across customers and phases. Each phase delivers a working vertical slice; no phase requires the next phase to be delivered before customers receive value.

---

### 1.2 Delivery OPEX — Cost to Run for Customers

The recurring cost to deliver Observable as a hosted service. For self-hosted customers, these costs are transferred to the customer (see `docs/91-customer-tco.md §1.2`).

#### Per-tenant infrastructure cost model

For a managed/hosted Observable offering, cost per tenant is primarily a function of ingest volume and retention duration.

| Component | Cost driver | Scaling characteristic |
|---|---|---|
| **Redpanda** | Ingest throughput (MB/s), partition count | Scales horizontally; cost grows with peak ingest rate, not average |
| **ClickHouse** (hot tier) | Data volume × retention days | Columnar compression gives 5–10× reduction vs raw; cold data moves to object storage |
| **PostgreSQL** | Control-plane metadata (tenant count, API keys, audit logs) | Scales with tenant count; not with telemetry volume |
| **Object storage** (warm/cold) | Volume × retention duration | Cheapest tier; cost is linear with retained data |
| **Kubernetes compute** | CPU/memory for service replicas | Scales with query concurrency and ingest throughput; idle cost is low |

**Cost floor per tenant:** The minimum viable hosted tenant (low ingest volume, 14-day hot retention) requires approximately 3–6 vCPU and 12–24 GB RAM shared across all services. Below this floor, multi-tenancy overhead does not amortize well; consider minimum viable ingest tiers in pricing.

**High-ingest cost structure:** At production scale (thousands of spans/sec, millions of log lines/day), ClickHouse storage dominates. The cardinality budget system (implemented in Phase 2) is the primary cost-control lever. Products that allow unbounded cardinality at the ingest layer will find their storage costs non-linear.

#### Delivery cost reduction levers

| Lever | Mechanism |
|---|---|
| **Retention tiering** | Move data from hot (ClickHouse) to cold (object storage) aggressively. Object storage is 10–50× cheaper per GB. |
| **Tail sampling** | Reduce trace volume at the ingest gateway before storage. ADR-011 mandates tail-based sampling. |
| **Cardinality budgets** | Hard caps prevent individual tenants from causing non-linear storage cost growth. |
| **Columnar compression** | ClickHouse achieves high compression ratios on telemetry data; optimize column ordering and codec selection for each table. |
| **Shared infrastructure** | Multi-tenant ClickHouse clusters amortize fixed overhead (node provisioning, cluster management) across multiple tenants. |

---

### 1.3 Support and Service Costs

Support costs are the cost of engineering and operations time spent resolving customer issues, maintaining integrations, and ensuring platform reliability.

#### Support tier model

| Tier | Target | SLA | Cost structure |
|---|---|---|---|
| **Community** | Open-source self-hosted users | Best-effort | GitHub Issues; no SLA commitment |
| **Standard** | Commercial customers — business hours | P1: 4h response | Included in base subscription |
| **Enterprise** | Enterprise customers — 24/7 | P1: 1h response | Premium support add-on |
| **Dedicated** | Large tenants or regulated deployments | P0: 15m response | Dedicated CSE allocation |

#### Support cost drivers

- **Documentation quality:** Well-maintained runbooks ([spec/12-deployment.md §19.7](../spec/12-deployment.md)) and operator guides reduce support volume significantly. Every operational incident class should have a documented triage path.
- **Platform self-telemetry:** Observable emits health metrics, traces, and logs from every service. A well-maintained internal health dashboard reduces MTTD (mean time to detect) before customers notice issues.
- **Agent compatibility surface:** Supporting N−2 agent versions multiplies the support matrix. The compatibility policy (agents: N−1 minor versions, platform: N−2 major) bounds this surface.
- **Migration support:** Customers migrating from Datadog, New Relic, or Dynatrace will require migration support. Documented migration paths for common tool combinations reduce this cost.

#### Security and patch OPEX

Observable publishes security patches on the following cadence:

| Severity | Patch SLA | Channel |
|---|---|---|
| Critical (CVSS ≥ 9.0) | 24–48 hours | All supported versions |
| High (CVSS 7.0–8.9) | 7 days | Current stable + N−1 |
| Medium/Low | Next regular release | Current stable only |

Dependency audits run nightly in CI (`cargo audit`, `npm audit`, container image scanning). See [ADR-019](../spec/adr/ADR-019-ci-scripts-runnable-locally.md) and [spec/11-testing.md](../spec/11-testing.md).

---

### 1.4 Warranty and Incident Costs

The cost of outages, data loss events, and SLA breaches. These are avoided costs that inform architectural decisions.

| Incident class | Primary mitigation | Spec reference |
|---|---|---|
| **Ingest data loss** | Durable Redpanda queue; at-least-once delivery; idempotent writes | [ADR-009](../spec/adr/ADR-009-queue-stream-backbone.md) |
| **Query outage** | Stateless query API; multiple replicas; graceful degradation | [spec/02 §3.2](../spec/02-architecture.md) |
| **Auth boundary failure** | Tenant isolation tests in CI; fail-closed design on cross-tenant results | [ADR-007](../spec/adr/ADR-007-multi-tenant-isolation.md) |
| **Schema migration failure** | Expand–migrate–contract pattern; forward-only migrations; N−1 compatibility gate | [ADR-013](../spec/adr/ADR-013-schema-governance.md) |
| **Cardinality cost explosion** | Per-tenant cardinality budgets enforced at ingest | [spec/03 §5.4](../spec/03-storage.md) |
| **Canary regression** | Automated SLO-based rollback during canary promotion | [spec/12 §19.3](../spec/12-deployment.md), `scripts/canary-promote.sh` |

Platform SLOs (ingest latency, query latency P95, alert delivery latency) are defined in [spec/11-testing.md §18](../spec/11-testing.md). Breaching these SLOs in a customer environment triggers warranty obligations for hosted deployments.

---

## 2. Product Lifecycle Management (PLM)

PLM defines how Observable as a product is versioned, supported, evolved, and retired.

```
PLM
├── 2.1 Roadmap and versioning
├── 2.2 Support windows
├── 2.3 End-of-life and deprecation strategy
├── 2.4 Upgrade incentive design
└── 2.5 Revenue model alignment
```

### 2.1 Roadmap and Versioning

**Phased roadmap:**

Observable's roadmap is structured into eight phases ([spec/10-process.md §17](../spec/10-process.md)). The phased model is deliberate — it enforces investment in reliability, tenancy, and cost controls before advanced features. This prevents the common PLM failure of building differentiated features on an unstable foundation.

| Phase | Milestone | PLM implication |
|---|---|---|
| Phase 1 | Internal MVP | Foundation is runnable; no external support commitment |
| Phase 2 | Governed MVP | Tenant isolation, cost controls, RBAC — minimum bar for internal production use |
| Phase 3 | Correlation and service operations | First externally demonstrable differentiation |
| Phase 4 | v1 production readiness | First external customer support commitment |
| Phase 5 | Reliability product | Incident and notification workflows complete |
| Phase 6 | Advanced telemetry | Profiling, RUM, eBPF, synthetics |
| Phase 7 | Enterprise readiness | Regional residency, BYOK, compliance reporting |
| Phase 8 | Intelligence | AI/ML features (advisory only, per ADR-014) |

**Semantic versioning:**
- `MAJOR` — breaking API or schema change requiring customer action
- `MINOR` — backward-compatible new capability
- `PATCH` — bug fixes and security patches; no behavior change

**API versioning policy:**
- Public APIs are versioned under `/v1/`, `/v2/`, etc.
- A new major API version may be introduced at any minor product version.
- The previous major API version must remain supported for a minimum of **two minor product release cycles** after the new version ships.
- Deprecation is announced in the CHANGELOG and via a `Deprecation` response header on deprecated endpoints.

**Schema versioning policy ([ADR-013](../spec/adr/ADR-013-schema-governance.md)):**
- All schema changes are versioned SQL migration files under `migrations/`.
- No ORM-generated schema changes are permitted.
- Migrations are forward-only; the expand–migrate–contract pattern is required for breaking changes.
- A migration that would break the N−1 service version is a **release blocker**.

---

### 2.2 Support Windows

Support windows define how long a given product version receives security patches, bug fixes, and compatibility guarantees.

#### Platform support channels

| Channel | Description | Intended environment |
|---|---|---|
| **stable** | Current release. Receives all patches. | Production |
| **preview** | Next release candidate. Receives security patches only. | Staging, early adopters |
| **lts** | Long-term support release. Receives critical and high security patches for an extended window. | Regulated environments, slow-moving enterprises |

**LTS cadence:** Designate one `minor` release per year as an LTS release. LTS releases receive security patches for 24 months after designation.

**Non-LTS support window:** Non-LTS releases receive patches until the next minor release ships, plus a 30-day migration window.

#### Agent support windows

Agents follow the same `stable`/`preview`/`lts` channel model ([spec/06 §10.8](../spec/06-agents.md)):
- The ingest gateway accepts telemetry from agents up to **two major versions** behind the current release.
- Agents at N−3 major version or older receive no security patches and are not guaranteed to be compatible with the ingest gateway.
- The k8s operator manages automated agent upgrades for DaemonSet-managed agents.

#### Dependency support windows

| Dependency | Observable support posture |
|---|---|
| ClickHouse | Track the latest stable release series. Observable migrates off EOL ClickHouse versions within 90 days of EOL announcement. |
| Redpanda | Track latest stable. Protocol compatibility means migration is low-risk. |
| PostgreSQL | PostgreSQL's 5-year support cycle aligns well with LTS product releases. |
| Rust toolchain | Minimum supported Rust version (MSRV) is `stable` at the time of the release branch cut. |
| Node.js (frontend build) | Track Node.js LTS releases. |

---

### 2.3 End-of-Life and Deprecation Strategy

**PLM principle:** Never leave customers stranded. Every EOL decision must ship with a documented migration path before the deprecation period begins.

#### Feature deprecation lifecycle

```
1. Feature marked deprecated in CHANGELOG and docs (no removal yet)
2. Deprecated response header or warning log emitted on usage
3. Minimum two minor release cycles (or 6 months, whichever is longer) before removal
4. Migration guide published alongside deprecation announcement
5. Feature removed; clients using the removed API receive a 410 Gone response
```

#### API deprecation triggers

An API version is deprecated when:
- A successor version with equivalent or superior capability has been stable for at least two minor release cycles, OR
- A security vulnerability in the old version cannot be patched without breaking compatibility, OR
- The usage is below 1% of API traffic for 60 consecutive days (data-driven)

#### Schema EOL strategy

ClickHouse tables follow a different EOL path from API versions:
- **Schema changes** require a migration file; the old schema is never removed in the same release
- **Table removal** (rare) requires: 90-day announcement, data export tooling shipped before removal, and confirmation that no supported service version writes to the table
- **Column deprecation** is handled via the expand–migrate–contract pattern; old columns become nullable before removal

#### Version EOL communication

| Channel | Content | Timing |
|---|---|---|
| CHANGELOG | EOL date, removed APIs, migration steps | Each release |
| Deprecation header | `Deprecation: <date>` on API responses | From deprecation announcement |
| Platform UI | Banner for operators running a version within 30 days of EOL | 30 days before EOL |
| Support ticket | Direct notification for Enterprise and Dedicated tier customers | 60 days before EOL |

---

### 2.4 Upgrade Incentive Design

The product team must design upgrade incentives that serve both Observable's commercial interests and the customer's lifecycle needs. Misaligned incentives are a primary source of customer dissatisfaction and support cost.

#### Incentive alignment principles

| Customer need | Observable incentive | Potential conflict | Resolution |
|---|---|---|---|
| Long, stable lifecycle | Support windows of 12–24 months (LTS) | Observable wants customers on the latest version for support efficiency | Offer security backports to LTS; do not require feature adoption to stay supported |
| Low upgrade OPEX | Zero-downtime rolling upgrades via Helm + canary | Observable benefits from customers upgrading frequently | Invest in upgrade automation; make upgrades boring, not events |
| Predictable OPEX | Stable pricing per ingest tier | Observable may want to introduce new metering | Grandfather existing tenants on volume pricing for 12 months post-change; announce pricing changes 90 days in advance |
| No forced upgrades | N−1 compatibility window for schemas and APIs | Observable wants to retire old API versions | Enforce minimum two minor release cycle deprecation windows; never remove an API in the same release it is deprecated |

#### Upgrade acceleration mechanisms

Mechanisms that encourage voluntary, timely upgrades without forcing them:

1. **Security bulletin transparency** — Publish CVE advisories on the Observable security page. Customers self-motivate on critical patches.
2. **Canary and rollback tooling** — Make the upgrade less risky. If `helm upgrade && helm rollback` is well-tested and documented, operators upgrade more willingly.
3. **Feature availability gates** — New Phase 3–8 features are only available on current stable. Customers on older versions can see announced features but are prompted to upgrade to access them.
4. **Health dashboard version indicator** — The platform health dashboard displays the current version and a "new version available" notice when a new stable release is published.

---

### 2.5 Revenue Model Alignment

**Primary revenue models for Observable:**

| Model | Description | TCO fit for customer |
|---|---|---|
| **Self-hosted / open core** | Core platform is open source; commercial support, enterprise features (BYOK, compliance reporting, dedicated CSE), and managed cloud offering are paid | Lowest OPEX for customers who can self-host; aligns Observable revenue with customer success, not data volume |
| **Managed cloud (SaaS)** | Observable-hosted deployment; customer pays for ingest volume and retention | Predictable pricing per GB or per million spans/logs; no per-seat or per-host fees — aligns with customer TCO model |
| **Support contracts** | Annual support contract for self-hosted deployments; tiered by SLA | Revenue proportional to customer scale; incentivizes Observable to invest in platform stability |
| **Professional services** | Migration assistance, custom integrations, architecture review | One-time revenue; reduces customer CAPEX for migration; avoid dependency — goal is customer self-sufficiency |

**Pricing anti-patterns to avoid:**

| Anti-pattern | Why it fails | Observable commitment |
|---|---|---|
| Per-host or per-seat pricing | Costs scale with team growth, not platform value. Creates perverse incentive to under-instrument. | Price on data volume and retention, not headcount. |
| Per-signal-type pricing (logs vs traces vs metrics) | Forces customers to choose which signals to collect. Undermines correlation value proposition. | Unified data model; single ingest tier covering all signal types. |
| Cardinality overage charges | Creates fear of instrumentation. Engineers avoid adding useful labels. | Cardinality budgets give customers control. Overages are configurable limits, not surprise bills. |
| Retroactive pricing changes on existing data | Erodes trust; forces expensive data migration | Grandfather data at the pricing in effect at ingest time. |

**Revenue model and PLM alignment:**

The product lifecycle strategy and revenue model must align to avoid conflicts that damage customer relationships:

- **Support windows** must be long enough that customers on annual support contracts are not forced to upgrade mid-contract to stay supported. LTS releases with 24-month windows match typical annual procurement cycles.
- **Feature gating** (features only on new versions) is acceptable for net-new features, but must not remove functionality from customers on a supported version.
- **Price increases** must be announced 90 days in advance and must not apply retroactively to data already ingested.
- **EOL decisions** must be driven by usage data and security requirements, not by artificial obsolescence designed to drive upgrade revenue.

---

## 3. How the Two Documents Interact

The customer-facing TCO document (`docs/91-customer-tco.md`) and this product lifecycle document define complementary constraints on product development decisions.

```
Customer document                    This document
(what the customer expects)          (what Observable commits to)
        │                                    │
        ▼                                    ▼
  Long lifecycle →                 24-month LTS support windows
  Low OPEX →                       Ingest-volume pricing, not per-seat
  Predictable upgrades →           Canary + rollback automation
  No vendor lock-in →              OTel-only contract, export APIs
  Stable support →                 N-1 API compatibility, deprecation windows
  Data residency control →         Self-hosted first, regional residency (Phase 7)
```

**Product development governance:** When evaluating a new feature, pricing change, or deprecation, cross-reference both documents:
1. Does this change create a new CAPEX or OPEX burden for the customer not disclosed in `docs/91-customer-tco.md`?
2. Does this change require updating a support window, compatibility promise, or deprecation timeline in this document?
3. If yes to either: update both documents in the same PR as the code change.

This review step prevents the most common lifecycle mismatch — a product decision that is internally rational but creates an invisible cost or risk for the customer.
