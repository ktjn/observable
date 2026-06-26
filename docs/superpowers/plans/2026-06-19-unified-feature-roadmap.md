# Observable Roadmap — Unified Feature Roadmap

> **Status:** Active. This document replaces and consolidates `2026-05-07-remaining-roadmap-plan.md`
> (post-Phase-3 plan) and `2026-06-04-observability-feature-parity-plan.md` (Phases P9-P14 gap
> analysis), both now in `archived/plans/` for historical reference. Read those archived documents
> for full historical closure evidence and the original competitive-parity workflow analysis —
> this document carries forward only the open backlog, reorganized and reprioritized.
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> (recommended) or `superpowers:executing-plans` to implement promoted slices task-by-task.

## 0. Prioritization Principle

Items within each tier are ordered by **user value** — breadth of audience, frequency of use, and
adoption impact — not by any historical phase numbering. Higher-value slices within a tier are
recommended to be picked first.

---

## 1. Operating Rules

Every iteration must:
1. Change one user-visible or operator-visible behavior.
2. Stay small enough for one reviewer sitting.
3. Include verification, rollback, and next-slice notes.
4. Every frontend slice uses the feature-based directory structure (`src/features/<domain>`),
   Base UI primitives, Tailwind v4, MSW handlers for new endpoints, and accessibility tests for
   major new views.
5. Every backend slice touching PostgreSQL, ClickHouse, Redpanda, object storage, or OpenFGA adds
   or updates a Testcontainers integration test, or states why one isn't applicable.
6. Update this document as part of the PR's definition of done: mark the finished slice, note any
   discovered dependency, and adjust the next recommended slice if priorities changed.

---

## 2. How To Use This Document

Treat this as a feature-first backlog driver:
1. Pick the next unchecked slice in the **highest tier that has a ready item**.
2. Write a detailed implementation plan in `docs/superpowers/plans/YYYY-MM-DD-<slice>.md` before
   implementing (per Promotion Rules, §7).
3. Implement only that slice; verify locally; update this document and `docs/agent-context.md`.
4. Move the detailed plan to `archived/plans/` when the slice ships.

Do not pull an item from the **Deferred** tier (§6) forward unless a concrete customer
requirement, security finding, or scaling incident forces it.

---

## 3. Tier 1 — Ready Now (no blocking prerequisites)

Small, standalone, high user-value slices. Ordered by user impact.

- [ ] **Prometheus Remote Write Receiver** (was P13-S1) — `POST /api/v1/write`, snappy-compressed
  protobuf, label-to-attribute mapping, tenant routing via `X-Tenant-ID`. Single biggest migration
  enabler — users keep their existing Prometheus/otel-collector agent stack and point it at
  Observable. Every organization running Prometheus today has this data; receiving it is the
  fastest path to showing value.
- [ ] **Alert Inhibition Rules** (was P12-S5) — suppress lower-severity alerts for the same
  service while a higher-severity alert is active; `Suppressed` state with "Suppressed by" label.
  Directly reduces alert fatigue for every user with alert rules configured.
- [ ] **Saved Views in Explorers** (was P14-S3) — save filter state + time range + columns per
  signal type; private/shared visibility. High-frequency daily UX improvement for all users.
- [ ] **Export APIs** (was P13-S4) — CSV/JSON export for log, trace, and metric query results;
  100k-row sync limit, async job beyond that. Users need to get data out for external analysis
  and reporting.
- [ ] **Fleet Management UI** — agent health and remote configuration UI (carried from the
  post-Phase-3 plan's Platform Administration gap; `/admin/fleet` is currently a read-only
  contract view pending a live agent-inventory endpoint). Essential for operators managing
  distributed agent deployments.
- [ ] **Admin Console RBAC and Quota Management Views** — `/admin/config` is read-only today;
  add RBAC mutation controls and quota editing (carried from the post-Phase-3 plan). Important
  for multi-tenant administrators.

### 3.5 UI Usability Remediation (cross-cutting workstream)

A full-surface UI review (2026-06-26) of the operator jobs **find errors fast** and **monitor the
platform at a glance**, driven by the Playwright visual suite, produced a sliced remediation plan:
`docs/superpowers/plans/2026-06-26-ui-usability-remediation.md`. Highest-impact items:

- [ ] **Histogram axes & scale** (P0) — Traces/Logs/Service-Detail volume charts have no time or
  count axis, so operators can't read *when* or *how many* for a spike.
- [ ] **De-jargon explorer filters** (P1) — remove "raw NLQ IR JSON" from 8 user-facing placeholders.
- [ ] **Discoverable brush-to-zoom** (P1) — histogram range-select is `sr-only`-hinted today.
- [ ] **Outlier emphasis in tables** (P1) — color/threshold cues on latency/error-rate columns.
- [ ] **Trend/threshold context on stat cards** (P1) — sparkline + delta so numbers are interpretable.
- [ ] Consistency follow-ups (P2): summary-row parity, button hierarchy, compact empty states,
  command palette, dashboard create-affordance + freshness indicators.

### Previously completed (Tier 1)
- [x] **Onboarding Wizard** (was P9-S1) — guided zero-to-first-trace flow.
- [x] **Change-Detection Alert Type** (was P12-S4) — window-comparison alert rules.
- [x] **Change Event API and Dashboard Overlay** (was P14-S4) — change-event markers on dashboards.
- [x] ~~**PagerDuty Notification Channel Adapter** (was P12-S1)~~ — **Retired 2026-06-20**.
- [x] ~~**Opsgenie Notification Channel Adapter** (was P12-S2)~~ — **Retired 2026-06-20**.

---

## 4. Tier 2 — Core Feature Builds (multi-slice, sequenced within the tier)

Larger feature areas requiring 2+ ordered slices. Items within this tier are also ordered by user
value. Promote after exhausting easy Tier 1 wins, or in parallel if reviewer bandwidth allows.

### Error Tracking (Sentry-equivalent workflow) — highest user-value gap
The single most requested competitive parity feature. Every development team familiar with
Sentry expects error tracking as table stakes. No hard prerequisites other than implementation
effort. **Recommended for early promotion to Tier 1 if Tier 1 items are completed or blocked.**

- [ ] **Error Tracking Ingestion and Fingerprinting** (was P9-S2) — extract fingerprints from span
  events with `exception.type`/`exception.stacktrace` on error-status spans; normalize (strip line
  numbers/addresses, truncate module paths); new `error_issues` ClickHouse table; `GET /v1/errors`.
- [ ] **Error Issues Explorer UI** (was P9-S3) — `/errors` route, list with service/status filters,
  detail page with occurrence sparkline, recent spans, assign/resolve actions; error-count badge on
  Service Catalog rows.
- [ ] **Error Issue Regression Detection** (was P9-S4) — re-firing of a resolved fingerprint after
  a later deploy flips status to `regressed` and notifies; new `error_regression` evaluator rule
  type. Depends on the two items above.

### Infrastructure Monitoring
Kubernetes-native infrastructure visibility. High value for platform teams running Observable
alongside their k8s workloads.

- [ ] **Infrastructure Catalog Data Model** (was P10-S1) — new `k8s-operator` service upserting
  `infrastructure_resources` (host/pod/container/namespace/cluster) every 30s; stale/terminated
  lifecycle; `GET /v1/infrastructure/resources`.
- [ ] **Infrastructure Explorer UI** (was P10-S2) — `/infrastructure` route, Hosts and Kubernetes
  (Cluster → Namespace → Pod) tabs, resource detail with metrics/logs/spans.
- [ ] **K8s Operator Deployment** (was P10-S3) — Helm chart, ClusterRole/Binding, service-account
  auth, operator self-observability.

### Service Health
- [ ] **Service Health Summary completion** (remaining scope of P9-S5) — fast-vs-slow burn
  red/yellow distinction, 30s background-poll refresh, open error-issue count badge (depends on
  Error Tracking above). Base catalog UI, RED metrics, and SLO-burn health badges already ship.

### Alerting Depth
- [ ] **Escalation Policy Builder** (was P12-S6) — `escalation_policies` with timed steps;
  evaluator tracks acknowledgement and dispatches the next step if unacked. Steps target the
  generic `webhook` channel type since PagerDuty/Opsgenie adapters were retired.
- [ ] **Prometheus Alert Rule Importer** (was P13-S2) — upload Prometheus alerting YAML, translate
  to threshold/change-detection rules, dry-run mapping report, `?apply=true` to create. Depends on
  Prometheus Remote Write (Tier 1).
- [ ] **PromQL Compatibility Façade** (was P8-S7 / P13-S3) — PromQL → existing NLQ IR translation
  inside the MCP server, reusing the P8-S6 NLQ execution path (no parallel query engine). Raised in
  priority by Prometheus Remote Write adoption.

### Engineering Intelligence
- [ ] **DORA Metrics Report** (was P14-S1) — deployment frequency, lead time, change failure rate,
  MTTR computed from existing deployment-marker/incident data; report page with trend sparklines.
- [ ] **Database Monitoring Layer** (was P14-S2) — top query patterns by duration/P99/count from
  `db.statement`/`db.system` span attributes; query-shape normalization; N+1 detection; new
  "Database" service-detail tab.

---

## 5. Tier 3 — Advanced Signal Expansion (larger investment, real prerequisites)

Genuine new signal types. Each has a stated technical or policy prerequisite — do not promote
until that prerequisite is actually satisfied, but don't gate these behind unrelated stability
work either.

- [ ] **Continuous Profiling Ingestion** (was P6-S1 / P11-S1) — one profile payload format stored,
  indexed, and queryable. **Prerequisite: object-storage path** (see §6, Warm Retention) — this is
  the one place a "deferred" item genuinely blocks a feature; consider unblocking object storage
  specifically to enable this rather than the full warm-retention slice.
- [ ] **Flame Graph Viewer** (was P11-S2) — `/profiling` icicle graph, compare mode, profile-to-trace
  linking via `profile_id`. Depends on the item above.
- [ ] **Browser RUM SDK and Ingestion** (was P6-S2 / P11-S3) — route/API/Web Vitals telemetry from
  one web app (Observable's own frontend is the natural first target). **Prerequisite: session
  attribution/privacy model decision** — this is a quick decision, not a long stability program.
- [ ] **Web Vitals Dashboard** (was P11-S4) — `/rum` route, LCP/CLS/INP percentiles, session list,
  deploy overlay. Depends on the item above.
- [ ] **HTTP Synthetic Check** (was P6-S4 / P11-S5) — UI-defined check (URL/method/assertion/
  schedule), results as a metric series, alertable.
- [ ] **Multi-Step Synthetic Check** (was P11-S6) — chained HTTP steps with response-field
  references and `traceparent` injection. Depends on the item above.
- [ ] **Mobile Signal Ingestion** (was P6-S3) — one mobile SDK ingestion path. Needs its own
  privacy review distinct from browser RUM.
- [ ] **eBPF-Assisted Enrichment** (was P6-S5) — one justified enrichment use case. Needs a
  boundary-focused review for privileged DaemonSet/host access before promotion, but that review
  is scoped to this feature, not a platform-wide hardening program.

---

## 6. Tier 4 — Intelligence Layer (advisory-only AI)

Governed by ADR-014 (advisory-only, never required for correctness) and ADR-021 patterns
established by the completed NLQ layer (P8-S6).

- [ ] **Anomaly Detection** (was P8-S1) — one bounded metric family, explainable candidates with
  source data, no autonomous writes.
- [ ] **Query Recommendations** (was P8-S2) — one explorer view suggests a query with explanation
  and dismissal.
- [ ] **Incident Summarization** (was P8-S3) — cites source events, ignorable without changing
  incident state. Depends on the completed incident timeline (P5-S1, done).
- [ ] **Capacity Forecasting** (was P8-S4) — one storage/ingest dimension, explainable and bounded.
- [ ] **Remediation Hooks with Approval Controls** (was P8-S5) — human-approved, audited; no
  AI-initiated write path.

---

## 7. Deferred — Stability, Compliance, and Enterprise Packaging

Demoted on purpose per this revision's feature-first directive. Do not promote without a concrete
trigger (named customer requirement, measured production incident, or a Tier 3 feature genuinely
blocked on it). This is not "never" — it's "not next."

- [ ] **Warm Retention / Object Storage Path** (was P4-S1, already archived/deferred) — full
  hot→S3-compatible movement path. Note: Continuous Profiling (§5) needs *an* object-storage path;
  if profiling is promoted, scope the minimum object-storage slice for that feature rather than
  reviving the full warm-retention plan.
- [ ] **Distributed Rate Limiting** — move `ingest-gateway` limiters from in-memory to a shared
  store for horizontal scaling. Only urgent if real multi-instance scaling is happening.
- [ ] **P4-S3b: SCIM/SSO Management** — only if a specific v1 customer requires automated
  provisioning before launch.
- [ ] **Session Replay** (was P6-S6) — hard privacy-review prerequisite, not accelerated.
- [ ] **Phase 7 — Enterprise Readiness (all items)**: regional residency (P7-S1), BYOK (P7-S2),
  tenant-isolated deployment packaging (P7-S3), compliance reporting (P7-S4), metering export
  (P7-S5, promote after DORA/usage-report data is richer), marketplace/private packaging (P7-S6).
  Every item here is explicitly conditional on a named target-customer requirement.
- [ ] Further load/chaos/tenant-escape/security-review cycles beyond what P4-S8/P4-S9 already
  established, unless a specific incident or audit demands it.

### Service Layer Architecture (from 2026-06-19 architecture review)

Findings and rationale: `docs/superpowers/specs/2026-06-19-service-layer-architecture-review.md`.
Demoted per this section's standard rule — promote only on a concrete trigger — **except the
first item, which is pre-promoted** because Tier 2's PromQL Compatibility Façade and all of Tier 4
(Intelligence Layer) build on the coupling it addresses.

- [ ] **Extract NLQ/AI from query-api** — pull `llm_adapter.rs` and the NLQ execution path into an
  independently deployable service or a narrowly-bounded crate, decoupling LLM-provider changes
  and AI feature iteration from the trace/log/metric read path. **Pre-promoted**: do this before
  the PromQL façade or any Tier 4 item adds more surface area to the coupling.
- [ ] **Repository/tenant-scoping layer for query-api** — thin data-access module wrapping
  ClickHouse/PostgreSQL access with type-level tenant scoping (e.g., a `TenantScopedQuery` builder
  that can't be constructed without a tenant id), replacing inline SQL-per-handler. Trigger: a
  tenant-isolation finding in a future security review, or before further query-api domain growth.
- [ ] **Queue-based stream-processor → storage-writer handoff** — replace the synchronous HTTP
  POST with a Redpanda topic, matching the ingest-gateway → stream-processor pattern, so
  storage-writer slowness can't back up Redpanda consumer lag. Trigger: observed consumer-lag
  incident, or before ingest volume scaling work.
- [ ] **Split alert-evaluator rule-sourcing from evaluation** — separate `RuleSource` (PostgreSQL
  fetch, future Prometheus importer) from a pure `Evaluator` function, reducing the worker loop to
  orchestration. Trigger: picking up the Prometheus Alert Rule Importer (Tier 2), which adds a
  second rule source.
- [ ] **Move threshold/condition types into models/alerts.mdl** — extend the existing Modelable
  `.mdl` file (per ADR-032) with `ThresholdOperator`/`ThresholdCondition` and generate both
  `alert-evaluator` and `query-api`'s Rust bindings from it, replacing JSON-serde-only
  round-tripping. Note: inherits ADR-032's `enum(...)` → Rust `String` emitter gap (Phase 1
  backlog item 3) until that's fixed — removes the duplication, not the type-safety gap. Low
  effort; promote opportunistically alongside any other alerting-area slice.
- [x] **Extract admin-service** (members, tokens, config, usage out of query-api) — privilege
  isolation: member/role management, API key/token lifecycle, and platform config are
  privilege-granting operations sharing a process boundary with the high-traffic trace/log/metric
  read path today. **Promoted 2026-06-20 per explicit user request; all 3 slices complete**
  (`libs/observable-auth` crate, `services/admin-service` scaffold + ingress routing, removal of
  the now-dead handler copies from query-api — see `archived/plans/2026-06-20-observable-auth-crate.md`,
  `archived/plans/2026-06-20-admin-service-scaffold.md`, and
  `archived/plans/2026-06-20-admin-service-cleanup.md`). Design:
  `docs/superpowers/specs/2026-06-19-admin-service-extraction-design.md`; status tracked in
  `spec/adr/ADR-033-admin-service-extraction.md`.
- [ ] **Shared observable-error crate** — common HTTP error mapping/problem+json shape/tracing
  integration consumed by all services, replacing each service's independent error-to-response
  mapping. Low effort, low coupling risk (pure utility, no tenant-scoping concerns) — unlike a
  shared data-access crate (deliberately rejected; see the repository-layer item above, which
  stays per-service). Promote opportunistically alongside any other cross-cutting slice.
- [ ] **Shared observable-observability crate** — generic `HttpMetricsCollector` (Prometheus
  registry/histogram pattern) and `ReadyzProbe` (Postgres/ClickHouse/Redpanda variants), replacing
  ~270 lines of near-identical `/metrics`/`/readyz` scaffolding copy-pasted across auth-service,
  storage-writer, query-api, ingest-gateway, alert-evaluator, and stream-processor. Medium effort
  (six mechanical call-site swaps), low risk (response shape unchanged).
- [ ] **DB client construction helpers in libs/domain** — add `create_clickhouse_client()` and
  `create_postgres_pool()` to the existing `libs/domain` shared crate, replacing ~40 lines of
  duplicated connection-string/env-var boilerplate across storage-writer, alert-evaluator,
  query-api, ingest-gateway, and auth-service. Low effort, low risk, no new crate needed.

---

## 8. Sequencing and Dependencies

```
Tier 1 (user-value order, promote immediately)
  Prometheus remote write → Alert inhibition → Saved views → Export APIs
  → Fleet management UI → Admin RBAC/quota UI
  (Onboarding wizard, change-detection alert, change event API — done)
  (PagerDuty/Opsgenie — retired)

Tier 2 (user-value order)
  Error tracking ingestion → Error issues UI → Regression detection → Service health completion
  Infra catalog model → Infra explorer UI → K8s operator Helm deployment
  Escalation policy builder (targets the generic webhook channel type)
  (Prometheus remote write) → Prometheus alert importer
  (P8-S6 NLQ, done) → PromQL façade
  DORA metrics, Database monitoring  ← standalone, no new infra

Tier 3
  Object-storage slice (scoped to profiling) → Profiling ingestion → Flame graph viewer
  Session/privacy decision → Browser RUM → Web Vitals dashboard
  HTTP synthetic check → Multi-step synthetic check
  Mobile ingestion, eBPF enrichment  ← standalone, own prerequisites

Tier 4
  Anomaly detection, query recommendations, incident summarization, capacity forecasting,
  remediation hooks  ← each standalone, advisory-only

Deferred
  Warm retention (full), distributed rate limiting, SCIM/SSO, session replay, all of Phase 7
```

---

## 9. ADR and Spec Sync Requirements

Carried forward from the parity analysis — update these when implementing the corresponding
feature:

| Feature area | Required spec / ADR change |
|---|---|
| Error tracking | New section in `spec/14-domain-model.md` for ErrorIssue; new section in `spec/05-frontend.md` for the Errors explorer |
| Infrastructure catalog | New section in `spec/14-domain-model.md` for InfrastructureResource; new section in `spec/06-agents.md` for k8s operator catalog population |
| DORA metrics | New section in `spec/07-alerting-slo.md` for DORA definitions and data sources |
| Database monitoring | New section in `spec/05-frontend.md` for the Database tab; new section in `spec/08-ai-ml.md` for the N+1 heuristic |
| Change event API | Extend `spec/18-deployment-markers.md` to cover non-deploy change event types |
| Prometheus remote write | `ADR-017` already accepted — add the implementation section to `spec/09-api.md` |
| Export APIs / Saved views | Add to `spec/05-frontend.md` §9.11 |
| Escalation policies | Extend `spec/07-alerting-slo.md` §11.4 |
| Deadman + change-detection alerts | Already in `spec/07-alerting-slo.md` §11.1; implementation spec needed in `spec/09-api.md` (deadman done — see `archived/plans/2026-06-18-p12-s3-deadman-alert.md`) |

---

## 10. Promotion Rules

1. Write a detailed implementation plan in `docs/superpowers/plans/YYYY-MM-DD-<slice>.md` before
   implementing.
2. Include exact file paths, tests, rollback path, telemetry impact, auth/tenancy impact,
   data-retention impact, and ADR/spec sync decision.
3. Keep the PR below the repo's tiny-iteration size target unless the reviewer accepts a larger
   slice.
4. Update this document and `docs/agent-context.md` when a slice is promoted, completed, or
   deferred; move the detailed plan to `archived/plans/` when it ships.
5. Pulling a Tier-4-or-higher item ahead of an unfinished lower-tier item is fine if it's smaller
   and ready; pulling a §7 Deferred item forward requires stating the concrete trigger in the PR.

---

## 11. Housekeeping Notes Found During This Consolidation

- GitHub issues **#388** (Trace Comparison) and **#389** (Query Workbench) describe already-shipped
  features and should be closed.
- The Trace UI Context Panel work tracked in project memory (`feat/trace-ui-context-panel` branch)
  is fully merged into `main` (0 commits ahead, branch is stale) — the corresponding context panel
  exists in `apps/frontend/src/pages/TraceDetail.tsx`. That memory record should be updated or
  removed.

---

## 12. Source Documents

- Predecessor active plans (now archived, retain full historical closure log and workflow-gap
  analysis): `archived/plans/2026-05-07-remaining-roadmap-plan.md`,
  `archived/plans/2026-06-04-observability-feature-parity-plan.md`
- Finish-started closure record: `archived/plans/2026-05-09-finish-started-work-plan-rf0-complete.md`
- Roadmap scope: `spec/10-process.md §17`, `spec/13-risks-roadmap.md §24`
- Architecture decisions: `spec/adr/`
- Product and platform specs: `spec/`
