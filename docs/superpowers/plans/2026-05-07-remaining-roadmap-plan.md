# Observable Roadmap — Post-Phase-3 Implementation Plan

> **Status:** Active. This document merges the historical Phases 2–8 reference with the remaining-roadmap gap review.
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement promoted slices task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve the post-Phase-3 roadmap as a practical sequence of small, reviewable vertical slices with explicit pause points, decision checkpoints, and entry/exit criteria.

**Scope:** The platform can already ingest telemetry, persist it, query it, and render the minimum UI described in `spec/10-process.md §17` and the archived [Phase 1 plan](../../../archived/plans/2026-04-17-phase1-internal-mvp.md). Phase 2 (governed MVP) and Phase 3 (correlation and service operations) are complete; see the [Historical Closure Log](#historical-closure-log) for evidence.

---

## 1. Operating Rules

This plan follows:
- `spec/10-process.md §16.8` tiny-agent iteration workflow
- `spec/10-process.md §17` phased roadmap
- `spec/13-risks-roadmap.md §24.3` near-term execution order

Every iteration in the remaining phases must:
1. Change one user-visible or operator-visible behavior.
2. Stay small enough for one reviewer sitting.
3. Include verification, rollback, and next-slice notes.
4. Stop at a checkpoint before crossing a new trust boundary.
5. Avoid bundling backend, frontend, infrastructure, and docs unless the slice requires all of them.
6. **Value-First Sequencing**: Cross-phase value-first slices may be promoted early only when their direct prerequisites are satisfied and the PR states which phase-gate risk remains.
7. **Mandatory UI Standards**: Every frontend slice MUST:
   - Use the **feature-based directory structure** (`src/features/<domain>`)
   - Use **Base UI** primitives for new interactive components (Shadcn pattern)
   - Implement **Tailwind CSS v4** for styling
   - Include **MSW handlers** for the new API endpoints
   - Include **Accessibility tests** (`playwright-axe`) for major new views
   - Follow the **Testing Trophy** (prioritize integration tests with RTL/MSW)
8. **Mandatory backend integration harness**: Every backend slice that touches PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object storage, OpenFGA, or another real containerized dependency boundary MUST add or update the narrowest applicable Testcontainers integration test unless the slice explicitly requires Docker Compose, kind, browser, or external-provider verification instead. If Testcontainers is not applicable, the PR must state why and name the replacement signal.
9. Update this plan document as part of the PR's definition of done:
   - Mark the finished slice state.
   - Add any new checkpoint answer or discovered dependency.
   - Adjust the next recommended slice if priorities changed.

---

## 2. How To Use This Document

Treat this as an interactive backlog driver, not a one-shot build spec.

For each slice:
1. Confirm the phase entry gate is actually satisfied.
2. Pick the next unchecked slice in priority order.
3. Write the slice contract in the PR body.
4. Implement only that slice.
5. Verify locally.
6. Update this plan document before opening or updating the PR:
   - Change the slice checkbox state.
   - Record the checkpoint answer if the slice resolves one.
   - Note any sequence changes caused by new information.
7. Pause at the checkpoint question and record the answer in the PR.
8. Merge only after review.
9. Start the next slice on a new branch.

If a checkpoint answer is "not yet" or "unclear", stop and resolve that before advancing.

**PR definition of done for this plan:** a slice PR is not done until the implementation and this plan document agree on the current state of the phase.

---

## 3. Standard Slice Packet

Use this packet shape for every promoted slice:

```markdown
Source spec:
Phase:
Parent phase item:
Acceptance target:
User/operator outcome:
Files or modules expected to change:
Out of scope:
Verification:
Baseline:
New errors introduced:
Telemetry impact:
Auth/tenancy impact:
Data retention or migration impact:
Rollback path:
ADR/spec sync:
Checkpoint question:
Next smallest slice:
```

---

## 4. Global Sequencing Rules

1. Finish operationally necessary parts of **Phase 4** before building the full **Phase 5** reliability product.
2. Do not start **Phase 6** advanced signals until retention, auth, and release safety are proven under load.
3. Treat **Phase 7** as customer-driven packaging and policy work; only do the parts required by target customers.
4. Treat **Phase 8** as optional until data quality, retention, labeling, and auditability are stable.
5. Preserve self-observability as a platform invariant: every platform component must emit logs, metrics, traces, health, readiness, and Prometheus-compatible metrics. Self-observability covers three instrumentation levels: service level, infrastructure level, and UI level.

---

## 5. Current Implementation Gaps

These gaps were identified during a direct review of `apps/frontend/src` and the Rust backend services against `spec/05-frontend.md` and the Phase 2–8 roadmap.

### UI Gaps

#### Service Detail & Navigation
- [x] **Tab Completion**: Add **Deployments** and **Alerts** tabs to `ServiceDetailPage.tsx` (COMPLETED 2026-05-09). The `deployments` and `alerts` tabs are present and wired. Incidents tab on service detail remains deferred; incidents are reachable via the global Incidents page.
- [ ] **Context Preservation**: Ensure all tabs (Logs, Metrics, Traces, etc.) consistently apply the service filter and global date range from the URL.

#### Dashboard Maturity
- [x] **Dashboard Detail View**: Create a dedicated page for viewing a single dashboard where `TimeSeriesGraph` and other visualizations are actually rendered for each panel (COMPLETED 2026-05-12). `DashboardDetailPage.tsx` exists with panel rendering, edit mode, add/edit forms, and `react-grid-layout` drag/resize.
- [x] **Dashboard Builder**: Implement the full drag-and-drop panel editor and configuration UI as specified in §9.7 (COMPLETED 2026-05-18). `DashboardDetailPage.tsx` now includes a `PanelTemplateLibrary` with pre-built templates (error rate, request rate, P99 latency, slow traces, recent errors, CPU usage) plus the existing edit mode, add/edit forms, and `react-grid-layout` drag/resize. Custom panel creation remains available alongside templates.

#### Reliability & Alerting
- [x] **SLO Management**: Expand `features/alerts` to include error budget tracking and burn-rate history (COMPLETED 2026-05-05). `AlertsPage.tsx` includes SLO creation form, `SloHealthCard` components with burn-rate thresholds, and `api/slos.ts` backend integration.
- [x] **Incident Timeline**: Implement the correlated event timeline for alert firings as specified in §9.2 (COMPLETED 2026-05-10). `IncidentDetailPage.tsx` renders a `Timeline` section with `IncidentEventItem` events (triggered, alert_fired, alert_resolved, etc.).
- [x] **Notification Channels**: Add UI for managing outbound notification integrations (webhooks, Slack) (COMPLETED 2026-05-10). `NotificationChannelsList.tsx` exists in `features/alerts/`.

#### Explorers & Workbench
- [ ] **Live Tail**: Add the "Live" toggle and streaming append behavior to `LogSearch.tsx` as specified in §9.18.
- [ ] **Trace Comparison**: Implement the side-by-side or diff view for comparing two traces.
- [ ] **Query Workbench**: Implement the Monaco-based multi-signal notebook for ad-hoc exploration.

#### Platform Administration
- [ ] **Fleet Management**: Add the agent health and remote configuration UI.
- [ ] **Admin Console**: Implement the tenant configuration, RBAC, and quota management views.

### Backend Gaps & Performance Issues

#### Security & Tenancy
- [x] **Query API Tenancy (RF-0)**: Fix `query-api/src/middleware/auth.rs` to validate credentials (session/API key) before trusting `X-Tenant-ID` (COMPLETED 2026-05-09). `auth.rs` implements dual-path validation: `Authorization: Bearer <api-key>` is verified against the `api_keys` table, and `Cookie: session=<jwt>` is validated via `auth-service` `POST /internal/validate-session`.
- [ ] **NLQ SQL Safety (RF-3)**: Implement field and type allowlisting in `query-api/src/sql_templates.rs` to prevent SQL injection and unsafe query shapes. Partial: `mcp_query.rs` has `allowed_metric_filter_fields()` and `traces.rs` has `valid_fields` allowlists; comprehensive allowlisting across all SQL template paths remains.

#### Performance & Scaling
- [ ] **Stream Processor Batching**: Refactor `stream-processor` to batch multiple telemetry envelopes before flushing to `storage-writer`. The current 1-request-per-envelope model is a high-volume bottleneck.
- [ ] **Distributed Rate Limiting**: Move `ingest-gateway` rate limiters from in-memory to a shared store (e.g., Redis) to support horizontal scaling.
- [ ] **ClickHouse Insert Efficiency**: Align `stream-processor` batching with `storage-writer` to ensure ClickHouse receives large, efficient blocks rather than many small inserts.

#### Reliability & Observability
- [x] **Alert Lifecycle (RF-4)**: Implement deduplication, `for_duration_secs` (pending state), and resolution logic in `alert-evaluator/src/evaluator.rs`. Completed in branch `feat/rf-4-alert-lifecycle-semantics`; see the finish-started plan for verification scope.
- [x] **Deployment Correlation (RF-5)**: Implement active `deployment_id` enrichment in the ingest path based on current deployment markers. Completed 2026-05-09; see `docs/agent-context.md`.
- [ ] **Self-Observability (RF-6)**: Add `/metrics` (Prometheus) and `/readyz` endpoints to all services; ensure `Platform API` ports are correctly configured in Helm/Compose.

#### Test Coverage & CI
- [ ] **Integration Test Regression (RF-2)**: Restore the integration test gate in `scripts/local-ci.sh` to ensure PostgreSQL and ClickHouse logic is verified by default.
- [ ] **Telemetry Loop Prevention**: Audit and unify the "observable" environment suppression logic across all ingest and processing paths to prevent recursive feedback loops.

---

## 6. Phase 4 — v1 Production Readiness

**Goal:** Make the product supportable for selected external customers.

**Entry gate:**
- Phase 3 exit gate passed
- Correlation flows are stable under real traffic
- Deployment and rollback automation already exists in basic form

**Exit gate:**
- v1 customers can be onboarded with documented support boundaries, restore paths, security posture, and test evidence

### Priority Slice Order

- [-] **P4-S1: Add one warm-retention movement path** (ARCHIVED/DEFERRED)
  - Outcome: aged data moves from hot ClickHouse storage to one S3-compatible object-storage path without breaking queries for the selected dataset.
  - Closure steps: add local MinIO or equivalent S3-compatible storage, define object key layout and retention metadata for one signal, add one writer/export movement path, and document rollback/disable behavior.
  - Checkpoint: do query semantics stay stable across tiers?
  - Archive: detailed implementation plan moved to `archived/plans/2026-05-05-p4-s1-warm-retention.md`. Deferred until object-storage prerequisites or cost/retention priorities change.

- [ ] **P4-S2: Add backup and restore drill for one dataset**
  - Outcome: one restore path is practiced and timed, not merely specified.
  - Closure steps: include object-storage state in the backup boundary if P4-S1 has landed; otherwise explicitly record why the first drill is hot-store-only.
  - Checkpoint: are measured RPO/RTO values acceptable?

- [x] **P4-S3: Add SSO/OIDC for one customer-compatible flow** (COMPLETED 2026-05-06)
  - Source spec: `spec/04-tenancy-security.md` §8.1, `spec/05-frontend.md` Phase 4+ Admin Console, `spec/13-risks-roadmap.md` §24.2, ADR-008, ADR-015, ADR-031.
  - Outcome: one external identity provider can authenticate a human user into the platform, and the UI can operate with the resulting principal rather than local-dev API-key assumptions.
  - Closure steps: add OIDC authorization-code-with-PKCE login/logout and callback handling; map IdP subject, email, and groups into the local principal model; issue/verify short-lived user sessions for query/admin APIs; filter `GET /v1/tenants` by the authenticated principal per ADR-031; remove unauthenticated access from tenant-list bootstrap endpoints except for explicitly public setup status; add an Admin Console identity settings read view showing configured provider, redirect URI, issuer, and tenant mapping state without exposing secrets; audit login success/failure and tenant-selection decisions.
  - UI operations: the AppShell must show authenticated user state, handle expired sessions without losing tenant/environment/time context, hide or collapse tenant selection for single-tenant users, and gate Admin/Fleet/Billing navigation by coarse RBAC.
  - Out of scope: SAML, SCIM provisioning, multi-IdP routing, user/group lifecycle sync, password or local-user auth, and building a first-party identity provider.
  - Verification: HTTP integration tests for protected query/admin paths, tenant-list filtering, missing/expired session rejection, and cross-tenant denial; frontend MSW/RTL tests for login state, session expiry, tenant picker filtering, and Admin Console identity settings; a documented manual smoke against one customer-compatible IdP or local Keycloak test realm.
  - Checkpoint: does this change only integrate a bought/leverage IdP per ADR-015, and are any auth-model deltas reflected in ADR-008/spec updates?
  - Archive: detailed implementation plan moved to `archived/plans/2026-05-06-identity-provider-zitadel.md`.

- [ ] **P4-S3b: Add SCIM/SSO management if required by target v1 customers**
  - Trigger: execute only when a selected v1 customer requires automated user/group provisioning or multi-provider SSO management before launch.
  - Outcome: tenant admins can manage SSO provisioning status and receive externally provisioned users/groups without manual database edits.
  - Closure steps: define the SCIM user/group subset needed for tenant membership and coarse role assignment; add a SCIM-compatible provisioning endpoint or supported IdP webhook path backed by PostgreSQL control-plane tables; map external groups to TenantAdmin, ProjectAdmin, Member, and Viewer assignments; expose an Admin Console SSO settings surface for provider metadata, group mappings, sync status, and last error; audit create/update/deactivate events; document deprovisioning behavior and rollback.
  - Out of scope: storing IdP secrets in the browser, custom password management, billing entitlement sync, and full enterprise policy features such as regional residency or BYOK.
  - Verification: Testcontainers-backed PostgreSQL integration tests for provision, update, deactivate, group mapping, and idempotent replay; HTTP integration tests for tenant-admin-only management paths; frontend MSW/RTL tests for group mapping and sync-error states; manual smoke against the selected IdP's SCIM test app when available.
  - Checkpoint: is SCIM required for the first v1 customer, or can it remain deferred without blocking external launch?

- [ ] **P4-S4: Add fine-grained authorization for one protected resource**
  - Outcome: one OpenFGA-style protected object has enforceable sharing semantics.
  - Checkpoint: is the ReBAC model additive to RBAC rather than conflicting with it?

- [x] **P4-S5: Add SLO definition and one burn-rate alert** (COMPLETED 2026-05-05)
  - Outcome: one service-level availability SLO can be created/read, `alert-evaluator` evaluates an `slo_burn_rate` rule using fast and slow ClickHouse span windows, and the Alerts & SLOs UI shows SLO health.
  - Closure steps: SLO definition model/API, `slo_burn_rate` evaluator dispatch, multi-window burn-rate evaluation, active firing state, and frontend SLO create/read workflow are complete.
  - Detail: [P4-S5 SLO burn-rate implementation plan](../../../archived/plans/2026-05-05-p4-s5-slo-burn-rate.md)
  - Checkpoint: are error budget semantics now reliable enough for customer use? Answer: yes for service-level availability SLOs backed by hot span data and multi-window burn-rate alerts. Latency, synthetic, incident, and notification behavior remain follow-up slices.

- [ ] **P4-S6: Add production runbook set for one failure class**
  - Outcome: one documented incident type has triage, rollback, and restore steps.
  - Checkpoint: can an operator execute this without tribal knowledge?

- [ ] **P4-S7: Add tenant usage and cost report for one billing interval**
  - Outcome: operators can explain where ingest and storage cost went.
  - Checkpoint: do we have enough signal to price or quota sanely?

- [ ] **P4-S8: Run load, chaos, tenant-escape, and upgrade/rollback suites**
  - Outcome: production-readiness claims are backed by repeatable evidence.
  - Checkpoint: what failed, and does it block external support?

- [ ] **P4-S9: Complete boundary-focused security review**
  - Outcome: auth, tenancy, query, and ingest boundaries have explicit review notes.
  - Checkpoint: are any findings severe enough to block v1?

### Phase 4 Pause Point

Before Phase 5 starts, answer:
- Could we support a real customer through an outage?
- Can we restore, roll back, and explain permissions cleanly?
- Do we have enough evidence to call the platform externally supportable?

---

## 7. Phase 5 — Reliability Product

**Goal:** Add the operator workflow layer for incidents, notification routing, and composite alerting.

**Entry gate:**
- Phase 4 exit gate passed
- SLO and alert foundations are stable

### Priority Slice Order

- [x] **P5-S1: Add incident timeline for one alert source** (COMPLETED 2026-05-18)
  - `IncidentDetailResponse` includes `rule_name` via LEFT JOIN; alert evaluator writes human-readable messages (`{name} fired: value={value:.2}`); `GET /v1/alerts/rules/:rule_id` returns rule detail + 20 recent firings; `AlertRuleDetailPage` at `/alerts/$ruleId`; incident timeline links to rule detail on `alert_fired`/`alert_resolved` events.

- [x] **P5-S2: Add one notification routing integration** (COMPLETED 2026-05-10)
  - Direct prerequisite: at least one stable alert source.
  - Completion signal: one alert firing reaches one operator channel with retry, delivery status, and audit notes.

- [x] **P5-S3: Add runbook workflow attachment to an alert or incident** (COMPLETED 2026-05-19)
  - `alert_rules.runbook_url TEXT` column (migration 029); `GET /v1/alerts/rules/:id` returns it; `POST /v1/alerts/rules` accepts it; `PATCH /v1/alerts/rules/:id/runbook` sets/clears it with http/https validation; evaluator copies it to `incidents.runbook_url` on incident creation; `AlertRuleDetailPage` shows inline-editable runbook row; `AlertsPage` create form includes optional Runbook URL field.

- [ ] **P5-S4: Add topology-aware impact view for one incident**
  - Direct prerequisite: P3 service topology and P5-S1 incident timeline.
  - Completion signal: one incident shows impacted services or dependencies from existing topology data.

- [ ] **P5-S5: Add composite alert evaluation for one rule pair**
  - Direct prerequisite: threshold/SLO evaluator stability.
  - Completion signal: two existing signals can combine into one derived firing without duplicating evaluator architecture.

- [ ] **P5-S6: Add reliability reporting for one team/service scope**
  - Direct prerequisite: SLOs and incident/alert history.
  - Completion signal: one team or service gets a reliability report over a bounded interval.

**Checkpoint question:** can responders complete detect → triage → notify → review inside the product for one real incident class?

---

## 8. Phase 6 — Advanced Telemetry

**Goal:** Add optional signal types without destabilizing the core platform.

**Entry gate:**
- Phase 5 workflows are stable
- Retention, privacy, and cost controls are already proven

### Priority Slice Order

- [ ] **P6-S1: Add continuous profiling ingestion and one query path**
  - Direct prerequisite: P4-S1 object-storage path.
  - Completion signal: one profile payload is stored, indexed, and queryable.
  - Missing clarity: choose the first profile format and whether the first ingestion path is OTLP-compatible, pprof upload, or an internal test endpoint.

- [ ] **P6-S2: Add browser RUM for one web app**
  - Direct prerequisite: session and privacy attribution model clear enough for frontend telemetry.
  - Completion signal: one browser app emits route/API/Web Vitals data that appears in the platform.
  - Missing clarity: choose whether the first app is Observable's own frontend or a synthetic demo app.

- [ ] **P6-S3: Add mobile signal ingestion for one SDK path**
  - Direct prerequisite: RUM privacy/retention decisions or a separate mobile-specific privacy review.
  - Completion signal: one mobile-compatible payload path is ingested and queryable.

- [ ] **P6-S4: Add one synthetic check workflow**
  - Direct prerequisite: notification or alert surface if failures should notify operators.
  - Completion signal: one configured synthetic check produces success/failure telemetry and optional alerting.

- [ ] **P6-S5: Add eBPF-assisted enrichment for one justified use case**
  - Direct prerequisite: boundary-focused security review for privileged DaemonSet deployment, host access, tenant attribution, and rollback.
  - Completion signal: one eBPF enrichment adds value without becoming required for core correctness.

- [ ] **P6-S6: Add session replay only after privacy review passes**
  - Direct prerequisite: explicit privacy review and retention/cost controls.
  - Completion signal: if approved, one heavily scoped replay capture path exists with masking and deletion behavior.

**Checkpoint question:** does each new signal remain modular, governed, and optional?

---

## 9. Phase 7 — Enterprise Readiness

**Goal:** Add packaging, policy, and deployment controls required by enterprise buyers.

**Entry gate:**
- At least one target customer requirement set exists
- Phase 6 work has not destabilized core operations

### Priority Slice Order

- [ ] **P7-S1: Add regional residency controls for one region pair**
  - Conditional on target-customer requirement.
  - Completion signal: one region-pair policy can route or constrain telemetry/control-plane data as specified.

- [ ] **P7-S2: Add BYOK for one storage boundary**
  - Conditional on target-customer requirement.
  - Completion signal: one storage boundary can use customer-managed keys with documented rotation and rollback.

- [ ] **P7-S3: Add tenant-isolated deployment packaging for one environment class**
  - Conditional on target-customer requirement.
  - Completion signal: one isolated deployment class is installable without forking product architecture.

- [ ] **P7-S4: Add compliance reporting for one framework**
  - Conditional on target-customer requirement.
  - Completion signal: one compliance report can be produced from audit/configuration evidence.

- [ ] **P7-S5: Add metering export for one billing flow**
  - Promote after P4-S7 cost reporting.
  - Completion signal: one billing or metering destination receives a bounded export.

- [ ] **P7-S6: Add marketplace or private deployment packaging**
  - Conditional on distribution strategy.
  - Completion signal: one packaging target can install and upgrade the platform with documented support boundaries.

**Checkpoint question:** which enterprise items are truly customer-blocking now, and which stay deferred?

---

## 10. Phase 8 — Intelligence

**Goal:** Add explainable, auditable intelligence features on top of a stable platform.

**Entry gate:**
- Historical retention is reliable
- Labeling and auditability are good enough to explain model behavior
- AI remains advisory, never required for correctness

### Priority Slice Order

- [ ] **P8-S1: Add anomaly detection for one clearly bounded metric family**
  - Direct prerequisite: stable historical metric retention and labeling.
  - Completion signal: one metric family produces explainable anomaly candidates with source data and no autonomous writes.

- [ ] **P8-S2: Add query recommendations for one explorer view**
  - Direct prerequisite: NLQ provenance and query safety remain stable.
  - Completion signal: one explorer can suggest a query with explanation and dismissal.

- [ ] **P8-S3: Add incident summarization with source links**
  - Direct prerequisite: P5-S1 incident timeline.
  - Completion signal: one incident summary cites source events and can be ignored without changing incident state.

- [ ] **P8-S4: Add capacity forecasting for one storage or ingest dimension**
  - Direct prerequisite: enough retained historical usage data and cost model.
  - Completion signal: one forecast is explainable, bounded, and advisory.

- [ ] **P8-S5: Add remediation hooks with explicit approval controls**
  - Direct prerequisite: approval/audit model and strong authorization.
  - Completion signal: one remediation suggestion can be approved by a human and audited; no AI-initiated write path exists.

- [x] **P8-S6: Add NL query layer for one explorer view using semantic annotations** (COMPLETED 2026-05-09)
  - Outcome: operators can ask natural-language questions against one signal type and receive an explained, sourced answer grounded in semantic annotations from the Schema Registry (P3-S14).
  - Implementation follows the three-stage pipeline in ADR-021: LLM emits a structured NLQ IR; an MCP server translates the IR to SQL using the time-series SQL template library; the MCP server returns a `VisualizationFrame` that the UI auto-renders without panel-type selection.
  - Governed by ADR-021 and within ADR-014 advisory-only, provenance-required, read-only constraints. Every response must include the NLQ IR, raw SQL, signals consulted, and an approximation statement.
  - Prerequisite: P3-S14 (Schema Registry with semantic annotations) must be complete.
  - Verification: MCP server unit tests assert SQL template output per operation type; integration tests cover end-to-end IR → VisualizationFrame with real ClickHouse and PostgreSQL via Testcontainers; frontend tests cover NLQ bar submission, provenance display, and auto-graph panel selection.
  - Checkpoint: does every response carry provenance (NLQ IR, source SQL, time range, signal type, sample rate) and can it be ignored without affecting platform correctness?

- [ ] **P8-S7: Add PromQL compatibility façade for metrics (optional)**
  - Outcome: operators can submit PromQL expressions against metric series and receive the same VisualizationFrame output as NLQ. A PromQL parser inside the MCP server translates PromQL expressions into the NLQ IR; execution, auto-graphing, and provenance are identical to P8-S6.
  - Scope: metrics-only; no log, trace, or cross-signal PromQL semantics are introduced. No new query engine — this is a front-end parser that emits the existing NLQ IR.
  - Prerequisite: P8-S6 complete.
  - Verification: unit tests assert PromQL → NlqIr mapping for rate, irate, sum by, and label selector cases; integration test confirms end-to-end PromQL → VisualizationFrame uses the same Testcontainers path as P8-S6.
  - Checkpoint: does the PromQL façade use the same MCP server execution path as NLQ (no parallel execution engine)?

**Checkpoint question:** can every AI output be explained, audited, and ignored without harming correctness?

---

## 11. Next Promotion Candidates

Promote only one candidate at a time.

1. **P5-S1: Add incident timeline for one alert source**
   - Promote after P4-S5 or P5-S2, depending on whether the product wants triage history before outbound notifications.
   - First detailed plan should consume existing threshold or SLO alert firings and append immutable timeline events.

2. **P6-S1: Add continuous profiling ingestion and one query path**
   - Promote only after P4-S1 warm retention/object storage is complete.
   - First detailed plan should store profile blobs in object storage and query one profile index path.

3. **P6-S2: Add browser RUM for one web app**
   - Promote after identity/session behavior is clear enough to attribute frontend sessions safely.
   - First detailed plan should use standard OTel browser-compatible payloads plus explicit `session_id` attributes before custom RUM endpoints.

---

## 12. Cross-Phase Review Rhythm

At the end of every 3–5 merged slices:
- Review whether the phase exit gate is closer or just the diff count is growing.
- Prune stale backlog items.
- Promote only the next 3 slices into active planning.
- Re-check ADR/spec sync needs.
- Record newly discovered risks in `spec/13-risks-roadmap.md` only if they change roadmap scope.

Do not keep a 50-slice active queue. Keep the active horizon short and the roadmap long.

---

## 13. Historical Closure Log

### Phase 2 — Governed MVP (COMPLETE)

**Goal:** Make the internal MVP safe to run continuously with tenant isolation, cost controls, release controls, and auditability.

**Exit gate satisfied:**
- Tenant isolation enforced and tested (P2-S1a through P2-S1d)
- Rate limits, quotas, RBAC, audit logs, and retention are working (P2-S2a, P2-S3a, P2-S4a, P2-S5a/b, P2-S6a)
- Deployment can roll forward and back through controlled automation (P2-S8a, P2-S8b)
- Performance baselines established (P2-S9a)

**Completed slices:** P2-S0 through P2-S9a.

### Phase 3 — Correlation And Service Operations (COMPLETE)

**Goal:** Turn isolated telemetry views into connected service operations workflows.

**Exit gate satisfied:**
- Operators can move between service, trace, log, metric, and deployment context without manual ID copying.
- Services are first-class entities (P3-S4, P3-S5, P3-S6, P3-S8, P3-S9, P3-S10)
- Dashboard artifacts are stable (P3-S12, P3-S12a, P3-S13)
- Schema Registry enables NLQ (P3-S14)
- Testcontainers harness established (P3-S15)
- UI renovation complete (UI-R1 through UI-R3)

**Completed slices:** P3-S0 through P3-S15, plus UI renovation gate and P8-S6 (NLQ layer).

---

## 14. Source Documents

- Finish-started closure record: `archived/plans/2026-05-09-finish-started-work-plan-rf0-complete.md`
- Roadmap scope: `spec/10-process.md §17`, `spec/13-risks-roadmap.md §24`
- Architecture decisions: `spec/adr/`
- Product and platform specs: `spec/`

---

## 15. Promotion Rules

Before implementing any item in this document:

1. Check `archived/plans/2026-05-09-finish-started-work-plan-rf0-complete.md` for closure notes from the started-work queue.
2. Write a detailed implementation plan in `docs/superpowers/plans/YYYY-MM-DD-<slice>.md`.
3. Include exact file paths, tests, rollback path, telemetry impact, auth/tenancy impact, data-retention impact, and ADR/spec sync decision.
4. Keep the PR below the repo's tiny-iteration size target unless the reviewer accepts a larger slice.
5. Update this document and `docs/agent-context.md` when a slice is promoted, completed, or deferred.

---

## 16. Sequencing Clarification

The old Phases 2-8 plan contains two sequencing models:

- Phase gates say to complete Phase 4 before broad Phase 5, complete operationally necessary Phase 4 before Phase 5, and prove retention/auth/release safety before Phase 6.
- The 2026-05-05 value-first reorder promotes selected user-visible slices earlier: SLOs, notifications, profiling, RUM, incidents, anomaly detection, synthetics, then warm retention.

Use this resolution:

- The started-work queue is archived; this document now controls the immediate queue.
- Cross-phase value-first slices may be promoted early only when their direct prerequisites are satisfied and the PR states which phase-gate risk remains.
- The formal external-v1 exit gate still requires the Phase 4 supportability items: restore, auth, runbooks, cost reporting, test evidence, and security review.

---

## 17. Roadmap Exit Gate

This document remains healthy when:

- No item is promoted without a detailed plan.
- Every promoted item names direct prerequisites and verification commands.
- Conditional enterprise/customer-driven items remain deferred until a concrete customer requirement exists.
- AI items remain advisory, explainable, auditable, and non-critical to platform correctness.
- Completed slices are marked in the detailed plan and summarized here.
