---
sessionId: session-260720-085047-1xzg
---

# Requirements

### Overview & Goals
Address the UI issues found during the screenshot-test visual review (see prior analysis). Goal: eliminate silent-blank states, fix a relative-time formatting bug, and tidy up visual inconsistencies, without changing any backend contracts.

### Scope
**In scope**
- `DataFreshness` negative-time bug (`Updated -1s ago`).
- Missing loading/empty/error states for: Logs histogram, Metrics chart, Service Detail Logs tab, Dashboard detail panels, Notification Channels tab.
- Visual polish: truncated placeholders/titles, unstyled Change Events type filter, ambiguous Topology view-toggle active state.
- Updating/adding Playwright visual & navigation specs to cover the new states.

**Out of scope**
- Backend/API changes (mock-data artifacts like the epoch `1970-01-01` timestamp are test-fixture issues, not noted as product bugs).
- New features (live tail already exists; error/exception grouping etc. are separate roadmap items from the prior feature-gap review).

### User Stories
- As a user viewing the Home/Services page, I want the "updated X ago" label to never show a negative number, so the UI looks trustworthy.
- As a user opening a dashboard whose panel data hasn't loaded yet (or failed), I want to see a clear loading/error message instead of an empty box.
- As a user on the Notification Channels tab, I want a clear error/timeout message if channels fail to load, instead of an infinite "Loading channels…" spinner.
- As a user filtering Change Events, I want the type filter to look consistent with the rest of the app's compact, bounded-width controls.
- As a user toggling between List/Topology views, I want the active view to be visually unambiguous.

### Functional Requirements
- `DataFreshness` must never render a negative duration; it should clamp to `0s ago` (or immediately recompute `now` when `dataUpdatedAt` changes) instead of using a stale `now` value captured at mount.
- Chart-bearing regions (Logs histogram, Metrics chart, Service Detail Logs tab) must show one of: skeleton loading, `EmptyState` (no data), or `ErrorState` (fetch failed) — never a bare empty box.
- `NotificationChannelsList` must surface `isError` from its query via `ErrorState` with a retry action, instead of only handling `isLoading`.
- Dashboard detail panel renderer (`DashboardDetailPage.tsx` inner panel component) must use `ErrorState`/`EmptyState` consistently instead of the current low-contrast `LoadingState` text for error/no-data cases, so a failed/empty panel is clearly visible rather than looking blank.
- The Change Events type `Select` must have a bounded width consistent with other filter controls on that page (matching the pattern already used elsewhere, e.g. Logs/Traces filter bars).
- Truncated text (Services/Infrastructure filter placeholders, Metrics "Selected Metric" panel title, trace waterfall service label) must not clip mid-word; apply `truncate` with a wider container or `title` tooltip attribute where the full text still needs to be discoverable.
- Topology List/Topology toggle must use a clearly distinguishable active-state style (e.g. filled/brand background) vs. the current similar-looking dark states.


# Technical Design

### Current Implementation
- `apps/frontend/src/components/ui/data-freshness.tsx` computes `now` via `useState(() => Date.now())` at mount and only updates every 30s via `setInterval`; if `dataUpdatedAt` arrives after mount (typical React Query flow), `now - dataUpdatedAt` can be negative until the first tick.
- Reusable state components already exist and are used inconsistently: `components/ui/empty-state.tsx`, `components/ui/error-state.tsx`, `components/ui/loading-state.tsx`. `LogSearch.tsx` already demonstrates the target pattern (histogram skeleton + `ErrorState`/`EmptyState` for the table), so it is the reference implementation for other pages.
- `features/alerts/NotificationChannelsList.tsx` destructures only `{ data, isLoading }` from its `useQuery`, has no `isError` handling — an errored fetch looks identical to a stuck loading state.
- `pages/DashboardDetailPage.tsx` inner panel-fetch component (~line 689-716) handles loading/error/no-data with plain `<LoadingState className="text-[var(--bad)]">` text rather than `ErrorState`/`EmptyState`, making failures easy to miss (matches the screenshot finding of an apparently panel-less dashboard).
- `features/changeEvents/ChangeEventsPage.tsx` uses the shared `Select` component, which defaults to `w-full`; without a width-limiting class on this call site it stretches full-width unlike other filter bars.
- `components/topology/TopologyMap.tsx` / the surrounding services page toggle buttons render both List/Topology states in visually similar dark styling.

### Key Decisions
- **Reuse existing `EmptyState`/`ErrorState`/`LoadingState` components everywhere** rather than inventing new ones — `LogSearch.tsx`'s histogram/table handling is the pattern to replicate for Metrics chart, Service Detail Logs tab, and Dashboard panels.
- **Fix `DataFreshness` by removing the stale-`now` race** rather than clamping only visually: recompute `now` synchronously whenever `dataUpdatedAt` changes (e.g., via `useEffect` on `[dataUpdatedAt]` or deriving from `Date.now()` directly, with a `Math.max(0, diffSec)` guard as defense in depth).
- **No new component library additions** — all fixes are localized to existing page/feature files plus one shared component.

### Proposed Changes
1. `data-freshness.tsx`: add `Math.max(0, diffSec)` clamp and re-sync `now` on `dataUpdatedAt` change via an additional `useEffect`.
2. `NotificationChannelsList.tsx`: destructure `isError`, `refetch` from `useQuery`; render `ErrorState` with a "Retry" button wired to `refetch` when `isError` is true, before the `isLoading`/empty/list branches.
3. `DashboardDetailPage.tsx` inner panel component: replace the two `LoadingState` branches for the error/no-data cases with `ErrorState`/`EmptyState` (keep `LoadingState` only for the genuine in-flight case), matching the visual weight used elsewhere.
4. Metrics chart (`features/metrics/ServiceMetricsWorkspace.tsx`) and Service Detail Logs tab: audit their query result branches and add `EmptyState`/`ErrorState` for empty-data/error cases the same way `LogSearch.tsx` does for its histogram, so a large chart region never renders as a bare gray box.
5. `ChangeEventsPage.tsx`: add a max-width class to the type `Select` (e.g. `className="max-w-[180px]"`) to match the sibling text filter's `min-w-[180px]` sizing convention already on that line.
6. Truncation fixes: add `truncate`/`title` attributes to the Services/Infrastructure filter inputs' placeholder-bearing elements, the Metrics "Selected Metric" panel title, and the trace waterfall service label container, widening the flex/grid track where feasible instead of just truncating.
7. Topology toggle: give the active button an explicit `bg-[var(--brand)] text-[var(--bg)]` (or equivalent already-used "active" treatment from another toggle in the codebase) so it's unmistakably different from the inactive dark style.

### File Structure
- Modified: `apps/frontend/src/components/ui/data-freshness.tsx`
- Modified: `apps/frontend/src/features/alerts/NotificationChannelsList.tsx`
- Modified: `apps/frontend/src/pages/DashboardDetailPage.tsx`
- Modified: `apps/frontend/src/features/metrics/ServiceMetricsWorkspace.tsx`
- Modified: `apps/frontend/src/pages/ServiceDetailPage.tsx` (Logs tab)
- Modified: `apps/frontend/src/features/changeEvents/ChangeEventsPage.tsx`
- Modified: `apps/frontend/src/pages/ServicesPage.tsx`, `apps/frontend/src/pages/InfrastructureInventoryPage.tsx` (truncation)
- Modified: `apps/frontend/src/pages/ServiceTopologyPage.tsx` and/or `apps/frontend/src/components/topology/TopologyMap.tsx` (toggle styling)
- Modified/added test coverage: `apps/frontend/src/components/ui/data-freshness.test.tsx`, plus updates to `apps/frontend/e2e/visual.spec.ts` / `navigation.spec.ts` mocks to exercise error/empty states.


# Testing

### Validation Approach
- Unit tests colocated with each changed component (`*.test.tsx`), following the existing pattern (e.g. `data-freshness.test.tsx` already has time-mocking tests to extend).
- Re-run `npm run test:visual` in `apps/frontend` after the change and manually review the regenerated screenshots to confirm: no blank chart boxes, notification-channel error state renders, dashboard panel error/empty states are visible and styled, Change Events filter is bounded-width, topology toggle active state is distinguishable.
- Run `bash scripts/local-ci.sh --skip-docker` (frontend-only changes) before pushing, per repo mandate.

### Key Scenarios
- `DataFreshness` renders `Updated 0s ago` (not negative) when `dataUpdatedAt` is set to a timestamp very close to "now", including the race where `dataUpdatedAt` updates after initial mount.
- Notification Channels tab: mock a failing `listNotificationChannels` request → `ErrorState` with retry renders; retry re-triggers the query.
- Dashboard detail: mock a panel-data endpoint that 500s → panel shows `ErrorState`; mock an endpoint returning no series → panel shows `EmptyState`.
- Metrics chart / Service Detail Logs tab: mock empty-array and error responses → verify `EmptyState`/`ErrorState` appear instead of a blank box.
- Change Events and Topology pages: visual screenshot check only (no pixel-diff), confirming layout/styling by eye per the existing visual-suite convention.

### Edge Cases
- `DataFreshness` with `dataUpdatedAt` in the future (clock skew) should still clamp to `0s ago`, not a negative number.
- Notification channel retry while a create/delete mutation is in flight should not race/duplicate requests.
- Dashboard panels that are still legitimately loading (not errored/empty) must keep showing the existing `LoadingState`, not flash an `EmptyState` first.


# Delivery Steps

### ✓ Step 1: Fix the negative relative-time formatter bug
The `Updated Xs ago` label never shows a negative value anywhere it appears (Home, Services).
- Update `apps/frontend/src/components/ui/data-freshness.tsx` to clamp `diffSec` to a minimum of 0.
- Add a `useEffect` keyed on `dataUpdatedAt` so `now` re-syncs immediately when new data arrives, closing the stale-mount race.
- Extend `data-freshness.test.tsx` with a regression test simulating `dataUpdatedAt` set after mount / in the near future.
- Re-run `npm run test:visual` and confirm `home.png` / `services.png` no longer show a negative value.

### ✓ Step 2: Add error/empty states to Notification Channels and Dashboard panels
Notification Channels and Dashboard detail panels show clear error/empty feedback instead of hanging or looking blank.
- In `features/alerts/NotificationChannelsList.tsx`, destructure `isError`/`refetch` from the channels query and render `ErrorState` (with a retry button calling `refetch`) ahead of the loading/empty/list branches.
- In `pages/DashboardDetailPage.tsx`'s inner panel-fetch component, replace the plain-text `LoadingState` error/no-data branches with `ErrorState`/`EmptyState`, keeping `LoadingState` only for the true in-flight case.
- Add/update tests covering the new branches (mocked failing/empty query results) for both components.
- Update `apps/frontend/e2e/navigation.spec.ts`'s dashboard-detail and alerts-channels-tab mocks so the visual suite exercises the new states, then re-run `npm run test:visual` and review `nav-dashboard-detail.png` / `nav-alerts-channels-tab.png`.

### ✓ Step 3: Add empty/error states to Logs and Metrics chart regions
Logs page, Metrics page, and the Service Detail Logs tab never render a bare gray box for chart regions.
- Audit `features/metrics/ServiceMetricsWorkspace.tsx` and `pages/ServiceDetailPage.tsx` (Logs tab) query-result branches, adding `EmptyState`/`ErrorState` handling mirroring the existing pattern already used for the Logs histogram in `pages/LogSearch.tsx`.
- Ensure the Logs histogram's existing `LoadingState`/`ErrorState` fallback also treats an all-zero/empty bucket response as an explicit empty state rather than a blank chart.
- Add/extend component tests for the new empty/error branches.
- Re-run `npm run test:visual`, reviewing `logs.png`, `metrics.png`, and `nav-service-detail.png` for the fix.

### ✓ Step 4: Visual polish: truncation, filter width, and toggle contrast
Remaining cosmetic issues from the review are resolved across Services, Infrastructure, Metrics, Traces, Change Events, and Topology.
- Fix clipped filter placeholders in `pages/ServicesPage.tsx` and `pages/InfrastructureInventoryPage.tsx` by widening the input/container or adding `truncate` with a `title` attribute.
- Fix the clipped "Selected Metric" panel title in the Metrics workspace and the clipped waterfall service label in the trace detail view the same way.
- Add a bounded max-width class to the type `Select` in `features/changeEvents/ChangeEventsPage.tsx` to match the sizing convention of the adjacent text filter.
- Give the active List/Topology toggle button an explicit brand-colored active style in `pages/ServiceTopologyPage.tsx` (or wherever the toggle lives) so the active state is unambiguous.
- Re-run `npm run test:visual` and review `services.png`, `infrastructure.png`, `metrics.png`, `nav-trace-detail.png`, `change-events.png`, and `nav-services-topology.png` to confirm all fixes.