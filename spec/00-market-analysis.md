# Market Analysis and Product Positioning

> **Living document.** The competitive landscape section should be reviewed and updated whenever a competitor ships a significant capability change or when a new phase plan is created. Tier 2 and Tier 3 gap entries should be resolved (either by adding a spec section or by explicitly marking the gap out-of-scope) before the affected phase begins.
>
> **Last reviewed:** 2026-04-19

---

## 0. Purpose

This document provides the product and competitive context that explains *why* Observable is built the way it is. It complements the technical specs by answering:

1. Who are the incumbents and where do they fall short?
2. What is Observable's differentiation thesis, and which ADRs encode it?
3. What capabilities are **in the spec but lack a delivery plan** (Tier 2 gaps)?
4. What capabilities are **not in the spec at all** but competitors offer (Tier 3 gaps)?

Current backlog status and named follow-on slices are tracked in the
[`unified feature roadmap`](../docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md) and are
not duplicated here.

---

## 1. Market Landscape

The enterprise observability market is dominated by a handful of closed-platform SaaS vendors, an open-source ecosystem that is powerful but operationally heavy, and a growing middle tier of specialist tools with narrow depth. Observable targets the gap between these groups.

### 1.1 Dynatrace

**Strengths:** Deepest automation (Davis AI); full-stack topology through OneAgent; zero-code instrumentation; Grail lakehouse for unified storage; strong enterprise compliance posture.

**Weaknesses:**
- **Proprietary everything.** OneAgent, Smartscape topology, DQL query language. Moving away from Dynatrace requires full re-instrumentation.
- **Price opacity.** Consumption-based DDU (Dynatrace Data Unit) pricing is notoriously unpredictable and expensive at scale.
- **Opaque AI.** Davis fires remediation decisions without operator understanding. Black-box automation is not acceptable in regulated industries.
- **Slow OTel adoption.** OTel support exists but is secondary; native integrations are the recommended path.
- **All-or-nothing agent model.** Hybrid OTel + OneAgent environments are awkward and unsupported.

### 1.2 New Relic

**Strengths:** Strong entity synthesis model (NerdGraph); generous free tier; unified telemetry data platform (NRDB); NRQL is developer-friendly; broad language SDK coverage.

**Weaknesses:**
- **Unpredictable pricing.** Data ingest + user-seat billing is hard to forecast; large teams become very expensive.
- **Proprietary NRDB.** Columnar store behind a closed API. No open-standards exit path.
- **UI complexity debt.** Years of feature accumulation without architectural restructure show in the navigation.
- **Cardinality surprises.** High-cardinality metric workloads can cause unexpected cost explosions with no preventive controls.
- **OTel as second-class citizen.** Native agents remain the recommended deep-integration path.

### 1.3 Datadog

**Strengths:** Best-in-class APM UX; 800+ integrations; strong CI/CD, security, and database monitoring product lines; best-in-class dashboarding; solid Kubernetes-native experience.

**Weaknesses:**
- **Most expensive at scale.** Per-host and per-product-line pricing. At production scale, Datadog frequently becomes the second-largest infrastructure cost after compute.
- **SaaS-only.** No self-hosted deployment option for regulated or data-residency-constrained customers.
- **Proprietary agents.** DDTrace is the recommended agent; OTel support is available but not full-depth.
- **No open data model.** All data sits behind Datadog's API with no export path.
- **Custom metrics cost surprises.** Custom metrics are billed per series — a common onboarding shock.

### 1.4 Grafana / OSS Stack (Loki + Mimir + Tempo + Pyroscope)

**Strengths:** Best open-source observability story; strong community; Grafana dashboards are the industry-standard visualization format; multi-tenancy in Mimir and Loki; cost-effective at scale when self-hosted.

**Weaknesses:**
- **Operational burden.** Running Mimir, Loki, Tempo, and Pyroscope at production scale requires significant SRE investment. It is a platform team project, not an off-the-shelf product.
- **No unified control plane.** Each component is configured independently. Tenancy, auth, and RBAC must be stitched together by the operator.
- **Grafana AGPL licensing risk.** Grafana itself is AGPL-3.0; embedding it in a commercial product creates legal exposure. Observable uses only the Apache-2.0 `@grafana/ui` components (see [ADR-016](adr/ADR-016-grafana-visualization-strategy.md)).
- **No first-class correlation engine.** Cross-signal joins are user-defined Grafana panel queries, not platform-enforced joins.
- **No entity model.** Grafana has no first-class concept of a "service"; everything is time series.

### 1.5 Honeycomb

**Strengths:** Pioneer of high-cardinality event analytics; BubbleUp for root-cause attribution; strong developer experience; columnar event storage.

**Weaknesses:**
- **Events only.** No native metrics or infrastructure layer; traces-as-events makes high-volume ingest expensive.
- **No SLO engine, no profiling, no k8s topology.**
- **SaaS-only.** No self-hosted path.
- **Expensive at volume.** Pricing scales with event ingest; microservice-heavy workloads are very costly.

### 1.6 Elastic / ELK Stack

**Strengths:** Excellent full-text log search; broad ecosystem; self-hostable; mature Kibana dashboards.

**Weaknesses:**
- **Licensing risk.** Elasticsearch and Kibana moved to SSPL (not OSI-approved), causing a community fork (OpenSearch) and ongoing adoption uncertainty.
- **Operationally complex.** Managing shards, replicas, and index lifecycle at scale is a full-time job.
- **Not observability-native.** APM and metrics were retrofitted onto a search engine; trace and metric workloads are expensive (inverted index is not columnar-optimized).

### 1.7 Chronosphere

**Strengths:** Purpose-built for Prometheus cardinality control; strong Mimir/Grafana compatibility; cost reduction story for teams escaping Datadog.

**Weaknesses:**
- **Metrics only.** No logs, traces, or profiling.
- **SaaS-only.** Limited deployment flexibility.
- **Narrow positioning.** Bought primarily to solve the "Prometheus cardinality is costing us too much" problem.

---

## 2. Observable's Differentiation Thesis

Observable is built on the thesis that the next generation of observability buyers will demand three things that no current vendor satisfies simultaneously:

1. **Open-standards first** — no vendor lock-in at the data or agent layer
2. **Full-stack depth** — all signals (traces, logs, metrics, profiles, events, synthetics) unified with a real correlation engine, not loosely coupled tabs
3. **Operational control** — self-hostable, explainable AI, predictable cost model

### 2.1 Core Differentiators

| Differentiator | How Observable Achieves It | Closest Competitor Gap |
|---|---|---|
| **OTel as the only ingest contract** | OTLP is the sole ingest format; no proprietary agent required ([ADR-001](adr/ADR-001-otel-external-contract.md)) | Dynatrace, New Relic, Datadog all recommend proprietary primary agents |
| **Open query substrate** | Arrow/DataFusion — open-standards execution layer; data is readable by any Arrow-compatible tool ([ADR-005](adr/ADR-005-arrow-datafusion.md)) | All SaaS vendors have closed query APIs |
| **Rust data plane** | 5–10× lower memory footprint vs JVM-based ingest pipelines; predictable latency; no GC pauses ([ADR-004](adr/ADR-004-rust-data-plane.md)) | Datadog, New Relic, Dynatrace ingest services are largely JVM or Go |
| **True multi-tenancy by design** | `tenant_id` enforced at every layer — storage, query, API, audit — not bolted on ([ADR-007](adr/ADR-007-multi-tenant-isolation.md)) | Grafana stack requires manual stitching; most SaaS has single-tenant underpinning |
| **ReBAC authorization model** | OpenFGA (Zanzibar model) supports fine-grained resource sharing semantics ([ADR-008](adr/ADR-008-authorization-model.md)) | Competitors offer coarse RBAC only; no resource-level sharing |
| **Advisory-only AI with provenance** | All AI outputs are read-only suggestions; no auto-remediation without explicit approval gates ([ADR-014](adr/ADR-014-ai-feature-boundaries.md), [spec/08](08-ai-ml.md)) | Dynatrace Davis AI operates autonomously; creates compliance risk |
| **Dashboard-as-code natively** | Every dashboard is a serializable artifact from creation; no "export" step; CI/CD-reviewable ([spec/05 §9.7](05-frontend.md)) | Grafana requires external git sync tooling; Datadog/New Relic treat code dashboards as optional |
| **BubbleUp-style comparison without ML** | Attribute distribution comparison implemented as a group-by query, not a model ([spec/05 §9.5](05-frontend.md)) | Honeycomb's BubbleUp is a key USP; Observable provides equivalent without the events-only constraint |
| **Entity-centric navigation** | Service is the first-class entity; signals are views of a service ([spec/05 §9.2](05-frontend.md)) | Grafana has no entity model; ELK is log-centric; Chronosphere is metric-centric |
| **eBPF-assisted enrichment** | Kernel-level network/syscall telemetry with zero code changes; enriches traces with process and socket metadata ([spec/06 §10.10](06-agents.md)) | Dynatrace OneAgent and Datadog NPM do this but as proprietary closed agents |
| **PII scrubbing at the edge** | Agents apply signed scrubbing rules before transmission; dual-layer with server-side ([spec/06 §10.11](06-agents.md)) | Most vendors offer server-side PII masking only; edge scrubbing is rare |
| **OpAMP remote config for agents** | Config pushed to agents without restarts; versioned and signed; auto-rollback on failure ([spec/06 §10.6](06-agents.md)) | OpAMP is an emerging standard; most vendors use proprietary config delivery |
| **Profiling as a correlated signal** | Profiling, traces, logs, and metrics share the same identity dimensions; flame graphs join to spans ([spec/03 §5.2](03-storage.md)) | Grafana Pyroscope is standalone; Datadog Continuous Profiler is separate from APM |
| **Synthetic-to-trace correlation** | Synthetic check injects W3C `traceparent`; resulting trace is linked back to the check result ([spec/07 §13.3](07-alerting-slo.md)) | Most synthetic tools produce pass/fail metrics only; no trace linkage |
| **Self-hostable with real multi-tenancy** | Kubernetes-first + Helm chart + GitOps delivery; no SaaS required ([ADR-010](adr/ADR-010-deployment-model.md), [ADR-020](adr/ADR-020-helm-chart-strategy.md)) | Honeycomb and New Relic are SaaS-only; Grafana stack requires heavy operator knowledge |

### 2.2 Market Gaps Observable Can Own

**Gap 1: The OTel-native Datadog/New Relic replacement.**
Thousands of companies are actively migrating off Datadog and New Relic due to cost. They need an OTel-native, full-stack replacement that does not require re-instrumentation. Observable is positioned to be the destination of that migration.

**Gap 2: Regulated industries that cannot use US-only SaaS.**
Financial services, healthcare, and government customers need data residency controls, BYOK encryption, audit trails, and self-hosted deployment. Observable's architecture (multi-tenancy by design, regional isolation, Kubernetes-native) targets this directly. Current options are either too DIY (Grafana stack) or too proprietary (Dynatrace on-premises).

**Gap 3: "Observability as code" engineering culture.**
The generation of platform engineers that treats infrastructure as code also expects dashboards, alerts, and SLO definitions to live in git alongside application code and be reviewed in CI. Observable treats these as first-class serializable artifacts from day one.

**Gap 4: Explainable AI in observability.**
As AI proliferates, regulated industries and mature SRE teams push back on black-box automation. Observable's advisory-only AI policy with provenance requirements ([ADR-014](adr/ADR-014-ai-feature-boundaries.md)) is a trust differentiator for enterprises where "show me why you think this is the root cause" is a hard requirement.

**Gap 5: Predictable cost at scale.**
Every SaaS vendor has horror stories of unexpected cardinality bills. Observable's cardinality budgets enforced at ingest, combined with the tiered retention model, give operators deterministic cost control before data reaches storage.

**Gap 6: Operational business analytics without a data platform.**
Traditional BI data platforms deliver certified answers — but at high cost and slow cadence. In practice, most operational and tactical business questions are never answered because the cost to model and pipeline the data exceeds the decision value. Observable's telemetry store, combined with an LLM natural language query layer, delivers approximate answers in seconds at near-zero marginal cost. This is not a replacement for financial reconciliation or regulatory reporting, but for the majority of operational questions — *"which customers are affected by this degradation?", "did this deploy hurt conversion?", "which tenants are consuming the most resources?"* — an approximate answer at decision time has higher expected value than a precise answer after the window has closed. Cross-signal corroboration (traces + metrics + logs + events converging on the same conclusion) further raises confidence without requiring a curated data warehouse. See [ADR-021](adr/ADR-021-nl-query-layer.md) for the architectural decision.

---

## 3. Tier 2 Gaps — Specced but No Delivery Slice

These features are described in the spec or ADRs but have no named implementation slice in the phase plans. They will be missed unless explicitly scheduled. Each entry should result in either a new phase slice or a conscious decision to defer with a documented rationale.

| Feature | Where Specced | Gap Description |
|---|---|---|
| **Rate limiting for log and metric ingest** | [spec/02 §4.1](02-architecture.md), [spec/04](04-tenancy-security.md) | Only trace ingest rate-limiting is implemented (Phase 2 P2-S2a). Log and metric ingest handlers have no quota enforcement. |
| **Cardinality budgets for logs and traces** | [spec/03 §5.4](03-storage.md), [ADR-013](adr/ADR-013-schema-governance.md) | Only metric cardinality observation is implemented (P2-S3a). No equivalent for logs or high-cardinality trace attributes. |
| **Hot retention worker for logs and metrics** | [spec/03 §5.3](03-storage.md), [ADR-012](adr/ADR-012-retention-tiering.md) | Only trace hot retention is implemented (P2-S4a). Log and metric tables have no automated deletion worker. |
| **Audit logging for config, dashboard, and admin actions** | [spec/04 §8.2](04-tenancy-security.md), Phase 2 item 5 | Only credential validation and query reads are audited. Config changes, alert mutations, and admin actions are not. |
| **Prometheus remote_write receiver** | [ADR-017](adr/ADR-017-prometheus-remote-write.md) (Accepted) | The ADR is ratified but there is no Phase 1–3 implementation slice. The ingest gateway only accepts OTLP. |
| **Schema Registry service** | [spec/03 §5.4](03-storage.md) | Fully specced (gRPC/HTTP API, PostgreSQL-backed, attribute indexing, versioning). No implementation slice in any phase plan. |
| **OTel Collector distribution** | [spec/06 §10.1](06-agents.md) | Specced as the recommended local-hop component. No build plan or delivery slice. |
| **Kubernetes operator** | [spec/06 §10.9](06-agents.md) | DaemonSet management, mutating admission webhook for auto-instrumentation injection, CRD exposure. Fully specced. No build slice. |
| **OpAMP remote config for agents** | [spec/06 §10.6](06-agents.md) | Fully specced (push config, signed/versioned payloads, rollback on failure). No build slice. |
| **Fleet management UI** | [spec/06 §10.7](06-agents.md), [spec/05 §9.2](05-frontend.md) | Agent health dashboard, buffer state, version, last export time. Listed as Phase 4+ frontend module but no backend service or UI slice. |
| **Dashboard builder (drag-and-drop)** | [spec/05 §9.3](05-frontend.md) Phase 2–3 | Panel library, query editor, variable management UI. P3-S8 covers dashboard-as-code API only, not the builder UI. |
| **Trace comparison view** | [spec/05 §9.3](05-frontend.md) Phase 2–3 | Side-by-side comparison of two traces. No frontend or query slice. |
| **BubbleUp-style comparison UI** | [spec/05 §9.5](05-frontend.md) | Attribute distribution comparison over anomalous vs baseline window. Specced as a query operation. No UI slice. |
| **Export APIs** (CSV, JSON, OTLP) | [spec/05 §9.11](05-frontend.md) | Spec states all data visible in the UI must be exportable. No API or UI slice in any phase plan. |
| **Onboarding / setup wizard** | [spec/05 §9.3](05-frontend.md) Phase 1 | Agent install wizard, API key generation, first-signal validation. Listed as a Phase 1 frontend module; never implemented. |
| **SLO management UI** | [spec/05 §9.3](05-frontend.md) Phase 2–3 | Create SLOs, view burn rate, error budget history. Phase 4 adds SLO backend (P4-S5) but no UI slice is planned alongside it. |
| **Alert routing UI** | [spec/05 §9.3](05-frontend.md) Phase 2–3 | Escalation policies, notification channel management. No UI slice. |
| **Saved views** | [spec/05 §9.11](05-frontend.md) | Named bookmarks for search configurations (filter set + time range + column selection). No slice. |
| **Deadman alerts** | [spec/07 §11.1](07-alerting-slo.md) | Alert when no data is received from a source within a window. No implementation slice. |
| **Change detection alerts** | [spec/07 §11.1](07-alerting-slo.md) | Alert on relative change vs a baseline window. No implementation slice. |
| **Topology-aware alert impact** | [spec/07 §11.1](07-alerting-slo.md) | Alert with blast-radius estimation from the service map. No implementation slice. |
| **Deployment regression alerts** | [spec/07 §11.1](07-alerting-slo.md) | Alert triggered by a deployment correlated with an error rate increase. No slice. |
| **Alert inhibition rules** | [spec/07 §11.4](07-alerting-slo.md) | Higher-severity alert suppresses lower-severity alerts for the same service. No slice. |
| **On-call rotation integration** | [spec/07 §11.4](07-alerting-slo.md), [ADR-015](adr/ADR-015-build-vs-buy.md) | ADR-015 says buy (PagerDuty/Opsgenie). No integration slice exists for either. |
| **User journey SLOs** | [spec/07 §12.1](07-alerting-slo.md) | SLO scoped to a synthetic check sequence across multiple services. No slice. |
| **Audit log export to SIEM** | [spec/04 §8.2](04-tenancy-security.md) | Audit records exist in PostgreSQL but no streaming or export path to external SIEMs (Splunk, Sentinel, etc.). |
| **SCIM provisioning** | [spec/04 §8.1](04-tenancy-security.md) | Listed as a required identity feature. No phase slice. |
| **Multi-step HTTP synthetic checks** | [spec/07 §13.1](07-alerting-slo.md) | Sequential HTTP request chains for user journey probing. No implementation slice beyond basic HTTP check. |

---

## 4. Tier 3 Gaps — Not in the Spec

These capabilities are offered by major competitors but are **not specced in Observable at all**. Each entry requires a decision: add a spec section and plan a delivery slice, or explicitly mark it out-of-scope with a documented rationale.

### 4.1 Log Pipeline / Parsing Engine ✅ Resolved — Collectable

**Description:** Structured field extraction from raw log strings — grok patterns, regex-based field extraction, JSON path parsing, key-value parsing, multiline log assembly, log routing rules.

**Why it matters:** Requiring pre-structured OTel logs is a deliberate offloading strategy: parsing complexity is pushed to the edge rather than absorbed by the Observable backend. This is architecturally correct but raises the adoption bar, because 80%+ of enterprise log volume originates from legacy applications, syslog, nginx, database engines, and cloud platform logs that cannot emit OTLP natively.

The alternative — accepting logs in raw native formats at the ingest gateway — does not simply add a parsing layer; it compounds complexity by requiring Observable to support a wide variety of *transports* (syslog, webhooks, Kafka, MQTT, etc.) **in addition to** a wide variety of *parsers* for each format, each with its own auth, framing, and reliability semantics. This violates ADR-001 and makes the ingest gateway surface hard to maintain.

Existing edge tools (Fluent Bit, OTel Collector) can bridge the gap in principle but are hard to use correctly for OTLP output. Fluent Bit's OTLP plugin requires manually mapping internal record fields to OTLP LogRecord structure (resource attributes vs log attributes vs body vs severity) with no guidance and silent errors. The OTel Collector has a 50–150 MB footprint and the same mapping ambiguity problem. Both are difficult to debug.

**Resolution:** [Collectable](16-collectable.md) — an independent compiled-mediator tool in the Observable repository. Users define a pipeline (transport + parser + OTLP mapping) in a web UI with live preview against sample data. The output is a compiled static Rust binary with the mapping baked in at compile time — no runtime config interpretation, no OTLP mapping guesswork. See [ADR-022](adr/ADR-022-collectable-mediator.md).

**Competitors:** Datadog Log Management Pipelines, New Relic Log Patterns, Elastic Logstash/Beats, Vector (open source)

---

### 4.2 Error Tracking and Grouping ⚠️ High Priority

**Description:** Automatic error fingerprinting, grouping of similar exceptions across deployments, issue lifecycle (open / resolved / regressed), owner assignment.

**Why it matters:** Engineering teams use error tracking (Sentry, Datadog Error Tracking) to treat exceptions as first-class issues distinct from raw logs and traces. The workflow is: exception fires → grouped with similar errors from the same service → assigned to an owner → marked resolved → alert if it regresses in a new deploy. Observable has alert rules and threshold evaluations but no structured error issue tracker.

**Competitors:** Datadog Error Tracking, New Relic Errors Inbox, Sentry

**Recommended action:** Evaluate as a Phase 3 or Phase 4 feature. Could be built on top of the existing span error status + deployment event model.

---

### 4.3 DORA Metrics ⚠️ High Priority

**Description:** The four DORA (DevOps Research and Assessment) metrics: deployment frequency, lead time for changes, change failure rate, and mean time to restore (MTTR).

**Why it matters:** Deployment events are planned (Phase 3, P3-S5) and RED metrics are planned (Phase 3, P3-S4). Combining these with incident timeline data (Phase 5) produces DORA metrics naturally. However, DORA is not named anywhere in the spec as a product feature, and it is a commonly requested capability for platform engineering and engineering leadership buyers. It would differentiate Observable for the DevOps platform market.

**Competitors:** Datadog DORA Metrics, LinearB, Sleuth, Cortex

**Recommended action:** Add DORA metrics as an explicit feature in [spec/07-alerting-slo.md](07-alerting-slo.md) or a new reliability-reporting section. The data is available once Phase 3 and Phase 5 are complete.

---

### 4.4 Cloud Log Forwarding / Webhook Ingest ✅ Resolved — Collectable

**Description:** Ingest path for logs emitted by managed cloud services through their native channels: AWS CloudWatch, Azure Monitor, GCP Cloud Logging, Heroku log drains, generic HTTPS log-push webhooks.

**Why it matters:** Most cloud-native workloads generate significant log volume from managed services (RDS, Lambda, ECS, Azure Functions, Cloud Run) that emit logs through cloud-native channels, not OTLP. Without a webhook receiver or cloud log forwarder, Observable requires operators to add a separate translation layer for every cloud service, which raises the operational burden considerably. Prometheus remote_write is planned ([ADR-017](adr/ADR-017-prometheus-remote-write.md)) for metrics but there is no equivalent for log sources.

**Competitors:** Datadog Lambda Forwarder and Firehose integration, New Relic Log API, Elastic Beats/Agent cloud integrations

**Recommended action:** Collectable's `http_webhook` transport handles these sources — it receives HTTPS POST payloads from Firehose, Heroku log drains, Splunk HEC, and generic webhooks, then emits OTLP to the Observable ingest gateway. No changes to the ingest gateway are required. See [spec/16-collectable.md](16-collectable.md).

---

### 4.5 Alertmanager / Prometheus Alert Rule Compatibility

**Description:** An import path for Prometheus alerting rules (YAML format) and Alertmanager-compatible API endpoints for alert routing, silences, and inhibitions.

**Why it matters:** Teams migrating from Prometheus/Grafana stacks have existing alert rule libraries written in Prometheus alerting rule format. Observable's alert-evaluator uses its own rule model with no Prometheus-compatible import. This creates a full rewrite burden at migration time. Compatibility does not require replacing the internal model — a translation layer at the API boundary is sufficient.

**Competitors:** Grafana (native Alertmanager), VictoriaMetrics (compatible API)

**Recommended action:** Consider adding a Prometheus alert rule import endpoint to [spec/09-api.md](09-api.md). A conversion layer that maps PromQL expressions and label matchers to Observable's internal alert rule model would remove a significant migration barrier.

---

### 4.6 StatsD / DogStatsD Ingest

**Description:** An intake path for custom application metrics emitted using the StatsD or DogStatsD UDP/TCP protocol — counts, gauges, timers, sets, histograms.

**Why it matters:** StatsD is widely used in legacy applications and by libraries that emit operational metrics without an OTel SDK. Requiring OTel SDK adoption for these workloads is a migration barrier. Prometheus remote_write ([ADR-017](adr/ADR-017-prometheus-remote-write.md)) covers Prometheus pull-model metrics but not push-model StatsD workloads.

**Competitors:** Datadog DogStatsD receiver, New Relic StatsD integration

**Recommended action:** Evaluate as a low-effort addition to the ingest gateway (UDP listener that normalizes StatsD payloads to MetricSeries domain types). Explicitly mark in-scope or out-of-scope in [spec/01-overview.md](01-overview.md).

---

### 4.7 Database Monitoring

**Description:** Query plan analysis, slow query attribution, connection pool metrics, lock wait analysis, and N+1 detection beyond what OTel DB spans carry.

**Why it matters:** Database performance is the leading source of backend latency in most production systems. OTel DB spans carry query text and duration, but competitors go deeper — Datadog Database Monitoring shows execution plans, identifies N+1 patterns, and correlates DB latency to application code paths. Observable traces will surface DB spans, but there is no query analysis layer on top of them.

**Competitors:** Datadog DBM, New Relic Database UI, SolarWinds DPA

**Recommended action:** Evaluate as a Phase 4+ feature. Could be implemented as a set of DataFusion operators over the trace store that detect slow query patterns and N+1 indicators, surfaced in service detail views.

---

### 4.8 Infrastructure Asset Catalog

**Description:** A persistent, queryable inventory of hosts, pods, containers, and cloud resources — independent of whether a telemetry signal was recently emitted by that resource.

**Why it matters:** Currently, infrastructure context in Observable is derived entirely from OTel resource attributes on signals. This means a silent host (one that has not emitted signals recently) is invisible in the platform. Infrastructure teams need to answer "what hosts do we have?" and "which pods are running version X?" without relying on live signal flow. Dynatrace's Smartscape and New Relic's infrastructure inventory are built around this persistent catalog model.

**Competitors:** Dynatrace Smartscape, New Relic Infrastructure, Datadog Infrastructure List

**Recommended action:** Consider adding an infrastructure catalog as a control plane entity in [spec/14-domain-model.md](14-domain-model.md). The k8s operator (planned in [spec/06 §10.9](06-agents.md)) can populate the catalog from the Kubernetes API server, independent of telemetry signal flow.

---

### 4.9 Sampling Rules UI and Cost Estimation

**Description:** A product surface for operators to define, preview, and push tail-sampling rules, including a cost-impact preview showing how a rule change would affect ingest volume and storage cost.

**Why it matters:** Tail sampling is specced ([ADR-011](adr/ADR-011-sampling-strategy.md)) and agents accept OpAMP remote config ([spec/06 §10.6](06-agents.md)), but there is no UI or API for operators to manage sampling rules without directly editing config files. At scale, sampling policy is a significant cost lever, and the inability to preview cost impact before applying a rule is a common enterprise complaint with existing platforms.

**Competitors:** Honeycomb Dynamic Sampling, Datadog APM Ingestion Controls

**Recommended action:** Add a sampling rules management API to [spec/09-api.md](09-api.md) and a corresponding UI module to [spec/05-frontend.md](05-frontend.md). Cost estimation can be derived from the cardinality budget tracker already in the ingest gateway.

---

### 4.10 Multi-Region Active-Active

**Description:** Active-active or active-passive deployment across multiple geographic regions with cross-region query federation.

**Status:** Explicitly deferred in [spec/13-risks-roadmap.md §24.2](13-risks-roadmap.md). No implementation path until single-region, multi-AZ operations are stable.

**Why it matters:** Global SaaS companies and regulated enterprises with cross-region data requirements need this. It is a hard blocker for some enterprise customer segments and is a standard capability in Datadog, Dynatrace, and Grafana Cloud.

**Recommended action:** No action until Phase 7. Add a Phase 7 slice for multi-AZ first, then multi-region active-passive, then active-active. Document the sequencing rationale in [spec/12-deployment.md](12-deployment.md).
