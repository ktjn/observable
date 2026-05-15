# UI Uplift — Remaining Pages

**Date:** 2026-05-15
**Status:** Active

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

## Slice 1 — Log & Trace Explorers (medium effort, high visibility)

Both pages are largely parallel in structure: NLQ filter input, facet sidebar, result table
with severity / status coloring. Treat them as a single slice.

### Log Explorer gaps

- **Severity row tinting:** Log rows with severity `error` or `critical` already show a
  Badge but have no left-border accent. Add `border-l-2 border-l-[var(--bad)]` on error/critical
  rows and `border-l-2 border-l-[var(--warn)]` on warn rows (same pattern as infra/services).
- **Severity pill bar:** The facet sidebar hides severity breakdown. Add a compact pill row
  above the results table showing severity counts (Error / Warn / Info / Debug) with live
  cross-filtered counts — same design as the health pills in §12.1 of the design guide.
- **Search input visibility:** A search input should be part of the primary toolbar (not buried
  in the NLQ modal) for free-text log message filtering.
- **Empty state:** The "no results" state is a plain text `signal-empty` div. Upgrade to
  `EmptyState` with a helpful message and suggestion to widen the time range.

### Trace Explorer gaps

- **Status row tinting:** Traces with `status = "error"` should have the red left-border
  accent. Traces with high latency (e.g., > 2s) get an orange accent (`var(--warn)`).
- **Status pills:** Add a pill bar: All / OK / Error — with counts.
- **Latency tonal color:** The duration column should use tonal coloring (same thresholds
  as P95 in services: < 100ms green, 100–500ms orange, > 500ms red).
- **Empty state:** Same upgrade as logs.

### Deliverables

- Edit `LogSearch.tsx` and the `LogResultsTable` component
- Edit `TraceSearch.tsx` and the `TraceResultsTable` component
- Playwright tests in `e2e/logs.spec.ts` and `e2e/traces.spec.ts`

---

## Slice 2 — Alerts & SLOs (high effort, high priority)

`AlertsPage.tsx` is the most complex page (554 lines). It has three tabs:
1. Alert Rules — list of rules with silence/delete actions + a creation form
2. SLOs — list of SLO definitions + a creation form
3. Notification Channels — list of channels

### Gaps

- **Alert rule row tinting:** Active (firing) rules should have `border-l-2 border-l-[var(--bad)]`.
  Silenced rules: muted / dimmed. No-alert rows: no border.
- **Severity badge colors:** The `severity` column already uses badges but the tones should
  map: critical → bad, warning → warn, info → neutral/info.
- **Summary metric cards:** Above the tab list, add 4 `MetricCard` tiles:
  - Total rules / Firing rules / Silenced rules / SLOs
- **SLO compliance bar:** Each SLO row should show a thin progress bar (similar to
  `UtilizationBar`) for the compliance percentage: green ≥ 99.9%, orange ≥ 95%, red < 95%.
- **Filter pills on Alert Rules tab:** All / Firing / Silenced pill filter.
- **Creation forms:** Move the inline form collapse/expand (`isCreating`) to a dedicated
  side sheet or modal (see design guide §5 for panel pattern). The inline expanding form
  clutters the table view. This is a larger UX change — see the deferred section below.

### Deliverables

- Edit `features/alerts/AlertsPage.tsx`
- Playwright tests in `e2e/alerts.spec.ts`

### Deferred (needs design decision)

- Side-sheet or modal for rule/SLO creation forms — requires a `Dialog` / `Sheet` primitive
  that does not yet exist in `components/ui/`. Defer until the primitive is added.
- SLO burn-rate graph per SLO — requires backend historical burn-rate endpoint.

---

## Slice 3 — Incidents (medium effort)

`IncidentsPage.tsx` already has a status tab bar (Tabs component). Gaps are minor:

- **Severity row tinting:** critical → bad left border, warning → warn left border.
- **Time ago formatting:** The `started_at` column shows ISO timestamps. Format as "2h ago",
  "3d ago" using `formatTimestamp` / `useTimeDisplay` (already used in infrastructure).
- **Duration column:** Add a column showing how long the incident has been open
  (`started_at` → now for triggered/acknowledged; `resolved_at - started_at` for resolved).
- **Summary metric cards:** 3 MetricCards above the tabs: Triggered / Acknowledged / Resolved.
- **Search input:** Add a search input in the toolbar-row to filter by incident title / service.

### Deliverables

- Edit `features/incidents/IncidentsPage.tsx`
- Playwright tests in `e2e/incidents.spec.ts`

---

## Slice 4 — Metrics Explorer (low effort)

`ServiceMetricsWorkspace.tsx` is in reasonable shape. Minor gaps:

- **Metric type filter pills:** The "type" dropdown (`counter`, `gauge`, `histogram`) is a
  `<select>`. Replace with pills following §12.1 — shows count per type.
- **Row tinting:** Not applicable — the metrics catalog table is not health-coded.
- **Metric name search:** Already has a text filter; confirm it is a visible `<input>` (it is
  in `filters.name`). Verify it is rendered as a proper `<input>` with placeholder, not buried.

### Deliverables

- Edit `features/metrics/ServiceMetricsWorkspace.tsx`
- No new test file needed — existing coverage is sufficient for this minor change.

---

## Slice 5 — Dashboards (low effort, mostly visual)

`DashboardsPage.tsx` renders dashboards as table rows. Switch to a card grid:

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Dashboard A  │  │ Dashboard B  │  │  + New       │
│ 3 panels     │  │ 7 panels     │  │              │
│ [Edit] [↓]   │  │ [Edit] [↓]   │  │              │
└──────────────┘  └──────────────┘  └──────────────┘
```

- Grid of 3 columns (responsive → 2 → 1)
- Each card: name, panel count, edit link, export button, delete button
- Import file input stays in the toolbar area
- Empty state uses `EmptyState` component with a "Create your first dashboard" CTA

### Deliverables

- Edit `pages/DashboardsPage.tsx`

---

## Slice 6 — Admin pages (low effort, low priority)

Admin pages are functional but basic. Minor improvements only:

- Apply `page-header` pattern consistently (eyebrow label + h1)
- Use `MetricCard` and `Panel` primitives where raw `<div>` blocks exist
- No filter pills or row tinting needed — admin tables are small

No dedicated Playwright tests needed; existing coverage is sufficient.

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
