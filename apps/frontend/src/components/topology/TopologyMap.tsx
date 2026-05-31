import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { TopologyEdge } from "../../api/services";

const NODE_R = 30;

export interface TopologyMapProps {
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
  }

  function handleClick(e: React.MouseEvent<SVGGElement>) {
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

export function TopologyMap({
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

  const nodesRef = useRef<SimNode[]>([]);
  const simRef = useRef<d3.Simulation<SimNode, SimLink> | null>(null);
  const [, forceRender] = useState(0);
  const svgRef = useRef<SVGSVGElement>(null);
  const [transform, setTransform] = useState<d3.ZoomTransform>(d3.zoomIdentity);

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
    simulation.tick(10);
    nodesRef.current = [...nodes];
    forceRender((n) => n + 1);

    return () => {
      simulation.stop();
    };
  }, [services.join(","), edges.map((e) => `${e.caller}→${e.callee}`).join(","), w, h]); // stable primitive deps

  useEffect(() => {
    simRef.current?.force("center", d3.forceCenter(w / 2, h / 2));
    simRef.current?.alpha(0.3).restart();
  }, [w, h]);

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

        <rect
          width={w}
          height={h}
          fill="transparent"
          data-testid="topology-background"
          onClick={onBackgroundClick}
        />

        <g transform={transform.toString()}>
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
                  const svgEl = svgRef.current;
                  const screenX = svgEl
                    ? svgEl.getBoundingClientRect().left + transform.applyX(midX)
                    : midX;
                  const screenY = svgEl
                    ? svgEl.getBoundingClientRect().top + transform.applyY(midY)
                    : midY;
                  onEdgeClick(
                    edge,
                    screenX - (svgEl?.getBoundingClientRect().left ?? 0),
                    screenY - (svgEl?.getBoundingClientRect().top ?? 0),
                  );
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
