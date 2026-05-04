# Signal Table: Overflow Fix + Virtual Scroll

**Date:** 2026-05-04  
**Scope:** `LogResultsTable`, `TraceResultsTable`

## Problem

Two issues affect the log and trace signal explorer tables:

1. **Long message lines overflow** — the global `white-space: nowrap` CSS rule on `td` elements means long log messages and operation names extend beyond the table column with no truncation or wrapping, breaking the layout.
2. **No row limit** — both tables fetch and render all matching rows with no cap, which is slow and memory-heavy on large result sets.

## Design

### 1. Overflow fix

Add a `message-cell` CSS class (or Tailwind utility override) applied to the message `<td>` in `LogResultsTable` and the operation `<td>` in `TraceResultsTable`. The class overrides the global `white-space: nowrap` rule with `white-space: normal; word-break: break-all`.

- All other columns (timestamp, level, service, duration, status) keep `nowrap` — no change to global CSS.
- No layout changes needed; the message/operation column is already the fluid last column in both tables.

### 2. Virtual scroll

Add a shared `VirtualTable` component at `src/components/ui/VirtualTable.tsx` using `@tanstack/react-virtual`.

**Interface:**
```tsx
<VirtualTable
  rows={logs}
  renderRow={(row, ref) => <LogResultsRow log={row} measureRef={ref} ... />}
  estimateSize={40}         // px hint; actual height measured dynamically
  height="600px"            // scroll container height
  totalLabel="1,240 logs"   // optional footer label
/>
```

**Internals:**
- A fixed-height `div` with `overflow-y-auto` acts as the scroll container; its ref is passed to `useVirtualizer`.
- `measureElement` is enabled so that wrapped-text rows are measured after render — row heights are dynamic.
- A single `<table>` with a `<thead>` (sticky, outside the virtual area) and a `<tbody>` that contains only the currently visible `<tr>` elements positioned inside a tall spacer.
- Each rendered `<tr>` receives a `ref` callback for height measurement.

**Row components unchanged:** `LogResultsRow` and `TraceResultsRow` require no modification beyond accepting the `measureRef` prop.

### 3. Fetch limit

Both `LogSearch` and `TraceSearch` use `submitNlqQuery`, which returns `frame.data` — a flat array with no separate total count. `NlqIrLike` / `NlqRequest` have no `limit` field, so the cap is enforced client-side:

- After the NLQ response, slice the result array to 500 rows: `data.slice(0, 500)`.
- When `data.length >= 500` (i.e. the cap was hit), show a footer: "Showing 500 results — narrow the time range or add filters to see fewer."
- When `data.length < 500`, no footer shown.

### 4. Scope boundaries

| Component | Changed | Reason |
|---|---|---|
| `LogResultsTable` | Yes | Main log signal view |
| `TraceResultsTable` | Yes | Main trace signal view |
| `LogList` | No | Already uses `break-all`; fixed `max-h` cap |
| `LogLiveTail` | No | Manages its own 200-row cap and layout |
| Backend | No | `limit` param already exists on both endpoints |

## Files affected

- `apps/frontend/src/features/signals/components/LogResultsTable.tsx` — add `message-cell` class; wrap with `VirtualTable`
- `apps/frontend/src/features/signals/components/TraceResultsTable.tsx` — add `message-cell` class on operation column; wrap with `VirtualTable`
- `apps/frontend/src/components/ui/VirtualTable.tsx` — new component
- `apps/frontend/src/pages/LogSearch.tsx` — slice result to 500; pass capped array + cap-hit flag to table
- `apps/frontend/src/pages/TraceSearch.tsx` — slice result to 500; pass capped array + cap-hit flag to table
- `apps/frontend/package.json` — add `@tanstack/react-virtual`

## Non-goals

- Backend pagination / cursor support
- Changes to `LogList`, `LogLiveTail`, `LogContextView`, `LogCorrelatedList`
- Horizontal scroll for very long unbreakable tokens (e.g. base64 blobs) — `break-all` handles this
