import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { getTopology, listEnvironments, TopologyEdge } from "../api/services";

export default function ServiceOverview() {
  const [environment, setEnvironment] = useState<string>("all");

  const { data: envsData } = useQuery({
    queryKey: ["environments"],
    queryFn: () => listEnvironments(),
  });

  const { data, isLoading, error } = useQuery({
    queryKey: ["topology", environment],
    queryFn: () => getTopology({ environment: environment === "all" ? undefined : environment }),
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
          {envsData?.items.map(env => (
            <option key={env} value={env}>{env}</option>
          ))}
        </select>
      </div>

      <div className="table-panel" style={{ padding: "2rem", overflow: "auto", background: "var(--bg-deep)" }}>
        {isLoading ? (
          <div className="loading-state">Loading topology...</div>
        ) : error ? (
          <div className="signal-empty">Error loading topology: {String(error)}</div>
        ) : !data || data.edges.length === 0 ? (
          <div className="signal-empty">No service relationships found in the selected lookback.</div>
        ) : (
          <div className="topology-map-container" style={{ minHeight: "600px", display: "flex", justifyContent: "center" }}>
             <TopologyMap edges={data.edges} />
          </div>
        )}
      </div>
    </section>
  );
}

function TopologyMap({ edges }: { edges: TopologyEdge[] }) {
  // Simple layout: find all unique services
  const services = Array.from(new Set(edges.flatMap(e => [e.caller, e.callee])));
  
  // Arrange them in a circle
  const radius = 200;
  const centerX = 400;
  const centerY = 300;
  
  const positions = services.reduce((acc, svc, i) => {
    const angle = (i / services.length) * 2 * Math.PI;
    acc[svc] = {
      x: centerX + radius * Math.cos(angle),
      y: centerY + radius * Math.sin(angle)
    };
    return acc;
  }, {} as Record<string, { x: number, y: number }>);

  return (
    <svg width="800" height="600" viewBox="0 0 800 600" style={{ background: "#111", borderRadius: "8px" }}>
      <defs>
        <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="28" refY="3.5" orient="auto">
          <polygon points="0 0, 10 3.5, 0 7" fill="#666" />
        </marker>
        <marker id="arrowhead-error" markerWidth="10" markerHeight="7" refX="28" refY="3.5" orient="auto">
          <polygon points="0 0, 10 3.5, 0 7" fill="#ff4d4d" />
        </marker>
      </defs>
      
      {/* Edges */}
      {edges.map((edge, i) => {
        const start = positions[edge.caller];
        const end = positions[edge.callee];
        if (!start || !end) return null;
        
        const isError = edge.error_rate > 0.05;
        
        return (
          <g key={i}>
            <line
              x1={start.x}
              y1={start.y}
              x2={end.x}
              y2={end.y}
              stroke={isError ? "#ff4d4d" : "#666"}
              strokeWidth={Math.max(1, Math.min(5, 1 + Math.log10(edge.request_count + 1)))}
              markerEnd={isError ? "url(#arrowhead-error)" : "url(#arrowhead)"}
              opacity={0.6}
            />
            <title>{`${edge.caller} -> ${edge.callee}\n${edge.request_count} reqs, ${(edge.error_rate * 100).toFixed(1)}% err, ${edge.p95_latency_ms.toFixed(1)}ms p95`}</title>
          </g>
        );
      })}
      
      {/* Nodes */}
      {services.map(svc => (
        <g key={svc} transform={`translate(${positions[svc].x}, ${positions[svc].y})`}>
          <circle r="30" fill="#222" stroke="#444" strokeWidth="2" />
          <foreignObject x="-25" y="-12" width="50" height="24">
            <div style={{ 
              width: "100%", 
              height: "100%", 
              display: "flex", 
              alignItems: "center", 
              justifyContent: "center",
              fontSize: "9px",
              fontWeight: "bold",
              textAlign: "center",
              overflow: "hidden",
              textOverflow: "ellipsis",
              color: "#fff",
              cursor: "pointer",
              lineHeight: "1"
            }}>
              <Link to="/services/$serviceId" params={{ serviceId: svc }} style={{ color: "inherit", textDecoration: "none" }}>
                {svc}
              </Link>
            </div>
          </foreignObject>
        </g>
      ))}
    </svg>
  );
}
