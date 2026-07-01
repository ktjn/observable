# Alert Inhibition Rules — Design

**Date:** 2026-07-01
**Status:** Approved
**Roadmap:** `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` §3 (Tier 1, next item after Prometheus Remote Write)

---

## 1. Goal

When a critical alert is firing for a service, automatically suppress lower-severity (warning, info) alerts for the same service so that operators focus on the root cause rather than a cascade of downstream noise. Suppressed alerts surface again immediately when the critical resolves.

---

## 2. Design Decisions

- **Implicit, not configurable.** No inhibition rule objects to manage. Critical always suppresses warning and info. Severity hierarchy: `critical > warning > info`.
- **Grouped by `service_name` tag.** A nullable `service_name` column is added to `alert_rules`. Inhibition only applies when both the inhibiting (critical) and inhibited (warning/info) rules share the same non-null `service_name` within the same tenant.
- **Suppression pass in the evaluator loop.** After each evaluation cycle, a two-phase pass: (1) suppress eligible lower-severity firings; (2) un-suppress firings whose inhibitor has resolved, transitioning them immediately to `active` with notifications enqueued.
- **API split: admin-service owns writes, query-api owns reads.** This feature migrates the existing alert rule mutation endpoints to admin-service and adds the new `service_name` update there.

---

## 3. Data Model

### Migration A — `service_name` on `alert_rules`

```sql
ALTER TABLE alert_rules ADD COLUMN service_name TEXT;
```

Nullable. Existing rules get `NULL` and are never inhibited.

### Migration B — `suppressed` state and `suppressed_by_firing_id` on `alert_firings`

```sql
ALTER TABLE alert_firings DROP CONSTRAINT alert_firings_state_check;
ALTER TABLE alert_firings ADD CONSTRAINT alert_firings_state_check
  CHECK (state IN ('pending', 'active', 'resolved', 'suppressed'));

ALTER TABLE alert_firings
  ADD COLUMN suppressed_by_firing_id UUID REFERENCES alert_firings(firing_id);
```

`suppressed_by_firing_id` records which critical firing is doing the suppressing — used for un-suppress and for the "Suppressed by: X" UI label.

The existing `alert_firings_one_open_per_rule_idx` unique index (`WHERE state IN ('pending','active')`) is unchanged — `suppressed` firings are excluded, which is correct.

### Migration C — Index for un-suppress query

```sql
CREATE INDEX alert_firings_suppressed_by_idx
  ON alert_firings(suppressed_by_firing_id)
  WHERE suppressed_by_firing_id IS NOT NULL;
```

---

## 4. Evaluator Suppression Pass

Added to `services/alert-evaluator/src/evaluator.rs` as `run_suppression_pass(tenant_id, pool)`, called at the end of `eval_alert_rules()`.

### Phase 1 — Suppress

Find all `pending` or `active` lower-severity firings whose service has an `active` critical firing, and set them to `suppressed`:

```sql
UPDATE alert_firings f
SET state = 'suppressed',
    suppressed_by_firing_id = inhibitor.firing_id
FROM alert_firings inhibitor
JOIN alert_rules r_inhibitor ON inhibitor.rule_id = r_inhibitor.rule_id
JOIN alert_rules r_target    ON f.rule_id = r_target.rule_id
WHERE inhibitor.tenant_id      = f.tenant_id
  AND inhibitor.state          = 'active'
  AND r_inhibitor.severity     = 'critical'
  AND r_inhibitor.service_name IS NOT NULL
  AND r_target.service_name    = r_inhibitor.service_name
  AND f.state                 IN ('pending', 'active')
  AND r_target.severity       IN ('warning', 'info')
  AND f.tenant_id              = $1
```

Phase 1 always runs before Phase 2 each cycle. This means if two criticals both suppress the same warning and one resolves, Phase 1 re-suppresses it (the other critical is still active) before Phase 2 can incorrectly un-suppress it.

### Phase 2 — Un-suppress

Find all `suppressed` firings whose inhibitor has resolved:

```sql
SELECT f.firing_id, f.rule_id, f.tenant_id, f.value, f.occurred_at,
       r.notification_channels, r.auto_trigger_incident
FROM alert_firings f
JOIN alert_firings inhibitor ON f.suppressed_by_firing_id = inhibitor.firing_id
JOIN alert_rules r ON f.rule_id = r.rule_id
WHERE inhibitor.state = 'resolved'
  AND f.state         = 'suppressed'
  AND f.tenant_id     = $1
```

For each row:
1. `UPDATE alert_firings SET state = 'active', suppressed_by_firing_id = NULL WHERE firing_id = $1`
2. Call existing `enqueue_notifications()` with the firing details.

---

## 5. API Changes

### admin-service (mutations — new home for existing + new endpoints)

The following endpoints **move from query-api to admin-service** (same request/response shape, new base URL):

| Method | Path | Action |
|--------|------|--------|
| `POST` | `/v1/alerts/rules` | Create alert rule |
| `PATCH` | `/v1/alerts/rules/{rule_id}/silence` | Silence / unsilence |
| `PATCH` | `/v1/alerts/rules/{rule_id}/runbook` | Update runbook URL |

New endpoint in admin-service:

```
PATCH /v1/alerts/rules/{rule_id}
{ "service_name": "payments" }   // null to clear
```

`CreateRuleRequest` gains:
```rust
service_name: Option<String>,
```

### query-api (reads — unchanged routing)

`AlertRuleItem` (list response) gains:
```rust
service_name: Option<String>,
suppressed: bool,   // true when this rule has a suppressed firing open
```

`FiringItem` (detail response) gains:
```rust
suppressed_by_rule_name: Option<String>,  // e.g. "CPU critical – payments"
```

Resolved by joining `suppressed_by_firing_id → alert_firings → alert_rules.name` in the detail query.

### Frontend

API call sites for the three moved mutations updated to point at admin-service base URL. No response shape changes.

---

## 6. UI Changes

**Alerts list:** Firings in `suppressed` state show a grey "Suppressed" badge. Tooltip: "Suppressed by: [rule name]" (from `suppressed_by_rule_name`).

**Rule create/edit form:** Optional "Service name" text input below the severity field. Placeholder: `e.g. payments`. Present for all rule types.

---

## 7. Tests

### Evaluator unit tests (`alert-evaluator/src/evaluator.rs`)

- Critical active + warning active, same tenant + service_name → warning → `suppressed`, `suppressed_by_firing_id` set
- Critical active + warning active, different service_name → no suppression
- Critical active + info active, same service_name → info → `suppressed`
- Critical active + warning rule with `service_name = NULL` → no suppression
- Critical resolves → suppressed warning → `active`, notification enqueued
- Two criticals for same service → warning stays suppressed while either is active; un-suppresses only when both resolve
- Warning already `resolved` before suppression pass → not affected

### API tests

**admin-service:**
- `POST /v1/alerts/rules` with `service_name` → stored, returned
- `PATCH /v1/alerts/rules/{rule_id}` with `service_name: "payments"` → persisted
- `PATCH /v1/alerts/rules/{rule_id}` with `service_name: null` → cleared

**query-api:**
- `GET /v1/alerts/rules` → `suppressed: true` when firing is suppressed
- `GET /v1/alerts/rules/{rule_id}` → `suppressed_by_rule_name` populated on suppressed firing

---

## 8. Out of Scope

- Configurable inhibition rules (alert X suppresses alert Y)
- Alertmanager-style label matchers
- Inhibition across tenants
- UI for managing inhibition rules (none needed — implicit)
- Moving `GET` endpoints from query-api (reads stay in query-api)
