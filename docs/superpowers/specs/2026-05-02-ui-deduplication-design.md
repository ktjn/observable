# UI Deduplication Design

**Date:** 2026-05-02  
**Status:** Approved  
**Scope:** Signal explorer unification, unified log-list component, directory structure fix

---

## Goals

1. **Maintainability** — eliminate divergence risk where a bug fixed in one explorer silently remains in another.
2. **Developer velocity** — adding a new signal type (e.g., Profiles) should require ~20 lines of thin wrapper code, not a full page reimplementation.

---

## 1. Directory Structure Fix

**Prerequisite — no logic changes.**

Move the two result table components into their proper location per the AGENTS.md mandate (`features/**/components/`):

```
features/signals/LogResultsTable.tsx   →  features/signals/components/LogResultsTable.tsx
features/signals/TraceResultsTable.tsx →  features/signals/components/TraceResultsTable.tsx
```

Update all import paths accordingly. The existing untracked directories (`components/shared/`, `hooks/`) remain as-is.

---

## 2. Generic `SignalExplorer` Component

### Location
`components/shared/SignalExplorer.tsx`

### Responsibilities
`SignalExplorer` owns all shared state and layout:
- Service selection
- Lookback / custom time-range selection
- Histogram with range-select interaction
- Results table
- Detail panel (toggle open/close on row click)

It does **not** own domain-specific rendering — that is delegated via slots.

### Layout

```
┌─────────────────────────────────────────────────┐
│ toolbar: service selector | lookback | save     │
│ histogram                                       │
├──────────────┬──────────────────────────────────┤
│  detail      │  results table                   │
│  panel       │                                  │
│  25% width   │  click row  → opens panel        │
│              │  click again → closes panel      │
└──────────────┴──────────────────────────────────┘
```

When no row is selected, the table occupies full width. The panel appears on the left at 25% of the pane width.

### Interface

```ts
interface SignalQueryParams {
  service: string
  timeRange: { from: string; to: string }
}

interface SignalExplorerProps<T> {
  useData: (params: SignalQueryParams) => { items: T[]; loading: boolean }
  getItemId: (item: T) => string
  renderTable: (
    items: T[],
    selectedId: string | null,
    onSelect: (id: string | null) => void
  ) => ReactNode
  renderPanel: (selectedItem: T, onClose: () => void) => ReactNode
  title?: string
}
```

`selectedId` is toggled by `SignalExplorer`: clicking the same row a second time sets it to `null`, closing the panel.

### Thin wrappers

`LogSearch` and `TraceSearch` become wrappers of ~20 lines each, passing:
- their domain-specific `useData` hook
- their `renderTable` (existing `LogResultsTable` / `TraceResultsTable`)
- their `renderPanel` (log context panel / trace detail panel)

The existing `SignalExplorerLayout` in `components/shared/` is superseded by this component and can be removed.

### State management
Two concerns are kept separate:
- `useSignalSearch` (already in `hooks/`) owns UI state: service, lookback minutes, custom range, and derives `timeRange` (ISO from/to strings). `SignalExplorer` calls this hook internally.
- `useData` prop receives the derived `SignalQueryParams` and is responsible only for fetching and returning items. Each signal type provides its own implementation (e.g., `useLogSearch`, `useTraceSearch`).

### Facet sidebar (trace-specific)
`TraceSearch` currently renders `FacetSidebar` alongside its results table. When reduced to a thin wrapper, `TraceSearch` passes a `renderTable` slot that composes `FacetSidebar` and `TraceResultsTable` together. `SignalExplorer` does not need a dedicated sidebar slot — the slot boundary is at the table region, which can include a sidebar internally.

---

## 3. Detail Panels

### Log detail panel
- Renders surrounding log lines using `LogList` (see Section 4).
- If the selected log has a `trace_id`, renders a clickable link that navigates to the Trace explorer filtered to that trace (uses React Router `useNavigate` with a `traceId` query param).

### Trace detail panel
- Renders the span waterfall / trace detail for the selected trace.
- Timestamps displayed as wall-clock dates using `formatTimestamp` — the same format used in the log explorer. Removes any duration-only display from `TraceResultsTable`.

---

## 4. Unified `LogList` Component

### Location
`components/shared/LogList.tsx`

### Responsibilities
Renders a scrollable list of log rows: timestamp (date format via `formatTimestamp`), severity `Badge`, service, message. Stateless — receives data via props.

### Interface

```ts
interface LogListProps {
  logs: LogEntry[]
  loading: boolean
  emptyMessage?: string
  onLogClick?: (log: LogEntry) => void
}
```

### Consumers
| Consumer | Change |
|---|---|
| `LogContextView` | Becomes a thin wrapper: owns fetch, passes results to `LogList` |
| `LogCorrelatedList` | Becomes a thin wrapper: owns fetch, passes results to `LogList` |
| Log detail panel (Section 3) | Uses `LogList` directly |

---

## 5. What Is Explicitly Out of Scope

- `VisualizationPanel` inline table variants — not touched in this iteration.
- NLQ / metrics / alerts features — no changes.
- Backend / API layer — no changes.

---

## 6. File Change Summary

| Action | Path |
|---|---|
| Move | `features/signals/LogResultsTable.tsx` → `features/signals/components/` |
| Move | `features/signals/TraceResultsTable.tsx` → `features/signals/components/` |
| Create | `components/shared/SignalExplorer.tsx` |
| Create | `components/shared/LogList.tsx` |
| Modify | `pages/LogSearch.tsx` — thin wrapper |
| Modify | `pages/TraceSearch.tsx` — thin wrapper |
| Modify | `components/LogContextView.tsx` — delegate rendering to `LogList` |
| Modify | `components/LogCorrelatedList.tsx` — delegate rendering to `LogList` |
| Modify | `features/signals/components/TraceResultsTable.tsx` — adopt `formatTimestamp` for dates |
| Delete | `components/shared/SignalExplorerLayout.tsx` — superseded |

---

## 7. Testing

- Existing `pages/view-unification.test.ts` — extend to cover `SignalExplorer` render-prop contract.
- `LogList` — unit test: renders rows, empty state, click handler.
- `LogContextView` / `LogCorrelatedList` — existing tests remain valid; rendering assertions move to `LogList` tests.
- Run `local-ci.sh --skip-docker` after all changes before pushing.
