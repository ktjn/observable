# Service Overview Panel Improvements

**Date:** 2026-05-02

## Summary

Improve the service detail page by anchoring the overview content above a signal tab strip that defaults to Logs, adding a response time + throughput graph with deployment markers overlaid, and introducing a reusable `TimeSeriesGraph` SVG component.

---

## 1. Layout Changes

### Current layout (ServiceDetailPage)

1. Page header
2. Metric cards (Request Rate, Error Rate, P95 Latency, Active Alerts)
3. Deployment Timeline strip (conditional)
4. Detail grid: Health panel + Signal Entry Points panel
5. ServiceInfraPanel
6. NlqPanel
7. Signal tabs: **Overview** | Logs | Metrics | Traces — default: Overview (empty placeholder)

### New layout

1. Page header
2. Metric cards (unchanged)
3. **ResponseTimeGraph panel** (new — see §3)
4. Detail grid: Health panel + Signal Entry Points panel (unchanged)
5. ServiceInfraPanel (unchanged)
6. NlqPanel (unchanged)
7. Signal tabs: **Logs** | Metrics | Traces — default: **Logs**

**Changes:**
- The "Overview" tab is removed. Its placeholder content is gone; the overview information is now always visible above the tabs (anchored).
- The existing standalone `DeploymentTimelineSection` (SVG strip) is removed — deployment markers move onto the new graph as overlaid annotations.
- The signal tabs default to `logs` instead of `overview`. The router default route `/services/$serviceId` renders the Logs tab.

---

## 2. Generic `TimeSeriesGraph` Component

**File:** `apps/frontend/src/components/ui/time-series-graph.tsx`

### Props

```ts
export interface TimeSeriesPoint {
  timestampMs: number;
  value: number;
}

export interface TimeSeriesSeries {
  key: string;
  label: string;
  color: string;
  dashed?: boolean;
  formatY?: (value: number) => string;  // per-series — each series normalizes independently
  points: TimeSeriesPoint[];
}

export interface TimeSeriesGraphProps {
  series: TimeSeriesSeries[];
  deploymentMarkers?: DeploymentMarker[];
  rangeStartMs: number;
  rangeEndMs: number;
  height?: number;           // SVG render height in px, default 80
  title?: string;
  eyebrow?: string;
  ariaLabel?: string;
}
```

### Internals

- SVG rendered inside a `<section>` wrapper (same pattern as `Histogram`).
- Width is measured via `ResizeObserver` on the wrapper element; the SVG `viewBox` uses the measured width.
- One `<polyline>` per series. Points are mapped from `(timestampMs, value)` space to SVG coordinates using the time range and per-series value extents.
- Dashed series use `strokeDasharray="4 2"`.
- Horizontal grid lines (3–4) rendered as `<line>` elements at rounded Y-axis values.
- Y-axis labels rendered as `<text>` elements on the left edge.
- Time axis labels (start / midpoints / end) rendered as `<text>` below the plot area.
- **Deployment markers:** vertical dashed `<line>` + diamond `<polygon>` at the top, positioned with the existing `markerPosition` and colored with `markerColor` utilities from `DeploymentTimeline.tsx`.
- **Hover tooltip:** a `<line>` crosshair follows the pointer (via `onPointerMove`); a positioned `<div>` tooltip shows all series values at the nearest timestamp bucket.
- No external dependencies.

### Reusability

The component is generic — it accepts any named series and any numeric Y values. Consumers control colors, labels, and formatting. It can be used for request rate graphs, error rate over time, infra CPU/memory trends, etc.

---

## 3. Response Time History API

### New backend endpoint

```
GET /v1/services/:name/response-time-history
  ?lookback_minutes=60
  &buckets=60
```

Returns pre-computed time-bucketed latency percentiles and throughput for the service.

### Response shape

```ts
export interface ResponseTimeHistoryBucket {
  start_ms: number;
  end_ms: number;
  p50_ms: number;
  p95_ms: number;
  request_rate: number;   // requests/sec averaged over this bucket
}

export interface ResponseTimeHistoryResponse {
  buckets: ResponseTimeHistoryBucket[];
}
```

### New frontend API function

Added to `apps/frontend/src/api/services.ts`:

```ts
export async function getServiceResponseTimeHistory(
  serviceName: string,
  params: { lookback_minutes?: number; buckets?: number },
): Promise<ResponseTimeHistoryResponse>
```

### Usage in ServiceDetailPage

A `useQuery` hook fetches the history when the page loads. The three series passed to `TimeSeriesGraph` are:

| Series key | Field | Color | Dashed |
|---|---|---|---|
| `p95` | `p95_ms` | `#818cf8` (purple) | no |
| `p50` | `p50_ms` | `#34d399` (green) | no |
| `request_rate` | `request_rate` | `#fb923c` (orange) | yes |

Each series carries its own `formatY`. P50/P95: `(v) => \`${Math.round(v)}ms\``. Request rate: `(v) => \`${v.toFixed(1)} rps\``. The graph normalizes each series independently to fill the plot height — there is no shared Y axis. Tooltip shows each series' formatted value at the hovered timestamp.

---

## 4. Router / Default Tab Change

The existing tab routing reads `signalTabFromPath`. Change the fallback from `"overview"` to `"logs"`:

```ts
function signalTabFromPath(pathname: string): ServiceSignalTab {
  if (pathname.endsWith("/logs")) return "logs";
  if (pathname.endsWith("/metrics")) return "metrics";
  if (pathname.endsWith("/traces")) return "traces";
  return "logs";   // was "overview"
}
```

`ServiceSignalTab` type loses the `"overview"` member. The `tabLinks` array in `ServiceSignalTabs` loses the Overview entry.

---

## 5. Out of Scope

- Dual Y-axis on the graph (one axis for ms, one for rps). Series are normalized independently to fill the plot area; exact Y values are readable via the tooltip.
- Graph zoom / range selection (unlike `Histogram`, no drag-to-zoom in this component for now).
- ServiceInfraPanel and NlqPanel restructuring — they remain where they are.
- Any changes to Logs, Metrics, or Traces tab content.
