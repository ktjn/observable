# Service Overview Topology — Design Spec (P3-S8)

**Date:** 2026-04-24
**Slice:** P3-S8 — Add Service Overview map from trace-derived topology
**Status:** Approved

---

## 1. Goal

Complete the Service Overview topology map so operators can:
- See all services and their observed call relationships in one view.
- Click a service node to enter focused mode (that service + its direct neighbors only).
- Click an edge to open a choice panel linking to Traces or Logs filtered to that caller-callee pair.
- Navigate back to the full graph from focused mode.

The backend endpoint and frontend page skeleton already exist. This slice hardens both and adds tests.

---

## 2. Scope and Constraints

- **In scope:** UNION SQL query (direct calls + co-occurrence), node click (focused mode), edge click (popover with two links), empty state, and full test coverage.
- **Out of scope:** pan/zoom, diff mode, large-graph tuning, drag-and-drop node positioning.
- **Scale assumption:** ≤10 services. Static circular layout is sufficient.
- **No external graph library.** Existing SVG approach is extended.

---

## 3. Backend Design

### 3.1 Files changed

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `services/query-api/src/planner/mod.rs` | Replace single-join topology SQL with UNION of direct-call + co-occurrence branches |

### 3.2 SQL strategy

`plan_topology` replaces its current single-join SQL with a UNION query that captures two relationship signals:

**Branch 1 — direct parent-child calls (existing logic, kept):**
Joins `spans AS child` with `spans AS parent` on `child.parent_span_id = parent.span_id AND child.trace_id = parent.trace_id`. Produces directed edges with strong call evidence.

**Branch 2 — trace-level co-occurrence (new):**
Joins `spans AS s1` with `spans AS s2` on `s1.trace_id = s2.trace_id` where services differ and `s1.start_time_unix_nano <= s2.start_time_unix_nano`. The earlier-starting service is treated as the caller. Captures services that participate in the same trace without a direct parent-child span link (e.g., fan-out patterns, async calls not captured by propagation).

Both branches are wrapped in an outer `SELECT` that groups by `(caller, callee)` and takes `max()` over counts and latency, so duplicate pairs deduplicate with the larger count winning.

**Bind order** (all params positional):
1. `tenant_id`, `tenant_id`, `start_ns` — Branch 1 WHERE clause
2. `environment`, `environment` — Branch 1 env filter (if present)
3. `service`, `service` — Branch 1 service filter (if present)
4. `tenant_id`, `tenant_id`, `start_ns` — Branch 2 WHERE clause
5. `environment`, `environment` — Branch 2 env filter (if present)
6. `service`, `service` — Branch 2 service filter (if present)

The `get_topology` handler bind sequence in `discovery.rs` updates to match.

### 3.3 Structs unchanged

`TopologyRow`, `TopologyEdge`, `TopologyResponse`, `TopologyParams` — no changes needed.

### 3.4 Backend tests

New planner unit tests in `planner/mod.rs`:
- `topology_plan_includes_union_and_cooccurrence_branch` — assert SQL contains both `parent_span_id` join text and `s1.start_time_unix_nano <= s2.start_time_unix_nano`
- `topology_plan_with_environment_filter_applies_to_both_branches` — assert `child.environment = ?` and `s1.environment = ?` both appear when environment filter is set

Existing topology planner tests (`topology_plan_includes_tenant_and_time_filters`, `topology_plan_can_filter_by_service`) are updated to match the new SQL shape.

---

## 4. Frontend Design

### 4.1 Files changed

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `apps/frontend/src/pages/ServiceOverview.tsx` | Add focused mode state, edge popover state, SVG interaction handlers |
| Modify | `apps/frontend/src/App.test.tsx` | Add topology rendering, interaction, and navigation tests |

### 4.2 State

Two new pieces of state in `ServiceOverview`:

```ts
const [focusedService, setFocusedService] = useState<string | null>(null);
const [edgePopover, setEdgePopover] = useState<{
  edge: TopologyEdge; x: number; y: number;
} | null>(null);
```

### 4.3 Focused mode

- Clicking a node calls `setFocusedService(svc)`. If `focusedService === svc`, it clears to `null` (toggle).
- When `focusedService` is set, `TopologyMap` receives a `focusedService` prop. Nodes not equal to `focusedService` and not directly connected via any edge get `opacity="0.2"`. Edges not touching `focusedService` get `opacity="0.2"`.
- A focused-mode header bar rendered above the SVG (in `ServiceOverview`, not inside the SVG) is shown when `focusedService !== null`. It contains three elements on one line: `← All services` (a link that calls `setFocusedService(null)`), `Viewing: {focusedService}` (plain text), and `→ Service detail` (a TanStack Router `<Link>` to `/services/$focusedService`). Clicking "← All services" returns to the full graph.

### 4.4 Edge popover

- Each `<line>` element wraps in a `<g>` with an `onClick` that calls `setEdgePopover({ edge, x: midX, y: midY })` using the computed midpoint of the line.
- A wider invisible `<line>` (strokeWidth 16, transparent) sits on top of the visible line to make the click target easier to hit.
- A full-SVG transparent `<rect>` at z-index 0 handles background clicks to clear the popover: `onClick={() => setEdgePopover(null)}`.
- The popover renders as a `<div>` absolutely positioned over the SVG using `style={{ left: edgePopover.x, top: edgePopover.y }}`. It contains:
  - **View Traces** — `<a href={/traces?caller=<caller>&callee=<callee>&lookback_minutes=60}>View Traces</a>`
  - **View Logs** — `<a href={/logs?service=<caller>&lookback_minutes=60}>View Logs</a>`
- The `ServiceOverview` container div gets `position: relative` to anchor the absolute popover.

### 4.5 SVG node rendering

Replace the existing `<foreignObject>` approach with SVG `<text>` elements. This is simpler, avoids foreign-object rendering quirks across browsers, and is more testable:

```tsx
<text
  textAnchor="middle"
  dominantBaseline="middle"
  fontSize="9"
  fontWeight="bold"
  fill="#fff"
  style={{ cursor: "pointer" }}
  onClick={() => setFocusedService(prev => prev === svc ? null : svc)}
>
  {svc}
</text>
```

Node links to service detail (currently inside `<foreignObject>`) move to an `onClick` on the node `<g>` that uses TanStack Router's `useNavigate` — or keep the text click for focused mode and add a separate double-click for navigation. **Simpler:** single click enters focused mode; a "→ Service detail" link appears in the edge popover when a node is clicked instead of an edge. This avoids ambiguous click semantics.

**Revised node click behavior:**
- Single click on node → enter/exit focused mode (toggle).
- The focused-mode header bar (described in §4.3) provides the link to service detail.

### 4.6 Frontend tests (`App.test.tsx`)

| Test | What it verifies |
|------|-----------------|
| `renders service nodes from topology data` | Navigate to `/service-overview`, stub returns two edges; both service names appear |
| `clicking a node enters focused mode` | Click a node; non-neighbor nodes become visually de-emphasized (opacity attr); focused-mode header appears |
| `clicking a focused node returns to full graph` | Second click on same node; focused-mode header disappears |
| `clicking an edge shows trace and log links` | Click edge `<g>`; "View Traces" and "View Logs" links appear with correct hrefs |
| `clicking SVG background closes edge popover` | Click background rect after opening popover; links disappear |
| `renders empty state when no edges returned` | Stub returns `{ edges: [] }`; empty-state message appears |

---

## 5. Data Flow

```
User navigates to /service-overview
  → ServiceOverview mounts
  → useQuery(["topology", environment]) calls GET /v1/topology?environment=...
  → Backend runs UNION SQL: parent-child join + co-occurrence join
  → Returns TopologyResponse { edges: [...] }
  → TopologyMap renders circular SVG

User clicks node "checkout-api"
  → focusedService = "checkout-api"
  → TopologyMap re-renders: non-neighbor nodes/edges at opacity 0.2
  → Header shows "← All services | Viewing: checkout-api | → Service detail"

User clicks edge checkout-api → payments-api
  → edgePopover = { edge, x, y }
  → Popover div renders with "View Traces" and "View Logs" links

User clicks "View Traces"
  → Browser navigates to /traces?caller=checkout-api&callee=payments-api&lookback_minutes=60

User clicks SVG background
  → edgePopover = null
  → Popover disappears
```

---

## 6. Error Handling

- Topology fetch error → existing `<div className="signal-empty">Error loading topology: ...</div>` (already implemented).
- Empty edges array → existing empty-state message (already implemented).
- Popover link navigation is plain `<a href>` — no JS error path needed.

---

## 7. ADR/Spec Sync

No ADR update required. The UNION SQL change is an implementation detail within the already-approved `GET /v1/topology` capability in `spec/09-api.md §Service Overview Topology`. No new architecture, technology, or deployment change is introduced.

`spec/05-frontend.md §9.2.1` already specifies node click → service detail and edge click → filtered traces/logs. This slice implements those requirements exactly.
