import { useEffect, useRef } from "react";
import * as d3 from "d3";
import type { CorrelatedEvent } from "../generated/pipeline.CorrelatedEvent.v1";

interface Props {
  correlations: CorrelatedEvent[];
}

const MARGIN = { top: 12, right: 16, bottom: 36, left: 56 };

/**
 * Scatter/line chart of correlation lag_ms over time.
 * X = event timestamp, Y = lag_ms between price and transaction events.
 */
export function CorrelationScatter({ correlations }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || correlations.length === 0) return;

    const el = svgRef.current;
    const W = el.clientWidth || 420;
    const H = el.clientHeight || 220;
    const innerW = W - MARGIN.left - MARGIN.right;
    const innerH = H - MARGIN.top - MARGIN.bottom;

    // Sort ascending for left-to-right timeline
    const pts = correlations.slice(0, 80).reverse();

    const svg = d3.select(el);
    svg.selectAll("*").remove();

    const g = svg
      .append("g")
      .attr("transform", `translate(${MARGIN.left},${MARGIN.top})`);

    const x = d3
      .scaleTime()
      .domain(d3.extent(pts, (d) => new Date(d.ts_unix_ms)) as [Date, Date])
      .range([0, innerW]);

    const y = d3
      .scaleLinear()
      .domain([0, d3.max(pts, (d) => d.lag_ms) ?? 5000])
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
    g.append("g")
      .attr("transform", `translate(0,${innerH})`)
      .call(d3.axisBottom(x).ticks(5).tickFormat(d3.timeFormat("%H:%M:%S") as (d: Date | d3.NumberValue) => string))
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
      .text("Lag (ms)");

    // Line
    const line = d3
      .line<CorrelatedEvent>()
      .x((d) => x(new Date(d.ts_unix_ms)))
      .y((d) => y(d.lag_ms))
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
      .attr("cx", (d) => x(new Date(d.ts_unix_ms)))
      .attr("cy", (d) => y(d.lag_ms))
      .attr("r", 3)
      .attr("fill", "#a78bfa")
      .attr("fill-opacity", 0.8);
  }, [correlations]);

  return (
    <div data-testid="correlation-scatter" className="h-full w-full">
      {correlations.length === 0 ? (
        <p className="text-sm text-muted animate-pulse p-4">
          Waiting for correlated events…
        </p>
      ) : (
        <svg ref={svgRef} className="h-full w-full" />
      )}
    </div>
  );
}
