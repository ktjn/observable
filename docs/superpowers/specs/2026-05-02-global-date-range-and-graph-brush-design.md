# Global Date Range Selector & Graph Brush Selection

**Date:** 2026-05-02
**Status:** Approved

## Summary

Two related features:
1. Move date/time range selection from per-page state into a global, URL-backed control in the AppShell header.
2. Add brush (drag) range selection to `TimeSeriesGraph`, matching what the `Histogram` already supports.

Both changes share the same sink: a `useGlobalDateRange` hook that reads/writes TanStack Router root-route search params.

---

## 1. URL Schema

Defined on the **root route** via `validateSearch`. All child routes inherit it automatically.

```ts
// Two mutually exclusive states in the URL:
?preset=1h                               // preset window, relative to now
?from=1746100000000&to=1746103600000     // absolute custom range (ms since epoch)
```

Presets: `5m | 15m | 30m | 1h | 3h | 12h`. Default (when neither param is present): `1h`.

If `from` and `to` are both present they take priority over `preset`. If only `preset` is present, `from`/`to` are absent. The URL is always shareable and produces the same time window when replayed (custom range) or a window relative to the opener's clock (preset).

---

## 2. `useGlobalDateRange` Hook

**Location:** `apps/frontend/src/hooks/useGlobalDateRange.ts`

**API:**
```ts
interface GlobalDateRange {
  preset: Preset | null          // null when a custom range is active
  fromMs: number                 // always populated, derived from preset or URL params
  toMs: number                   // always populated
  setPreset: (p: Preset) => void
  setCustomRange: (from: number, to: number) => void
  clearCustomRange: () => void   // resets to default preset (1h)
}
```

Implementation reads `preset`, `from`, `to` from `rootRoute.useSearch()` and writes back via `router.navigate({ to: ".", search: (prev) => ({ ...prev, ... }) })`. The `fromMs`/`toMs` values are derived inside a `useMemo`: if a custom range is active, use `from`/`to` directly; otherwise compute `Date.now() - presetMs`.

---

## 3. AppShell Header

Replace the static `<span className="context-pill">Last 1h</span>` with a `GlobalDateRangePicker` component.

**Preset state** — shows a `<select>` dropdown:
```
[ Last 1h ▼ ]
  Last 5 min
  Last 15 min
  Last 30 min
● Last 1 hour
  Last 3 hours
  Last 12 hours
```
On change → `setPreset(value)`.

**Custom range state** — shows label + reset button:
```
[ May 1 10:00 → 10:45  ✕ ]
```
`✕` calls `clearCustomRange()`.

The component lives at `apps/frontend/src/components/GlobalDateRangePicker.tsx`.

---

## 4. TimeSeriesGraph Brush Selection

`time-series-graph.tsx` currently supports hover only. Add drag-to-select like `histogram.tsx`:

- New optional prop: `onRangeSelect?: (fromMs: number, toMs: number) => void`
- Drag state tracked with a `useRef` (same pattern as histogram)
- `onPointerDown` on the SVG: record `startX`, capture pointer
- `onPointerMove`: update `endX`, render a translucent rect overlay showing the selection
- `onPointerUp`: convert pixel positions to timestamps using the existing `toX()` inverse, fire `onRangeSelect(fromMs, toMs)`, clear drag state
- When `onRangeSelect` is not provided, no drag interaction is active (cursor stays `crosshair`)

The coordinate inverse (`pixelToMs`) is straightforward since `toX` is a linear map:
```
ms = rangeStartMs + (x / width) * (rangeEndMs - rangeStartMs)
```

---

## 5. Migration: Remove Per-Page Date Controls

### `ServiceDetailPage`
- Remove `lookback_minutes` from URL search params and `readLookbackMinutes()`
- Replace with `useGlobalDateRange()` → consume `fromMs`, `toMs`
- Pass `fromMs`/`toMs` down to `ServiceLogsTab`, `ServiceTracesTab`, `ResponseTimeGraphSection`

### `LogSearch` / `TraceSearch`
- Remove `lookbackMinutes`/`customRangeMs` state from `useSignalSearch`
- Consume `useGlobalDateRange()` for `fromMs`/`toMs`
- Wire histogram `onRangeSelect` → `setCustomRange` from global hook
- Keep `useSignalSearch` for the `service` field only (or inline it)

### `SignalExplorer`
- Remove the date/lookback `<select>` dropdown
- Remove `lookbackMinutes`, `onLookbackChange`, `onClearRange`, `customRangeMs` props
- The date range is now shown/controlled globally in the AppShell — `SignalExplorer` becomes display + filter only

### `useSignalSearch`
- Remove `lookbackMinutes`, `setLookbackMinutes`, `customRangeMs`, `handleHistogramRangeSelect`, `handleClearRange`, `from`, `to`, `histogramFromMs`, `histogramToMs`
- Keep only `service` / `setService` (or delete the hook entirely if callers inline it)

---

## 6. Wiring Histogram & Graph to Global Range

Both `Histogram` (in LogSearch/TraceSearch) and `TimeSeriesGraph` (in ServiceDetailPage) receive:
```tsx
onRangeSelect={setCustomRange}
```
from `useGlobalDateRange()`. A brush on either component updates the URL, which re-renders all active queries on all pages simultaneously.

---

## 7. Files Changed

| File | Change |
|------|--------|
| `router.ts` | Add `validateSearch` to root route |
| `hooks/useGlobalDateRange.ts` | New hook |
| `components/AppShell.tsx` | Replace static pill with `GlobalDateRangePicker` |
| `components/GlobalDateRangePicker.tsx` | New component |
| `components/ui/time-series-graph.tsx` | Add brush selection |
| `pages/ServiceDetailPage.tsx` | Consume global range, drop `lookback_minutes` |
| `pages/LogSearch.tsx` | Consume global range |
| `pages/TraceSearch.tsx` | Consume global range |
| `components/shared/SignalExplorer.tsx` | Remove date controls |
| `hooks/useSignalSearch.ts` | Trim to `service` only or delete |

---

## 8. Non-Goals

- No custom date/time picker UI (text input for arbitrary timestamps) — presets + brush selection only for now
- No per-page override of the global range
- No persistence beyond URL (no localStorage fallback)
