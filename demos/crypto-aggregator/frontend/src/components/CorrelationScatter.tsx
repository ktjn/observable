import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { CorrelatedEvent } from "../generated/pipeline.CorrelatedEvent.v1";

interface Props {
  correlations: CorrelatedEvent[];
}

const MARGIN = { top: 12, right: 16, bottom: 36, left: 56 };

const INTERVALS = [
  { label: "1s",  ms: 1_000 },
  { label: "10s", ms: 10_000 },
  { label: "30s", ms: 30_000 },
  { label: "1m",  ms: 60_000 },
] as const;

type IntervalMs = (typeof INTERVALS)[number]["ms"];

interface LagBucket {
  ts: number;
  avgLag: number;
}

function aggregateLag(correlations: CorrelatedEvent[], intervalMs: IntervalMs): LagBucket[] {
  const map = new Map<number, number[]>();
  for (const c of correlations) {
    const bucket = Math.floor(c.ts_unix_ms / intervalMs) * intervalMs;
    const arr = map.get(bucket) ?? [];
    arr.push(c.lag_ms);
    map.set(bucket, arr);
  }
  return Array.from(map.entries())
    .map(([ts, vals]) => ({ ts, avgLag: vals.reduce((a, b) => a + b, 0) / vals.length }))
    .sort((a, b) => a.ts - b.ts);
}

/**
 * Line chart showing correlation lag_ms aggregated over a selectable time interval.
 * Events are sorted by ts_unix_ms so out-of-order SSE delivery doesn't zigzag the line.
 */
export function CorrelationScatter({ correlations }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [intervalMs, setIntervalMs] = useState<IntervalMs>(1_000);
  const [tick, setTick] = useState(0);

  const correlationsRef = useRef(correlations);
  useEffect(() => { correlationsRef.current = correlations; }, [correlations]);

  useEffect(() => {
    setTick((t) => t + 1);
    const id = setInterval(() => setTick((t) => t + 1), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  useEffect(() => {
    const correlations = correlationsRef.current;
    if (!svgRef.current || correlations.length === 0) return;

    const pts = aggregateLag(correlations, intervalMs);
    if (pts.length < 2) return;

    const el = svgRef.current;
    const W = el.clientWidth || 420;
    const H = el.clientHeight || 220;
    const innerW = W - MARGIN.left - MARGIN.right;
    const innerH = H - MARGIN.top - MARGIN.bottom;

    const svg = d3.select(el);
    svg.selectAll("*").remove();

    const g = svg
      .append("g")
      .attr("transform", `translate(${MARGIN.left},${MARGIN.top})`);

    const x = d3
      .scaleTime()
      .domain(d3.extent(pts, (d) => new Date(d.ts)) as [Date, Date])
      .range([0, innerW]);

    const y = d3
      .scaleLinear()
      .domain([0, d3.max(pts, (d) => d.avgLag) ?? 5000])
      .nice()
      .range([innerH, 0]);

    // Grid lines
    g.append("g")
      .call(
        d3.axisLeft(y)
          .ticks(4)
          .tickSize(-innerW)
          .tickFormat(() => ""),
      )
      .call((ax) => ax.select(".domain").remove())
      .call((ax) => ax.selectAll("line").attr("stroke", "#1e293b").attr("stroke-dasharray", "3,3"));

    // Axes
    const fmt = intervalMs >= 60_000
      ? d3.timeFormat("%H:%M")
      : d3.timeFormat("%H:%M:%S");
    g.append("g")
      .attr("transform", `translate(0,${innerH})`)
      .call(d3.axisBottom(x).ticks(5).tickFormat(fmt as (d: Date | d3.NumberValue) => string))
      .call((ax) => ax.select(".domain").attr("stroke", "#334155"))
      .call((ax) => ax.selectAll("text").attr("fill", "#94a3b8").attr("font-size", 10));

    g.append("g")
      .call(d3.axisLeft(y).ticks(4).tickFormat((d) => `${d}ms`))
      .call((ax) => ax.select(".domain").attr("stroke", "#334155"))
      .call((ax) => ax.selectAll("text").attr("fill", "#94a3b8").attr("font-size", 10));

    // Axis label
    g.append("text")
      .attr("transform", "rotate(-90)")
      .attr("x", -innerH / 2)
      .attr("y", -42)
      .attr("text-anchor", "middle")
      .attr("fill", "#64748b")
      .attr("font-size", 11)
      .text("Avg Lag (ms)");

    // Line
    const line = d3
      .line<LagBucket>()
      .x((d) => x(new Date(d.ts)))
      .y((d) => y(d.avgLag))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(pts)
      .attr("fill", "none")
      .attr("stroke", "#a78bfa")
      .attr("stroke-width", 1.5)
      .attr("d", line);

    // Dots
    g.selectAll("circle")
      .data(pts)
      .join("circle")
      .attr("cx", (d) => x(new Date(d.ts)))
      .attr("cy", (d) => y(d.avgLag))
      .attr("r", 3)
      .attr("fill", "#a78bfa")
      .attr("fill-opacity", 0.8);
  }, [tick, intervalMs]);

  return (
    <div data-testid="correlation-scatter" className="flex h-full w-full flex-col gap-1">
      {/* Interval selector */}
      <div className="flex justify-end">
        <select
          value={intervalMs}
          onChange={(e) => setIntervalMs(Number(e.target.value) as IntervalMs)}
          className="rounded border border-slate-700 bg-card px-2 py-0.5 text-xs text-muted focus:outline-none focus:ring-1 focus:ring-accent"
          aria-label="Aggregation interval"
        >
          {INTERVALS.map(({ label, ms }) => (
            <option key={ms} value={ms}>{label}</option>
          ))}
        </select>
      </div>
      {/* Chart */}
      <div className="min-h-0 flex-1">
        {correlations.length === 0 ? (
          <p className="text-sm text-muted animate-pulse p-4">
            Waiting for correlated events…
          </p>
        ) : (
          <svg ref={svgRef} className="h-full w-full" />
        )}
      </div>
    </div>
  );
}

