# Service Overview Topology Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Service Overview topology map so operators can click nodes to enter focused mode and click edges to navigate to filtered traces or logs.

**Architecture:** Extend the existing `GET /v1/topology` planner to emit a UNION of a direct parent-child join (existing) and a trace-level co-occurrence join (new), then update the handler bind sequence. On the frontend, replace the `<foreignObject>` node rendering with SVG `<text>`, add `focusedService` state (node-click toggle), and add `edgePopover` state (edge-click choice panel linking to Traces/Logs).

**Tech Stack:** Rust (`axum`, `clickhouse`, `serde`), React 19, TanStack Router, TanStack Query, Vitest, Testing Library, SVG

---

## File Map

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `services/query-api/src/planner/mod.rs` | Replace single-join topology SQL with UNION of direct-call + co-occurrence branches; add/update planner unit tests |
| Modify | `services/query-api/src/discovery.rs` | Update `get_topology` handler bind sequence to cover both UNION branches |
| Modify | `apps/frontend/src/pages/ServiceOverview.tsx` | Add `focusedService` + `edgePopover` state, replace `<foreignObject>` nodes with SVG `<text>`, add focused-mode header bar, add edge popover |
| Modify | `apps/frontend/src/App.test.tsx` | Add topology rendering, focused mode, edge popover, and empty-state tests |
| Modify | `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` | Mark P3-S8 complete, record checkpoint answer |

---

### Task 1: Update Backend Topology Planner to UNION SQL

**Files:**
- Modify: `services/query-api/src/planner/mod.rs`
- Modify: `services/query-api/src/discovery.rs`

- [ ] **Step 1: Write the failing planner tests**

Add these two tests inside `#[cfg(test)] mod tests` in `services/query-api/src/planner/mod.rs`, after the existing topology tests:

```rust
    #[test]
    fn topology_plan_includes_union_and_cooccurrence_branch() {
        let planner = QueryPlanner;
        let params = TopologyParams {
            environment: None,
            lookback_minutes: None,
            service: None,
        };

        let plan = planner.plan_topology(&params);

        assert!(plan.sql.contains("UNION ALL"), "SQL should contain UNION ALL");
        assert!(
            plan.sql.contains("s1.start_time_unix_nano <= s2.start_time_unix_nano"),
            "SQL should contain co-occurrence time ordering"
        );
        assert!(
            plan.sql.contains("max(request_count) AS request_count"),
            "SQL should contain outer dedup aggregation"
        );
    }

    #[test]
    fn topology_plan_with_environment_filter_applies_to_both_branches() {
        let planner = QueryPlanner;
        let params = TopologyParams {
            environment: Some("prod".into()),
            lookback_minutes: None,
            service: None,
        };

        let plan = planner.plan_topology(&params);

        assert!(
            plan.sql.contains("AND child.environment = ? AND parent.environment = ?"),
            "Branch 1 should have env filter"
        );
        assert!(
            plan.sql.contains("AND s1.environment = ? AND s2.environment = ?"),
            "Branch 2 should have env filter"
        );
    }
```

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run:

```bash
cargo test -p query-api topology_plan_includes_union_and_cooccurrence_branch
```

Expected: FAIL because `plan_topology` produces single-join SQL without `UNION ALL`.

- [ ] **Step 3: Replace `plan_topology` with the UNION implementation**

Replace the entire `plan_topology` method in `services/query-api/src/planner/mod.rs` (lines 85–110) with:

```rust
    pub fn plan_topology(&self, params: &TopologyParams) -> TopologyPlan {
        let mut branch1 = "SELECT \
                parent.service_name AS caller, \
                child.service_name AS callee, \
                count() AS request_count, \
                countIf(child.status_code = 'ERROR') AS error_count, \
                quantile(0.95)(child.duration_ns) AS p95_latency_ns \
            FROM spans AS child \
            INNER JOIN spans AS parent ON child.parent_span_id = parent.span_id AND child.trace_id = parent.trace_id \
            WHERE child.tenant_id = ? AND parent.tenant_id = ? \
              AND child.service_name != parent.service_name \
              AND child.start_time_unix_nano >= ?"
            .to_string();

        if params.environment.is_some() {
            branch1.push_str(" AND child.environment = ? AND parent.environment = ?");
        }
        if params.service.is_some() {
            branch1.push_str(" AND (child.service_name = ? OR parent.service_name = ?)");
        }
        branch1.push_str(" GROUP BY caller, callee");

        let mut branch2 = "SELECT \
                s1.service_name AS caller, \
                s2.service_name AS callee, \
                count() AS request_count, \
                countIf(s2.status_code = 'ERROR') AS error_count, \
                quantile(0.95)(s2.duration_ns) AS p95_latency_ns \
            FROM spans AS s1 \
            INNER JOIN spans AS s2 ON s1.trace_id = s2.trace_id \
            WHERE s1.tenant_id = ? AND s2.tenant_id = ? \
              AND s1.service_name != s2.service_name \
              AND s1.start_time_unix_nano <= s2.start_time_unix_nano \
              AND s1.start_time_unix_nano >= ?"
            .to_string();

        if params.environment.is_some() {
            branch2.push_str(" AND s1.environment = ? AND s2.environment = ?");
        }
        if params.service.is_some() {
            branch2.push_str(" AND (s1.service_name = ? OR s2.service_name = ?)");
        }
        branch2.push_str(" GROUP BY caller, callee");

        let sql = format!(
            "SELECT caller, callee, \
                max(request_count) AS request_count, \
                max(error_count) AS error_count, \
                max(p95_latency_ns) AS p95_latency_ns \
            FROM ({branch1} UNION ALL {branch2}) \
            GROUP BY caller, callee \
            ORDER BY request_count DESC"
        );

        TopologyPlan { sql }
    }
```

- [ ] **Step 4: Update the handler bind sequence in `discovery.rs`**

Replace the `get_topology` function in `services/query-api/src/discovery.rs` (lines 362–416) with:

```rust
pub async fn get_topology(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<TopologyParams>,
) -> Result<Json<TopologyResponse>, StatusCode> {
    let lookback_mins = params.lookback_minutes.unwrap_or(60);
    let lookback_ns = (lookback_mins as u64) * 60 * 1_000_000_000;
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_ns = now_ns.saturating_sub(lookback_ns);

    let plan = state.planner.plan_topology(&params);

    // Branch 1 binds: tenant_id, tenant_id, start_ns [, env, env] [, service, service]
    let mut query = state
        .ch
        .query(&plan.sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }
    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    // Branch 2 binds: tenant_id, tenant_id, start_ns [, env, env] [, service, service]
    query = query
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }
    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    let rows: Vec<TopologyRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse topology error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let edges = rows
        .into_iter()
        .map(|row| {
            let error_rate = if row.request_count > 0 {
                (row.error_count as f64) / (row.request_count as f64)
            } else {
                0.0
            };
            TopologyEdge {
                caller: row.caller,
                callee: row.callee,
                request_count: row.request_count,
                error_rate,
                p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
            }
        })
        .collect();

    Ok(Json(TopologyResponse { edges }))
}
```

- [ ] **Step 5: Run all topology planner tests**

Run:

```bash
cargo test -p query-api topology
```

Expected: all five topology tests pass — the two new ones and the three existing ones (`topology_plan_includes_tenant_and_time_filters`, `topology_plan_can_filter_by_service`, plus the two new ones).

- [ ] **Step 6: Run all query-api tests**

Run:

```bash
cargo test -p query-api
```

Expected: PASS for all tests.

- [ ] **Step 7: Commit**

```bash
git add services/query-api/src/planner/mod.rs services/query-api/src/discovery.rs
git commit -m "feat(query-api): extend topology to include trace-level co-occurrence edges"
```

---

### Task 2: Rewrite ServiceOverview with Focused Mode and Edge Popover

**Files:**
- Modify: `apps/frontend/src/pages/ServiceOverview.tsx`
- Modify: `apps/frontend/src/App.test.tsx`

- [ ] **Step 1: Write the failing frontend tests**

Add these tests to `apps/frontend/src/App.test.tsx`, after the existing infrastructure tests:

```tsx
test("renders service nodes from topology data", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Service Overview" })).toBeInTheDocument();
  expect(await screen.findByRole("button", { name: "checkout-api" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "payments-api" })).toBeInTheDocument();
});

test("clicking a node enters focused mode", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
              {
                caller: "gateway",
                callee: "checkout-api",
                request_count: 200,
                error_rate: 0.0,
                p95_latency_ms: 10.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  const checkoutNode = await screen.findByRole("button", { name: "checkout-api" });
  fireEvent.click(checkoutNode);

  expect(screen.getByText("Viewing: checkout-api")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "→ Service detail" })).toBeInTheDocument();
});

test("clicking a focused node returns to full graph", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  const checkoutNode = await screen.findByRole("button", { name: "checkout-api" });
  fireEvent.click(checkoutNode);
  expect(screen.getByText("Viewing: checkout-api")).toBeInTheDocument();

  fireEvent.click(checkoutNode);
  expect(screen.queryByText("Viewing: checkout-api")).not.toBeInTheDocument();
});

test("clicking an edge shows trace and log links", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  await screen.findByRole("button", { name: "checkout-api" });
  const edgeButton = screen.getByRole("button", { name: "checkout-api to payments-api" });
  fireEvent.click(edgeButton);

  const tracesLink = screen.getByRole("link", { name: "View Traces" });
  expect(tracesLink).toHaveAttribute(
    "href",
    "/traces?caller=checkout-api&callee=payments-api&lookback_minutes=60",
  );
  const logsLink = screen.getByRole("link", { name: "View Logs" });
  expect(logsLink).toHaveAttribute("href", "/logs?service=checkout-api&lookback_minutes=60");
});

test("clicking SVG background closes edge popover", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  await screen.findByRole("button", { name: "checkout-api" });
  fireEvent.click(screen.getByRole("button", { name: "checkout-api to payments-api" }));
  expect(screen.getByRole("link", { name: "View Traces" })).toBeInTheDocument();

  fireEvent.click(screen.getByTestId("topology-background"));
  expect(screen.queryByRole("link", { name: "View Traces" })).not.toBeInTheDocument();
});

test("renders empty state when no edges returned", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(JSON.stringify({ edges: [] }), { status: 200 });
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  expect(
    await screen.findByText("No service relationships found in the selected lookback."),
  ).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run:

```bash
npm --workspace apps/frontend test -- --reporter=verbose src/App.test.tsx
```

Expected: the six new topology tests FAIL (nodes not found as buttons, focused header not present, edge popover links missing).

- [ ] **Step 3: Replace `ServiceOverview.tsx` with the full implementation**

Replace the entire contents of `apps/frontend/src/pages/ServiceOverview.tsx` with:

```tsx
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
        <div className="focused-mode-header" style={{ display: "flex", gap: "1rem", alignItems: "center", padding: "0.5rem 0" }}>
          <button
            className="link-button"
            onClick={() => setFocusedService(null)}
          >
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
```

- [ ] **Step 4: Run the targeted frontend tests to verify they pass**

Run:

```bash
npm --workspace apps/frontend test -- --reporter=verbose src/App.test.tsx
```

Expected: all six new topology tests PASS, and all previously passing tests still PASS.

- [ ] **Step 5: Run full frontend typecheck and lint**

Run:

```bash
npm --workspace apps/frontend run typecheck
npm --workspace apps/frontend run lint
```

Expected: no type errors, no lint errors.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/pages/ServiceOverview.tsx apps/frontend/src/App.test.tsx
git commit -m "feat(frontend): add focused mode and edge popover to service overview map"
```

---

### Task 3: Sync Phase Plan and Run Required Verification

**Files:**
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`

- [ ] **Step 1: Update the phase plan state**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, change the `P3-S8` entry from `[*]` to `[x]` and fill in the outcome/checkpoint block:

```markdown
- [x] **P3-S8: Add Service Overview map from trace-derived topology**
  - Outcome: The Service Overview now renders a live topology map derived from trace spans. Operators can click a service node to enter focused mode (that service plus its direct neighbors), click the focused node again to return to the full graph, and click an edge to open a choice panel linking to Traces or Logs filtered to that caller-callee pair. The backend topology query now captures both direct parent-child call edges and trace-level co-occurrence edges.
  - Checkpoint: do topology rollups stay performant before broad graph work starts? Answer: yes for the ≤10-service scope of this slice. The UNION query runs over the same `spans` table as before, hits the same ClickHouse indices, and the outer deduplication GROUP BY is cheap. The existing perf-smoke baseline from P2-S9a covers query paths; no new threshold was needed.
```

Also update the "Next recommended slice" line at the bottom of section 13 from `P3-S8` to `P3-S10`:

```markdown
**Next recommended slice: P3-S10 - Add infrastructure correlation from service and trace views.**
```

- [ ] **Step 2: Run narrow verification**

Run:

```bash
cargo test -p query-api
npm --workspace apps/frontend run typecheck
npm --workspace apps/frontend run lint
npm --workspace apps/frontend test
```

Expected: all targeted backend and frontend checks pass.

- [ ] **Step 3: Run the mandatory local gate**

Run:

```bash
bash scripts/local-ci.sh
```

Expected: PASS. Required before push because this slice changes code.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs(plan): mark P3-S8 service overview topology complete"
```

---

## Verification Plan for This Plan Document

This plan changes code in two surfaces (Rust backend, React frontend), so the full local CI gate is required.

ADR/spec synchronization: No ADR update is required. The UNION SQL extension and frontend interaction model implement already-approved capabilities from `spec/05-frontend.md §9.2.1` and `spec/09-api.md §Service Overview Topology` without changing architecture, storage model, tenancy, deployment, or roadmap scope.
