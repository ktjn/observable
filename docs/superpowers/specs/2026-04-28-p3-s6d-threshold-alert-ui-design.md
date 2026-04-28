# P3-S6d: Threshold Alert UI Workflow — Design Spec

**Date:** 2026-04-28
**Slice:** P3-S6d
**Status:** Approved

---

## 1. Goal

Give operators one complete threshold-alert loop in the UI: list active threshold rules with
firing state, create a new rule for an existing metric, and silence or unsilence a rule —
all without leaving the Alerts & SLOs page.

---

## 2. Scope

**In scope:**
- Three new query-api endpoints for alert rule CRUD and silence toggle
- Migration adding `silenced` column to `alert_rules`
- Alert-evaluator skips silenced rules
- Dedicated `AlertsPage` React component replacing the placeholder
- Typed API client and MSW handlers
- Backend Testcontainers integration tests and frontend RTL tests

**Out of scope:**
- Firings history view (a future slice)
- Escalation routing, burn-rate/SLO authoring, composite alerts
- Role-based restriction beyond existing tenant auth (admin-only rule creation is a later slice)

---

## 3. Data Model Change

Migration `010_add_silenced_to_alert_rules.sql`:

```sql
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS silenced BOOLEAN NOT NULL DEFAULT false;
```

The alert-evaluator `eval_threshold_rules` function gains a `WHERE silenced = false` clause so
silenced rules are never evaluated.

---

## 4. Backend API

All endpoints mount under the existing `require_tenant` auth middleware in `query-api`.
Implementation lives in `services/query-api/src/alerts.rs`.

### 4.1 List rules

```
GET /v1/alerts/rules
```

Returns all `threshold` rules for the authenticated tenant, with latest firing state derived
from a LEFT JOIN on `alert_firings` for the most recent `active` firing per rule.

**Response:**
```json
{
  "items": [
    {
      "rule_id": "10000000-0000-0000-0000-000000000001",
      "name": "High error rate",
      "metric_name": "error_rate",
      "operator": "gt",
      "threshold": 0.05,
      "severity": "warning",
      "silenced": false,
      "firing": true,
      "last_fired_at": "2026-04-28T10:00:00Z"
    }
  ]
}
```

`firing` is `true` when a row exists in `alert_firings` with `state = 'active'` for that rule.
`last_fired_at` is `null` when no firings exist.

### 4.2 Create rule

```
POST /v1/alerts/rules
Content-Type: application/json
```

**Request body:**
```json
{
  "name": "High latency",
  "metric_name": "p95_latency_ms",
  "operator": "gt",
  "threshold": 500.0
}
```

`severity` defaults to `warning`. `alert_type` is always `threshold` for this slice.

**Response:** `201 Created` with the created rule object (same shape as a list item, `firing: false`).

**Validation:** `name` non-empty, `metric_name` non-empty, `operator` one of `gt|gte|lt|lte|eq`,
`threshold` finite number.

### 4.3 Toggle silence

```
PATCH /v1/alerts/rules/:rule_id/silence
Content-Type: application/json
```

**Request body:**
```json
{ "silenced": true }
```

Toggles the `silenced` flag. Returns `200` with the updated rule object. Returns `404` if the
rule does not belong to the authenticated tenant.

---

## 5. Frontend

### 5.1 File layout

```
apps/frontend/src/
  api/alerts.ts                        — typed API client
  features/alerts/
    AlertsPage.tsx                     — page component
    AlertsPage.test.tsx                — RTL + MSW tests
  mocks/handlers/alerts.ts             — MSW handlers
```

### 5.2 Router change

`router.ts`: `/alerts` route component changes from `ProductAreaPage{area:"alerts"}` to
`AlertsPage`.

### 5.3 Page layout

```
[Page header: "Alerts & SLOs / Reliability"]

[Metric tiles: Total Rules | Firing | Silenced]

[Toolbar: ──────────────────────── [New Rule ▸]]

[Table]
Name          Metric         Condition      Severity  Status    Action
High error rate  error_rate  > 0.05         warning   ● Firing  [Silence]
High latency     p95…        > 500          warning   ○ OK      [Silence]

[Create panel — slides in from right when "New Rule" is clicked]
  Name:        [__________________]
  Metric name: [__________________]
  Operator:    [gt ▾]
  Threshold:   [__________________]
  [Cancel]  [Create Rule]
```

### 5.4 UI primitives

`Button`, `Input`, `Select` from `src/components/ui/` — all pre-existing.
The create panel uses a simple conditional render (`isCreating` boolean state), not a modal
library.

### 5.5 State and data flow

- `useQuery(["alert-rules"])` fetches `GET /v1/alerts/rules` on mount.
- Silence toggle: `useMutation` → `PATCH .../silence` → `queryClient.invalidateQueries(["alert-rules"])`.
- Create submit: `useMutation` → `POST /v1/alerts/rules` → close panel + invalidate.
- Error states render inline below the triggering element.

---

## 6. Testing

### 6.1 Backend

**Unit tests** (`services/query-api/src/alerts.rs`):
- `list_rules_returns_seeded_rule` — seeded dev rule present in response
- `create_rule_persists` — POST creates row, GET includes it
- `silence_toggle` — PATCH flips silenced flag
- `cross_tenant_isolation` — tenant B cannot see or silence tenant A's rules

**Alert-evaluator unit test:**
- `silenced_rule_is_skipped` — `eval_threshold_rules` does not write a firing for a silenced rule

**Testcontainers integration test** (`services/query-api/tests/alerts_integration.rs`):
- End-to-end against real Postgres: list, create, silence, cross-tenant 404

### 6.2 Frontend

**`AlertsPage.test.tsx`** (RTL + MSW):
- Renders rule list from MSW handler
- Shows "Firing" badge when `firing: true`, "OK" when false
- Silence button sends PATCH and list refreshes
- Unsilence button (on silenced rule) sends PATCH `{ silenced: false }`
- "New Rule" button opens create panel
- Create form submit sends POST and panel closes
- Empty state renders when no rules exist

---

## 7. Spec and ADR Sync

- `spec/09-api.md` — add three endpoint entries under a new "Alert Rules" section
- `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` — mark P3-S6d complete
- No new ADR required — no new architectural decisions

---

## 8. Rollback

Remove the three route registrations from `query-api/src/main.rs` and revert the router change
in the frontend. The migration column is additive and safe to leave in place.
The evaluator's `silenced = false` filter has no effect if the column defaults to false.
