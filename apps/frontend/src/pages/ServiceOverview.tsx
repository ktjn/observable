import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { getTopology, listEnvironments, type TopologyEdge } from "../api/services";

export default function ServiceOverview() {
  const [environment, setEnvironment] = useState<string>("all");
  const [focusedService, setFocusedService] = useState<string | null>(null);
  const [edgePopover, setEdgePopover] = useState<{
    edge: TopologyEdge;
    x: number;
    y: number;
  } | null>(null);

  const { data: envsData } = useQuery({
    queryKey: ["environments"],
    queryFn: () => listEnvironments(),
  });

  const { data, isLoading, error } = useQuery({
    queryKey: ["topology", environment],
    queryFn: () =>
      getTopology({ environment: environment === "all" ? undefined : environment }),
  });

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Topology</div>
          <h1>Service Overview</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <select
          className="select-input"
          aria-label="Environment filter"
          value={environment}
          onChange={(e) => setEnvironment(e.target.value)}
        >
          <option value="all">All environments</option>
          {envsData?.items.map((env) => (
            <option key={env} value={env}>
              {env}
            </option>
          ))}
        </select>
      </div>

      {focusedService && (
        <div
          className="focused-mode-header"
          style={{ display: "flex", gap: "1rem", alignItems: "center", padding: "0.5rem 0" }}
        >
          <button className="link-button" onClick={() => setFocusedService(null)}>
            ← All services
          </button>
          <span>Viewing: {focusedService}</span>
          <Link to="/services/$serviceId" params={{ serviceId: focusedService }}>
            → Service detail
          </Link>
        </div>
      )}

      <div
        className="table-panel"
        style={{
          padding: "2rem",
          overflow: "auto",
          background: "var(--bg-deep)",
          position: "relative",
        }}
      >
        {isLoading ? (
          <div className="loading-state">Loading topology...</div>
        ) : error ? (
          <div className="signal-empty">Error loading topology: {String(error)}</div>
        ) : !data || data.edges.length === 0 ? (
          <div className="signal-empty">
            No service relationships found in the selected lookback.
          </div>
        ) : (
          <div
            className="topology-map-container"
            style={{
              minHeight: "600px",
              display: "flex",
              justifyContent: "center",
              position: "relative",
            }}
          >
            {edgePopover && (
              <div
                className="edge-popover"
                style={{
                  position: "absolute",
                  left: edgePopover.x,
                  top: edgePopover.y,
                  zIndex: 10,
                  background: "var(--bg-surface, #1a1a1a)",
                  border: "1px solid var(--border, #444)",
                  borderRadius: "4px",
                  padding: "0.5rem",
                  display: "flex",
                  flexDirection: "column",
                  gap: "0.25rem",
                }}
              >
                <a
                  href={`/traces?caller=${encodeURIComponent(edgePopover.edge.caller)}&callee=${encodeURIComponent(edgePopover.edge.callee)}&lookback_minutes=60`}
                >
                  View Traces
                </a>
                <a
                  href={`/logs?service=${encodeURIComponent(edgePopover.edge.caller)}&lookback_minutes=60`}
                >
                  View Logs
                </a>
              </div>
            )}
            <TopologyMap
              edges={data.edges}
              focusedService={focusedService}
              onNodeClick={(svc) => {
                setEdgePopover(null);
                setFocusedService((prev) => (prev === svc ? null : svc));
              }}
              onEdgeClick={(edge, x, y) => setEdgePopover({ edge, x, y })}
              onBackgroundClick={() => setEdgePopover(null)}
            />
          </div>
        )}
      </div>
    </section>
  );
}

interface TopologyMapProps {
  edges: TopologyEdge[];
  focusedService: string | null;
  onNodeClick: (svc: string) => void;
  onEdgeClick: (edge: TopologyEdge, x: number, y: number) => void;
  onBackgroundClick: () => void;
}

function TopologyMap({
  edges,
  focusedService,
  onNodeClick,
  onEdgeClick,
  onBackgroundClick,
}: TopologyMapProps) {
  const services = Array.from(new Set(edges.flatMap((e) => [e.caller, e.callee])));

  const radius = 200;
  const centerX = 400;
  const centerY = 300;

  const positions = services.reduce(
    (acc, svc, i) => {
      const angle = (i / services.length) * 2 * Math.PI;
      acc[svc] = {
        x: centerX + radius * Math.cos(angle),
        y: centerY + radius * Math.sin(angle),
      };
      return acc;
    },
    {} as Record<string, { x: number; y: number }>,
  );

  const connectedServices = focusedService
    ? new Set(
        edges
          .filter((e) => e.caller === focusedService || e.callee === focusedService)
          .flatMap((e) => [e.caller, e.callee]),
      )
    : null;

  return (
    <svg
      width="800"
      height="600"
      viewBox="0 0 800 600"
      style={{ background: "#111", borderRadius: "8px" }}
    >
      <defs>
        <marker
          id="arrowhead"
          markerWidth="10"
          markerHeight="7"
          refX="28"
          refY="3.5"
          orient="auto"
        >
          <polygon points="0 0, 10 3.5, 0 7" fill="#666" />
        </marker>
        <marker
          id="arrowhead-error"
          markerWidth="10"
          markerHeight="7"
          refX="28"
          refY="3.5"
          orient="auto"
        >
          <polygon points="0 0, 10 3.5, 0 7" fill="#ff4d4d" />
        </marker>
      </defs>

      {/* Background rect — catches clicks on empty space to dismiss popover */}
      <rect
        width="800"
        height="600"
        fill="transparent"
        data-testid="topology-background"
        onClick={onBackgroundClick}
      />

      {/* Edges */}
      {edges.map((edge, i) => {
        const start = positions[edge.caller];
        const end = positions[edge.callee];
        if (!start || !end) return null;

        const isError = edge.error_rate > 0.05;
        const isActive =
          !focusedService ||
          edge.caller === focusedService ||
          edge.callee === focusedService;

        const midX = (start.x + end.x) / 2;
        const midY = (start.y + end.y) / 2;

        return (
          <g
            key={i}
            role="button"
            aria-label={`${edge.caller} to ${edge.callee}`}
            style={{ cursor: "pointer" }}
            onClick={(e) => {
              e.stopPropagation();
              onEdgeClick(edge, midX, midY);
            }}
          >
            <line
              x1={start.x}
              y1={start.y}
              x2={end.x}
              y2={end.y}
              stroke={isError ? "#ff4d4d" : "#666"}
              strokeWidth={Math.max(1, Math.min(5, 1 + Math.log10(edge.request_count + 1)))}
              markerEnd={isError ? "url(#arrowhead-error)" : "url(#arrowhead)"}
              opacity={isActive ? 0.6 : 0.15}
            />
            {/* Wide transparent hit target for easier clicking */}
            <line
              x1={start.x}
              y1={start.y}
              x2={end.x}
              y2={end.y}
              stroke="transparent"
              strokeWidth={16}
            />
            <title>{`${edge.caller} → ${edge.callee}\n${edge.request_count} reqs, ${(edge.error_rate * 100).toFixed(1)}% err, ${edge.p95_latency_ms.toFixed(1)}ms p95`}</title>
          </g>
        );
      })}

      {/* Nodes */}
      {services.map((svc) => {
        const pos = positions[svc];
        const isActive = !focusedService || (connectedServices?.has(svc) ?? true);

        return (
          <g
            key={svc}
            role="button"
            aria-label={svc}
            transform={`translate(${pos.x}, ${pos.y})`}
            style={{ cursor: "pointer" }}
            onClick={(e) => {
              e.stopPropagation();
              onNodeClick(svc);
            }}
          >
            <circle
              r="30"
              fill={focusedService === svc ? "#1a3a5c" : "#222"}
              stroke={focusedService === svc ? "#4a9edd" : "#444"}
              strokeWidth="2"
              opacity={isActive ? 1 : 0.2}
            />
            <text
              textAnchor="middle"
              dominantBaseline="middle"
              fontSize="9"
              fontWeight="bold"
              fill="#fff"
              opacity={isActive ? 1 : 0.2}
            >
              {svc}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
