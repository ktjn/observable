# Remaining Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve the rest of the post-Phase-3 roadmap after separating out already-started and already-scoped work.

**Architecture:** Keep this document as the long-horizon backlog. Promote only the next one or two items into detailed implementation plans after the finish-started-work plan is complete or explicitly paused.

**Tech Stack:** Rust, Axum, SQLx/PostgreSQL, ClickHouse, Redpanda, object storage, OpenFGA-style authorization, Kubernetes/Helm, React 19, Vite, Base UI, Tailwind CSS v4, Testcontainers, Docker Compose, Playwright.

---

## Source Documents

- Finish-started companion: `docs/superpowers/plans/2026-05-07-finish-started-work-plan.md`
- Historical active roadmap/reference: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Roadmap scope: `spec/10-process.md §17`, `spec/13-risks-roadmap.md §24`
- Specs and ADRs named by each promoted slice

## Promotion Rules

Before implementing any item in this document:

1. Check `2026-05-07-finish-started-work-plan.md` for blocking started work.
2. Write a detailed implementation plan in `docs/superpowers/plans/YYYY-MM-DD-<slice>.md`.
3. Include exact file paths, tests, rollback path, telemetry impact, auth/tenancy impact, data-retention impact, and ADR/spec sync decision.
4. Keep the PR below the repo's tiny-iteration size target unless the reviewer accepts a larger slice.
5. Update this document and `docs/agent-context.md` when a slice is promoted, completed, or deferred.

## Sequencing Clarification

The old Phases 2-8 plan contains two sequencing models:

- Phase gates say to complete Phase 4 before broad Phase 5, complete operationally necessary Phase 4 before Phase 5, and prove retention/auth/release safety before Phase 6.
- The 2026-05-05 value-first reorder promotes selected user-visible slices earlier: SLOs, notifications, profiling, RUM, incidents, anomaly detection, synthetics, then warm retention.

Use this resolution:

- The **finish-started plan** controls the immediate queue.
- Cross-phase value-first slices may be promoted early only when their direct prerequisites are satisfied and the PR states which phase-gate risk remains.
- The formal external-v1 exit gate still requires the Phase 4 supportability items: restore, auth, runbooks, cost reporting, test evidence, and security review.

---

## Next Promotion Candidates

Promote only one candidate at a time after the finish-started plan is current.

1. **P5-S2: Add one notification routing integration**
   - Promote after P4-S5 SLO burn-rate is complete.
   - First detailed plan should target one outbound channel, preferably webhook or Slack-compatible webhook, with retry and audit behavior.
   - Missing clarity: choose the first channel and decide whether secrets live in PostgreSQL config, environment variables, or an external secret store for this slice.

2. **P5-S1: Add incident timeline for one alert source**
   - Promote after P4-S5 or P5-S2, depending on whether the product wants triage history before outbound notifications.
   - First detailed plan should consume existing threshold or SLO alert firings and append immutable timeline events.
   - Missing clarity: decide whether incident objects are created automatically from alert firings or manually from the Alerts UI for the first slice.

3. **P6-S1: Add continuous profiling ingestion and one query path**
   - Promote only after P4-S1 warm retention/object storage is complete.
   - First detailed plan should store profile blobs in object storage and query one profile index path.
   - Missing clarity: choose the first profile format and whether the first ingestion path is OTLP-compatible, pprof upload, or an internal test endpoint.

4. **P6-S2: Add browser RUM for one web app**
   - Promote after identity/session behavior is clear enough to attribute frontend sessions safely.
   - First detailed plan should use standard OTel browser-compatible payloads plus explicit `session_id` attributes before custom RUM endpoints.
   - Missing clarity: choose whether the first app is Observable's own frontend or a synthetic demo app.

---

## Phase 4 Remaining Work

- [ ] **P4-S2: Add backup and restore drill for one dataset**
  - Promote after P4-S1 if the drill must include warm object-storage state; otherwise explicitly record why the first drill is hot-store-only.
  - Completion signal: one restore path is executed, timed, and documented with measured RPO/RTO.
  - Required evidence: restore command transcript or script output, rollback note, and runbook update.

- [ ] **P4-S3b: Add SCIM/SSO management if required by target v1 customers**
  - Conditional. Do not promote until a selected v1 customer requires automated user/group provisioning or multi-provider SSO management.
  - Completion signal: tenant admins can manage provisioning status and receive externally provisioned users/groups without manual database edits.
  - Required evidence: PostgreSQL Testcontainers tests for provision/update/deactivate/group mapping/idempotent replay, HTTP integration tests for tenant-admin-only paths, and frontend MSW/RTL tests.

- [ ] **P4-S4: Add fine-grained authorization for one protected resource**
  - Promote after OIDC/user-session foundations are stable.
  - Completion signal: one OpenFGA-style protected object has enforceable sharing semantics.
  - Missing clarity: choose the first protected resource. Recommended first resource: dashboard, because it is user-visible and naturally shareable.

- [ ] **P4-S6: Add production runbook set for one failure class**
  - Promote after at least one failure class has real alerts and rollback/restore mechanics.
  - Completion signal: an operator can execute triage, rollback, and restore steps without tribal knowledge.
  - Missing clarity: choose the first failure class. Recommended first class: ingest degraded or query degraded, because both map to existing smoke/perf signals.

- [ ] **P4-S7: Add tenant usage and cost report for one billing interval**
  - Promote after rate limits, cardinality observations, and retention movement are stable enough to explain usage.
  - Completion signal: operators can explain ingest volume, retained bytes, and query cost for one tenant over one interval.
  - Missing clarity: decide whether the first report is operator-only or tenant-admin-visible.

- [ ] **P4-S8: Run load, chaos, tenant-escape, and upgrade/rollback suites**
  - Promote after the specific surface being tested has stable local scripts.
  - Completion signal: production-readiness claims are backed by repeatable evidence with known failures recorded.
  - Missing clarity: choose the first suite. Recommended first suite: tenant-escape, because it protects external-v1 trust.

- [ ] **P4-S9: Complete boundary-focused security review**
  - Promote after OIDC, tenant filtering, and fine-grained authorization are stable enough to review.
  - Completion signal: auth, tenancy, query, and ingest boundaries have explicit review notes and blocking findings are tracked.
  - Missing clarity: decide whether review output lives in `spec/`, `docs/`, or a PR-attached checklist.

---

## Phase 5 Remaining Work

- [ ] **P5-S1: Add incident timeline for one alert source**
  - Direct prerequisite: at least one stable alert source, preferably P4-S5 burn-rate alerts.
  - Completion signal: one alert source can produce a durable timeline with source links.

- [ ] **P5-S2: Add one notification routing integration**
  - Direct prerequisite: at least one stable alert source.
  - Completion signal: one alert firing reaches one operator channel with retry, delivery status, and audit notes.

- [ ] **P5-S3: Add runbook workflow attachment to an alert or incident**
  - Direct prerequisite: P4-S6 runbook format or P5-S1 incident model.
  - Completion signal: an alert or incident can link to an executable or checkable runbook workflow.

- [ ] **P5-S4: Add topology-aware impact view for one incident**
  - Direct prerequisite: P3 service topology and P5-S1 incident timeline.
  - Completion signal: one incident shows impacted services or dependencies from existing topology data.

- [ ] **P5-S5: Add composite alert evaluation for one rule pair**
  - Direct prerequisite: threshold/SLO evaluator stability.
  - Completion signal: two existing signals can combine into one derived firing without duplicating evaluator architecture.

- [ ] **P5-S6: Add reliability reporting for one team/service scope**
  - Direct prerequisite: SLOs and incident/alert history.
  - Completion signal: one team or service gets a reliability report over a bounded interval.

---

## Phase 6 Remaining Work

- [ ] **P6-S1: Add continuous profiling ingestion and one query path**
  - Direct prerequisite: P4-S1 object-storage path.
  - Completion signal: one profile payload is stored, indexed, and queryable.

- [ ] **P6-S2: Add browser RUM for one web app**
  - Direct prerequisite: session and privacy attribution model clear enough for frontend telemetry.
  - Completion signal: one browser app emits route/API/Web Vitals data that appears in the platform.

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

---

## Phase 7 Remaining Work

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

---

## Phase 8 Remaining Work

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

- [ ] **P8-S7: Add PromQL compatibility facade for metrics**
  - Direct prerequisite: P8-S6 NLQ IR to VisualizationFrame path remains stable.
  - Completion signal: PromQL metrics expressions translate to the existing `NlqIr` path without a parallel execution engine.
  - Missing clarity: decide whether this remains optional or becomes required for v1 customer compatibility.

---

## Remaining Roadmap Exit Gate

This document remains healthy when:

- No item is promoted without a detailed plan.
- Every promoted item names direct prerequisites and verification commands.
- Conditional enterprise/customer-driven items remain deferred until a concrete customer requirement exists.
- AI items remain advisory, explainable, auditable, and non-critical to platform correctness.
- Completed slices are marked in the detailed plan and summarized here or in the historical Phases 2-8 reference.

