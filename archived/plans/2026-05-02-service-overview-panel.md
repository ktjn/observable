# Service Overview Panel Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Anchor the service overview above signal tabs (defaulting to Logs), add a time-series response-time + throughput graph with overlaid deployment markers, and introduce a reusable `TimeSeriesGraph` SVG component.

**Architecture:** New Rust handler in `discovery.rs` computes bucketed P50/P95/req-rate from `observable.spans` via ClickHouse; a new `TimeSeriesGraph` React component renders named SVG polylines and deployment marker overlays; `ServiceDetailPage` wires everything together and removes the old Overview tab.

**Tech Stack:** Rust/Axum (backend), ClickHouse (data), React 18, TanStack Query, Vitest (frontend tests)

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Modify | `services/query-api/src/planner/mod.rs` | Add `ResponseTimeHistogramPlan` + `plan_response_time_histogram` |
| Modify | `services/query-api/src/discovery.rs` | Add types + `get_service_response_time_history` handler |
| Modify | `services/query-api/src/main.rs` | Register new route |
| Modify | `services/query-api/tests/clickhouse_integration.rs` | Integration test for the new endpoint |
| Modify | `apps/frontend/src/api/services.ts` | Add `ResponseTimeHistoryBucket`, `ResponseTimeHistoryResponse`, `getServiceResponseTimeHistory` |
| Create | `apps/frontend/src/components/ui/time-series-graph.tsx` | Generic `TimeSeriesGraph` SVG component |
| Create | `apps/frontend/src/components/ui/time-series-graph.test.tsx` | Unit tests for pure math helpers |
| Modify | `apps/frontend/src/pages/ServiceDetailPage.tsx` | Wire graph, change default tab, remove Overview tab |

---

## Task 1: Planner — `plan_response_time_histogram`

**Files:**
- Modify: `services/query-api/src/planner/mod.rs`

- [ ] **Step 1: Write the failing unit test**

Add at the bottom of `services/query-api/src/planner/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_response_time_histogram_divides_range_into_equal_intervals() {
        let planner = QueryPlanner;
        let from_ns = 0u64;
        let to_ns = 60 * 1_000_000_000u64; // 60 seconds in ns
        let plan = planner.plan_response_time_histogram(from_ns, to_ns, 60);
        assert_eq!(plan.interval_ns, 1_000_000_000, "interval should be 1 second");
        assert_eq!(plan.from_ns, 0);
        assert!(plan.sql.contains("quantile(0.50)"), "sql must include P50");
        assert!(plan.sql.contains("quantile(0.95)"), "sql must include P95");
        assert!(plan.sql.contains("service_name = ?"), "sql must filter by service");
    }
}
```

- [ ] **Step 2: Run test to verify it fails to compile**

```bash
cd services/query-api && cargo test plan_response_time_histogram -- --nocapture 2>&1 | head -20
```

Expected: compilation error — `plan_response_time_histogram` not found on `QueryPlanner`.

- [ ] **Step 3: Add the plan struct and method**

At the top of the `planner/mod.rs` structs section (after the existing `LogHistogramPlan` struct), add:

```rust
pub struct ResponseTimeHistogramPlan {
    pub sql: String,
    pub from_ns: u64,
    pub interval_ns: u64,
}
```

Then add this method inside `impl QueryPlanner` (after `plan_trace_histogram`):

```rust
pub fn plan_response_time_histogram(
    &self,
    from_ns: u64,
    to_ns: u64,
    bucket_count: u32,
) -> ResponseTimeHistogramPlan {
    let range_ns = to_ns.saturating_sub(from_ns).max(1);
    let interval_ns = (range_ns / bucket_count as u64).max(1);

    let sql = "SELECT \
           intDiv(start_time_unix_nano - ?, ?) AS bucket_idx, \
           quantile(0.50)(duration_ns) AS p50_ns, \
           quantile(0.95)(duration_ns) AS p95_ns, \
           count() AS span_count \
         FROM observable.spans \
         WHERE tenant_id = ? \
           AND service_name = ? \
           AND start_time_unix_nano >= ? \
           AND start_time_unix_nano <= ? \
         GROUP BY bucket_idx \
         ORDER BY bucket_idx ASC"
        .to_string();

    ResponseTimeHistogramPlan { sql, from_ns, interval_ns }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd services/query-api && cargo test plan_response_time_histogram -- --nocapture
```

Expected: `test planner::tests::plan_response_time_histogram_divides_range_into_equal_intervals ... ok`

- [ ] **Step 5: Commit**

```bash
git add services/query-api/src/planner/mod.rs
git commit -m "feat(query-api): add plan_response_time_histogram to QueryPlanner"
```

---

## Task 2: Handler — `get_service_response_time_history`

**Files:**
- Modify: `services/query-api/src/discovery.rs`

- [ ] **Step 1: Add imports and types**

At the top of `services/query-api/src/discovery.rs`, add `HashMap` to the existing imports. The existing `use` block starts with:

```rust
use crate::middleware::auth::TenantContext;
```

Add this line after the existing imports:

```rust
use std::collections::HashMap;
```

Then add these structs after the existing `ServiceDetailResponse` struct (around line 79):

```rust
#[derive(Deserialize)]
pub struct ResponseTimeHistoryParams {
    pub lookback_minutes: Option<u32>,
    pub buckets: Option<u32>,
}

#[derive(Serialize)]
pub struct ResponseTimeBucket {
    pub start_ms: u64,
    pub end_ms: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub request_rate: f64,
}

#[derive(Serialize)]
pub struct ResponseTimeHistoryResponse {
    pub buckets: Vec<ResponseTimeBucket>,
}
```

- [ ] **Step 2: Add the handler function**

Add after `get_service_summary` (after line 360):

```rust
pub async fn get_service_response_time_history(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(params): Query<ResponseTimeHistoryParams>,
) -> Result<Json<ResponseTimeHistoryResponse>, StatusCode> {
    let lookback_mins = params.lookback_minutes.unwrap_or(60);
    let bucket_count = params.buckets.unwrap_or(60).clamp(1, 200);
    let lookback_ns = (lookback_mins as u64) * 60 * 1_000_000_000;
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = now_ns.saturating_sub(lookback_ns);
    let to_ns = now_ns;

    let plan = state
        .planner
        .plan_response_time_histogram(from_ns, to_ns, bucket_count);

    let mut cursor = state
        .ch
        .query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(ctx.tenant_id)
        .bind(&service_name)
        .bind(from_ns)
        .bind(to_ns)
        .fetch::<(i64, f64, f64, u64)>()
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse response time histogram error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let interval_secs = plan.interval_ns as f64 / 1_000_000_000.0;
    let mut raw: HashMap<i64, (f64, f64, u64)> = HashMap::new();
    while let Some((bucket_idx, p50_ns, p95_ns, span_count)) =
        cursor.next().await.map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse response time histogram fetch error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        if bucket_idx >= 0 && bucket_idx < bucket_count as i64 {
            raw.insert(bucket_idx, (p50_ns, p95_ns, span_count));
        }
    }

    let buckets = (0..bucket_count)
        .map(|i| {
            let start_ns = plan.from_ns + i as u64 * plan.interval_ns;
            let end_ns = start_ns + plan.interval_ns;
            let (p50_ns, p95_ns, span_count) =
                raw.remove(&(i as i64)).unwrap_or((0.0, 0.0, 0));
            ResponseTimeBucket {
                start_ms: start_ns / 1_000_000,
                end_ms: end_ns / 1_000_000,
                p50_ms: p50_ns / 1_000_000.0,
                p95_ms: p95_ns / 1_000_000.0,
                request_rate: span_count as f64 / interval_secs,
            }
        })
        .collect();

    Ok(Json(ResponseTimeHistoryResponse { buckets }))
}
```

- [ ] **Step 3: Verify compilation**

```bash
cd services/query-api && cargo check 2>&1 | tail -5
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...`

- [ ] **Step 4: Commit**

```bash
git add services/query-api/src/discovery.rs
git commit -m "feat(query-api): add get_service_response_time_history handler"
```

---

## Task 3: Route Registration

**Files:**
- Modify: `services/query-api/src/main.rs`

- [ ] **Step 1: Register the route**

In `services/query-api/src/main.rs`, after the existing `"/v1/services/:service_name/summary"` route (around line 87), add:

```rust
.route(
    "/v1/services/:service_name/response-time-history",
    get(discovery::get_service_response_time_history),
)
```

- [ ] **Step 2: Verify compilation**

```bash
cd services/query-api && cargo check 2>&1 | tail -5
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...`

- [ ] **Step 3: Commit**

```bash
git add services/query-api/src/main.rs
git commit -m "feat(query-api): register /v1/services/:name/response-time-history route"
```

---

## Task 4: Backend Integration Test

**Files:**
- Modify: `services/query-api/tests/clickhouse_integration.rs`

- [ ] **Step 1: Add the helper function and test**

Add these imports at the top of `services/query-api/tests/clickhouse_integration.rs` (after the existing imports):

```rust
use query_api::discovery::{ResponseTimeBucket, ResponseTimeHistoryResponse};
```

Then add the helper and test at the end of the file:

```rust
async fn run_response_time_histogram(
    ch: &Client,
    tenant_id: Uuid,
    service: &str,
    from_ns: u64,
    to_ns: u64,
    bucket_count: u32,
) -> Vec<(i64, f64, f64, u64)> {
    let planner = QueryPlanner;
    let plan = planner.plan_response_time_histogram(from_ns, to_ns, bucket_count);
    ch.query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(tenant_id)
        .bind(service)
        .bind(from_ns)
        .bind(to_ns)
        .fetch_all::<(i64, f64, f64, u64)>()
        .await
        .expect("response time histogram query succeeded")
}

#[tokio::test]
async fn response_time_histogram_buckets_latency_by_time_window() {
    let container = ClickHouse::default()
        .with_tag("24.3")
        .with_env_var("CLICKHOUSE_USER", "default")
        .with_env_var("CLICKHOUSE_PASSWORD", "test")
        .start()
        .await
        .expect("clickhouse container started");

    let port = container.get_host_port_ipv4(8123).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    let ch = apply_migrations(&base_url, "default", "test").await;

    let tenant = Uuid::new_v4();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = now_ns.saturating_sub(60 * 60 * 1_000_000_000); // 1 hour ago

    // Bucket 0 (first 30 min): fast span — 1ms
    let mut early = make_span(tenant, "trace-rt-early", "span-rt-early");
    early.service_name = "svc-rt".into();
    early.start_time_unix_nano = from_ns + 1_000;
    early.duration_ns = 1_000_000; // 1ms
    insert_span(&ch, early).await;

    // Bucket 1 (second 30 min): slow span — 100ms
    let mut late = make_span(tenant, "trace-rt-late", "span-rt-late");
    late.service_name = "svc-rt".into();
    late.start_time_unix_nano = from_ns + 30 * 60 * 1_000_000_000 + 1_000;
    late.duration_ns = 100_000_000; // 100ms
    insert_span(&ch, late).await;

    let rows = run_response_time_histogram(&ch, tenant, "svc-rt", from_ns, now_ns, 2).await;

    assert_eq!(rows.len(), 2, "two buckets returned");

    let (idx0, p50_b0, _p95_b0, count_b0) = rows[0];
    assert_eq!(idx0, 0, "first row is bucket 0");
    assert_eq!(count_b0, 1, "bucket 0 has one span");
    assert!(
        (p50_b0 - 1_000_000.0).abs() < 10_000.0,
        "bucket 0 p50 ≈ 1ms in nanoseconds, got {p50_b0}"
    );

    let (idx1, p50_b1, _p95_b1, count_b1) = rows[1];
    assert_eq!(idx1, 1, "second row is bucket 1");
    assert_eq!(count_b1, 1, "bucket 1 has one span");
    assert!(
        p50_b1 > p50_b0,
        "bucket 1 latency ({p50_b1}) must exceed bucket 0 ({p50_b0})"
    );
}
```

- [ ] **Step 2: Run the integration test**

```bash
cd services/query-api && cargo test --test clickhouse_integration response_time_histogram -- --nocapture
```

Expected: `test response_time_histogram_buckets_latency_by_time_window ... ok`
(Requires Docker. This test spins up a ClickHouse container — may take ~30s on first run.)

- [ ] **Step 3: Commit**

```bash
git add services/query-api/tests/clickhouse_integration.rs
git commit -m "test(query-api): integration test for response_time_histogram bucketing"
```

---

## Task 5: Frontend API Function

**Files:**
- Modify: `apps/frontend/src/api/services.ts`

- [ ] **Step 1: Add types and fetch function**

At the end of `apps/frontend/src/api/services.ts`, add:

```ts
export interface ResponseTimeHistoryBucket {
  start_ms: number;
  end_ms: number;
  p50_ms: number;
  p95_ms: number;
  request_rate: number;
}

export interface ResponseTimeHistoryResponse {
  buckets: ResponseTimeHistoryBucket[];
}

export async function getServiceResponseTimeHistory(
  serviceName: string,
  params: {
    lookback_minutes?: number;
    buckets?: number;
  } = {},
): Promise<ResponseTimeHistoryResponse> {
  const encodedService = encodeURIComponent(serviceName);
  const url = new URL(
    `/v1/services/${encodedService}/response-time-history`,
    window.location.origin,
  );
  if (params.lookback_minutes) {
    url.searchParams.set("lookback_minutes", String(params.lookback_minutes));
  }
  if (params.buckets) {
    url.searchParams.set("buckets", String(params.buckets));
  }
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd apps/frontend && npm run build 2>&1 | tail -10
```

Expected: build succeeds with no errors related to `services.ts`.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/api/services.ts
git commit -m "feat(frontend): add getServiceResponseTimeHistory API function"
```

---

## Task 6: `TimeSeriesGraph` Component

**Files:**
- Create: `apps/frontend/src/components/ui/time-series-graph.tsx`
- Create: `apps/frontend/src/components/ui/time-series-graph.test.tsx`

- [ ] **Step 1: Write the failing unit tests**

Create `apps/frontend/src/components/ui/time-series-graph.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { buildPolylinePoints, toX, toY } from "./time-series-graph";

describe("toX", () => {
  it("maps range start to 0", () => {
    expect(toX(1000, 1000, 2000, 400)).toBe(0);
  });
  it("maps range end to width", () => {
    expect(toX(2000, 1000, 2000, 400)).toBe(400);
  });
  it("maps midpoint to half width", () => {
    expect(toX(1500, 1000, 2000, 400)).toBe(200);
  });
  it("returns 0 when range is zero", () => {
    expect(toX(1000, 1000, 1000, 400)).toBe(0);
  });
});

describe("toY", () => {
  it("maps max value to plotTop", () => {
    expect(toY(100, 0, 100, 10, 70)).toBe(10);
  });
  it("maps min value to plotBottom", () => {
    expect(toY(0, 0, 100, 10, 70)).toBe(70);
  });
  it("maps mid value to vertical midpoint", () => {
    expect(toY(50, 0, 100, 10, 70)).toBe(40);
  });
  it("returns midpoint when value range is zero", () => {
    expect(toY(5, 5, 5, 10, 70)).toBe(40);
  });
});

describe("buildPolylinePoints", () => {
  it("returns empty string for no points", () => {
    const series = { key: "s", label: "S", color: "#fff", points: [] };
    expect(buildPolylinePoints(series, 0, 100, 400, 10, 70)).toBe("");
  });
  it("maps a single point correctly", () => {
    const series = {
      key: "s", label: "S", color: "#fff",
      points: [{ timestampMs: 50, value: 50 }],
    };
    // toX(50, 0, 100, 400) = 200; toY(50, 50, 50, 10, 70) = 40 (zero-range midpoint)
    expect(buildPolylinePoints(series, 0, 100, 400, 10, 70)).toBe("200,40");
  });
  it("maps two points spanning the full range", () => {
    const series = {
      key: "s", label: "S", color: "#fff",
      points: [
        { timestampMs: 0, value: 0 },
        { timestampMs: 100, value: 100 },
      ],
    };
    // Point 1: toX(0,0,100,400)=0, toY(0,0,100,10,70)=70 → "0,70"
    // Point 2: toX(100,0,100,400)=400, toY(100,0,100,10,70)=10 → "400,10"
    expect(buildPolylinePoints(series, 0, 100, 400, 10, 70)).toBe("0,70 400,10");
  });
});
```

- [ ] **Step 2: Run to verify tests fail**

```bash
cd apps/frontend && npm test -- --reporter=verbose time-series-graph.test 2>&1 | tail -20
```

Expected: fails — module `./time-series-graph` not found.

- [ ] **Step 3: Create the component**

Create `apps/frontend/src/components/ui/time-series-graph.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import type { DeploymentMarker } from "../../api/deployments";
import { markerColor, markerPosition } from "../DeploymentTimeline";

export interface TimeSeriesPoint {
  timestampMs: number;
  value: number;
}

export interface TimeSeriesSeries {
  key: string;
  label: string;
  color: string;
  dashed?: boolean;
  formatY?: (value: number) => string;
  points: TimeSeriesPoint[];
}

export interface TimeSeriesGraphProps {
  series: TimeSeriesSeries[];
  deploymentMarkers?: DeploymentMarker[];
  rangeStartMs: number;
  rangeEndMs: number;
  height?: number;
  title?: string;
  eyebrow?: string;
  ariaLabel?: string;
}

const PLOT_TOP = 10;
const AXIS_HEIGHT = 18;
const PLOT_BOTTOM_MARGIN = 6;

export function toX(
  timestampMs: number,
  rangeStartMs: number,
  rangeEndMs: number,
  width: number,
): number {
  const span = rangeEndMs - rangeStartMs;
  if (span <= 0) return 0;
  return Math.round(((timestampMs - rangeStartMs) / span) * width);
}

export function toY(
  value: number,
  min: number,
  max: number,
  plotTop: number,
  plotBottom: number,
): number {
  const range = max - min;
  if (range === 0) return Math.round((plotTop + plotBottom) / 2);
  const ratio = (value - min) / range;
  return Math.round(plotBottom - ratio * (plotBottom - plotTop));
}

export function buildPolylinePoints(
  series: TimeSeriesSeries,
  rangeStartMs: number,
  rangeEndMs: number,
  width: number,
  plotTop: number,
  plotBottom: number,
): string {
  if (series.points.length === 0) return "";
  const values = series.points.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  return series.points
    .map(
      (p) =>
        `${toX(p.timestampMs, rangeStartMs, rangeEndMs, width)},${toY(p.value, min, max, plotTop, plotBottom)}`,
    )
    .join(" ");
}

export function TimeSeriesGraph({
  series,
  deploymentMarkers = [],
  rangeStartMs,
  rangeEndMs,
  height = 80,
  title,
  eyebrow,
  ariaLabel = "Time series graph",
}: TimeSeriesGraphProps) {
  const wrapperRef = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(400);
  const [hoverX, setHoverX] = useState<number | null>(null);
  const [deployTooltip, setDeployTooltip] = useState<DeploymentMarker | null>(null);

  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setWidth(Math.round(entry.contentRect.width));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const plotBottom = height - AXIS_HEIGHT - PLOT_BOTTOM_MARGIN;
  const gridYs = [0.25, 0.5, 0.75].map((r) =>
    Math.round(PLOT_TOP + r * (plotBottom - PLOT_TOP)),
  );

  const timeSteps = [0, 0.25, 0.5, 0.75, 1];
  const timeLabels = timeSteps.map((ratio) => ({
    x: Math.round(ratio * width),
    label: formatTimeLabel(rangeStartMs + ratio * (rangeEndMs - rangeStartMs)),
    anchor: ratio === 0 ? "start" : ratio === 1 ? "end" : "middle",
  }));

  const hoverTimestampMs =
    hoverX != null
      ? rangeStartMs + (hoverX / width) * (rangeEndMs - rangeStartMs)
      : null;

  function nearestPoint(s: TimeSeriesSeries): TimeSeriesPoint | null {
    if (hoverTimestampMs == null || s.points.length === 0) return null;
    return s.points.reduce((a, b) =>
      Math.abs(a.timestampMs - hoverTimestampMs) <=
      Math.abs(b.timestampMs - hoverTimestampMs)
        ? a
        : b,
    );
  }

  return (
    <section
      ref={wrapperRef}
      role="group"
      aria-label={ariaLabel}
      className="border border-[var(--border)] bg-[var(--surface)] p-3"
    >
      {(eyebrow || title || series.length > 0) && (
        <div className="mb-2 flex items-center justify-between gap-3">
          <div>
            {eyebrow && (
              <div className="text-xs font-bold uppercase text-[var(--muted)]">{eyebrow}</div>
            )}
            {title && (
              <h2 className="m-0 text-sm font-bold text-[var(--text-strong)]">{title}</h2>
            )}
          </div>
          <div className="flex flex-wrap gap-3 text-xs text-[var(--muted)]">
            {series.map((s) => (
              <span key={s.key} className="inline-flex items-center gap-1">
                <svg width="12" height="4" aria-hidden="true">
                  <line
                    x1="0" y1="2" x2="12" y2="2"
                    stroke={s.color}
                    strokeWidth="2"
                    strokeDasharray={s.dashed ? "3 2" : undefined}
                  />
                </svg>
                {s.label}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="relative" style={{ height }}>
        <svg
          width="100%"
          height={height}
          viewBox={`0 0 ${width} ${height}`}
          aria-hidden="true"
          onPointerMove={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            setHoverX(Math.round(e.clientX - rect.left));
          }}
          onPointerLeave={() => setHoverX(null)}
          style={{ cursor: "crosshair", overflow: "visible" }}
        >
          {gridYs.map((y) => (
            <line
              key={y}
              x1={0} y1={y} x2={width} y2={y}
              stroke="var(--border)"
              strokeWidth={0.5}
            />
          ))}

          {series.map((s) => (
            <polyline
              key={s.key}
              points={buildPolylinePoints(s, rangeStartMs, rangeEndMs, width, PLOT_TOP, plotBottom)}
              fill="none"
              stroke={s.color}
              strokeWidth={1.5}
              strokeDasharray={s.dashed ? "4 2" : undefined}
            />
          ))}

          {deploymentMarkers.map((m) => {
            const x = markerPosition(
              new Date(m.started_at).getTime(),
              rangeStartMs,
              rangeEndMs,
              width,
            );
            const color = markerColor(m.status);
            return (
              <g key={m.deployment_id}>
                <line
                  x1={x} y1={PLOT_TOP}
                  x2={x} y2={plotBottom}
                  stroke={color}
                  strokeWidth={1}
                  strokeDasharray="3 2"
                  opacity={0.7}
                />
                <polygon
                  points={`${x},${PLOT_TOP - 2} ${x - 4},${PLOT_TOP + 5} ${x + 4},${PLOT_TOP + 5}`}
                  fill={color}
                  onMouseEnter={() => setDeployTooltip(m)}
                  onMouseLeave={() => setDeployTooltip(null)}
                  style={{ cursor: "default" }}
                  aria-label={`Deployment ${m.service_version} — ${m.status}`}
                />
              </g>
            );
          })}

          {hoverX != null && (
            <line
              x1={hoverX} y1={PLOT_TOP}
              x2={hoverX} y2={plotBottom}
              stroke="var(--muted)"
              strokeWidth={1}
              strokeDasharray="2 2"
              opacity={0.4}
            />
          )}

          {timeLabels.map(({ x, label, anchor }) => (
            <text
              key={x}
              x={x}
              y={height - 2}
              fontSize={9}
              fill="var(--muted)"
              textAnchor={anchor as "start" | "middle" | "end"}
            >
              {label}
            </text>
          ))}
        </svg>

        {hoverX != null && (
          <div
            className="pointer-events-none absolute z-10 min-w-[110px] border border-[var(--border)] bg-[var(--surface)] p-2 text-xs shadow-md"
            style={{ left: hoverX + 10, top: PLOT_TOP }}
          >
            {series.map((s) => {
              const pt = nearestPoint(s);
              if (!pt) return null;
              return (
                <div key={s.key} className="flex items-center gap-1.5">
                  <span
                    className="inline-block h-px w-2.5 shrink-0"
                    style={{ background: s.color }}
                  />
                  <span className="text-[var(--muted)]">{s.label}:</span>
                  <span className="font-mono font-bold text-[var(--text-strong)]">
                    {s.formatY ? s.formatY(pt.value) : pt.value.toFixed(1)}
                  </span>
                </div>
              );
            })}
          </div>
        )}

        {deployTooltip && (
          <div role="tooltip" className="deployment-timeline-tooltip">
            <div><strong>{deployTooltip.service_version}</strong></div>
            <div>{deployTooltip.status}</div>
            {deployTooltip.deployed_by && <div>by {deployTooltip.deployed_by}</div>}
            {deployTooltip.commit_sha && (
              <div className="deployment-timeline-commit">
                {deployTooltip.commit_sha.slice(0, 8)}
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}

function formatTimeLabel(ms: number): string {
  const d = new Date(ms);
  return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}
```

- [ ] **Step 4: Run unit tests to verify they pass**

```bash
cd apps/frontend && npm test -- --reporter=verbose time-series-graph.test 2>&1 | tail -20
```

Expected: all 10 tests pass.

- [ ] **Step 5: Verify TypeScript compiles**

```bash
cd apps/frontend && npm run build 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/components/ui/time-series-graph.tsx apps/frontend/src/components/ui/time-series-graph.test.tsx
git commit -m "feat(frontend): add TimeSeriesGraph SVG component with deployment marker overlay"
```

---

## Task 7: Wire ServiceDetailPage

**Files:**
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`

- [ ] **Step 1: Update imports**

Replace the top of `apps/frontend/src/pages/ServiceDetailPage.tsx` with:

```tsx
import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams, useSearch } from "@tanstack/react-router";
import { listDeployments } from "../api/deployments";
import {
  getServiceResponseTimeHistory,
  getServiceSummary,
  ServiceSummary,
} from "../api/services";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import {
  TimeSeriesGraph,
  TimeSeriesSeries,
} from "../components/ui/time-series-graph";
import { NlqPanel } from "../features/nlq/NlqPanel";
import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";
import { ServiceInfraPanel } from "../components/ServiceInfraPanel";
import { LogExplorer } from "./LogSearch";
import { TraceExplorer } from "./TraceSearch";
```

- [ ] **Step 2: Update `ServiceSignalTab` type and tab routing**

Replace these two definitions:

```tsx
type ServiceSignalTab = "logs" | "metrics" | "traces";
```

Replace `signalTabFromPath`:

```tsx
function signalTabFromPath(pathname: string): ServiceSignalTab {
  if (pathname.endsWith("/metrics")) return "metrics";
  if (pathname.endsWith("/traces")) return "traces";
  return "logs";
}
```

- [ ] **Step 3: Replace `ServiceSignalTabs` and remove the Overview entry**

Replace the entire `ServiceSignalTabs` function:

```tsx
function ServiceSignalTabs({
  serviceName,
  activeTab,
  lookbackMinutes,
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
  lookbackMinutes: number;
}) {
  const encodedService = encodeURIComponent(serviceName);
  const tabLinks = [
    { tab: "logs" as const,    label: "Logs",    to: "/services/$serviceId/logs" },
    { tab: "metrics" as const, label: "Metrics", to: "/services/$serviceId/metrics" },
    { tab: "traces" as const,  label: "Traces",  to: "/services/$serviceId/traces" },
  ];
  const preservedSearch = { lookback_minutes: lookbackMinutes };

  return (
    <Panel className="overflow-hidden">
      <nav className="modern-signal-tabs" aria-label="Service signals">
        {tabLinks.map((link) => (
          <Link
            key={link.tab}
            to={link.to}
            params={{ serviceId: encodedService }}
            search={preservedSearch}
            className={activeTab === link.tab ? "modern-signal-tab active" : "modern-signal-tab"}
            aria-current={activeTab === link.tab ? "page" : undefined}
          >
            {link.label}
          </Link>
        ))}
      </nav>
      {activeTab === "logs" && (
        <ServiceLogsTab serviceName={serviceName} lookbackMinutes={lookbackMinutes} />
      )}
      {activeTab === "metrics" && <ServiceMetricsWorkspace serviceName={serviceName} />}
      {activeTab === "traces" && (
        <ServiceTracesTab serviceName={serviceName} lookbackMinutes={lookbackMinutes} />
      )}
    </Panel>
  );
}
```

- [ ] **Step 4: Add `ResponseTimeGraphSection`, replace `DeploymentTimelineSection`**

Remove the entire `DeploymentTimelineSection` function and add `ResponseTimeGraphSection` in its place:

```tsx
function ResponseTimeGraphSection({
  serviceName,
  lookbackMinutes,
}: {
  serviceName: string;
  lookbackMinutes: number;
}) {
  const nowMs = Date.now();
  const startMs = nowMs - lookbackMinutes * 60 * 1000;

  const { data: historyData } = useQuery({
    queryKey: ["service-response-time", serviceName, lookbackMinutes],
    queryFn: () =>
      getServiceResponseTimeHistory(serviceName, {
        lookback_minutes: lookbackMinutes,
        buckets: 60,
      }),
  });

  const { data: deploymentData } = useQuery({
    queryKey: ["deployments", serviceName, lookbackMinutes],
    queryFn: () =>
      listDeployments({
        service_name: serviceName,
        start_time: new Date(startMs).toISOString(),
        end_time: new Date(nowMs).toISOString(),
        limit: 20,
      }),
  });

  if (!historyData?.buckets.length) return null;

  const p95Series: TimeSeriesSeries = {
    key: "p95",
    label: "P95",
    color: "#818cf8",
    formatY: (v) => `${Math.round(v)}ms`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.p95_ms })),
  };

  const p50Series: TimeSeriesSeries = {
    key: "p50",
    label: "P50",
    color: "#34d399",
    formatY: (v) => `${Math.round(v)}ms`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.p50_ms })),
  };

  const rateSeries: TimeSeriesSeries = {
    key: "request_rate",
    label: "Req/s",
    color: "#fb923c",
    dashed: true,
    formatY: (v) => `${v.toFixed(1)} rps`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.request_rate })),
  };

  return (
    <TimeSeriesGraph
      series={[p95Series, p50Series, rateSeries]}
      deploymentMarkers={deploymentData?.items ?? []}
      rangeStartMs={startMs}
      rangeEndMs={nowMs}
      eyebrow="Performance"
      title={`Response Time & Throughput — Last ${lookbackMinutes}m`}
      ariaLabel="Service response time and throughput graph"
    />
  );
}
```

- [ ] **Step 5: Update `ServiceDetailView` to use `ResponseTimeGraphSection`**

In `ServiceDetailView`, replace the `<DeploymentTimelineSection ... />` line with:

```tsx
<ResponseTimeGraphSection
  serviceName={service.service_name}
  lookbackMinutes={lookbackMinutes}
/>
```

- [ ] **Step 6: Verify TypeScript compiles and existing tests pass**

```bash
cd apps/frontend && npm run build 2>&1 | tail -5
```

Expected: no TypeScript errors.

```bash
cd apps/frontend && npm test -- --reporter=verbose detail-renovation 2>&1 | tail -15
```

Expected: `UI-R1 detail renovation › keeps service and infrastructure detail surfaces on modern primitives ... ok`

```bash
cd apps/frontend && npm test -- --reporter=verbose ServiceDetailPage 2>&1 | tail -15
```

Expected: all existing ServiceDetailPage tests pass.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "feat(frontend): wire ResponseTimeGraphSection, default tabs to Logs, remove Overview tab"
```

---

## Done

After Task 7 the feature is complete:
- `/services/:id` loads with Logs tab active by default
- Overview content (metric cards + response-time graph + health panel) is always visible above the tabs
- `TimeSeriesGraph` is a standalone reusable component in `components/ui/`
- Deployment markers overlay the response-time graph as dashed vertical lines with diamond heads
