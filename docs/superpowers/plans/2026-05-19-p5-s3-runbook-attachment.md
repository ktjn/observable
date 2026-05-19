# P5-S3 Runbook Attachment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire a `runbook_url` field from alert rules through to auto-created incidents so responders see a clickable runbook link on the incident detail page, and operators can set/edit it on the alert rule detail page.

**Architecture:** One new Postgres migration adds `runbook_url` to `alert_rules`. The query-api gains a new `PATCH /v1/alerts/rules/:id/runbook` endpoint and includes `runbook_url` in the rule detail response. The alert-evaluator copies `runbook_url` from the rule into the incident INSERT. The frontend adds an inline-edit runbook row to `AlertRuleDetailPage` and an optional field to the create-rule form in `AlertsPage`. The `incidents` table already has `runbook_url` and `IncidentDetailPage` already renders it.

**Tech Stack:** Rust (sqlx, axum), TypeScript (React, TanStack Query), Vitest + Testing Library, Testcontainers (Postgres + ClickHouse)

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `migrations/postgres/029_add_runbook_url_to_alert_rules.sql` | Create | Add `runbook_url TEXT` column to `alert_rules` |
| `services/query-api/src/alerts.rs` | Modify | Extend read/create paths; add validation fn + PATCH handler |
| `services/query-api/src/main.rs` | Modify | Register `PATCH /v1/alerts/rules/{rule_id}/runbook` |
| `services/alert-evaluator/src/evaluator.rs` | Modify | Fetch `runbook_url` in SELECT; propagate to incident INSERT |
| `services/alert-evaluator/tests/incident_integration.rs` | Modify | Add `runbook_url_copied_from_rule_to_incident` test |
| `apps/frontend/src/api/alerts.ts` | Modify | Add `runbook_url` to types; add `setAlertRuleRunbook` fn |
| `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx` | Modify | Runbook row with inline edit |
| `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx` | Modify | Tests for runbook URL display; update `sampleRule` fixture |
| `apps/frontend/src/features/alerts/AlertsPage.tsx` | Modify | Add Runbook URL field to create-rule form |

---

## Task 1: Migration — add runbook_url to alert_rules

**Files:**
- Create: `migrations/postgres/029_add_runbook_url_to_alert_rules.sql`

- [ ] **Step 1: Create the migration file**

```sql
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS runbook_url TEXT;
```

Write this content to `migrations/postgres/029_add_runbook_url_to_alert_rules.sql`.

- [ ] **Step 2: Commit**

```bash
git add migrations/postgres/029_add_runbook_url_to_alert_rules.sql
git commit -m "feat(db): add runbook_url column to alert_rules"
```

---

## Task 2: query-api — extend alert rule detail read path

**Files:**
- Modify: `services/query-api/src/alerts.rs`

- [ ] **Step 1: Write a failing unit test**

Add this test to the `#[cfg(test)]` block at the bottom of `services/query-api/src/alerts.rs` (before the closing `}`):

```rust
#[test]
fn alert_rule_detail_response_includes_runbook_url() {
    let r = AlertRuleDetailResponse {
        rule_id: Uuid::nil(),
        name: "test".into(),
        severity: "warning".into(),
        alert_type: "threshold".into(),
        condition: serde_json::json!({}),
        silenced: false,
        firing: false,
        firings: vec![],
        runbook_url: Some("https://example.com/runbook".into()),
    };
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["runbook_url"], "https://example.com/runbook");
}
```

- [ ] **Step 2: Verify it fails to compile**

```bash
cargo test -p query-api -- alert_rule_detail_response_includes_runbook_url 2>&1 | head -20
```

Expected: compile error — `runbook_url` field not found on `AlertRuleDetailResponse`.

- [ ] **Step 3: Add runbook_url to AlertRuleDetailRow**

In `services/query-api/src/alerts.rs`, find the `struct AlertRuleDetailRow` block (currently lines 57–65) and add `runbook_url: Option<String>` after `firing: bool`:

```rust
#[derive(sqlx::FromRow)]
struct AlertRuleDetailRow {
    rule_id: Uuid,
    name: String,
    severity: String,
    alert_type: String,
    condition: serde_json::Value,
    silenced: bool,
    firing: bool,
    runbook_url: Option<String>,
}
```

- [ ] **Step 4: Add runbook_url to AlertRuleDetailResponse**

Find the `pub struct AlertRuleDetailResponse` block (currently lines 45–54) and add `runbook_url: Option<String>` after `firing: bool`:

```rust
#[derive(Serialize)]
pub struct AlertRuleDetailResponse {
    pub rule_id: Uuid,
    pub name: String,
    pub severity: String,
    pub alert_type: String,
    pub condition: serde_json::Value,
    pub silenced: bool,
    pub firing: bool,
    pub firings: Vec<FiringItem>,
    pub runbook_url: Option<String>,
}
```

- [ ] **Step 5: Extend the SELECT query in get_alert_rule**

In `get_alert_rule`, replace the SQL string (currently lines 291–299) with:

```rust
let row: Option<AlertRuleDetailRow> = sqlx::query_as(
    "SELECT r.rule_id, r.name, r.severity, r.alert_type, r.condition, r.silenced, r.runbook_url, \
     EXISTS( \
         SELECT 1 FROM alert_firings af \
         WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
           AND af.state = 'active' AND r.silenced = false \
     ) AS firing \
     FROM alert_rules r \
     WHERE r.rule_id = $1 AND r.tenant_id = $2",
)
.bind(rule_id)
.bind(tenant_id)
.fetch_optional(db)
.await?;
```

- [ ] **Step 6: Include runbook_url in the returned AlertRuleDetailResponse**

In `get_alert_rule`, find the `Ok(Some(AlertRuleDetailResponse { ... }))` block and add `runbook_url: row.runbook_url`:

```rust
Ok(Some(AlertRuleDetailResponse {
    rule_id: row.rule_id,
    name: row.name,
    severity: row.severity,
    alert_type: row.alert_type,
    condition: row.condition,
    silenced: row.silenced,
    firing: row.firing,
    runbook_url: row.runbook_url,
    firings: firings
        .into_iter()
        .map(|f| FiringItem {
            firing_id: f.firing_id,
            state: f.state,
            value: f.value,
            occurred_at: f.occurred_at,
            resolved_at: f.resolved_at,
        })
        .collect(),
}))
```

- [ ] **Step 7: Run the test**

```bash
cargo test -p query-api -- alert_rule_detail_response_includes_runbook_url
```

Expected: `test alert_rule_detail_response_includes_runbook_url ... ok`

- [ ] **Step 8: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 9: Commit**

```bash
git add services/query-api/src/alerts.rs
git commit -m "feat(alerts): include runbook_url in alert rule detail response"
```

---

## Task 3: query-api — add PATCH runbook endpoint + extend create path

**Files:**
- Modify: `services/query-api/src/alerts.rs`
- Modify: `services/query-api/src/main.rs`

- [ ] **Step 1: Write failing validation unit tests**

Add these four tests to the `#[cfg(test)]` block in `services/query-api/src/alerts.rs`:

```rust
#[test]
fn validate_runbook_url_accepts_https() {
    assert!(validate_runbook_url(&Some("https://example.com/runbook".into())).is_ok());
}

#[test]
fn validate_runbook_url_accepts_http() {
    assert!(validate_runbook_url(&Some("http://internal.example.com".into())).is_ok());
}

#[test]
fn validate_runbook_url_rejects_missing_scheme() {
    assert!(validate_runbook_url(&Some("example.com/runbook".into())).is_err());
}

#[test]
fn validate_runbook_url_accepts_none() {
    assert!(validate_runbook_url(&None).is_ok());
}
```

- [ ] **Step 2: Verify compile failure**

```bash
cargo test -p query-api -- validate_runbook_url 2>&1 | head -10
```

Expected: compile error — `validate_runbook_url` not found.

- [ ] **Step 3: Implement validate_runbook_url**

Add this function directly above `handle_update_rule_runbook` (before the `pub async fn handle_silence_rule` block, for example after `silence_alert_rule`):

```rust
fn validate_runbook_url(url: &Option<String>) -> Result<(), String> {
    if let Some(u) = url {
        if !u.starts_with("http://") && !u.starts_with("https://") {
            return Err("runbook_url must start with http:// or https://".into());
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run validation tests**

```bash
cargo test -p query-api -- validate_runbook_url
```

Expected: all four tests pass.

- [ ] **Step 5: Add UpdateRunbookRequest struct**

Add after the `SilenceRequest` struct:

```rust
#[derive(Deserialize)]
pub struct UpdateRunbookRequest {
    pub runbook_url: Option<String>,
}
```

- [ ] **Step 6: Implement update_alert_rule_runbook**

Add after `silence_alert_rule`:

```rust
pub async fn update_alert_rule_runbook(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    rule_id: Uuid,
    runbook_url: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET runbook_url = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(runbook_url)
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(updated.is_some())
}
```

- [ ] **Step 7: Implement handle_update_rule_runbook**

Add after `handle_silence_rule`:

```rust
pub async fn handle_update_rule_runbook(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRunbookRequest>,
) -> Result<StatusCode, StatusCode> {
    if let Err(msg) = validate_runbook_url(&req.runbook_url) {
        tracing::warn!(message = %msg, "invalid runbook URL");
        return Err(StatusCode::BAD_REQUEST);
    }
    match update_alert_rule_runbook(
        &state.db,
        ctx.tenant_id,
        rule_id,
        req.runbook_url.as_deref(),
    )
    .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to update runbook URL");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 8: Add runbook_url to CreateRuleRequest**

In `services/query-api/src/alerts.rs`, find `pub struct CreateRuleRequest` and add `pub runbook_url: Option<String>` after `auto_trigger_incident`:

```rust
#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
    pub notification_channels: Option<Vec<Uuid>>,
    pub auto_trigger_incident: Option<bool>,
    pub runbook_url: Option<String>,
}
```

- [ ] **Step 9: Extend create_alert_rule INSERT to include runbook_url**

In `create_alert_rule`, replace the `INSERT` query (currently around line 230) with:

```rust
let rule_id: Uuid = sqlx::query_scalar(
    "INSERT INTO alert_rules \
     (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url) \
     VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6) \
     RETURNING rule_id",
)
.bind(tenant_id)
.bind(&req.name)
.bind(&condition)
.bind(&channels)
.bind(auto_trigger)
.bind(req.runbook_url.as_deref())
.fetch_one(db)
.await
.map_err(CreateRuleError::Db)?;
```

- [ ] **Step 10: Register route in main.rs**

In `services/query-api/src/main.rs`, after the existing silence route (around line 124), add:

```rust
.route(
    "/v1/alerts/rules/{rule_id}/runbook",
    patch(alerts::handle_update_rule_runbook),
)
```

- [ ] **Step 11: Run all query-api tests**

```bash
cargo test -p query-api
```

Expected: all tests pass.

- [ ] **Step 12: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 13: Commit**

```bash
git add services/query-api/src/alerts.rs services/query-api/src/main.rs
git commit -m "feat(alerts): add PATCH /runbook endpoint and runbook_url to create path"
```

---

## Task 4: alert-evaluator — propagate runbook_url to incidents

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`
- Modify: `services/alert-evaluator/tests/incident_integration.rs`

- [ ] **Step 1: Add runbook_url to AlertRuleRow**

In `services/alert-evaluator/src/evaluator.rs`, find the `pub struct AlertRuleRow` block (around line 79) and add `pub runbook_url: Option<String>` after `auto_trigger_delay_secs`:

```rust
#[derive(sqlx::FromRow, Clone)]
pub struct AlertRuleRow {
    pub rule_id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub condition: serde_json::Value,
    pub severity: String,
    pub for_duration_secs: Option<i64>,
    pub notification_channels: Vec<Uuid>,
    pub auto_trigger_incident: bool,
    pub auto_trigger_delay_secs: Option<i64>,
    pub runbook_url: Option<String>,
}
```

- [ ] **Step 2: Extend the threshold rules SELECT query**

In `eval_threshold_rules` (around line 119), replace the SQL string with:

```rust
let rules: Vec<AlertRuleRow> = sqlx::query_as(
    "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
     auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
     FROM alert_rules WHERE alert_type = 'threshold' AND silenced = false",
)
.fetch_all(db)
.await?;
```

- [ ] **Step 3: Extend the SLO burn-rate rules SELECT query**

In `eval_slo_burn_rate_rules` (around line 203), replace its SQL string with:

```rust
let rules: Vec<AlertRuleRow> = sqlx::query_as(
    "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
     auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
     FROM alert_rules WHERE alert_type = 'slo_burn_rate' AND silenced = false",
)
.fetch_all(db)
.await?;
```

- [ ] **Step 4: Propagate runbook_url in upsert_incident_from_firing**

In `upsert_incident_from_firing` (around line 518), replace the `INSERT INTO incidents` query with:

```rust
let id: Uuid = sqlx::query_scalar(
    "INSERT INTO incidents \
     (tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, runbook_url) \
     VALUES ($1, $2, $3, 'triggered', $4, $5, $6) \
     RETURNING incident_id",
)
.bind(rule.tenant_id)
.bind(&rule.name)
.bind(&rule.severity)
.bind(dedup_key)
.bind(rule.rule_id)
.bind(rule.runbook_url.as_deref())
.fetch_one(db)
.await?;
```

- [ ] **Step 5: Write a failing integration test**

Add a helper function and a new test to `services/alert-evaluator/tests/incident_integration.rs`.

Add the helper after `create_threshold_rule_with_auto_trigger`:

```rust
async fn create_threshold_rule_with_runbook(
    pool: &PgPool,
    tenant_id: Uuid,
    metric_name: &str,
    threshold: f64,
    runbook_url: &str,
) -> Uuid {
    let rule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition, auto_trigger_incident, runbook_url) \
         VALUES ($1, $2, 'runbook test rule', 'threshold', 'warning', $3, true, $4)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "metric_name": metric_name,
        "operator": "gt",
        "threshold": threshold,
    }))
    .bind(runbook_url)
    .execute(pool)
    .await
    .expect("threshold rule with runbook inserted");
    rule_id
}
```

Add the test at the end of the file:

```rust
#[tokio::test]
async fn runbook_url_copied_from_rule_to_incident() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let metric_name = "runbook_propagation_metric";
    let runbook = "https://runbooks.example.com/high-error-rate";
    let rule_id =
        create_threshold_rule_with_runbook(&pool, tenant_id, metric_name, 0.05, runbook).await;

    insert_metric_point(&ch, tenant_id, metric_name, 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();

    let fetched_runbook: Option<String> = sqlx::query_scalar(
        "SELECT runbook_url FROM incidents \
         WHERE tenant_id = $1 AND triggered_by_rule_id = $2",
    )
    .bind(tenant_id)
    .bind(rule_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(fetched_runbook.as_deref(), Some(runbook));
}
```

- [ ] **Step 6: Run the integration test**

```bash
cargo test -p alert-evaluator -- runbook_url_copied_from_rule_to_incident
```

Expected: `test runbook_url_copied_from_rule_to_incident ... ok`

- [ ] **Step 7: Run all alert-evaluator tests**

```bash
cargo test -p alert-evaluator
```

Expected: all tests pass.

- [ ] **Step 8: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 9: Commit**

```bash
git add services/alert-evaluator/src/evaluator.rs services/alert-evaluator/tests/incident_integration.rs
git commit -m "feat(evaluator): propagate runbook_url from alert rule to incident on creation"
```

---

## Task 5: Frontend API — add types and setAlertRuleRunbook

**Files:**
- Modify: `apps/frontend/src/api/alerts.ts`
- Modify: `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`

- [ ] **Step 1: Add runbook_url to AlertRuleDetailResponse**

In `apps/frontend/src/api/alerts.ts`, replace the `AlertRuleDetailResponse` interface (currently lines 76–85) with:

```typescript
export interface AlertRuleDetailResponse {
  rule_id: string;
  name: string;
  severity: string;
  alert_type: string;
  condition: Record<string, unknown>;
  silenced: boolean;
  firing: boolean;
  firings: FiringItem[];
  runbook_url: string | null;
}
```

- [ ] **Step 2: Add runbook_url to CreateRuleRequest**

Replace the `CreateRuleRequest` interface (currently lines 24–31) with:

```typescript
export interface CreateRuleRequest {
  name: string;
  metric_name: string;
  operator: string;
  threshold: number;
  notification_channels?: string[];
  auto_trigger_incident?: boolean;
  runbook_url?: string;
}
```

- [ ] **Step 3: Add setAlertRuleRunbook function**

Add at the end of `apps/frontend/src/api/alerts.ts`:

```typescript
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

- [ ] **Step 4: Update sampleRule fixture in AlertRuleDetailPage.test.tsx**

In `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`, add `runbook_url: null` to the `sampleRule` object (after `firings: [...]`):

```typescript
const sampleRule: alertsApi.AlertRuleDetailResponse = {
  rule_id: "rule-1",
  name: "High CPU Alert",
  severity: "critical",
  alert_type: "threshold",
  condition: { metric_name: "cpu_usage", operator: "gt", threshold: 90 },
  silenced: false,
  firing: true,
  firings: [
    {
      firing_id: "f-1",
      state: "active",
      value: 95.3,
      occurred_at: "2026-05-18T10:00:05Z",
      resolved_at: null,
    },
    {
      firing_id: "f-2",
      state: "resolved",
      value: 91.0,
      occurred_at: "2026-05-18T09:00:00Z",
      resolved_at: "2026-05-18T09:30:00Z",
    },
  ],
  runbook_url: null,
};
```

- [ ] **Step 5: Run typecheck**

```bash
cd apps/frontend && npm run typecheck
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/api/alerts.ts apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx
git commit -m "feat(frontend): add runbook_url to AlertRuleDetailResponse type and setAlertRuleRunbook"
```

---

## Task 6: AlertRuleDetailPage — inline runbook edit

**Files:**
- Modify: `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`
- Modify: `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`

- [ ] **Step 1: Write two failing tests**

Add these tests at the end of `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx` (before the last closing line, if any):

```typescript
test("renders runbook URL as a link when present", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue({
    ...sampleRule,
    runbook_url: "https://runbooks.example.com/high-cpu",
  });
  renderPage();
  await waitFor(() =>
    screen.getByRole("link", { name: "https://runbooks.example.com/high-cpu" }),
  );
  expect(
    screen.getByRole("link", { name: "https://runbooks.example.com/high-cpu" }),
  ).toHaveAttribute("href", "https://runbooks.example.com/high-cpu");
});

test("renders dash when runbook URL is null", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue({
    ...sampleRule,
    runbook_url: null,
  });
  renderPage();
  await waitFor(() =>
    screen.getByRole("heading", { level: 1, name: "High CPU Alert" }),
  );
  expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(1);
});
```

- [ ] **Step 2: Verify tests fail**

```bash
cd apps/frontend && npx vitest run src/features/alerts/AlertRuleDetailPage.test.tsx
```

Expected: the two new tests FAIL (link element not found).

- [ ] **Step 3: Update imports in AlertRuleDetailPage.tsx**

Replace the import block at the top of `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx` with:

```typescript
import { useState } from "react";
import { useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getAlertRule, setAlertRuleRunbook, type FiringItem } from "../../api/alerts";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { EmptyState } from "../../components/ui/empty-state";
import { Input } from "../../components/ui/input";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";
```

- [ ] **Step 4: Add state and mutation inside AlertRuleDetailPage**

Inside `export function AlertRuleDetailPage()`, after the `const { data, isLoading } = useQuery(...)` block, add:

```typescript
const queryClient = useQueryClient();
const [editingRunbook, setEditingRunbook] = useState(false);
const [runbookDraft, setRunbookDraft] = useState("");

const runbookMutation = useMutation({
  mutationFn: (url: string | null) => setAlertRuleRunbook(tenantId, ruleId, url),
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: ["alertRule", tenantId, ruleId] });
    setEditingRunbook(false);
  },
});
```

- [ ] **Step 5: Add Runbook row to the info panel**

In the JSX, find the `<div className="grid grid-cols-2 gap-4 text-sm">` inside the first `<Panel>`. After the closing `</div>` of the Condition cell, add:

```tsx
<div className="col-span-2">
  <div className="field-label">Runbook</div>
  <div className="mt-1 flex items-center gap-2">
    {editingRunbook ? (
      <>
        <Input
          type="url"
          className="flex-1"
          value={runbookDraft}
          onChange={(e) => setRunbookDraft(e.target.value)}
          placeholder="https://..."
          aria-label="Runbook URL"
        />
        <Button
          onClick={() => runbookMutation.mutate(runbookDraft || null)}
          disabled={runbookMutation.isPending}
        >
          Save
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            setRunbookDraft(data.runbook_url ?? "");
            setEditingRunbook(false);
          }}
        >
          Cancel
        </Button>
      </>
    ) : (
      <>
        {data.runbook_url ? (
          <a
            href={data.runbook_url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--brand)] hover:underline"
          >
            {data.runbook_url}
          </a>
        ) : (
          <span className="text-[var(--muted)]">—</span>
        )}
        <button
          type="button"
          aria-label="Edit runbook URL"
          onClick={() => {
            setRunbookDraft(data.runbook_url ?? "");
            setEditingRunbook(true);
          }}
          className="text-xs text-[var(--muted)] hover:text-[var(--text)]"
        >
          ✎
        </button>
      </>
    )}
  </div>
</div>
```

- [ ] **Step 6: Run tests**

```bash
cd apps/frontend && npx vitest run src/features/alerts/AlertRuleDetailPage.test.tsx
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx \
        apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx
git commit -m "feat(ui): add runbook URL row with inline edit to AlertRuleDetailPage"
```

---

## Task 7: AlertsPage — runbook URL field in create-rule form

**Files:**
- Modify: `apps/frontend/src/features/alerts/AlertsPage.tsx`

- [ ] **Step 1: Add formRunbookUrl state**

In `apps/frontend/src/features/alerts/AlertsPage.tsx`, after the `const [formError, setFormError] = useState<string | null>(null);` line (around line 43), add:

```typescript
const [formRunbookUrl, setFormRunbookUrl] = useState("");
```

- [ ] **Step 2: Add Runbook URL input to the create form**

In the create-rule form JSX, after the `autoTriggerIncident` checkbox label (around line 297), add:

```tsx
<div className="space-y-1">
  <label
    className="text-xs font-bold uppercase text-[var(--muted)]"
    htmlFor="runbook-url"
  >
    Runbook URL{" "}
    <span className="font-normal normal-case text-[var(--muted)]">(optional)</span>
  </label>
  <Input
    id="runbook-url"
    type="url"
    placeholder="https://..."
    value={formRunbookUrl}
    onChange={(e) => setFormRunbookUrl(e.target.value)}
  />
</div>
```

- [ ] **Step 3: Include runbook_url in handleCreateSubmit**

In `handleCreateSubmit`, replace the `createMutation.mutate({...})` call (around line 110) with:

```typescript
createMutation.mutate({
  name: formName,
  metric_name: formMetric,
  operator: formOperator,
  threshold,
  notification_channels: selectedChannels,
  auto_trigger_incident: autoTriggerIncident,
  runbook_url: formRunbookUrl || undefined,
});
```

- [ ] **Step 4: Reset formRunbookUrl on success**

In `createMutation.onSuccess` (around line 74), add `setFormRunbookUrl("")` after `setAutoTriggerIncident(true)`:

```typescript
onSuccess: () => {
  queryClient.invalidateQueries({ queryKey: ["alert-rules", tenantId] });
  setIsCreating(false);
  setFormName("");
  setFormMetric("");
  setFormOperator("gt");
  setFormThreshold("");
  setSelectedChannels([]);
  setAutoTriggerIncident(true);
  setFormRunbookUrl("");
  setFormError(null);
},
```

- [ ] **Step 5: Run typecheck**

```bash
cd apps/frontend && npm run typecheck
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertsPage.tsx
git commit -m "feat(ui): add optional Runbook URL field to alert rule create form"
```

---

## Verification

After all tasks are complete:

- [ ] **Run full Rust test suite**

```bash
cargo test -p query-api && cargo test -p alert-evaluator
```

Expected: all tests pass.

- [ ] **Run frontend typecheck**

```bash
cd apps/frontend && npm run typecheck
```

Expected: no errors.

- [ ] **End-to-end smoke (optional, requires Docker)**

1. `docker compose up -d`
2. Open `/alerts` → create a rule with a runbook URL
3. Trigger a metric value above threshold (or wait for the evaluator)
4. Open `/incidents` → click the new incident → confirm runbook link is visible
5. Open `/alerts/$ruleId` → confirm runbook URL is shown → click ✎ → edit → save → confirm updated URL appears

- [ ] **Update the roadmap plan**

In `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, mark P5-S3 as completed:

```markdown
- [x] **P5-S3: Add runbook workflow attachment to an alert or incident** (COMPLETED 2026-05-19)
  - `alert_rules.runbook_url TEXT` column (migration 029); `GET /v1/alerts/rules/:id` returns it; `POST /v1/alerts/rules` accepts it; `PATCH /v1/alerts/rules/:id/runbook` sets/clears it with http/https validation; evaluator copies it to `incidents.runbook_url` on incident creation; `AlertRuleDetailPage` shows inline-editable runbook row; `AlertsPage` create form includes optional Runbook URL field.
```

- [ ] **Final commit**

```bash
git add docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md
git commit -m "docs(roadmap): mark P5-S3 runbook attachment complete"
```
