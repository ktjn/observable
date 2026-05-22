# Reliability Reporting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a service-scoped reliability report over a bounded interval so operators can see incident load, SLO health, and recent deployment context from one place.

**Architecture:** Expose a new query-api endpoint that aggregates Postgres control-plane data for a single service and optional environment/time window. Render the report as a new service-detail tab that reuses the existing service context, global time range, and shared UI primitives instead of inventing a separate reporting surface.

**Tech Stack:** Rust (`axum`, `sqlx`, `serde`), PostgreSQL Testcontainers integration, React 19, TanStack Router, TanStack Query, Vitest, RTL, MSW-style fetch mocks.

---

### Task 1: Backend reliability report endpoint

**Files:**
- Create: `services/query-api/src/reliability.rs`
- Modify: `services/query-api/src/main.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Write the failing HTTP integration test**

Add a report test that seeds one service with:
- two incidents inside the requested interval,
- one incident for a different service that must not appear,
- at least one SLO definition for the target service,
- at least one deployment marker for the target service.

Assert `GET /v1/services/{service_name}/reliability-report?from=...&to=...` returns:
- the target service name,
- the requested interval,
- incident counts that only reflect the target service,
- a non-empty incident list,
- an SLO summary scoped to the target service,
- a deployment summary scoped to the target service.

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test -p query-api --test http_api_integration reliability_report -- --nocapture`
Expected: fail because the route and handler do not exist yet.

- [ ] **Step 3: Implement the minimal endpoint**

Create `services/query-api/src/reliability.rs` with:
- a `ReliabilityReportQuery` that accepts `from`, `to`, and optional `environment`,
- a `ReliabilityReportResponse` that includes the service name, window, incident summary, SLO summary, deployment summary, and recent incident rows,
- a handler that filters by `tenant_id`, `service_name`, and interval bounds,
- SQL that uses `incidents.service_name`, `slo_definitions.service_name`, and `deployment_markers.service_name` so the report is service-scoped without a schema change.

Wire the handler into `services/query-api/src/main.rs` at:
- `GET /v1/services/{service_name}/reliability-report`

- [ ] **Step 4: Run the backend test again**

Run: `cargo test -p query-api --test http_api_integration reliability_report -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit the backend slice**

```bash
git add services/query-api/src/reliability.rs services/query-api/src/main.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(query-api): add service reliability report endpoint"
```

### Task 2: Frontend reliability report tab

**Files:**
- Create: `apps/frontend/src/api/reliability.ts`
- Create: `apps/frontend/src/features/services/ServiceReliabilityTab.tsx`
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`
- Modify: `apps/frontend/src/router.ts`
- Modify: `apps/frontend/src/App.test.tsx`
- Create: `apps/frontend/src/features/services/ServiceReliabilityTab.test.tsx`

- [ ] **Step 1: Write the failing UI test**

Add a focused Vitest test for `ServiceReliabilityTab` that mocks:
- the new reliability-report endpoint,
- the global date range,
- the tenant context.

Assert the tab renders:
- summary cards for incidents, open incidents, SLOs, and deployments,
- a recent-incidents table,
- the service name and interval label,
- an empty state when the API returns no data.

- [ ] **Step 2: Run the test and confirm it fails**

Run: `npm test -- --run src/features/services/ServiceReliabilityTab.test.tsx`
Expected: fail because the API client and tab do not exist yet.

- [ ] **Step 3: Implement the tab and API client**

Add `apps/frontend/src/api/reliability.ts` with a typed `getServiceReliabilityReport(tenantId, serviceName, params)` helper.

Add `apps/frontend/src/features/services/ServiceReliabilityTab.tsx` that:
- uses `useTenantContext()` and `useGlobalDateRange()`,
- fetches the report through React Query,
- reuses `MetricCard`, `Panel`, `Badge`, `EmptyState`, and table styling already used in alert/incident views,
- keeps the tab locked to the current service context.

Update `apps/frontend/src/pages/ServiceDetailPage.tsx` and `apps/frontend/src/router.ts` so `/services/$serviceId/reliability` becomes a first-class service tab.

- [ ] **Step 4: Run the frontend tests again**

Run: `npm test -- --run src/features/services/ServiceReliabilityTab.test.tsx src/App.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit the frontend slice**

```bash
git add apps/frontend/src/api/reliability.ts apps/frontend/src/features/services/ServiceReliabilityTab.tsx apps/frontend/src/features/services/ServiceReliabilityTab.test.tsx apps/frontend/src/pages/ServiceDetailPage.tsx apps/frontend/src/router.ts apps/frontend/src/App.test.tsx
git commit -m "feat(frontend): add service reliability report tab"
```

### Task 3: Plan, docs, and verification

**Files:**
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Modify: `docs/agent-context.md`
- Modify: `docs/superpowers/plans/2026-05-22-p5-s6-reliability-reporting.md`

- [ ] **Step 1: Mark the roadmap slice complete**

Update the P5-S6 checkbox in `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` and record the final slice summary there.

- [ ] **Step 2: Update agent context**

Record the new reliability report route and any new gotchas in `docs/agent-context.md` so future agents know where the service report lives and how it is scoped.

- [ ] **Step 3: Run the required verification**

Run:
- `cargo fmt --all`
- `cargo test -p query-api --test http_api_integration reliability_report -- --nocapture`
- `npm test -- --run src/features/services/ServiceReliabilityTab.test.tsx src/App.test.tsx`
- `bash scripts/local-ci.sh`

Expected:
- Rust formatting passes,
- the focused backend and frontend tests pass,
- local CI passes before the branch is pushed.

- [ ] **Step 4: Archive the finished detailed plan**

Move this file to `archived/plans/` once the slice is complete and update any active links that pointed at it.

- [ ] **Step 5: Final commit**

```bash
git add docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md docs/agent-context.md docs/superpowers/plans/2026-05-22-p5-s6-reliability-reporting.md
git commit -m "docs: track service reliability reporting slice"
```
