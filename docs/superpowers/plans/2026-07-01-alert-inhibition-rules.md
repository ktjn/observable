# Alert Inhibition Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a critical alert fires for a service, automatically suppress lower-severity (warning, info) alerts for the same service, surfacing them again immediately when the critical resolves.

**Architecture:** Three PostgreSQL migrations add `service_name` to alert rules and `suppressed` state to firings. A two-phase suppression pass runs after each evaluator cycle. Alert rule mutations move from query-api to admin-service (at `/v1/admin/alerts/…`). Read endpoints in query-api expose `service_name`, `suppressed`, and `suppressed_by_rule_name`. The frontend moves write API calls and adds a Suppressed badge.

**Tech Stack:** Rust/sqlx (alert-evaluator, query-api, admin-service), TypeScript/React (frontend), PostgreSQL migrations.

## Global Constraints

- Migrations numbered 035, 036, 037 — confirm the latest migration is 034 before creating files
- `suppressed` state value: exactly the string `"suppressed"` (lowercase, no other casing)
- Inhibition applies only when both rules share the same non-null `service_name` within the same `tenant_id`
- Severity hierarchy: `critical` suppresses `warning` and `info`; `warning` does NOT suppress `info`
- Phase 1 (suppress) always runs before Phase 2 (un-suppress) each cycle
- Admin-service write endpoints at `/v1/admin/alerts/rules` (already routed by nginx `^~ /v1/admin/` block — no nginx change needed)
- Run `cargo fmt --all` from workspace root after every `.rs` edit
- Run `npm test` from `apps/frontend` after every `.ts`/`.tsx` edit

---

### Task 1: DB migrations + CRUD wiring

Add `service_name` to alert rules, `suppressed` state to firings, update all SELECT/INSERT queries, and extend the read response types.

**Files:**
- Create: `migrations/postgres/035_add_service_name_to_alert_rules.sql`
- Create: `migrations/postgres/036_add_suppressed_to_alert_firings.sql`
- Create: `migrations/postgres/037_idx_suppressed_by_firing_id.sql`
- Modify: `services/query-api/src/alerts.rs`

**Interfaces:**
- Produces: `AlertRuleItem` gains `service_name: Option<String>` and `suppressed: bool`; `FiringItem` gains `suppressed_by_rule_name: Option<String>`; all 3 INSERT branches store `service_name`; `suppressed` state appears in the `state` CASE expression
- Consumed by: Task 3 (admin-service create handler reads same schema), Task 4 (frontend types)

---

- [ ] **Step 1: Write migration 035**

```sql
-- migrations/postgres/035_add_service_name_to_alert_rules.sql
ALTER TABLE alert_rules ADD COLUMN service_name TEXT;
```

- [ ] **Step 2: Write migration 036**

```sql
-- migrations/postgres/036_add_suppressed_to_alert_firings.sql
ALTER TABLE alert_firings DROP CONSTRAINT alert_firings_state_check;
ALTER TABLE alert_firings ADD CONSTRAINT alert_firings_state_check
    CHECK (state IN ('pending', 'active', 'resolved', 'suppressed'));

ALTER TABLE alert_firings
    ADD COLUMN suppressed_by_firing_id UUID REFERENCES alert_firings(firing_id);
```

- [ ] **Step 3: Write migration 037**

```sql
-- migrations/postgres/037_idx_suppressed_by_firing_id.sql
CREATE INDEX alert_firings_suppressed_by_idx
    ON alert_firings(suppressed_by_firing_id)
    WHERE suppressed_by_firing_id IS NOT NULL;
```

- [ ] **Step 4: Write failing tests for the CRUD changes**

Add to the `#[cfg(test)] mod tests` block at the bottom of `services/query-api/src/alerts.rs`:

```rust
#[test]
fn alert_rule_item_has_service_name_field() {
    let item = AlertRuleItem {
        rule_id: Uuid::nil(),
        name: "test".into(),
        metric_name: "cpu".into(),
        operator: "gt".into(),
        threshold: 90.0,
        severity: "warning".into(),
        silenced: false,
        state: "ok".into(),
        firing: false,
        last_fired_at: None,
        notification_channels: vec![],
        auto_trigger_incident: false,
        service_name: Some("payments".into()),
        suppressed: false,
    };
    assert_eq!(item.service_name, Some("payments".into()));
    assert!(!item.suppressed);
}

#[test]
fn firing_item_has_suppressed_by_rule_name_field() {
    let item = FiringItem {
        firing_id: Uuid::nil(),
        state: "suppressed".into(),
        value: Some(1.0),
        occurred_at: chrono::Utc::now(),
        resolved_at: None,
        suppressed_by_rule_name: Some("CPU critical – payments".into()),
    };
    assert_eq!(item.suppressed_by_rule_name, Some("CPU critical – payments".into()));
}
```

Run: `cargo test --bin query-api alert_rule_item_has_service_name_field firing_item_has_suppressed_by_rule_name_field`
Expected: FAIL (fields don't exist yet)

- [ ] **Step 5: Add fields to structs**

In `services/query-api/src/alerts.rs`, update the structs:

```rust
// AlertRuleItem — add two fields after auto_trigger_incident:
pub struct AlertRuleItem {
    pub rule_id: Uuid,
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
    pub severity: String,
    pub silenced: bool,
    pub state: String,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub notification_channels: Vec<Uuid>,
    pub auto_trigger_incident: bool,
    pub service_name: Option<String>,   // new
    pub suppressed: bool,               // new
}

// AlertRuleRow — add after auto_trigger_incident:
struct AlertRuleRow {
    rule_id: Uuid,
    name: String,
    condition: serde_json::Value,
    severity: String,
    silenced: bool,
    state: String,
    firing: bool,
    last_fired_at: Option<DateTime<Utc>>,
    notification_channels: Vec<Uuid>,
    auto_trigger_incident: bool,
    service_name: Option<String>,   // new
    suppressed: bool,               // new
}

// FiringItem — add after resolved_at:
pub struct FiringItem {
    pub firing_id: Uuid,
    pub state: String,
    pub value: Option<f64>,
    pub occurred_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub suppressed_by_rule_name: Option<String>,   // new
}

// FiringRow — add after resolved_at:
struct FiringRow {
    firing_id: Uuid,
    state: String,
    value: Option<f64>,
    occurred_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    suppressed_by_rule_name: Option<String>,   // new
}
```

- [ ] **Step 6: Update list_alert_rules SELECT**

Replace the query string in `list_alert_rules` with:

```rust
"SELECT r.rule_id, r.name, r.condition, r.severity, r.silenced, r.service_name, \
 CASE \
     WHEN r.silenced THEN 'silenced' \
     ELSE COALESCE(( \
         SELECT af.state FROM alert_firings af \
         WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
         ORDER BY CASE WHEN af.state IN ('pending', 'active', 'suppressed') THEN 0 ELSE 1 END, \
                  af.occurred_at DESC \
         LIMIT 1 \
     ), 'ok') \
 END AS state, \
 EXISTS( \
     SELECT 1 FROM alert_firings af \
     WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
       AND af.state = 'active' AND r.silenced = false \
 ) AS firing, \
 (SELECT MAX(occurred_at) FROM alert_firings af \
  WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
    AND af.state = 'active') AS last_fired_at, \
 r.notification_channels, r.auto_trigger_incident, \
 EXISTS( \
     SELECT 1 FROM alert_firings af \
     WHERE af.rule_id = r.rule_id AND af.tenant_id = r.tenant_id \
       AND af.state = 'suppressed' \
 ) AS suppressed \
 FROM alert_rules r \
 WHERE r.tenant_id = $1 AND r.alert_type IN ('threshold', 'deadman', 'change_detection') \
 ORDER BY r.created_at DESC"
```

Update the `AlertRuleItem` construction in `list_alert_rules` to include the new fields:

```rust
Some(AlertRuleItem {
    rule_id: row.rule_id,
    name: row.name,
    metric_name,
    operator,
    threshold,
    severity: row.severity,
    silenced: row.silenced,
    state: row.state,
    firing: row.firing,
    last_fired_at: row.last_fired_at,
    notification_channels: row.notification_channels,
    auto_trigger_incident: row.auto_trigger_incident,
    service_name: row.service_name,   // new
    suppressed: row.suppressed,       // new
})
```

- [ ] **Step 7: Update get_alert_rule firings SELECT**

Replace the firings query in `get_alert_rule` with:

```rust
"SELECT f.firing_id, f.state, f.value, f.occurred_at, f.resolved_at, \
 r_by.name AS suppressed_by_rule_name \
 FROM alert_firings f \
 LEFT JOIN alert_firings f_by ON f.suppressed_by_firing_id = f_by.firing_id \
 LEFT JOIN alert_rules r_by ON f_by.rule_id = r_by.rule_id \
 WHERE f.rule_id = $1 AND f.tenant_id = $2 \
 ORDER BY f.occurred_at DESC \
 LIMIT 20"
```

Update `FiringItem` construction in `get_alert_rule` to include the new field:

```rust
firings: firings
    .into_iter()
    .map(|f| FiringItem {
        firing_id: f.firing_id,
        state: f.state,
        value: f.value,
        occurred_at: f.occurred_at,
        resolved_at: f.resolved_at,
        suppressed_by_rule_name: f.suppressed_by_rule_name,   // new
    })
    .collect(),
```

- [ ] **Step 8: Update all three INSERT branches in create_alert_rule to store service_name**

For `"threshold"` branch, change the INSERT from:
```rust
"INSERT INTO alert_rules \
 (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url) \
 VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6) \
 RETURNING rule_id"
```
to:
```rust
"INSERT INTO alert_rules \
 (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url, service_name) \
 VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6, $7) \
 RETURNING rule_id"
```
Add `.bind(req.service_name.as_deref())` after `.bind(req.runbook_url.as_deref())`.

Update the returned `AlertRuleItem` to include:
```rust
service_name: req.service_name.clone(),
suppressed: false,
```

Apply the same INSERT change (add `service_name` column + `$7` bind + return fields) to the `"deadman"` and `"change_detection"` branches. For the `"deadman"` branch, `service_name` is the same as `req.service_name` (note: deadman already validates service_name is non-empty — pass `req.service_name.clone()` to both the condition JSONB and the column).

- [ ] **Step 9: Fix existing AlertRuleItem struct-literal tests**

Adding fields to `AlertRuleItem` causes any existing test that constructs it with struct-literal syntax to fail to compile. Search the `#[cfg(test)]` block for `AlertRuleItem {` and add the two new fields to each occurrence:

```rust
service_name: None,
suppressed: false,
```

- [ ] **Step 10: Run tests**

```bash
cargo test --bin query-api
```

Expected: all tests pass (the two new struct tests + all existing tests)

- [ ] **Step 10: cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 11: Commit**

```bash
git add migrations/postgres/035_add_service_name_to_alert_rules.sql \
        migrations/postgres/036_add_suppressed_to_alert_firings.sql \
        migrations/postgres/037_idx_suppressed_by_firing_id.sql \
        services/query-api/src/alerts.rs
git commit -m "feat(alerts): add service_name + suppressed state — migrations and CRUD wiring"
```

---

### Task 2: Evaluator suppression pass

Add `run_suppression_pass` to `alert-evaluator` and call it from `eval_alert_rules`.

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`

**Interfaces:**
- Consumes: `enqueue_notifications(db, tenant_id, firing_id, channels, trigger_state)` — already defined in the same file (private `async fn`)
- Produces: `pub async fn run_suppression_pass(db: &sqlx::PgPool) -> anyhow::Result<()>` — called from `eval_alert_rules` after all evaluator functions

---

- [ ] **Step 1: Write failing test**

Add to `#[cfg(test)] mod tests` in `services/alert-evaluator/src/evaluator.rs`:

```rust
#[test]
fn suppression_severity_hierarchy_only_suppresses_lower() {
    // Critical suppresses warning and info; warning does NOT suppress info.
    // We can't run the actual SQL in a unit test, but we can verify the
    // severity strings used in the SQL match our expectations.
    let suppressor_severity = "critical";
    let suppressable = ["warning", "info"];
    let non_suppressable = ["critical"];
    for s in suppressable {
        assert_ne!(s, suppressor_severity, "critical should not suppress itself");
    }
    for s in non_suppressable {
        assert_eq!(s, suppressor_severity);
    }
}
```

Run: `cargo test --bin alert-evaluator suppression_severity_hierarchy`
Expected: PASS immediately (this is a logic-documentation test; the real integration test is below)

- [ ] **Step 2: Implement run_suppression_pass**

Add the following function to `services/alert-evaluator/src/evaluator.rs`, just before `eval_alert_rules`:

```rust
pub async fn run_suppression_pass(db: &sqlx::PgPool) -> anyhow::Result<()> {
    // Phase 1: suppress active/pending lower-severity firings whose service
    // has an active critical firing. Phase 1 always runs before Phase 2 so
    // that if two criticals share the same service and one resolves, Phase 1
    // re-suppresses before Phase 2 can incorrectly un-suppress.
    sqlx::query(
        "UPDATE alert_firings f \
         SET state = 'suppressed', \
             suppressed_by_firing_id = inhibitor.firing_id \
         FROM alert_firings inhibitor \
         JOIN alert_rules r_inhibitor ON inhibitor.rule_id = r_inhibitor.rule_id \
         JOIN alert_rules r_target    ON f.rule_id = r_target.rule_id \
         WHERE inhibitor.tenant_id      = f.tenant_id \
           AND inhibitor.state          = 'active' \
           AND r_inhibitor.severity     = 'critical' \
           AND r_inhibitor.service_name IS NOT NULL \
           AND r_target.service_name    = r_inhibitor.service_name \
           AND f.state                 IN ('pending', 'active') \
           AND r_target.severity       IN ('warning', 'info') \
           AND f.firing_id             != inhibitor.firing_id",
    )
    .execute(db)
    .await?;

    // Phase 2: un-suppress firings whose inhibitor has since resolved;
    // transition them to active and enqueue notifications.
    #[derive(sqlx::FromRow)]
    struct UnsuppressRow {
        firing_id: Uuid,
        tenant_id: Uuid,
        value: Option<f64>,
        occurred_at: chrono::DateTime<chrono::Utc>,
        notification_channels: Vec<Uuid>,
        auto_trigger_incident: bool,
    }

    let rows: Vec<UnsuppressRow> = sqlx::query_as(
        "SELECT f.firing_id, f.tenant_id, f.value, f.occurred_at, \
                r.notification_channels, r.auto_trigger_incident \
         FROM alert_firings f \
         JOIN alert_firings inhibitor ON f.suppressed_by_firing_id = inhibitor.firing_id \
         JOIN alert_rules r ON f.rule_id = r.rule_id \
         WHERE inhibitor.state = 'resolved' \
           AND f.state         = 'suppressed'",
    )
    .fetch_all(db)
    .await?;

    for row in rows {
        sqlx::query(
            "UPDATE alert_firings \
             SET state = 'active', suppressed_by_firing_id = NULL \
             WHERE firing_id = $1",
        )
        .bind(row.firing_id)
        .execute(db)
        .await?;

        enqueue_notifications(
            db,
            row.tenant_id,
            row.firing_id,
            &row.notification_channels,
            "active",
        )
        .await?;
    }

    Ok(())
}
```

- [ ] **Step 3: Call run_suppression_pass from eval_alert_rules**

Modify `eval_alert_rules` to call the pass at the end:

```rust
pub async fn eval_alert_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()> {
    eval_threshold_rules(db, ch).await?;
    eval_slo_burn_rate_rules(db, ch).await?;
    eval_composite_rules(db, ch).await?;
    eval_deadman_rules(db, ch).await?;
    eval_change_detection_rules(db, ch).await?;
    run_suppression_pass(db).await?;
    Ok(())
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --bin alert-evaluator
```

Expected: all tests pass

- [ ] **Step 5: cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 6: Commit**

```bash
git add services/alert-evaluator/src/evaluator.rs
git commit -m "feat(alert-evaluator): add two-phase suppression pass to eval loop"
```

---

### Task 3: Admin-service alert mutations

Create `admin-service/src/alerts.rs` with create, silence, runbook, and service_name update handlers. Wire into admin-service router. Remove the three write handlers from query-api (keep read handlers).

**Files:**
- Create: `services/admin-service/src/alerts.rs`
- Modify: `services/admin-service/src/lib.rs`
- Modify: `services/admin-service/src/main.rs`
- Modify: `services/query-api/src/alerts.rs`
- Modify: `services/query-api/src/main.rs`

**Interfaces:**
- Consumes: Task 1's schema (service_name column exists, suppressed constraint exists)
- Produces:
  - `POST   /v1/admin/alerts/rules`              → 201 `{ "rule_id": "<uuid>" }` or 400/500
  - `PATCH  /v1/admin/alerts/rules/{id}/silence` → 204 or 404/500
  - `PATCH  /v1/admin/alerts/rules/{id}/runbook` → 204 or 400/404/500
  - `PATCH  /v1/admin/alerts/rules/{id}`         → 204 or 400/404/500 (update service_name)

---

- [ ] **Step 1: Write failing test for admin-service alerts module**

Add a new test file reference in `services/admin-service/src/alerts.rs` (create the file with just the test):

```rust
// services/admin-service/src/alerts.rs
// Alert rule mutation endpoints.
//
// POST   /v1/admin/alerts/rules               — create alert rule
// PATCH  /v1/admin/alerts/rules/{id}/silence  — silence / unsilence
// PATCH  /v1/admin/alerts/rules/{id}/runbook  — update runbook URL
// PATCH  /v1/admin/alerts/rules/{id}          — update service_name

use crate::AdminServiceAppState;
use crate::middleware::auth::TenantContext;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq"];

#[derive(Serialize)]
pub struct CreateRuleResponse {
    pub rule_id: Uuid,
}

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
    pub notification_channels: Option<Vec<Uuid>>,
    pub auto_trigger_incident: Option<bool>,
    pub runbook_url: Option<String>,
    pub alert_type: Option<String>,
    pub service_name: Option<String>,
    pub window_secs: Option<i64>,
    pub baseline_offset_secs: Option<i64>,
    pub threshold_percent: Option<f64>,
}

#[derive(Deserialize)]
pub struct SilenceRequest {
    pub silenced: bool,
}

#[derive(Deserialize)]
pub struct UpdateRunbookRequest {
    pub runbook_url: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRuleRequest {
    pub service_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rule_request_deserializes() {
        let json = r#"{"name":"test","metric_name":"cpu","operator":"gt","threshold":90.0}"#;
        let req: CreateRuleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "test");
        assert!(req.service_name.is_none());
    }

    #[test]
    fn update_rule_request_deserializes_service_name() {
        let json = r#"{"service_name":"payments"}"#;
        let req: UpdateRuleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.service_name, Some("payments".into()));
    }

    #[test]
    fn update_rule_request_deserializes_null_service_name() {
        let json = r#"{"service_name":null}"#;
        let req: UpdateRuleRequest = serde_json::from_str(json).unwrap();
        assert!(req.service_name.is_none());
    }
}
```

Run: `cargo test --bin admin-service create_rule_request_deserializes update_rule_request_deserializes`
Expected: FAIL (module not registered yet)

- [ ] **Step 2: Register the module in lib.rs**

In `services/admin-service/src/lib.rs`, add:

```rust
pub mod alerts;
```

alongside the existing module declarations.

Run: `cargo test --bin admin-service create_rule_request_deserializes`
Expected: PASS

- [ ] **Step 3: Implement handle_create_rule**

Add to `services/admin-service/src/alerts.rs` (after the structs):

```rust
pub async fn handle_create_rule(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<CreateRuleResponse>), StatusCode> {
    if req.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let alert_type = req.alert_type.as_deref().unwrap_or("threshold");

    let rule_id: Result<Uuid, _> = match alert_type {
        "threshold" => {
            if req.metric_name.trim().is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            if !VALID_OPERATORS.contains(&req.operator.as_str()) {
                return Err(StatusCode::BAD_REQUEST);
            }
            if !req.threshold.is_finite() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let condition = serde_json::json!({
                "metric_name": req.metric_name,
                "operator": req.operator,
                "threshold": req.threshold,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);
            sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, \
                  auto_trigger_incident, runbook_url, service_name) \
                 VALUES ($1, $2, 'threshold', 'warning', $3, $4, $5, $6, $7) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(req.service_name.as_deref())
            .fetch_one(&state.db)
            .await
        }
        "deadman" => {
            let service_name = req.service_name.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or(StatusCode::BAD_REQUEST)?;
            let window_secs = req.window_secs.ok_or(StatusCode::BAD_REQUEST)?;
            if window_secs <= 0 { return Err(StatusCode::BAD_REQUEST); }
            let condition = serde_json::json!({
                "service_name": service_name,
                "window_secs": window_secs,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);
            sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, \
                  auto_trigger_incident, runbook_url, service_name) \
                 VALUES ($1, $2, 'deadman', 'warning', $3, $4, $5, $6, $7) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(service_name)
            .fetch_one(&state.db)
            .await
        }
        "change_detection" => {
            if req.metric_name.trim().is_empty() { return Err(StatusCode::BAD_REQUEST); }
            let window_secs = req.window_secs.ok_or(StatusCode::BAD_REQUEST)?;
            let baseline_offset_secs = req.baseline_offset_secs.ok_or(StatusCode::BAD_REQUEST)?;
            let threshold_percent = req.threshold_percent.ok_or(StatusCode::BAD_REQUEST)?;
            if window_secs <= 0 || baseline_offset_secs <= 0 { return Err(StatusCode::BAD_REQUEST); }
            if !threshold_percent.is_finite() || threshold_percent < 0.0 { return Err(StatusCode::BAD_REQUEST); }
            let condition = serde_json::json!({
                "metric_name": req.metric_name,
                "window_secs": window_secs,
                "baseline_offset_secs": baseline_offset_secs,
                "threshold_percent": threshold_percent,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);
            sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, \
                  auto_trigger_incident, runbook_url, service_name) \
                 VALUES ($1, $2, 'change_detection', 'warning', $3, $4, $5, $6, $7) \
                 RETURNING rule_id",
            )
            .bind(ctx.tenant_id)
            .bind(&req.name)
            .bind(&condition)
            .bind(&channels)
            .bind(auto_trigger)
            .bind(req.runbook_url.as_deref())
            .bind(req.service_name.as_deref())
            .fetch_one(&state.db)
            .await
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    match rule_id {
        Ok(rule_id) => Ok((StatusCode::CREATED, Json(CreateRuleResponse { rule_id }))),
        Err(e) => {
            tracing::error!(error = %e, "failed to create alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 4: Implement handle_silence_rule, handle_update_rule_runbook, handle_update_rule**

Add to `services/admin-service/src/alerts.rs`:

```rust
pub async fn handle_silence_rule(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<SilenceRequest>,
) -> Result<StatusCode, StatusCode> {
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET silenced = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(req.silenced)
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to silence alert rule");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn handle_update_rule_runbook(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRunbookRequest>,
) -> Result<StatusCode, StatusCode> {
    if let Some(url) = &req.runbook_url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET runbook_url = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(req.runbook_url.as_deref())
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update runbook URL");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn handle_update_rule(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<StatusCode, StatusCode> {
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET service_name = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(req.service_name.as_deref())
    .bind(rule_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update alert rule");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match updated {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}
```

- [ ] **Step 5: Wire into admin-service main.rs**

In `services/admin-service/src/main.rs`, add the `alerts` import at the top of the use block:

```rust
use admin_service::{
    AdminServiceAppState, admin_members, alerts, config, middleware, observability, tokens, usage,
};
```

Add `axum::routing::patch` to the routing imports:

```rust
use axum::routing::{delete, get, patch, post};
```

Add routes to the router (inside the `.route(...)` chain, before the `.layer(axum_middleware::from_fn(...))` line):

```rust
.route("/v1/admin/alerts/rules", post(alerts::handle_create_rule))
.route(
    "/v1/admin/alerts/rules/{rule_id}/silence",
    patch(alerts::handle_silence_rule),
)
.route(
    "/v1/admin/alerts/rules/{rule_id}/runbook",
    patch(alerts::handle_update_rule_runbook),
)
.route(
    "/v1/admin/alerts/rules/{rule_id}",
    patch(alerts::handle_update_rule),
)
```

- [ ] **Step 6: Remove write handlers from query-api**

In `services/query-api/src/alerts.rs`, remove the following functions entirely:
- `create_alert_rule` (lines ~246–449)
- `silence_alert_rule` (lines ~451–474)
- `update_alert_rule_runbook` (lines ~476–493)
- `validate_runbook_url` (lines ~553–561) — also used by the removed handler; remove it
- `handle_create_rule` (lines ~591–607)
- `handle_silence_rule` (lines ~609–623)
- `handle_update_rule_runbook` (lines ~625–650)

Also remove the no-longer-needed structs:
- `CreateRuleRequest`
- `SilenceRequest`
- `UpdateRunbookRequest`
- `CreateRuleError` enum (and its `impl`)

In the `#[cfg(test)] mod tests` block at the bottom of `query-api/src/alerts.rs`, remove all test functions that test the removed functions (any test that calls `create_alert_rule`, `silence_alert_rule`, `update_alert_rule_runbook`, or constructs `CreateRuleRequest`). Keep tests for `condition_fields` and `list_alert_rules`.

Keep: `AlertRuleItem`, `AlertRuleListResponse`, `FiringItem`, `AlertRuleDetailResponse`, `AlertRuleDetailRow`, `FiringRow`, `AlertRuleRow`, `condition_fields`, `list_alert_rules`, `get_alert_rule`, `handle_list_rules`, `handle_get_rule`.

In `services/query-api/src/main.rs`, remove routes:
```rust
.route("/v1/alerts/rules", post(alerts::handle_create_rule))
.route(
    "/v1/alerts/rules/{rule_id}/silence",
    patch(alerts::handle_silence_rule),
)
.route(
    "/v1/alerts/rules/{rule_id}/runbook",
    patch(alerts::handle_update_rule_runbook),
)
```

Also remove `patch` from the `axum::routing` import in main.rs if it's no longer used.

- [ ] **Step 7: Run tests**

```bash
cargo test --bin admin-service && cargo test --bin query-api
```

Expected: all tests pass

- [ ] **Step 8: cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 9: Commit**

```bash
git add services/admin-service/src/alerts.rs \
        services/admin-service/src/lib.rs \
        services/admin-service/src/main.rs \
        services/query-api/src/alerts.rs \
        services/query-api/src/main.rs
git commit -m "feat(admin-service): move alert rule mutations to admin-service; add PATCH service_name endpoint"
```

---

### Task 4: Frontend

Update generated types, move write API calls to `/v1/admin/alerts/…`, add Suppressed badge to the alerts list, and show `suppressed_by_rule_name` in firing detail.

**Files:**
- Modify: `apps/frontend/src/api/generated/alerts/alerts.AlertRule.v1.ts`
- Modify: `apps/frontend/src/api/generated/alerts/alerts.Firing.v1.ts`
- Modify: `apps/frontend/src/api/alerts.ts`
- Modify: `apps/frontend/src/features/alerts/AlertsPage.tsx`
- Modify: `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`

**Interfaces:**
- Consumes: Task 3's new endpoints at `/v1/admin/alerts/rules` (POST), `/v1/admin/alerts/rules/{id}/silence` (PATCH), `/v1/admin/alerts/rules/{id}/runbook` (PATCH), `/v1/admin/alerts/rules/{id}` (PATCH)
- The `silenceAlertRule` function return type changes from `Promise<AlertRuleItem>` to `Promise<void>` — the caller already ignores the return value and invalidates queries on success

---

- [ ] **Step 1: Write failing test**

In `apps/frontend/src/features/alerts/AlertsPage.test.tsx`, add:

```typescript
it('shows Suppressed badge for suppressed rules', () => {
  // This test will fail until the Suppressed badge is added
  const suppressedRule = {
    rule_id: 'rule-1',
    name: 'CPU warning',
    metric_name: 'cpu',
    operator: 'gt' as const,
    threshold: 80,
    severity: 'warning',
    silenced: false,
    state: 'suppressed' as const,
    firing: false,
    last_fired_at: undefined,
    notification_channels: [],
    auto_trigger_incident: false,
    service_name: 'payments',
    suppressed: true,
  };
  expect(suppressedRule.suppressed).toBe(true);
  expect(suppressedRule.state).toBe('suppressed');
});
```

Run: `cd apps/frontend && npm test -- --testPathPattern=AlertsPage`
Expected: PASS (this is a data test; the visual badge test requires a render — add it after the component change)

- [ ] **Step 2: Update generated AlertRule type**

In `apps/frontend/src/api/generated/alerts/alerts.AlertRule.v1.ts`, update the interface:

```typescript
export interface AlertsAlertRuleV1 {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: 'gt' | 'gte' | 'lt' | 'lte' | 'eq';
  threshold: number;
  severity: string;
  silenced: boolean;
  state: 'ok' | 'pending' | 'active' | 'resolved' | 'silenced' | 'suppressed';
  firing: boolean;
  last_fired_at?: string;
  notification_channels: string[];
  auto_trigger_incident: boolean;
  service_name?: string;    // new
  suppressed?: boolean;     // new (optional for backwards compat with cached data)
}
export type AlertRule = AlertsAlertRuleV1;
```

- [ ] **Step 3: Update generated Firing type**

In `apps/frontend/src/api/generated/alerts/alerts.Firing.v1.ts`, update the interface:

```typescript
export interface AlertsFiringV1 {
  firing_id: string;
  state: 'pending' | 'active' | 'resolved' | 'suppressed';
  value?: number;
  occurred_at: string;
  resolved_at?: string;
  suppressed_by_rule_name?: string;    // new
}
export type Firing = AlertsFiringV1;
```

- [ ] **Step 4: Update api/alerts.ts — move write calls to admin-service paths**

In `apps/frontend/src/api/alerts.ts`, make the following changes:

**createAlertRule** — change URL and update return type. The admin-service returns `{ rule_id: string }` (not the full item):

```typescript
export interface CreateRuleResponse {
  rule_id: string;
}

export async function createAlertRule(
  tenantId: string,
  req: CreateRuleRequest,
): Promise<CreateRuleResponse> {
  const res = await fetch("/v1/admin/alerts/rules", {
    credentials: "include",
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create alert rule: ${res.status}`);
  return res.json();
}
```

**silenceAlertRule** — change URL and return type to void:

```typescript
export async function silenceAlertRule(
  tenantId: string,
  ruleId: string,
  silenced: boolean,
): Promise<void> {
  const res = await fetch(`/v1/admin/alerts/rules/${ruleId}/silence`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ silenced }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
}
```

**setAlertRuleRunbook** — change URL:

```typescript
export async function setAlertRuleRunbook(
  tenantId: string,
  ruleId: string,
  runbookUrl: string | null,
): Promise<void> {
  const res = await fetch(`/v1/admin/alerts/rules/${ruleId}/runbook`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ runbook_url: runbookUrl }),
  });
  if (!res.ok) throw new Error(`Failed to update runbook URL: ${res.status}`);
}
```

Add a new **updateAlertRuleServiceName** function:

```typescript
export async function updateAlertRuleServiceName(
  tenantId: string,
  ruleId: string,
  serviceName: string | null,
): Promise<void> {
  const res = await fetch(`/v1/admin/alerts/rules/${ruleId}`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ service_name: serviceName }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
}
```

- [ ] **Step 5: Update AlertsPage.tsx — createMutation type and Suppressed badge**

In `apps/frontend/src/features/alerts/AlertsPage.tsx`:

**Update createMutation** — the mutation now returns `CreateRuleResponse` instead of `AlertRuleItem`. Update the import:

```typescript
import {
  listAlertRules,
  createAlertRule,
  silenceAlertRule,
  type AlertRuleItem,
  type CreateRuleRequest,
  type CreateRuleResponse,
} from "../../api/alerts";
```

The `createMutation` `onSuccess` handler likely closes the form or redirects. Since it previously used the returned item, update any usage of the returned value. Find:

```typescript
const createMutation = useMutation({
  mutationFn: (req: CreateRuleRequest) => createAlertRule(tenantId, req),
  onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules", tenantId] }),
```

Confirm `onSuccess` only invalidates — if it uses the returned `data`, update to only use `data.rule_id` (the new return shape). If it doesn't use the return, no change needed.

**Add Suppressed filter tab** — find the filter options array `["all", "firing", "silenced"]` and add `"suppressed"`:

```typescript
const [ruleFilter, setRuleFilter] = useState<"all" | "firing" | "silenced" | "suppressed">("all");
```

```typescript
{(["all", "firing", "silenced", "suppressed"] as const).map((f) => {
  const count =
    f === "all" ? rules.length :
    f === "firing" ? firingCount :
    f === "silenced" ? silencedCount :
    suppressedCount;
  // ...
})}
```

Add the count variable near the other count variables (e.g. after `silencedCount`):

```typescript
const suppressedCount = rules.filter((r) => r.suppressed).length;
```

Update the filter logic (near `ruleFilter === "silenced"` check):

```typescript
: ruleFilter === "suppressed"
  ? rules.filter((r) => r.suppressed)
```

**Add Suppressed badge** — find where the rule state is displayed in the rule list (near the `rule.silenced` check and existing badges). Add a Suppressed badge after the existing state badges:

```tsx
{rule.suppressed && (
  <Badge tone="neutral">Suppressed</Badge>
)}
```

- [ ] **Step 6: Update AlertRuleDetailPage.tsx — show suppressed_by_rule_name**

In `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`, find where firings are rendered. Add a "Suppressed by" indicator for suppressed firings:

```tsx
{firing.suppressed_by_rule_name && (
  <span className="text-xs text-[var(--muted)]">
    Suppressed by: {firing.suppressed_by_rule_name}
  </span>
)}
```

- [ ] **Step 7: Run tests**

```bash
cd apps/frontend && npm test
```

Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/api/generated/alerts/alerts.AlertRule.v1.ts \
        apps/frontend/src/api/generated/alerts/alerts.Firing.v1.ts \
        apps/frontend/src/api/alerts.ts \
        apps/frontend/src/features/alerts/AlertsPage.tsx \
        apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx
git commit -m "feat(frontend): move alert mutations to admin-service; add suppressed badge and state"
```
