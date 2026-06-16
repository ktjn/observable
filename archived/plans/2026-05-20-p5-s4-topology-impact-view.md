# P5-S4: Topology-Aware Impact View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a live topology subgraph of impacted services inside the incident detail page, derived at query time from existing SLO and topology data — no schema migration required.

**Architecture:** Extend `GET /v1/incidents/:id` to join through `alert_rules → slo_definitions` and return `impacted_service: Option<String>`. Extract the existing `TopologyMap` D3 component to a shared location. Render a fixed-height topology panel inside `IncidentDetailPage` when `impacted_service` is set.

**Tech Stack:** Rust/axum/sqlx (backend), React/TanStack Query/D3 (frontend), Testcontainers (integration tests), Vitest/RTL (frontend tests).

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `services/query-api/src/incidents.rs` | Modify | Extend SQL + add `impacted_service` to structs |
| `services/query-api/tests/http_api_integration.rs` | Modify | Two new integration tests |
| `apps/frontend/src/components/topology/TopologyMap.tsx` | Create | Extracted `TopologyMap` + `DraggableNode` components |
| `apps/frontend/src/pages/ServiceTopologyPage.tsx` | Modify | Import `TopologyMap` from new shared location |
| `apps/frontend/src/api/incidents.ts` | Modify | Add `impacted_service: string \| null` to type |
| `apps/frontend/src/features/incidents/IncidentDetailPage.tsx` | Modify | Add Impact panel |
| `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx` | Modify | Two new tests + updated fixture |

---

## Task 1: Backend — write failing integration tests

**Files:**
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Add the two test functions at the end of the file**

Append these two test functions after the existing `get_incident_detail_includes_rule_name` test block:

```rust
#[tokio::test]
async fn get_incident_detail_includes_impacted_service_for_slo_rule() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let app = build_app_with_pg(ch, pg.clone());
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    // Seed an SLO definition for "payments" service
    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'payments', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Payments SLO') \
         RETURNING slo_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("slo inserted");

    // Seed an slo_burn_rate alert rule referencing the SLO
    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Payments SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .bind(serde_json::json!({
        "slo_id": slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&pg)
    .await
    .expect("slo_burn_rate rule inserted");

    // Seed an incident linked to that rule
    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'Payments SLO burn', 'critical', 'triggered', 'slo-dedup-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&pg)
    .await
    .expect("slo incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(
        body["impacted_service"], "payments",
        "slo_burn_rate incident must carry impacted_service from slo_definitions"
    );
}

#[tokio::test]
async fn get_incident_detail_impacted_service_null_for_threshold_rule() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let app = build_app_with_pg(ch, pg.clone());
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'High CPU', 'threshold', 'warning', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":80}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("threshold rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'High CPU', 'warning', 'triggered', 'threshold-dedup-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&pg)
    .await
    .expect("threshold incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert!(
        body["impacted_service"].is_null(),
        "threshold incident must have null impacted_service"
    );
}
```

- [ ] **Step 2: Run the new tests and confirm they fail with a compile error** (field doesn't exist yet)

```powershell
cd services/query-api
cargo test get_incident_detail_includes_impacted_service_for_slo_rule -- --nocapture 2>&1 | head -30
```

Expected: compilation error — `no field impacted_service on IncidentDetailResponse` or similar.

---

## Task 2: Backend — implement impacted_service in incidents.rs

**Files:**
- Modify: `services/query-api/src/incidents.rs`

- [ ] **Step 1: Add `impacted_service` field to `IncidentDetailRow`**

In `services/query-api/src/incidents.rs`, replace the `IncidentDetailRow` struct:

```rust
#[derive(sqlx::FromRow)]
struct IncidentDetailRow {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    dedup_key: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    triggered_by_rule_id: Option<Uuid>,
    runbook_url: Option<String>,
    rule_name: Option<String>,
    impacted_service: Option<String>,
}
```

- [ ] **Step 2: Add `impacted_service` field to `IncidentDetailResponse`**

Replace the `IncidentDetailResponse` struct:

```rust
#[derive(Serialize)]
pub struct IncidentDetailResponse {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub dedup_key: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
    pub runbook_url: Option<String>,
    pub rule_name: Option<String>,
    pub timeline: Vec<IncidentEventItem>,
    pub impacted_service: Option<String>,
}
```

- [ ] **Step 3: Extend the SQL in `get_incident` to join through slo_definitions**

Replace the `sqlx::query_as` call inside `get_incident` (the one that fetches `IncidentDetailRow`):

```rust
let row: Option<IncidentDetailRow> = sqlx::query_as(
    "SELECT i.incident_id, i.title, i.severity, i.status, i.dedup_key, \
            i.triggered_at, i.resolved_at, i.triggered_by_rule_id, i.runbook_url, \
            r.name AS rule_name, \
            CASE WHEN r.alert_type = 'slo_burn_rate' \
                 THEN s.service_name \
            END AS impacted_service \
     FROM incidents i \
     LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id \
     LEFT JOIN slo_definitions s \
            ON r.alert_type = 'slo_burn_rate' \
           AND (r.condition->>'slo_id')::uuid = s.slo_id \
           AND s.tenant_id = i.tenant_id \
     WHERE i.incident_id = $1 AND i.tenant_id = $2",
)
.bind(incident_id)
.bind(tenant_id)
.fetch_optional(db)
.await?;
```

- [ ] **Step 4: Pass `impacted_service` through in the `Ok(Some(...))` return**

Replace the `Ok(Some(IncidentDetailResponse { ... }))` block:

```rust
Ok(Some(IncidentDetailResponse {
    incident_id: row.incident_id,
    title: row.title,
    severity: row.severity,
    status: row.status,
    dedup_key: row.dedup_key,
    triggered_at: row.triggered_at,
    resolved_at: row.resolved_at,
    triggered_by_rule_id: row.triggered_by_rule_id,
    runbook_url: row.runbook_url,
    rule_name: row.rule_name,
    timeline,
    impacted_service: row.impacted_service,
}))
```

- [ ] **Step 5: Run cargo fmt**

```powershell
cargo fmt --all
```

- [ ] **Step 6: Run the integration tests**

```powershell
cd services/query-api
cargo test get_incident_detail -- --nocapture 2>&1 | tail -20
```

Expected output — all three incident detail tests pass:
```
test get_incident_detail_includes_rule_name ... ok
test get_incident_detail_includes_impacted_service_for_slo_rule ... ok
test get_incident_detail_impacted_service_null_for_threshold_rule ... ok
```

- [ ] **Step 7: Commit**

```powershell
git add services/query-api/src/incidents.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(incidents): derive impacted_service from slo_definitions in incident detail"
```

---

## Task 3: Extract TopologyMap to shared component

**Files:**
- Create: `apps/frontend/src/components/topology/TopologyMap.tsx`
- Modify: `apps/frontend/src/pages/ServiceTopologyPage.tsx`

- [ ] **Step 1: Create `apps/frontend/src/components/topology/TopologyMap.tsx`**

Create the file with the extracted component. This is a mechanical move of the existing code:

```typescript
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
```

- [ ] **Step 2: Update `ServiceTopologyPage.tsx` to import from the new location**

At the top of `apps/frontend/src/pages/ServiceTopologyPage.tsx`, replace:

```typescript
import { getTopology, listServices, type TopologyEdge } from "../api/services";
```

with:

```typescript
import { getTopology, listServices } from "../api/services";
import { TopologyMap } from "../components/topology/TopologyMap";
import type { TopologyEdge } from "../api/services";
```

Then delete the `interface TopologyMapProps`, `interface SimNode`, `interface SimLink`, `const NODE_R`, `interface DraggableNodeProps`, the `TopologyMap` function, and the `DraggableNode` function from `ServiceTopologyPage.tsx`. These are now in the shared file.

- [ ] **Step 3: Run TypeScript check to verify no compile errors**

```powershell
cd apps/frontend
npm run typecheck
```

Expected: exit 0, no errors.

- [ ] **Step 4: Commit**

```powershell
git add apps/frontend/src/components/topology/TopologyMap.tsx apps/frontend/src/pages/ServiceTopologyPage.tsx
git commit -m "refactor(topology): extract TopologyMap to shared component"
```

---

## Task 4: Frontend — update API type and test fixture

**Files:**
- Modify: `apps/frontend/src/api/incidents.ts`
- Modify: `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx`

- [ ] **Step 1: Add `impacted_service` to `IncidentDetailResponse` in `api/incidents.ts`**

Replace the `IncidentDetailResponse` interface:

```typescript
export interface IncidentDetailResponse {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  dedup_key: string;
  triggered_at: string;
  resolved_at: string | null;
  triggered_by_rule_id: string | null;
  runbook_url: string | null;
  rule_name: string | null;
  timeline: IncidentEventItem[];
  impacted_service: string | null;
}
```

- [ ] **Step 2: Update the `baseDetail` fixture in `IncidentDetailPage.test.tsx`**

Add `impacted_service: null` to the `baseDetail` object (between `rule_name` and `timeline`):

```typescript
const baseDetail: incidentsApi.IncidentDetailResponse = {
  incident_id: "inc-1",
  title: "CPU spike",
  severity: "critical",
  status: "triggered",
  dedup_key: "rule-abc",
  triggered_at: "2026-05-18T10:00:00Z",
  resolved_at: null,
  triggered_by_rule_id: "rule-abc",
  runbook_url: null,
  rule_name: "High CPU Alert",
  impacted_service: null,
  timeline: [
    {
      event_time: "2026-05-18T10:00:01Z",
      event_type: "triggered",
      actor: "system",
      message: "Alert rule transitioned to active",
    },
    {
      event_time: "2026-05-18T10:00:05Z",
      event_type: "alert_fired",
      actor: "system",
      message: "High CPU Alert fired: value=95.30",
    },
  ],
};
```

- [ ] **Step 3: Run existing tests to confirm they still pass**

```powershell
cd apps/frontend
npx vitest run src/features/incidents/IncidentDetailPage.test.tsx
```

Expected: all 4 existing tests pass.

- [ ] **Step 4: Commit**

```powershell
git add apps/frontend/src/api/incidents.ts apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx
git commit -m "feat(incidents-api): add impacted_service field to IncidentDetailResponse"
```

---

## Task 5: Frontend — add failing tests for Impact panel

**Files:**
- Modify: `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx`

- [ ] **Step 1: Add the import for services API mock at the top of the test file**

After the existing import line `import * as incidentsApi from "../../api/incidents";`, add:

```typescript
import * as servicesApi from "../../api/services";
```

- [ ] **Step 2: Add two new failing tests at the end of the test file**

```typescript
test("renders Impacted Services panel when impacted_service is set", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue({
    ...baseDetail,
    impacted_service: "payments",
  });
  vi.spyOn(servicesApi, "getTopology").mockResolvedValue({
    edges: [
      {
        caller: "api-gateway",
        callee: "payments",
        request_count: 500,
        error_rate: 0.1,
        p95_latency_ms: 120,
      },
    ],
  });
  renderPage();
  await waitFor(() => screen.getByText("Impacted Services"));
  expect(screen.getByText("Impacted Services")).toBeInTheDocument();
  expect(screen.getByText("→ Service Detail")).toBeInTheDocument();
});

test("does not render Impacted Services panel when impacted_service is null", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue({
    ...baseDetail,
    impacted_service: null,
  });
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1 }));
  expect(screen.queryByText("Impacted Services")).not.toBeInTheDocument();
});
```

- [ ] **Step 3: Run the new tests and confirm they fail**

```powershell
cd apps/frontend
npx vitest run src/features/incidents/IncidentDetailPage.test.tsx
```

Expected: the two new tests fail (panel not rendered yet), the 4 existing tests pass.

---

## Task 6: Frontend — implement the Impact panel in IncidentDetailPage

**Files:**
- Modify: `apps/frontend/src/features/incidents/IncidentDetailPage.tsx`

- [ ] **Step 1: Add the new imports at the top of `IncidentDetailPage.tsx`**

Add after the existing import block:

```typescript
import { getTopology } from "../../api/services";
import { TopologyMap } from "../../components/topology/TopologyMap";
```

(`Link`, `useQuery`, `LoadingState`, `useTenantContext` are already imported.)

- [ ] **Step 2: Add the topology query inside the `IncidentDetailPage` function**

Add this block after the existing `useQuery` for the incident, and before the loading/not-found checks:

```typescript
const triggeredAtMs = data ? new Date(data.triggered_at).getTime() : 0;
const resolvedAtMs = data?.resolved_at ? new Date(data.resolved_at).getTime() : Date.now();

const {
  data: topologyData,
  isLoading: topoLoading,
  isError: topoError,
} = useQuery({
  queryKey: ["topology-impact", tenantId, data?.impacted_service, triggeredAtMs, resolvedAtMs],
  queryFn: () =>
    getTopology(tenantId, {
      service: data!.impacted_service!,
      from: triggeredAtMs,
      to: resolvedAtMs,
    }),
  enabled: !!data?.impacted_service,
});
```

- [ ] **Step 3: Add the Impact panel JSX after the Timeline Panel**

Inside the `return (...)`, after the closing `</Panel>` of the Timeline panel, add:

```tsx
{data.impacted_service && (
  <Panel>
    <div className="flex items-center justify-between mb-3">
      <h3 className="text-sm font-semibold">Impacted Services</h3>
      <div className="flex gap-3 text-xs">
        <Link
          to="/services/$serviceId"
          params={{ serviceId: data.impacted_service }}
          className="text-[var(--brand)] hover:underline"
        >
          → Service Detail
        </Link>
        <Link to="/topology" className="text-[var(--brand)] hover:underline">
          → View in Topology
        </Link>
      </div>
    </div>
    {topoLoading ? (
      <LoadingState>Loading topology…</LoadingState>
    ) : topoError ? (
      <p className="text-sm text-[var(--muted)]">Could not load topology data.</p>
    ) : (
      <div style={{ height: 320 }}>
        <TopologyMap
          edges={topologyData?.edges ?? []}
          allServices={Array.from(
            new Set([
              data.impacted_service,
              ...(topologyData?.edges.flatMap((e) => [e.caller, e.callee]) ?? []),
            ]),
          )}
          focusedService={data.impacted_service}
          onNodeClick={() => {}}
          onEdgeClick={() => {}}
          onBackgroundClick={() => {}}
        />
        {(topologyData?.edges ?? []).length === 0 && (
          <p className="text-xs text-[var(--muted)] mt-2">
            No observed call relationships during this incident.
          </p>
        )}
      </div>
    )}
  </Panel>
)}
```

- [ ] **Step 4: Run the tests**

```powershell
cd apps/frontend
npx vitest run src/features/incidents/IncidentDetailPage.test.tsx
```

Expected: all 6 tests pass.

- [ ] **Step 5: Run TypeScript check**

```powershell
npm run typecheck
```

Expected: exit 0.

- [ ] **Step 6: Commit**

```powershell
git add apps/frontend/src/features/incidents/IncidentDetailPage.tsx apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx
git commit -m "feat(incidents): add topology impact panel for SLO burn-rate incidents"
```

---

## Task 7: Run local CI and open PR

- [ ] **Step 1: Run full local CI (skip smoke to keep it fast; Docker build covers Rust fmt + clippy)**

```powershell
bash scripts/local-ci.sh --skip-smoke
```

Expected: all stages pass. Fix any lint or typecheck errors before continuing.

- [ ] **Step 2: Update the roadmap plan**

In `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, find the P5-S4 line and mark it complete:

```markdown
- [x] **P5-S4: Add topology-aware impact view for one incident** (COMPLETED 2026-05-20)
  - SLO burn-rate incidents show an inline D3 topology subgraph in the incident detail page. `impacted_service` derived at query time via `alert_rules → slo_definitions`; `TopologyMap` extracted to `components/topology/`. No DB migration.
```

- [ ] **Step 3: Update `docs/agent-context.md`**

Add a note under the incidents/alerting section:

> `IncidentDetailResponse` now includes `impacted_service: Option<String>`, derived at query time from `slo_definitions` for `slo_burn_rate` rules. `TopologyMap` D3 component lives at `apps/frontend/src/components/topology/TopologyMap.tsx` (extracted from `ServiceTopologyPage`).

- [ ] **Step 4: Commit the docs updates**

```powershell
git add docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md docs/agent-context.md
git commit -m "docs: mark P5-S4 complete, update agent-context with TopologyMap location"
```

- [ ] **Step 5: Push and open PR**

```powershell
git push -u origin feat/p5-s4-topology-impact-view
gh pr create --title "feat(p5-s4): add topology impact view to incident detail for SLO incidents" --body "$(cat <<'EOF'
## Summary
- Extends `GET /v1/incidents/:id` to derive `impacted_service` at query time via `alert_rules → slo_definitions` for `slo_burn_rate` rules (threshold incidents return `null`)
- Extracts `TopologyMap` D3 component from `ServiceTopologyPage` to `components/topology/TopologyMap.tsx`
- Adds an inline topology panel to `IncidentDetailPage` showing the impacted service and its call-graph neighbors during the incident's time window

## No migration required
All data already exists in `slo_definitions`. The new field is nullable and the panel is conditionally rendered — threshold incidents are unaffected.

## Verification
- 2 new Testcontainers HTTP integration tests: SLO incident returns `impacted_service`, threshold incident returns null
- 2 new RTL tests: Impact panel renders when set, absent when null
- `bash scripts/local-ci.sh --skip-smoke` passes

## Phase gate
P5-S4 complete. P5-S5 (composite alert evaluation) is next.

## ADR/spec sync
No new architectural decisions. Uses existing topology API, SLO model, and incident model. `docs/agent-context.md` updated with `TopologyMap` location.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

**Spec coverage check:**
- ✅ Backend derives `impacted_service` from SLO join (Task 2)
- ✅ No migration (confirmed — all joins on existing tables)
- ✅ TopologyMap extracted (Task 3)
- ✅ `api/incidents.ts` updated (Task 4)
- ✅ Impact panel with D3 graph, link row, error/loading states (Task 6)
- ✅ Time window scoped to incident duration (Task 6, Step 2 — `triggeredAtMs`/`resolvedAtMs`)
- ✅ HTTP integration tests: SLO→impacted_service, threshold→null (Tasks 1–2)
- ✅ Frontend tests: panel present/absent (Tasks 5–6)
- ✅ `agent-context.md` updated (Task 7)
- ✅ Roadmap plan marked complete (Task 7)

**Type consistency:** `impacted_service: Option<String>` in Rust, `impacted_service: string | null` in TypeScript — consistent across all tasks. `TopologyMapProps` exported from `TopologyMap.tsx` and consumed with the same field names in `IncidentDetailPage.tsx`.

**Placeholder scan:** All steps contain complete code. No TBDs.
