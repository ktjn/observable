import { useEffect, useRef } from "react";
import * as d3 from "d3";
import type { PriceEvent } from "../generated/pipeline.PriceEvent.v1";

interface Props {
  prices: PriceEvent[];
}

const MARGIN = { top: 12, right: 16, bottom: 36, left: 60 };
const MAX_POINTS = 60;

/**
 * Line chart showing price_usd over time for the most recent price events.
 */
export function PriceChart({ prices }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || prices.length === 0) return;

    const el = svgRef.current;
    const W = el.clientWidth || 500;
    const H = el.clientHeight || 200;
    const innerW = W - MARGIN.left - MARGIN.right;
    const innerH = H - MARGIN.top - MARGIN.bottom;

    // Take last MAX_POINTS sorted by ts ascending for left-to-right timeline
    const pts = prices.slice(0, MAX_POINTS).reverse();

    const svg = d3.select(el);
    svg.selectAll("*").remove();

    const g = svg
      .append("g")
      .attr("transform", `translate(${MARGIN.left},${MARGIN.top})`);

    const x = d3
      .scaleTime()
      .domain(d3.extent(pts, (d) => new Date(d.ts_unix_ms)) as [Date, Date])
      .range([0, innerW]);

    const [minP, maxP] = d3.extent(pts, (d) => d.price_usd) as [number, number];
    const pad = (maxP - minP) * 0.1 || 100;
    const y = d3
      .scaleLinear()
      .domain([minP - pad, maxP + pad])
      .nice()
      .range([innerH, 0]);

    // Grid lines
    g.append("g")
      .attr("class", "grid")
      .call(
        d3.axisLeft(y)
          .ticks(4)
          .tickSize(-innerW)
          .tickFormat(() => ""),
      )
      .call((ax) => ax.select(".domain").remove())
      .call((ax) => ax.selectAll("line").attr("stroke", "#1e293b").attr("stroke-dasharray", "3,3"));

    // Axes
    g.append("g")
      .attr("transform", `translate(0,${innerH})`)
      .call(d3.axisBottom(x).ticks(5).tickFormat(d3.timeFormat("%H:%M:%S") as (d: Date | d3.NumberValue) => string))
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

    // Line
    const line = d3
      .line<PriceEvent>()
      .x((d) => x(new Date(d.ts_unix_ms)))
      .y((d) => y(d.price_usd))
      .curve(d3.curveMonotoneX);

    // Area fill
    const area = d3
      .area<PriceEvent>()
      .x((d) => x(new Date(d.ts_unix_ms)))
      .y0(innerH)
      .y1((d) => y(d.price_usd))
      .curve(d3.curveMonotoneX);

    const gradId = "price-area-grad";
    const defs = svg.append("defs");
    const grad = defs.append("linearGradient")
      .attr("id", gradId)
      .attr("x1", "0").attr("y1", "0")
      .attr("x2", "0").attr("y2", "1");
    grad.append("stop").attr("offset", "0%").attr("stop-color", "#22d3ee").attr("stop-opacity", 0.25);
    grad.append("stop").attr("offset", "100%").attr("stop-color", "#22d3ee").attr("stop-opacity", 0.01);

    g.append("path")
      .datum(pts)
      .attr("fill", `url(#${gradId})`)
      .attr("d", area);

    g.append("path")
      .datum(pts)
      .attr("fill", "none")
      .attr("stroke", "#22d3ee")
      .attr("stroke-width", 1.5)
      .attr("d", line);
  }, [prices]);

  return (
    <div data-testid="price-chart" className="h-full w-full">
      {prices.length === 0 ? (
        <p className="text-sm text-muted animate-pulse p-4">Waiting for price events…</p>
      ) : (
        <svg ref={svgRef} className="h-full w-full" />
      )}
    </div>
  );
}
