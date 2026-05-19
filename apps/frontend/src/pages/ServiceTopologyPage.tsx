import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { getTopology, listServices, type TopologyEdge } from "../api/services";
import { Button } from "../components/ui/button";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
import { deriveViewFiltersFromIr, type NlqIrLike } from "../features/nlq/queryFilters";
import { useTenantContext } from "../hooks/useTenantContext";
import { LogExplorer } from "./LogSearch";

const TOPOLOGY_BASE_IR: NlqIrLike = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

export default function ServiceTopologyPage() {
  const [environment, setEnvironment] = useState<string>("all");
  const [focusedService, setFocusedService] = useState<string | null>(null);
  const [edgePopover, setEdgePopover] = useState<{
    edge: TopologyEdge;
    x: number;
    y: number;
  } | null>(null);
  const { tenantId } = useTenantContext();

  const { data, isLoading, error } = useQuery({
    queryKey: ["topology", tenantId, environment],
    queryFn: () =>
      getTopology(tenantId, { environment: environment === "all" ? undefined : environment }),
  });

  const { data: servicesData } = useQuery({
    queryKey: ["services", tenantId],
    queryFn: () => listServices(tenantId),
  });

  const allServiceNames = (servicesData?.items ?? []).filter((s) => s !== "");

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Topology</div>
          <h1>Service Overview</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <QueryFilterInput
          baseIr={TOPOLOGY_BASE_IR}
          placeholder='Focus topology, e.g. "prod payments service" or raw NLQ IR JSON'
          onIr={(ir) => {
            const filters = deriveViewFiltersFromIr(ir, "topology");
            setEnvironment(filters.environment ?? "all");
            setFocusedService(filters.service ?? null);
            setEdgePopover(null);
          }}
        />
      </div>

      {focusedService && (
        <div className="flex gap-4 items-center py-2">
          <Button variant="secondary" onClick={() => setFocusedService(null)}>
            ← All services
          </Button>
          <span>Viewing: {focusedService}</span>
          <Link to="/services/$serviceId" params={{ serviceId: focusedService }}>
            → Service detail
          </Link>
        </div>
      )}

      <TablePanel className="overflow-hidden relative bg-[var(--surface-inset)] h-[calc(100vh-12rem)]">
        {isLoading ? (
          <LoadingState>Loading topology…</LoadingState>
        ) : error ? (
          <div className="signal-empty">Error loading topology: {String(error)}</div>
        ) : (
          <div className="relative h-full w-full flex flex-col">
            {/* Popover uses SVG midpoint coordinates transformed by zoom. */}
            {edgePopover && (
              <div
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
                  href={`/traces?caller=${encodeURIComponent(edgePopover.edge.caller)}&callee=${encodeURIComponent(edgePopover.edge.callee)}`}
                >
                  View Traces
                </a>
                <a
                  href={`/logs?service=${encodeURIComponent(edgePopover.edge.caller)}`}
                >
                  View Logs
                </a>
              </div>
            )}
            {allServiceNames.length === 0 ? (
              <div className="signal-empty">No services found in the selected time range.</div>
            ) : (
              <div className="flex flex-col flex-1 gap-2 min-h-0">
                {(!data || data.edges.length === 0) && (
                  <p className="text-xs text-[var(--muted)] shrink-0">
                    No observed call relationships yet — services shown as standalone nodes.
                  </p>
                )}
                <div className="flex-1 min-h-0">
                  <TopologyMap
                    edges={data?.edges ?? []}
                    allServices={allServiceNames}
                    focusedService={focusedService}
                    onNodeClick={(svc) => {
                      setEdgePopover(null);
                      setFocusedService((prev) => (prev === svc ? null : svc));
                    }}
                    onEdgeClick={(edge, x, y) => setEdgePopover({ edge, x, y })}
                    onBackgroundClick={() => setEdgePopover(null)}
                  />
                </div>
              </div>
            )}
          </div>
        )}
      </TablePanel>

      {focusedService && (
        <section aria-label="Focused service logs">
          <LogExplorer
            key={focusedService}
            initialService={focusedService}
            lockedService
            showHeader={false}
            showServiceColumn={false}
            showPromote={false}
            tableAriaLabel="Focused service logs"
          />
        </section>
      )}
    </section>
  );
}

interface TopologyMapProps {
  edges: TopologyEdge[];
  allServices: string[];
  focusedService: string | null;
  onNodeClick: (svc: string) => void;
  onEdgeClick: (edge: TopologyEdge, x: number, y: number) => void;
  onBackgroundClick: () => void;
}

interface SimNode extends d3.SimulationNodeDatum {
  id: string;
}

interface SimLink extends d3.SimulationLinkDatum<SimNode> {
  edge: TopologyEdge;
}

const NODE_R = 30;

function TopologyMap({
  edges,
  allServices,
  focusedService,
  onNodeClick,
  onEdgeClick,
  onBackgroundClick,
}: TopologyMapProps) {
  const services =
    allServices.length > 0
      ? allServices
      : Array.from(new Set(edges.flatMap((e) => [e.caller, e.callee])));

  // Track actual rendered size so the SVG fills its container.
  const wrapperRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ w: 800, h: 600 });

  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      if (width > 0 && height > 0) setDims({ w: Math.round(width), h: Math.round(height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const { w, h } = dims;

  // Stable node positions in a ref so D3 simulation can mutate them.
  const nodesRef = useRef<SimNode[]>([]);
  const simRef = useRef<d3.Simulation<SimNode, SimLink> | null>(null);
  const [, forceRender] = useState(0);
  const svgRef = useRef<SVGSVGElement>(null);
  const [transform, setTransform] = useState<d3.ZoomTransform>(d3.zoomIdentity);

  // Rebuild simulation whenever services, edges, or container size changes.
  useEffect(() => {
    const prevById = new Map(nodesRef.current.map((n) => [n.id, n]));
    const nodes: SimNode[] = services.map((id) => {
      const prev = prevById.get(id);
      return prev ? { ...prev } : { id };
    });

    const links: SimLink[] = edges
      .filter((e) => services.includes(e.caller) && services.includes(e.callee))
      .map((edge) => ({ source: edge.caller, target: edge.callee, edge }));

    const simulation = d3
      .forceSimulation<SimNode>(nodes)
      .force(
        "link",
        d3
          .forceLink<SimNode, SimLink>(links)
          .id((d) => d.id)
          .distance(120),
      )
      .force("charge", d3.forceManyBody<SimNode>().strength(-300))
      .force("center", d3.forceCenter(w / 2, h / 2))
      .force("collide", d3.forceCollide<SimNode>(NODE_R + 10))
      .alphaDecay(0.02)
      .on("tick", () => {
        nodesRef.current = [...nodes];
        forceRender((n) => n + 1);
      });

    simRef.current = simulation;

    // Warm up a few ticks so nodes aren't all piled at (0,0), but keep
    // enough alpha that the animated settling is visible in the browser.
    simulation.tick(10);
    nodesRef.current = [...nodes];
    forceRender((n) => n + 1);

    return () => {
      simulation.stop();
    };
  }, [services.join(","), edges.map((e) => `${e.caller}→${e.callee}`).join(","), w, h]); // stable primitive deps

  // Update force center when size changes without rebuilding simulation.
  useEffect(() => {
    simRef.current?.force("center", d3.forceCenter(w / 2, h / 2));
    simRef.current?.alpha(0.3).restart();
  }, [w, h]);

  // Zoom & pan.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const zoom = d3
      .zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.2, 4])
      .on("zoom", (event: d3.D3ZoomEvent<SVGSVGElement, unknown>) => {
        setTransform(event.transform);
      });
    d3.select(svg).call(zoom);
    return () => {
      d3.select(svg).on(".zoom", null);
    };
  }, []);

  const posById = new Map(nodesRef.current.map((n) => [n.id, { x: n.x ?? w / 2, y: n.y ?? h / 2 }]));

  const connectedServices = focusedService
    ? new Set(
        edges
          .filter((e) => e.caller === focusedService || e.callee === focusedService)
          .flatMap((e) => [e.caller, e.callee]),
      )
    : null;

  return (
    <div ref={wrapperRef} style={{ width: "100%", height: "100%" }}>
    <svg
      ref={svgRef}
      width="100%"
      height="100%"
      viewBox={`0 0 ${w} ${h}`}
      style={{ background: "#111", borderRadius: "8px", display: "block" }}
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
        width={w}
        height={h}
        fill="transparent"
        data-testid="topology-background"
        onClick={onBackgroundClick}
      />

      <g transform={transform.toString()}>
        {/* Edges */}
        {edges.map((edge) => {
          const start = posById.get(edge.caller);
          const end = posById.get(edge.callee);
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
              key={`${edge.caller}→${edge.callee}`}
              role="button"
              aria-label={`${edge.caller} to ${edge.callee}`}
              style={{ cursor: "pointer" }}
              onClick={(e) => {
                e.stopPropagation();
                // Convert SVG-space mid to screen coords for popover placement.
                const svgEl = svgRef.current;
                const screenX = svgEl
                  ? svgEl.getBoundingClientRect().left + transform.applyX(midX)
                  : midX;
                const screenY = svgEl
                  ? svgEl.getBoundingClientRect().top + transform.applyY(midY)
                  : midY;
                onEdgeClick(edge, screenX - (svgEl?.getBoundingClientRect().left ?? 0), screenY - (svgEl?.getBoundingClientRect().top ?? 0));
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
          const pos = posById.get(svc) ?? { x: w / 2, y: h / 2 };
          const isActive = !focusedService || (connectedServices?.has(svc) ?? true);

          return (
            <DraggableNode
              key={svc}
              svc={svc}
              pos={pos}
              isActive={isActive}
              isFocused={focusedService === svc}
              onClick={(e) => {
                e.stopPropagation();
                onNodeClick(svc);
              }}
              onDrag={(dx, dy) => {
                const node = nodesRef.current.find((n) => n.id === svc);
                if (node) {
                  node.x = (node.x ?? 0) + dx;
                  node.y = (node.y ?? 0) + dy;
                  node.fx = node.x;
                  node.fy = node.y;
                  forceRender((n) => n + 1);
                }
              }}
            />
          );
        })}
      </g>
    </svg>
    </div>
  );
}

interface DraggableNodeProps {
  svc: string;
  pos: { x: number; y: number };
  isActive: boolean;
  isFocused: boolean;
  onClick: (e: React.MouseEvent) => void;
  onDrag: (dx: number, dy: number) => void;
}

function DraggableNode({ svc, pos, isActive, isFocused, onClick, onDrag }: DraggableNodeProps) {
  const dragStart = useRef<{ x: number; y: number } | null>(null);
  const moved = useRef(false);

  function handlePointerDown(e: React.PointerEvent<SVGGElement>) {
    e.currentTarget.setPointerCapture(e.pointerId);
    dragStart.current = { x: e.clientX, y: e.clientY };
    moved.current = false;
  }

  function handlePointerMove(e: React.PointerEvent<SVGGElement>) {
    if (!dragStart.current) return;
    const dx = e.clientX - dragStart.current.x;
    const dy = e.clientY - dragStart.current.y;
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) moved.current = true;
    dragStart.current = { x: e.clientX, y: e.clientY };
    onDrag(dx, dy);
  }

  function handlePointerUp() {
    dragStart.current = null;
    // The native click event fires after pointerUp; handleClick below handles it.
  }

  function handleClick(e: React.MouseEvent<SVGGElement>) {
    // Suppress click if the pointer was dragged.
    if (moved.current) {
      moved.current = false;
      return;
    }
    onClick(e);
  }

  return (
    <g
      role="button"
      aria-label={svc}
      transform={`translate(${pos.x}, ${pos.y})`}
      style={{ cursor: "grab" }}
      onClick={handleClick}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      <circle
        r={NODE_R}
        fill={isFocused ? "#1a3a5c" : "#222"}
        stroke={isFocused ? "#4a9edd" : "#444"}
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
        style={{ pointerEvents: "none", userSelect: "none" }}
      >
        {svc}
      </text>
    </g>
  );
}
