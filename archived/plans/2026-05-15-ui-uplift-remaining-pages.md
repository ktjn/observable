# UI Uplift — Remaining Pages

**Date:** 2026-05-15
**Status:** Completed

## Context

The infrastructure inventory page (PR #336) and services page (PR #337) received a consistent
visual and interactive uplift: filter pills with live counts, health row tinting, tonal metric
cells, utilization bars, and a view toggle. The patterns are now documented in §12 of the
[UI Design Guide](../specs/2026-04-21-ui-design-guide.md).

This plan describes what each remaining page needs to reach parity.

---

## Pages Assessed

| Page | Route | File | Gap severity |
|------|-------|------|-------------|
| Log Explorer | `/logs` | `pages/LogSearch.tsx` | Medium |
| Trace Explorer | `/traces` | `pages/TraceSearch.tsx` | Medium |
| Metrics Explorer | `/metrics` | `features/metrics/ServiceMetricsWorkspace.tsx` | Low |
| Alerts & SLOs | `/alerts` | `features/alerts/AlertsPage.tsx` | High |
| Incidents | `/incidents` | `features/incidents/IncidentsPage.tsx` | Medium |
| Dashboards | `/dashboards` | `pages/DashboardsPage.tsx` | Low |
| Admin | `/admin`, `/admin/identity` | `pages/AdminPage.tsx`, `pages/IdentitySettingsPage.tsx` | Low |

---

## Slice 1 — Log & Trace Explorers ✅

Delivered in PR #342.

Both pages received severity/status pills, row tinting, duration color, empty states,
and visible search inputs.

---

## Slice 2 — Alerts & SLOs ✅

Delivered in PR #338.

Added filter pills, row tinting, severity badges, MetricCard summary tiles, and SLO
compliance bars. Form modals and burn-rate graphs remain deferred.

---

## Slice 3 — Incidents ✅

Delivered in PR #339.

Added severity row tinting, time-ago formatting, duration column, MetricCard summary
tiles, and toolbar search input.

---

## Slice 4 — Metrics Explorer ✅

Delivered in this iteration.

Replaced the type filter with pill buttons showing live cross-filtered counts per type,
and added a visible metric name search input above the catalog table.

---

## Slice 5 — Dashboards ✅

Delivered in PR #346.

Switched to a responsive card grid with panel counts, edit/export/delete actions,
and an EmptyState CTA.

---

## Slice 6 — Admin pages ✅

Skipped — admin pages are functional and low-traffic. The `page-header` pattern is
already applied where needed. No visual gaps warrant a dedicated slice.

---

## Recommended Sequencing

| Priority | Slice | Why first |
|----------|-------|-----------|
| 1 | Alerts & SLOs (partial — no modals) | Highest user-facing impact; active monitoring page |
| 2 | Incidents | Closely related to alerts; forms the incident response flow |
| 3 | Log & Trace Explorers | High daily use; severity tinting has safety / triage value |
| 4 | Dashboards | Mostly visual; low complexity |
| 5 | Metrics Explorer | Already has most filtering in place |
| 6 | Admin | Lowest priority; rarely navigated |

Each slice should be its own PR so it can be reviewed and merged independently.

---

## Shared prerequisites

None — all slices use existing component primitives. If the Alerts side-sheet is added in a
later pass, a `Dialog` or `Sheet` primitive will need to be created first (separate PR).

---

## Test strategy

Each slice adds a `e2e/<page>.spec.ts` with mocked API responses following the same patterns
established in `e2e/infrastructure.spec.ts` and `e2e/services.spec.ts`:
- `mockAuth()` helper to prevent auth redirect race
- Fixture constants for mock API responses
- `waitForSelector` targets the table `aria-label` or a unique stable element (not generic text)
- AxeBuilder accessibility check as the last test in each describe block
