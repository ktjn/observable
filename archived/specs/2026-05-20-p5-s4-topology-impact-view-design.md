# P5-S4: Topology-Aware Impact View for One Incident

**Date:** 2026-05-20
**Phase:** 5 — Reliability Product
**Slice:** P5-S4
**Status:** Approved — ready for implementation

---

## Goal

Show which services are impacted when an incident fires, using existing topology data. Operators can see the blast radius — the focal service and its immediate call-graph neighbors — without leaving the incident detail page.

---

## Scope

- **In scope:** SLO burn-rate incidents only. Service derived at query time via `alert_rules.condition.slo_id → slo_definitions.service_name`. No DB migration required.
- **Out of scope:** Threshold-based incidents (no service context available without a migration), multi-hop topology traversal, interactive edge popovers in the incident view, drag/zoom gestures inside the incident panel.

---

## Architecture

### Backend — `services/query-api/src/incidents.rs`

Extend `GET /v1/incidents/:id` to derive `impacted_service` at query time by extending the existing LEFT JOIN chain:

```sql
SELECT i.incident_id, i.title, i.severity, i.status, i.dedup_key,
       i.triggered_at, i.resolved_at, i.triggered_by_rule_id, i.runbook_url,
       r.name AS rule_name,
       CASE WHEN r.alert_type = 'slo_burn_rate'
            THEN s.service_name
       END AS impacted_service
FROM incidents i
LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id
LEFT JOIN slo_definitions s
       ON r.alert_type = 'slo_burn_rate'
      AND (r.condition->>'slo_id')::uuid = s.slo_id
      AND s.tenant_id = i.tenant_id
WHERE i.incident_id = $1 AND i.tenant_id = $2
```

`IncidentDetailResponse` gains one new field:

```rust
pub impacted_service: Option<String>,
```

`IncidentDetailRow` (the sqlx FromRow struct) gains the same field. No migration — all data already exists in `slo_definitions`.

For threshold-based incidents or incidents with no rule link, `impacted_service` is `null`.

### Frontend — Component Extraction

**Extract `TopologyMap` and `DraggableNode`** from `apps/frontend/src/pages/ServiceTopologyPage.tsx` into a new shared file:

```
apps/frontend/src/components/topology/TopologyMap.tsx
```

The component interface, D3 simulation logic, zoom, drag, and SVG rendering are unchanged. `ServiceTopologyPage.tsx` imports from the new location. This is a mechanical move, not a redesign.

### Frontend — `IncidentDetailPage.tsx`

When `data.impacted_service` is set, render a new **"Impacted Services"** `Panel` below the Timeline panel:

1. `useQuery` calls `getTopology(tenantId, { service: data.impacted_service, from: triggeredAtMs, to: resolvedAtMs ?? Date.now() })` — scoped to the incident's duration.
2. `allServices` is derived from the edge list: `Array.from(new Set(edges.flatMap(e => [e.caller, e.callee])))`, with `data.impacted_service` always included even if the edge list is empty.
3. `<TopologyMap>` renders at `height: 320px` inside the Panel with `focusedService={data.impacted_service}`.
4. Below the graph, a small link row:
   - `→ View in Topology` links to `/topology` (the topology page, which accepts a NLQ filter — no deep-link param needed for this slice)
   - `→ Service Detail` links to `/services/${data.impacted_service}`

When `data.impacted_service` is null (threshold incident or no rule link), the panel is absent entirely — no empty state shown.

### Frontend — `api/incidents.ts`

Add `impacted_service: string | null` to `IncidentDetailResponse`.

---

## Data Flow

```
GET /v1/incidents/:id
  → incidents JOIN alert_rules JOIN slo_definitions
  → impacted_service: "payments-service" | null

Frontend:
  IncidentDetailPage loads incident
  → if impacted_service:
      getTopology({ service, from: triggered_at, to: resolved_at ?? now })
      → render TopologyMap (focusedService = impacted_service)
      → render link row
```

---

## Error Handling

- If topology fetch fails, show a small inline error message inside the Impact panel: "Could not load topology data." Do not fail the whole incident detail page.
- If topology returns zero edges but `impacted_service` is set, render `TopologyMap` with a single node (the focal service) and a note: "No observed call relationships during this incident."

---

## Testing

### Backend — `tests/http_api_integration.rs`

Two new test cases using `mode: "interpret"` (no ClickHouse):

1. **SLO incident returns `impacted_service`:** Seed `slo_definitions` with `service_name = "payments"`, seed an `slo_burn_rate` alert rule with `condition = {"slo_id": "<uuid>", ...}`, seed an incident linked to that rule. Assert `GET /v1/incidents/:id` returns `impacted_service: "payments"`.

2. **Threshold incident returns null:** Seed a threshold rule and linked incident. Assert `impacted_service: null`.

### Frontend — `IncidentDetailPage.test.tsx`

- Add MSW handler for `GET /v1/topology` returning a stub `TopologyResponse` with two edges.
- **Test A:** When API response includes `impacted_service: "payments"`, "Impacted Services" panel is present and the topology SVG renders.
- **Test B:** When `impacted_service` is null, no "Impacted Services" panel renders.
- Existing timeline tests remain unmodified.

---

## Files Expected to Change

| File | Change |
|------|--------|
| `services/query-api/src/incidents.rs` | Extend SQL + `IncidentDetailResponse` + `IncidentDetailRow` |
| `tests/http_api_integration.rs` | Two new HTTP integration tests |
| `apps/frontend/src/components/topology/TopologyMap.tsx` | New file — extracted from `ServiceTopologyPage.tsx` |
| `apps/frontend/src/pages/ServiceTopologyPage.tsx` | Import `TopologyMap` from new location |
| `apps/frontend/src/api/incidents.ts` | Add `impacted_service` field |
| `apps/frontend/src/features/incidents/IncidentDetailPage.tsx` | Add Impact panel |
| `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx` | New MSW handler + two new tests |

---

## Rollback Path

Pure additive: the new `impacted_service` field is nullable and the Impact panel is conditionally rendered. Reverting the frontend PR leaves the backend field unreferenced but harmless. No migration to revert.

---

## ADR / Spec Sync

No new architectural decisions. Change uses existing topology API (P3-S8), existing SLO model (P4-S5), and existing incident model (P5-S1). No ADR update needed — state that in the PR.

`docs/agent-context.md` should note that `IncidentDetailResponse` now includes `impacted_service` and that `TopologyMap` lives in `components/topology/`.

---

## Checkpoint Question

Does the impact view remain purely read-only and advisory, adding context without changing incident state or requiring topology data for correctness?

**Answer:** Yes. The Impact panel is conditional and cosmetic — absent topology data does not degrade the incident workflow.

---

## Next Slice

P5-S5: Add composite alert evaluation for one rule pair.
