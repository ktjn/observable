import { useEffect, useRef } from "react";
import * as d3 from "d3";
import type { CorrelatedEvent } from "../generated/pipeline.CorrelatedEvent.v1";

interface Props {
  correlations: CorrelatedEvent[];
}

const MARGIN = { top: 12, right: 16, bottom: 36, left: 56 };

/**
 * Scatterplot of lag_ms (x) vs price_usd (y), coloured by price source.
 * Each dot is a CorrelatedEvent.
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

    const svg = d3.select(el);
    svg.selectAll("*").remove();

    const g = svg
      .append("g")
      .attr("transform", `translate(${MARGIN.left},${MARGIN.top})`);

    const x = d3
      .scaleLinear()
      .domain([0, d3.max(correlations, (d) => d.lag_ms) ?? 5000])
      .nice()
      .range([0, innerW]);

    const y = d3
      .scaleLinear()
      .domain([0, d3.max(correlations, (d) => d.price_usd) ?? 100000])
      .nice()
      .range([innerH, 0]);

    const color = d3
      .scaleOrdinal<string>()
      .domain(["DexPaprika", "Coinbase"])
      .range(["#a78bfa", "#22d3ee"]);

    // Axes
    g.append("g")
      .attr("transform", `translate(0,${innerH})`)
      .call(d3.axisBottom(x).ticks(5).tickFormat((d) => `${d}ms`))
      .call((ax) => ax.select(".domain").attr("stroke", "#334155"))
      .call((ax) => ax.selectAll("text").attr("fill", "#94a3b8").attr("font-size", 10));

    g.append("g")
      .call(d3.axisLeft(y).ticks(4).tickFormat((d) => `$${(d as number / 1000).toFixed(0)}k`))
      .call((ax) => ax.select(".domain").attr("stroke", "#334155"))
      .call((ax) => ax.selectAll("text").attr("fill", "#94a3b8").attr("font-size", 10));

    // Axis labels
    g.append("text")
      .attr("x", innerW / 2)
      .attr("y", innerH + 30)
      .attr("text-anchor", "middle")
      .attr("fill", "#64748b")
      .attr("font-size", 11)
      .text("Lag (ms)");

    g.append("text")
      .attr("transform", "rotate(-90)")
      .attr("x", -innerH / 2)
      .attr("y", -42)
      .attr("text-anchor", "middle")
      .attr("fill", "#64748b")
      .attr("font-size", 11)
      .text("Price (USD)");

    // Dots
    g.selectAll("circle")
      .data(correlations.slice(-80))
      .join("circle")
      .attr("cx", (d) => x(d.lag_ms))
      .attr("cy", (d) => y(d.price_usd))
      .attr("r", 4)
      .attr("fill", (d) => color(d.price_source))
      .attr("fill-opacity", 0.7);
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
