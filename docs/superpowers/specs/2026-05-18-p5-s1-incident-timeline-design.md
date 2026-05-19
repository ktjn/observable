# P5-S1: Incident Timeline with Source Links — Design Spec

**Date:** 2026-05-18
**Roadmap slice:** P5-S1
**Status:** Approved, ready for implementation planning

---

## Problem

The incident timeline infrastructure is complete end-to-end: `incidents` and `incident_events` tables exist, the alert evaluator writes `triggered`/`alert_fired`/`alert_resolved` events, the API returns them, and `IncidentDetailPage.tsx` renders a Timeline panel. However two gaps prevent P5-S1's completion signal ("one alert source can produce a durable timeline with **source links**"):

1. Timeline event messages are not human-readable — `alert_fired` events show `"Firing {uuid}"` instead of the rule name and triggering value.
2. There is no navigation path from an incident timeline event to the alert rule that fired it.

---

## Scope

### In scope
- Enrich `IncidentDetailResponse` with `rule_name`
- Enrich `alert_fired`/`alert_resolved` event messages with rule name and value
- New `GET /v1/alerts/{ruleId}` endpoint returning rule detail + recent firings
- New `AlertRuleDetailPage` at `/alerts/$ruleId`
- Source link on timeline events (`alert_fired`, `alert_resolved`) linking to the rule detail page
- Render `runbook_url` on `IncidentDetailPage` (already in API, currently silently ignored)
- HTTP integration tests for new/changed backend behaviour
- Frontend unit tests for both changed and new pages

### Out of scope
- Acknowledge/resolve actions from the incident page
- Manual annotation or comment events
- Linking timeline events to the correlated trace or metric query
- SLO detail page

---

## Architecture

### Data flow (unchanged)

```
alert-evaluator
  → alert_firings (PostgreSQL)
  → incidents (PostgreSQL)          [auto_trigger_incident = true]
  → incident_events (PostgreSQL)    [triggered, alert_fired, alert_resolved]

query-api
  GET /v1/incidents/:id             → IncidentDetailResponse (with rule_name)
  GET /v1/alerts/:ruleId            → AlertRuleDetailResponse (new)

frontend
  IncidentDetailPage                → timeline with source links
  AlertRuleDetailPage               → rule metadata + firing history
```

---

## Backend Changes

### 1. `services/query-api/src/incidents.rs`

Add `rule_name: Option<String>` to `IncidentDetailResponse` and `IncidentDetailRow`.

Query becomes a LEFT JOIN:

```sql
SELECT i.incident_id, i.title, i.severity, i.status, i.dedup_key,
       i.triggered_at, i.resolved_at, i.triggered_by_rule_id, i.runbook_url,
       r.name AS rule_name
FROM incidents i
LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id
WHERE i.incident_id = $1 AND i.tenant_id = $2
```

`rule_name` is `None` when `triggered_by_rule_id` is null or the rule has been deleted.

### 2. `services/alert-evaluator/src/evaluator.rs`

Enrich messages at the two call sites that write incident events:

| Function | Event type | Old message | New message |
|---|---|---|---|
| `upsert_incident_from_firing` | `alert_fired` | `"Firing {firing_id}"` | `"{rule.name} fired: value={value:.2}"` |
| `resolve_incident_for_firing` | `alert_resolved` | `"Firing {firing_id} resolved"` | `"{rule.name} resolved: value={value:.2}"` |

`rule.name` and `value` are already in scope at both call sites. The `firing_id` is dropped from the message — it is still the primary key of `alert_firings` and queryable via the new detail endpoint.

### 3. New `GET /v1/alerts/{ruleId}` endpoint in `services/query-api/src/alerts.rs`

**Response shape:**

```json
{
  "rule_id": "uuid",
  "name": "High CPU Alert",
  "severity": "critical",
  "alert_type": "threshold",
  "condition": { "metric_name": "cpu_usage", "operator": "gt", "threshold": 90.0 },
  "silenced": false,
  "firing": true,
  "firings": [
    {
      "firing_id": "uuid",
      "state": "active",
      "value": 95.3,
      "occurred_at": "2026-05-18T10:00:05Z",
      "resolved_at": null
    }
  ]
}
```

`firings` is the 20 most recent rows from `alert_firings` for the rule, ordered by `occurred_at DESC`. Returns 404 when the rule does not exist in the tenant.

Route registered at `GET /v1/alerts/:rule_id`.

---

## Frontend Changes

### 1. `apps/frontend/src/api/incidents.ts`

Add `rule_name: string | null` to `IncidentDetailResponse`.

### 2. `apps/frontend/src/api/alerts.ts`

Add `getAlertRule(tenantId, ruleId)` function and `AlertRuleDetailResponse` / `FiringItem` interfaces.

### 3. `apps/frontend/src/features/incidents/IncidentDetailPage.tsx`

**Timeline source link:** On `alert_fired` and `alert_resolved` events, when `triggered_by_rule_id` is non-null, render a `<Link to="/alerts/$ruleId">→ View rule</Link>` inline after the actor line.

**Runbook URL:** Add a third cell to the metadata panel showing `runbook_url` as a clickable external link when present; `—` otherwise.

### 4. New `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`

Two panels:

**Rule metadata panel**
- Name, severity badge, alert type, silenced badge
- Condition rendered as a readable summary (e.g. `cpu_usage > 90`) for threshold rules; SLO reference for burn-rate rules

**Firing history panel**
- Table: State (badge), Value, Occurred At, Resolved At
- 20-row cap (matches API)
- Empty state: "No firings recorded."

### 5. `apps/frontend/src/router.ts`

New route:

```ts
const alertRuleDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts/$ruleId",
  component: AlertRuleDetailPage,
});
```

Registered alongside `alertsRoute`.

---

## Testing

### Backend — `services/query-api/tests/http_api_integration.rs`

| Test | What it covers |
|---|---|
| `get_incident_detail_includes_rule_name` | Incident with `triggered_by_rule_id` set returns matching `rule_name` |
| `get_incident_detail_rule_name_null_when_no_rule` | Incident with null `triggered_by_rule_id` returns `rule_name: null` |
| `get_alert_rule_returns_detail_with_firings` | Rule + 2 firings → correct fields and `firings` array length |
| `get_alert_rule_returns_404_for_wrong_tenant` | Cross-tenant isolation enforced |

All tests use `mode: "interpret"` to avoid needing live ClickHouse (incident/alert paths exit before any CH query).

### Frontend — new test files

**`apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx`**
- Renders timeline events with correct glyphs and humanized labels
- Renders `→ View rule` link on `alert_fired` event when `triggered_by_rule_id` is set
- Does not render `→ View rule` link when `triggered_by_rule_id` is null
- Renders `runbook_url` as a link when present; `—` when null

**`apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`**
- Renders rule name, severity, alert type
- Renders firings table with correct row count
- Renders empty state when `firings` is empty

---

## Rollback

All backend changes are additive (new field, new endpoint, message text change). Rolling back means reverting the three changed files and one new frontend page. No migration required — no schema changes.

---

## ADR / Spec Sync

No new architectural decisions. This slice is within the existing incident model (ADR-008 auth model, existing `incidents`/`incident_events` schema). `docs/agent-context.md` update required to note the new `/alerts/$ruleId` route and the `rule_name` enrichment.

---

## Completion Signal

An alert rule with `auto_trigger_incident = true` fires → an incident is created → `IncidentDetailPage` shows the timeline with human-readable `alert_fired` message → clicking `→ View rule` opens `AlertRuleDetailPage` showing the rule's condition and the specific firing row.
