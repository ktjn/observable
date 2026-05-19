# TraceDetail Uplift Design

**Date:** 2026-05-15
**Status:** Approved

---

## Problem

`TraceDetailPage` and `TraceDetail` predate the design-system uplift series. They use a raw `div.grid` wrapper, an inline `h1`, and bare `<p>Loading…</p>` / `<p>Not found</p>` error states. There are no MetricCards, no Panel wrappers, and no service color legend — making the trace detail page visually inconsistent with every other uplifted page.

---

## Goals

- Apply `page-stack` / `page-header` layout to match the rest of the uplifted pages.
- Fix bare loading and error states in `TraceDetailPage`.
- Add a MetricCard summary row (spans, duration, services, errors).
- Add a service color legend so operators can map waterfall bar colors to service names at a glance.
- Wrap the waterfall and correlated-logs sections in `Panel` components.

## Out of Scope

- Split-view layout (waterfall left / correlated logs right with synchronized time cursor) — tracked separately per spec §9.4.
- Any changes to `TraceSearch`, `TraceContextSidebar`, or `SignalExplorer`.
- New API calls or data changes.

---

## Architecture

All changes are local to three files:
- `apps/frontend/src/pages/TraceDetailPage.tsx` — minimal fix for loading/error states.
- `apps/frontend/src/pages/TraceDetail.tsx` — layout uplift, MetricCards, legend, Panel wrappers.
- `apps/frontend/src/components/LogCorrelatedList.tsx` — remove the `<h3>` label (moved to Panel title in parent).

No new components are introduced. The uplift reuses existing primitives (`MetricCard`, `Panel`, `LoadingState`, `EmptyState`, `Link`).

---

## Component Changes

### `TraceDetailPage.tsx`

Replace bare paragraph tags with design-system components:

```tsx
// Before
if (isLoading) return <p>Loading…</p>;
if (!data) return <p>Not found</p>;

// After
if (isLoading) return <LoadingState>Loading trace…</LoadingState>;
if (!data) return <EmptyState title="Trace not found." />;
```

Imports added: `LoadingState`, `EmptyState`.

---

### `TraceDetail.tsx`

#### 1. Wrapper

```
div.grid gap-4  →  section.page-stack
```

#### 2. Page header

Replaces the current raw `h1` + `<p>` subtitle block:

```tsx
<div className="page-header">
  <div>
    <div className="text-xs font-bold uppercase text-[var(--muted)]">Traces</div>
    <h1>{traceId.substring(0, 16)}…</h1>
  </div>
  <Link to="/traces" className="secondary-link">Back to traces</Link>
</div>
```

The span-count and total-duration line moves into the MetricCard row below.

#### 3. MetricCard row

Four cards in a 4-column responsive grid:

| Label | Value | Tone |
|---|---|---|
| Total Spans | `spans.length` | info |
| Duration | `{totalMs.toFixed(2)}ms` | info |
| Services | count of unique `service_name` values | info |
| Errors | count of spans where `status_code === "ERROR"` | bad if > 0, good if 0 |

```tsx
<div className="grid grid-cols-4 gap-3 max-[700px]:grid-cols-2">
  <MetricCard label="Total Spans" value={spans.length} tone="info" />
  <MetricCard label="Duration" value={`${totalMs.toFixed(2)}ms`} tone="info" />
  <MetricCard label="Services" value={uniqueServiceCount} tone="info" />
  <MetricCard label="Errors" value={errorCount} tone={errorCount > 0 ? "bad" : "good"} />
</div>
```

#### 4. Service color legend

A compact flex row below the MetricCards and above the infrastructure pills. One pill per unique service, ordered by first appearance in the spans array. Each pill shows a small filled circle (4×4px, inline-block) in the service color, followed by the service name.

```tsx
<div className="flex flex-wrap gap-x-4 gap-y-1" aria-label="Service color legend">
  {uniqueServices.map((name) => (
    <span key={name} className="flex items-center gap-1.5 text-xs text-[var(--muted)]">
      <span
        aria-hidden="true"
        className="inline-block h-2 w-2 rounded-full shrink-0"
        style={{ background: serviceColor(name) }}
      />
      {name}
    </span>
  ))}
</div>
```

`uniqueServices` is derived once via `[...new Set(spans.map(s => s.service_name))]`.

#### 5. Infrastructure pills

Unchanged — stays as the existing `div aria-label="Infrastructure"` flex row with link pills.

#### 6. Waterfall Panel

Wrap the `TimeRuler` + span rows + `SpanContextPanel` in a `Panel`:

```tsx
<Panel eyebrow="Waterfall" title="Spans">
  <div className="overflow-x-auto">
    <TimeRuler totalMs={totalMs} />
    <div className="flex items-start gap-3 max-[900px]:flex-col">
      <div className="flex-1 min-w-0">
        {/* span rows */}
      </div>
      {selectedSpan && (
        <SpanContextPanel ... />
      )}
    </div>
  </div>
</Panel>
```

#### 7. Correlated logs Panel

Wrap `LogCorrelatedList` in a `Panel`:

```tsx
<Panel eyebrow="Correlation" title="Correlated Logs">
  <LogCorrelatedList traceId={traceId} spanId={selectedSpanId} />
</Panel>
```

Remove the `<h3>` inside `LogCorrelatedList` (it duplicates the Panel title). The dynamic label (`"Exact span logs…"` vs `"Trace-correlated logs"`) moves to the Panel `title` prop, passed down via a new `label` prop on `LogCorrelatedList`, or simply inlined in the parent.

**Preferred approach:** Keep `LogCorrelatedList` unchanged; pass the dynamic title to the wrapping `Panel` from `TraceDetail`:

```tsx
const logPanelTitle = selectedSpanId
  ? `Exact span logs (${selectedSpanId.substring(0, 8)}…) and trace-level logs`
  : "Trace-correlated logs";

<Panel eyebrow="Correlation" title={logPanelTitle}>
  <LogCorrelatedList traceId={traceId} spanId={selectedSpanId} />
</Panel>
```

Remove the `<h3>` from `LogCorrelatedList` (it becomes redundant).

#### 8. `SpanContextPanel`

No changes. Already uses CSS vars and the correct structure.

---

## File Map

| File | Action |
|---|---|
| `apps/frontend/src/pages/TraceDetailPage.tsx` | Modify — replace bare `<p>` states with `LoadingState` / `EmptyState` |
| `apps/frontend/src/pages/TraceDetail.tsx` | Modify — page-stack wrapper, page-header, MetricCards, service legend, Panel wrappers, remove `<h3>` from `LogCorrelatedList` call |
| `apps/frontend/src/components/LogCorrelatedList.tsx` | Modify — remove the `<h3>` label (moved to Panel title in parent) |

---

## Testing

Existing tests in `TraceDetail.test.tsx` and `TraceDetail.renovation.test.tsx` cover waterfall rendering and span selection. No new tests are required for the layout changes, but the test for `LogCorrelatedList` heading text (`"Trace-correlated logs"`) will need updating to reflect the heading moving into the parent Panel.

Check: `cd apps/frontend && npx vitest run` — full suite must pass.
