# UI Usability Remediation — Error Finding & Platform Monitoring

> **Status:** Active. Workstream plan. Source of findings: a full-surface UI review conducted
> 2026-06-26 using the Playwright visual-verification suite (`apps/frontend/e2e/visual.spec.ts`,
> 11 full-page screenshots covering Home, Services, Service Detail, Traces, Logs, Metrics,
> Infrastructure, Alerts & SLOs, Incidents, Change Events, Dashboards).
>
> **Scope:** This plan targets the two primary operator jobs-to-be-done — **finding errors fast**
> and **monitoring the platform at a glance** — not a visual redesign. It is feature-first and
> slice-able per the operating rules in `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`.
> Each slice changes one user-visible behavior, stays reviewer-sized, and ships with verification.
>
> **Cross-reference:** Tracked from the unified roadmap under §3.5 "UI Usability Remediation
> (cross-cutting)". The design-system modernization plan
> (`archived/plans/2026-06-18-frontend-design-system-modernization.md`) covers token/primitive
> foundations; this plan covers *operator-task usability* on top of those primitives.

---

## 1. How the Review Was Done

1. Installed frontend deps and ran the existing visual suite against the dev server with mocked API
   responses (`npm run test:visual`-equivalent). The suite already mocks representative data for
   every major page, so screenshots reflect populated, not empty, states.
2. Read each screenshot and traced the rendering code for any finding that looked like a defect
   rather than a styling choice, so the plan distinguishes **verified code issues** from
   **design/consistency observations**.
3. Findings are graded by operator impact on the two target jobs, then ordered into slices.

Re-running the review: `cd apps/frontend && npm install && npx playwright test e2e/visual.spec.ts`
writes PNGs to `apps/frontend/e2e/screenshots/` (git-ignored). In this environment the pinned
Playwright browser build differs from the pre-installed one; launch with
`executablePath: /opt/pw-browsers/chromium-1194/chrome-linux/chrome` via a throwaway config.

---

## 2. Findings

Severity legend: **P0** blocks the core job; **P1** materially slows it; **P2** polish / consistency.

### Verified code-level issues (traced to source)

| # | Sev | Finding | Evidence |
|---|-----|---------|----------|
| F1 | **P0** | **Volume/severity histograms have no time (x) or count (y) axis.** The chart draws stacked bars and three unlabeled gridlines only. An operator can see the *shape* of a spike but cannot read *when* it happened or *how many* events it represents — the core question in error finding. Affects Traces, Logs, and Service Detail (all consume the same component). | `src/components/ui/histogram.tsx` — renders `<rect>` bars + 3 gridlines (lines 147-190); no `<text>`/axis ticks; the only time hint is an `sr-only` paragraph (line 131). |
| F2 | **P1** | **Developer jargon "raw NLQ IR JSON" is exposed in every explorer filter placeholder.** End users are told they can paste "raw NLQ IR JSON" — an internal intermediate representation — in 8 filter inputs across Traces, Logs, Metrics, Services, Infrastructure, Topology, and live-tail. Confusing and leaks implementation detail. | `grep "raw NLQ IR"` → `src/components/shared/SignalExplorer.tsx:95`, `src/features/metrics/ServiceMetricsWorkspace.tsx:177`, `src/features/nlq/QueryFilterInput.tsx:100`, `src/components/LogLiveTail.tsx:70`, `src/pages/{InfrastructureInventoryPage,ProductAreaPage,ServiceTopologyPage,ServicesPage}.tsx`. |
| F3 | **P2** | **Global timezone control reads "ISO8601 Client TZ [ms]"** — engineer jargon occupying prime header space, shown to every user on every page as the default. | `src/lib/timeDisplay.tsx:13-17`. |

### Design / consistency observations (from screenshots)

| # | Sev | Finding |
|---|-----|---------|
| F4 | **P1** | **Outliers are not visually flagged in tables.** A 3000 ms trace duration sits in plain text next to a 50 ms one; a 620 ms P95 and 6.50% error rate read the same weight as healthy values. Error finding is outlier spotting — these need pre-attentive cues (color scale, inline bar, or threshold emphasis), not just sortability. (Health *badges* are color-coded; the numeric columns that drive triage are not.) |
| F5 | **P1** | **Stat cards carry no trend or threshold context.** "Avg P95 Latency 305ms", "Avg Error Rate 2.60%", "MTTR 90m" are bare numbers. For monitoring, the operator needs "is this normal?" — a sparkline, a delta-vs-previous-window, or a threshold color. |
| F6 | **P1** | **Brush-to-zoom on the histogram is undiscoverable.** Range selection exists (`onRangeSelect`) but the only affordance is an `sr-only` hint; sighted users get a crosshair cursor with no label. The platform's main time-narrowing interaction is hidden. |
| F7 | **P2** | **Summary stat-card row is inconsistent across explorers.** Present on Home, Services, Alerts, Incidents, Infrastructure, Metrics; absent on Traces, Logs, and Change Events — the three highest-frequency error-finding views, which would benefit most from an at-a-glance error/total count. |
| F8 | **P2** | **Primary/secondary button styling is inconsistent.** "Promote to dashboard" renders as a filled accent button while the adjacent "Apply query" — the actually-primary action — is a plain bordered button. No consistent action hierarchy across explorers. |
| F9 | **P2** | **Empty states consume prime real estate.** "No open incidents" and "No infrastructure entities" render as large centered blocks in the most valuable screen region, pushing real content down on otherwise-healthy systems. |
| F10 | **P2** | **No global quick-jump / command palette.** No keyboard-driven way to jump to a service, paste a trace ID, or hop to a page — a standard expectation for monitoring tools where speed-to-signal matters. |
| F11 | **P2** | **Dashboards page lacks a "New dashboard" affordance and card metadata.** Only "Import" is offered for creation; cards show panel count + name but no description, owner, or last-updated, and no preview. |
| F12 | **P2** | **Data freshness / live-ness is not surfaced.** Only the Logs view has a "Live" toggle. Elsewhere there is no "last updated" / auto-refresh indicator, so a stale monitoring view is indistinguishable from a current one. |

> **Checked and *not* a defect (recorded to prevent re-flagging):** Service Detail *does* render RED
> time-series via `TimeSeriesGraph` (`src/pages/ServiceDetailPage.tsx:413`); it only appeared chartless
> in the screenshot because that test mocks NLQ with empty data. The Logs histogram box also appeared
> blank in one screenshot due to mock/timing, but the component path is correct (`LogSearch.tsx:259-276`).
> Neither is a production bug; F1 (missing axes) is the real, persistent histogram issue.

---

## 3. Roadmap (sliced, ordered by operator impact)

Each slice is independently shippable. Follow the unified-roadmap operating rules: feature-based
dirs under `src/features/<domain>` / shared primitives, Base UI, Tailwind v4, accessibility tests
for new views, and update both this doc and `docs/agent-context.md` on completion.

### Slice 1 — Histogram axes & readable scale (fixes F1) — **P0, do first**
- Add an x-axis with 3-5 time ticks (using the existing `format` prop) and a y-axis max label to
  `src/components/ui/histogram.tsx`. Reserve bottom/left gutters in the SVG `viewBox`; render
  `<text>` ticks; keep bars/gridlines aligned.
- Keep it dependency-free (already hand-rolled SVG; no new chart lib).
- **Verify:** unit test in `histogram.test.tsx` asserting tick `<text>` count and label content for a
  known bucket set; visual-suite re-screenshot of Traces + Logs.
- **Rollback:** revert the component; bars-only render is unchanged.

### Slice 2 — Discoverable brush-to-zoom (fixes F6) — **P1**
- Add a visible affordance to the histogram header ("Drag to zoom" hint + a reset-zoom control when a
  custom range is active) and a hover tooltip showing bucket time + counts (data already in the
  `title` attr on each segment — promote it to a real tooltip).
- **Verify:** interaction test (pointer drag → `onRangeSelect` called with expected ms); a11y check
  that the hint is no longer `sr-only`-only.

### Slice 3 — De-jargon filter placeholders & timezone label (fixes F2, F3) — **P1, fast win**
- Replace the 8 "… or raw NLQ IR JSON" placeholders with plain-language examples only (keep the
  natural-language example, drop the IR mention). Power-user raw-IR entry can stay supported but
  unadvertised, or move behind a small "advanced" affordance.
- Soften the default timezone label (e.g. "Local time (ms)") in `src/lib/timeDisplay.tsx`; keep the
  precise ISO options available in the dropdown.
- **Verify:** grep shows zero user-facing "raw NLQ IR JSON"; existing explorer tests still pass.

### Slice 4 — Outlier emphasis in signal tables (fixes F4) — **P1**
- Introduce a shared cell renderer for latency/error-rate/duration columns that applies a threshold
  color ramp (and optionally an inline micro-bar) so slow/erroring rows pop. Apply to Traces
  (duration, status), Services & Service Detail (P95, error rate), Infrastructure (CPU/mem/disk already
  have bars — unify with the same scale), Logs (severity already colored — leave).
- Centralize thresholds so they are consistent and configurable.
- **Verify:** unit tests for the threshold→class mapping; visual re-screenshot of Traces + Services.

### Slice 5 — Trend/threshold context on stat cards (fixes F5) — **P1**
- Extend the shared stat-card component to optionally take a sparkline series and/or a
  delta-vs-previous-window and a threshold tone. Wire P95/error-rate/MTTR/avg-error cards on Home,
  Services, Incidents, Infrastructure.
- **Verify:** component test for sparkline + delta rendering; accessibility (numbers remain the
  accessible name, sparkline `aria-hidden`).

### Slice 6 — Explorer summary-row consistency (fixes F7) — **P2**
- Add the standard stat-card summary row (total / error / matched counts) to Traces, Logs, and
  Change Events, reusing the existing summary component used by Services/Infra.
- **Verify:** visual re-screenshot; a11y test for the three views.

### Slice 7 — Action hierarchy & empty-state compaction (fixes F8, F9) — **P2**
- Adopt one primary/secondary button convention from the design system; make "Apply query" primary
  and "Promote to dashboard"/"Export" secondary across explorers.
- Replace oversized empty blocks with compact inline empty states that don't dominate the viewport.
- **Verify:** visual re-screenshot of Home (incidents panel) + Service Detail.

### Slice 8 — Global quick-jump / command palette (fixes F10) — **P2**
- Add a `⌘K`/`Ctrl-K` palette: jump to page, search services, paste a trace ID → trace detail.
- **Verify:** interaction + a11y tests (focus trap, escape, screen-reader labels).

### Slice 9 — Dashboards create-affordance, card metadata, freshness (fixes F11, F12) — **P2**
- Add a "New dashboard" action and surface description/last-updated on cards.
- Add a shared "last updated / auto-refresh" indicator to monitoring views (Home, Services,
  Infrastructure, explorers) so staleness is visible.
- **Verify:** visual re-screenshot; tests for the freshness indicator states.

---

## 4. Sequencing

```
Slice 1 (histogram axes, P0)
  → Slice 3 (de-jargon, fast P1) ∥ Slice 2 (brush discoverability, P1)
  → Slice 4 (outlier emphasis) → Slice 5 (stat-card trends)
  → Slice 6 (summary rows) → Slice 7 (buttons + empty states)
  → Slice 8 (command palette) → Slice 9 (dashboards + freshness)
```

Slices 1-5 deliver the bulk of the error-finding/monitoring value; 6-9 are consistency and
quality-of-life. Slices 2, 4, 5, 6 share the histogram/stat-card/table primitives, so ordering them
together minimizes rework.

## 5. ADR / Spec Sync

- Mostly presentation-layer; no ADR change expected. If Slice 4 introduces configurable thresholds or
  Slice 8 adds a palette, note the UX in `spec/05-frontend.md` in the same PR (state "no ADR needed"
  otherwise).
- Update `docs/agent-context.md` if the shared stat-card / histogram / table-cell primitives change
  ownership or API.

## 6. Definition of Done (per slice)

1. One user-visible behavior changed; reviewer-sized.
2. Unit/interaction tests for the changed primitive; accessibility test for any new/changed view.
3. Visual-suite screenshot refreshed and eyeballed for the affected page(s).
4. This document's finding row checked off and the unified roadmap §3.5 entry updated.
5. Detailed slice notes appended here; move this plan to `archived/plans/` when all P0/P1 slices ship.
