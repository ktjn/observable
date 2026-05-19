# P5-S3 Runbook Attachment Design

**Date:** 2026-05-19  
**Phase:** 5 — Reliability Product  
**Slice:** P5-S3 — Runbook workflow attachment to alert rules and incidents  
**Status:** Approved

---

## Context

Incidents are auto-created by `alert-evaluator` when a threshold or SLO burn-rate rule fires. The
`incidents` table already has a `runbook_url TEXT` column and `IncidentDetailPage` already renders
it as a clickable link. However, there is no way to attach a runbook URL to an alert rule, and the
evaluator never populates `incidents.runbook_url` when auto-creating an incident. This slice wires
the missing pieces.

---

## Goal

An operator can attach a runbook URL to an alert rule. When that rule auto-fires an incident, the
runbook URL is copied to the incident so responders see it immediately on the incident detail page.
The URL can also be set when creating the rule, and edited later via an inline field on the alert
rule detail page.

---

## Scope

### In scope
- `alert_rules.runbook_url` column (new migration)
- `GET /v1/alerts/rules/:id` returns `runbook_url`
- `POST /v1/alerts/rules` accepts optional `runbook_url`
- `PATCH /v1/alerts/rules/:id/runbook` sets/clears `runbook_url` on an existing rule
- `alert-evaluator` copies `runbook_url` from rule to incident at incident creation time
- `AlertRuleDetailPage` shows runbook URL with inline edit (pencil → input → Save/Cancel)
- `AlertsPage` create-rule form includes optional runbook URL field

### Out of scope
- Per-incident runbook URL override (no PATCH on incidents)
- Runbook URL validation beyond `http://` or `https://` prefix check
- Runbook content rendering or embedding
- Runbook workflow execution (P5-S3b or later)

---

## Data Model

### New migration: `029_add_runbook_url_to_alert_rules.sql`

```sql
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS runbook_url TEXT;
```

No default, nullable. Existing rows get `NULL`.

The `incidents.runbook_url TEXT` column already exists (migration 027). No change needed.

---

## Backend

### `services/query-api/src/alerts.rs`

**Structs to extend:**

```rust
pub struct AlertRuleDetailResponse {
    // existing fields ...
    pub runbook_url: Option<String>,   // ADD
}

struct AlertRuleDetailRow {
    // existing fields ...
    runbook_url: Option<String>,       // ADD
}

pub struct CreateRuleRequest {
    // existing fields ...
    pub runbook_url: Option<String>,   // ADD
}
```

**`get_alert_rule` query** — extend the `SELECT` to include `r.runbook_url`.

**`create_alert_rule`** — insert `runbook_url` from the request (or `NULL`). No validation needed
beyond what the DB enforces (TEXT, nullable).

**New handler: `handle_update_rule_runbook`**

```
PATCH /v1/alerts/rules/:id/runbook
Body: { "runbook_url": "https://..." }   // null clears the field
```

- Validate: if non-null, must start with `http://` or `https://` (400 otherwise)
- `UPDATE alert_rules SET runbook_url = $1 WHERE rule_id = $2 AND tenant_id = $3`
- Returns 204 on success, 404 if not found

**New request/response types:**

```rust
#[derive(Deserialize)]
pub struct UpdateRunbookRequest {
    pub runbook_url: Option<String>,
}
```

No response body (204). Register the route in `main.rs`:

```
.route("/v1/alerts/rules/:id/runbook", patch(handle_update_rule_runbook))
```

### `services/alert-evaluator/src/evaluator.rs`

**`AlertRuleRow`** — add `runbook_url: Option<String>`.

**Both `SELECT` queries** (threshold rules at line ~120, SLO rules at line ~203) — add
`r.runbook_url` to the column list.

**`upsert_incident_from_firing`** — add `runbook_url: Option<&str>` parameter. Extend the
`INSERT INTO incidents` to include `runbook_url`:

```sql
INSERT INTO incidents (tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, runbook_url)
VALUES ($1, $2, $3, 'triggered', $4, $5, $6)
RETURNING incident_id
```

Bind `rule.runbook_url.as_deref()` as `$6`. The `resolve_incident_for_firing` function does not
need changes (it only updates `status` and `resolved_at`).

---

## Frontend

### `apps/frontend/src/api/alerts.ts`

```typescript
export interface AlertRuleDetailResponse {
  // existing fields ...
  runbook_url: string | null;   // ADD
}

export interface CreateRuleRequest {
  // existing fields ...
  runbook_url?: string;          // ADD (optional)
}

export async function setAlertRuleRunbook(
  tenantId: string,
  ruleId: string,
  runbookUrl: string | null,
): Promise<void> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}/runbook`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ runbook_url: runbookUrl }),
  });
  if (!res.ok) throw new Error(`Failed to update runbook URL: ${res.status}`);
}
```

### `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`

Add a **Runbook** row to the info panel. Behaviour:

- If `data.runbook_url` is set: display it as a `<a>` link with a pencil icon button beside it
- If not set: display "—" with a pencil icon button
- Clicking pencil: replace with `<input type="url">` pre-filled with current value, plus **Save**
  and **Cancel** buttons
- **Save**: calls `setAlertRuleRunbook`, then invalidates `["alertRule", tenantId, ruleId]` query
- **Cancel**: reverts to display mode without saving
- Local state: `editingRunbook: boolean`, `runbookDraft: string`

Inline edit is self-contained within the component — no separate form component needed.

### `apps/frontend/src/features/alerts/AlertsPage.tsx`

Add an optional **Runbook URL** `<input type="url">` field to the create-rule form, below the
existing fields. Pass the value (or `undefined` if blank) in the `CreateRuleRequest`.

---

## Propagation Flow

```
Operator sets runbook_url on alert rule (create or PATCH)
        ↓
alert-evaluator fires threshold/SLO rule
        ↓
upsert_incident_from_firing reads rule.runbook_url
        ↓
INSERT INTO incidents (..., runbook_url) VALUES (..., $6)
        ↓
IncidentDetailPage already renders incidents.runbook_url as a link ✓
```

For incidents created before this change, `runbook_url` remains `NULL` (existing behavior, already
handled by the conditional render in `IncidentDetailPage`).

---

## Tests

### Backend unit tests (`alerts.rs`)
- `handle_update_rule_runbook` rejects a URL that doesn't start with `http://` or `https://` → 400
- `handle_update_rule_runbook` accepts `null` (clears the field) → 204

### `alert-evaluator` integration test
- Insert a rule with `runbook_url = 'https://runbooks.example.com/high-error-rate'`
- Simulate a firing (call `upsert_incident_from_firing` directly or via the evaluator loop)
- Assert the created incident row has the same `runbook_url`

### Frontend
- `npm run typecheck` in `apps/frontend` covers interface correctness
- Existing `AlertRuleDetailPage.test.tsx` — add one test: when `runbook_url` is non-null, the
  rendered page contains the URL text

---

## Verification

1. `cargo test -p query-api` passes
2. `cargo test -p alert-evaluator` passes (or integration test for evaluator)
3. `npm run typecheck` in `apps/frontend` passes
4. Bring up compose: create a rule with a runbook URL → trigger a firing → open the incident →
   runbook link is visible
5. Open the rule detail page → runbook URL shows → inline edit works → save → refreshed page shows
   updated URL
6. `bash scripts/local-ci.sh` green

---

## Files Changed

| File | Change |
|------|--------|
| `migrations/postgres/029_add_runbook_url_to_alert_rules.sql` | New — add column |
| `services/query-api/src/alerts.rs` | Add `runbook_url` to structs, queries, create handler; add `handle_update_rule_runbook` |
| `services/query-api/src/main.rs` | Register `PATCH /v1/alerts/rules/:id/runbook` |
| `services/alert-evaluator/src/evaluator.rs` | Fetch and propagate `runbook_url` |
| `apps/frontend/src/api/alerts.ts` | Add `runbook_url` to types; add `setAlertRuleRunbook` |
| `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx` | Runbook row with inline edit |
| `apps/frontend/src/features/alerts/AlertsPage.tsx` | Runbook URL field in create form |
