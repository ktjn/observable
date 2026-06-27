import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { PriceEvent } from "../generated/pipeline.PriceEvent.v1";

interface Props {
  prices: PriceEvent[];
}

const MARGIN = { top: 24, right: 80, bottom: 36, left: 60 };

const INTERVALS = [
  { label: "1s",  ms: 1_000 },
  { label: "10s", ms: 10_000 },
  { label: "30s", ms: 30_000 },
  { label: "1m",  ms: 60_000 },
] as const;

type IntervalMs = (typeof INTERVALS)[number]["ms"];

/** Colour palette keyed by asset symbol prefix */
const ASSET_COLORS: Record<string, string> = {
  BTC: "#f59e0b",  // amber
  ETH: "#6366f1",  // indigo
  SOL: "#22c55e",  // green
  BNB: "#eab308",  // yellow
  XRP: "#06b6d4",  // cyan
};
const FALLBACK_COLORS = ["#ec4899", "#8b5cf6", "#14b8a6", "#f97316", "#64748b"];

function assetColor(asset: string, index: number): string {
  const upper = asset.toUpperCase();
  for (const key of Object.keys(ASSET_COLORS)) {
    if (upper.startsWith(key)) return ASSET_COLORS[key];
  }
  return FALLBACK_COLORS[index % FALLBACK_COLORS.length];
}

interface AssetBucket {
  ts: number;
  close: number;  // last price_usd in bucket
}

function aggregateByAsset(
  prices: PriceEvent[],
  intervalMs: IntervalMs,
): Map<string, AssetBucket[]> {
  // per-asset, per-bucket: keep track of all prices; "close" = last by ts_unix_ms
  const map = new Map<string, Map<number, { close: number; lastTs: number }>>();
  for (const p of prices) {
    const bucket = Math.floor(p.ts_unix_ms / intervalMs) * intervalMs;
    if (!map.has(p.asset)) map.set(p.asset, new Map());
    const assetMap = map.get(p.asset)!;
    const existing = assetMap.get(bucket);
    if (!existing || p.ts_unix_ms >= existing.lastTs) {
      assetMap.set(bucket, { close: p.price_usd, lastTs: p.ts_unix_ms });
    }
  }

  const result = new Map<string, AssetBucket[]>();
  for (const [asset, bucketMap] of map.entries()) {
    const buckets = Array.from(bucketMap.entries())
      .map(([ts, v]) => ({ ts, close: v.close }))
      .sort((a, b) => a.ts - b.ts);
    result.set(asset, buckets);
  }
  return result;
}

/**
 * Combo chart: stacked bars showing closing price per asset per time bucket,
 * with per-asset price lines overlaid on a shared log y-axis.
 * A combobox controls the bucket width (1s / 10s / 30s / 1m).
 */
export function PriceChart({ prices }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [intervalMs, setIntervalMs] = useState<IntervalMs>(1_000);
  const [tick, setTick] = useState(0);

  const pricesRef = useRef(prices);
  useEffect(() => { pricesRef.current = prices; }, [prices]);

  useEffect(() => {
    setTick((t) => t + 1);
    const id = setInterval(() => setTick((t) => t + 1), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  useEffect(() => {
    const prices = pricesRef.current;
    if (!svgRef.current || prices.length === 0) return;

    const byAsset = aggregateByAsset(prices, intervalMs);
    const assets = Array.from(byAsset.keys()).sort();
    if (assets.length === 0) return;

    // Collect all unique bucket timestamps across all assets
    const allTs = Array.from(
      new Set(Array.from(byAsset.values()).flatMap((bs) => bs.map((b) => b.ts))),
    ).sort((a, b) => a - b);
    if (allTs.length < 2) return;

    // Build stacked data: each row is a time bucket, each key is an asset's close price
    type Row = { ts: number } & Record<string, number>;
    const rows: Row[] = allTs.map((ts) => {
      const row: Row = { ts };
      for (const asset of assets) {
        const bucket = byAsset.get(asset)?.find((b) => b.ts === ts);
        row[asset] = bucket?.close ?? 0;
      }
      return row;
    });

    const stack = d3.stack<Row>().keys(assets).value((d, key) => d[key] ?? 0);
    const series = stack(rows);

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

    // X axis — time bands (one band per bucket)
    const x = d3
      .scaleBand()
      .domain(allTs.map(String))
      .range([0, innerW])
      .padding(0.15);

    // Y axis — linear, domain = [0, max stacked total]
    const maxTotal = d3.max(rows, (row) =>
      assets.reduce((sum, a) => sum + (row[a] ?? 0), 0),
    ) ?? 100;
    const y = d3
      .scaleLinear()
      .domain([0, maxTotal * 1.08])
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

    // Stacked bars
    for (const layer of series) {
      const asset = layer.key;
      const color = assetColor(asset, assets.indexOf(asset));
      g.append("g")
        .selectAll("rect")
        .data(layer)
        .join("rect")
        .attr("x", (d) => x(String(d.data.ts)) ?? 0)
        .attr("y", (d) => y(d[1]))
        .attr("height", (d) => Math.max(0, y(d[0]) - y(d[1])))
        .attr("width", x.bandwidth())
        .attr("fill", color)
        .attr("fill-opacity", 0.35);
    }

    // Per-asset price lines (closing price)
    for (const [i, asset] of assets.entries()) {
      const color = assetColor(asset, i);
      const assetBuckets = byAsset.get(asset) ?? [];
      if (assetBuckets.length < 2) continue;

      // For the line we need the cumulative baseline at this asset's layer
      // to draw the line at the top of the asset's bar segment.
      // Simpler: use the raw close price (y-axis is per-stack total, so the
      // line will sit near the top of the asset's segment).
      const lineGen = d3
        .line<AssetBucket>()
        .x((d) => (x(String(d.ts)) ?? 0) + x.bandwidth() / 2)
        .y((d) => y(d.close))
        .curve(d3.curveMonotoneX);

      g.append("path")
        .datum(assetBuckets)
        .attr("fill", "none")
        .attr("stroke", color)
        .attr("stroke-width", 1.5)
        .attr("d", lineGen);
    }

    // X axis — show only a few tick labels to avoid overlap
    const fmt = intervalMs >= 60_000
      ? d3.timeFormat("%H:%M")
      : d3.timeFormat("%H:%M:%S");
    const tickStep = Math.max(1, Math.floor(allTs.length / 5));
    const xAxisTicks = allTs.filter((_, i) => i % tickStep === 0);
    g.append("g")
      .attr("transform", `translate(0,${innerH})`)
      .call(
        d3.axisBottom(x)
          .tickValues(xAxisTicks.map(String))
          .tickFormat((d) => fmt(new Date(Number(d)))),
      )
      .call((ax) => ax.select(".domain").attr("stroke", "#334155"))
      .call((ax) => ax.selectAll("text").attr("fill", "#94a3b8").attr("font-size", 10));

    // Y axis
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

    // Legend — top right inside the chart
    const legend = svg
      .append("g")
      .attr("transform", `translate(${MARGIN.left + innerW + 8}, ${MARGIN.top})`);
    for (const [i, asset] of assets.entries()) {
      const color = assetColor(asset, i);
      const gy = legend.append("g").attr("transform", `translate(0,${i * 16})`);
      gy.append("circle").attr("r", 4).attr("fill", color).attr("cx", 4).attr("cy", 4);
      gy.append("text")
        .attr("x", 12)
        .attr("y", 8)
        .attr("fill", "#94a3b8")
        .attr("font-size", 10)
        .text(asset);
    }
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

