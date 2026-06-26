import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { PriceEvent } from "../generated/pipeline.PriceEvent.v1";

interface Props {
  prices: PriceEvent[];
}

const MARGIN = { top: 12, right: 16, bottom: 36, left: 60 };

const INTERVALS = [
  { label: "1s",  ms: 1_000 },
  { label: "10s", ms: 10_000 },
  { label: "30s", ms: 30_000 },
  { label: "1m",  ms: 60_000 },
] as const;

type IntervalMs = (typeof INTERVALS)[number]["ms"];

interface Bucket {
  ts: number;      // bucket start time (ms)
  price: number;   // average price_usd in bucket
}

function aggregate(prices: PriceEvent[], intervalMs: IntervalMs): Bucket[] {
  const map = new Map<number, number[]>();
  for (const p of prices) {
    const bucket = Math.floor(p.ts_unix_ms / intervalMs) * intervalMs;
    const arr = map.get(bucket) ?? [];
    arr.push(p.price_usd);
    map.set(bucket, arr);
  }
  return Array.from(map.entries())
    .map(([ts, vals]) => ({ ts, price: vals.reduce((a, b) => a + b, 0) / vals.length }))
    .sort((a, b) => a.ts - b.ts);
}

/**
 * Line chart showing price_usd aggregated over a selectable time interval.
 * A combobox above-right controls the bucket width (1s / 10s / 30s / 1m).
 * The chart only redraws at the cadence of the selected interval.
 */
export function PriceChart({ prices }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [intervalMs, setIntervalMs] = useState<IntervalMs>(1_000);
  const [tick, setTick] = useState(0);

  // Keep a ref to prices so the interval callback always has the latest data
  // without needing to be recreated on every render.
  const pricesRef = useRef(prices);
  useEffect(() => { pricesRef.current = prices; }, [prices]);

  // Fire a redraw tick at the cadence of the selected interval.
  useEffect(() => {
    // Immediate draw when interval changes
    setTick((t) => t + 1);
    const id = setInterval(() => setTick((t) => t + 1), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  useEffect(() => {
    const prices = pricesRef.current;
    if (!svgRef.current || prices.length === 0) return;

    const pts = aggregate(prices, intervalMs);
    if (pts.length < 2) return;

    const el = svgRef.current;
    const W = el.clientWidth || 500;
    const H = el.clientHeight || 200;
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

    const [minP, maxP] = d3.extent(prices, (d) => d.price_usd) as [number, number];
    const pad = (maxP - minP) * 0.1 || 500;
    const y = d3
      .scaleLinear()
      .domain([minP - pad, maxP + pad])
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
      .call(d3.axisLeft(y).ticks(4).tickFormat((d) => `$${((d as number) / 1000).toFixed(0)}k`))
      .call((ax) => ax.select(".domain").attr("stroke", "#334155"))
      .call((ax) => ax.selectAll("text").attr("fill", "#94a3b8").attr("font-size", 10));

    // Y-axis label
    g.append("text")
      .attr("transform", "rotate(-90)")
      .attr("x", -innerH / 2)
      .attr("y", -48)
      .attr("text-anchor", "middle")
      .attr("fill", "#64748b")
      .attr("font-size", 11)
      .text("Price (USD)");

    // Area + line
    const line = d3
      .line<Bucket>()
      .x((d) => x(new Date(d.ts)))
      .y((d) => y(d.price))
      .curve(d3.curveMonotoneX);

    const area = d3
      .area<Bucket>()
      .x((d) => x(new Date(d.ts)))
      .y0(innerH)
      .y1((d) => y(d.price))
      .curve(d3.curveMonotoneX);

    const gradId = "price-area-grad";
    const defs = svg.append("defs");
    const grad = defs.append("linearGradient")
      .attr("id", gradId)
      .attr("x1", "0").attr("y1", "0")
      .attr("x2", "0").attr("y2", "1");
    grad.append("stop").attr("offset", "0%").attr("stop-color", "#22d3ee").attr("stop-opacity", 0.25);
    grad.append("stop").attr("offset", "100%").attr("stop-color", "#22d3ee").attr("stop-opacity", 0.01);

    g.append("path").datum(pts).attr("fill", `url(#${gradId})`).attr("d", area);
    g.append("path").datum(pts).attr("fill", "none").attr("stroke", "#22d3ee").attr("stroke-width", 1.5).attr("d", line);
  }, [tick, intervalMs]);

  return (
    <div data-testid="price-chart" className="flex h-full w-full flex-col gap-1">
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
        {prices.length === 0 ? (
          <p className="text-sm text-muted animate-pulse p-4">Waiting for price events…</p>
        ) : (
          <svg ref={svgRef} className="h-full w-full" />
        )}
      </div>
    </div>
  );
}
