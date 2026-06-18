# P12-S3 Deadman Alert Type Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `deadman` alert rule type that fires when a service has stopped emitting spans for a configurable window, reusing the existing alert rule/firing/notification machinery.

**Architecture:** A new pure evaluation function plus a new `eval_deadman_rules` DB/ClickHouse query in `services/alert-evaluator`, wired into the existing eval loop. The query-api's `alerts.rs` gains a second branch in `create_alert_rule` and a second shape in `condition_fields` so deadman rules can be created and listed through the existing `AlertRuleItem` schema (no generated-type changes — see the approved design doc). The frontend's `AlertsPage.tsx` gets an alert-type selector that swaps the threshold fields for service-name/window fields.

**Tech Stack:** Rust (axum, sqlx, clickhouse crate, tokio), React 19 + TypeScript + TanStack Query, Vitest + React Testing Library.

## Global Constraints

- Reuse existing `AlertRuleItem`/`metric_name`/`operator`/`threshold` fields for deadman display — do not extend the modelable-generated schema (per the approved design doc, `docs/superpowers/specs/2026-06-18-p12-s3-deadman-alert-design.md`).
- No environment scoping on deadman conditions — condition carries only `service_name` + `window_secs`.
- "No span ever seen" counts as stale and must fire immediately.
- Run `cargo fmt --all` after every Rust edit, before staging.
- Rust changes crossing PostgreSQL/ClickHouse need the narrowest applicable Testcontainers integration test (already true of `eval_threshold_rules`/`eval_slo_burn_rate_rules`/`create_alert_rule`/`list_alert_rules`, which deadman extends).
- Work happens on branch `feat/p12-s3-deadman-alert` (already checked out, holds the design-doc commit). Never commit to `main` directly.

---

### Task 1: Deadman pure-logic evaluator

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`

**Interfaces:**
- Produces: `pub struct DeadmanCondition { pub service_name: String, pub window_secs: i64 }` (deserialized from rule `condition` JSONB, same pattern as `ThresholdCondition`).
- Produces: `pub fn evaluate_deadman(last_seen_secs_ago: Option<i64>, condition: &DeadmanCondition) -> EvalResult` — `None` (never seen) or `Some(elapsed) where elapsed >= window_secs` fires; otherwise OK. Used by Task 2's `eval_deadman_rules`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of `services/alert-evaluator/src/evaluator.rs` (after the existing `multi_window_requires_fast_and_slow_to_fire` test):

```rust
    fn deadman_cond(window_secs: i64) -> DeadmanCondition {
        DeadmanCondition {
            service_name: "checkout".into(),
            window_secs,
        }
    }

    #[test]
    fn deadman_fires_when_never_seen() {
        assert_eq!(evaluate_deadman(None, &deadman_cond(300)), EvalResult::Firing);
    }

    #[test]
    fn deadman_fires_when_stale() {
        assert_eq!(
            evaluate_deadman(Some(301), &deadman_cond(300)),
            EvalResult::Firing
        );
    }

    #[test]
    fn deadman_fires_exactly_at_boundary() {
        assert_eq!(
            evaluate_deadman(Some(300), &deadman_cond(300)),
            EvalResult::Firing
        );
    }

    #[test]
    fn deadman_ok_when_fresh() {
        assert_eq!(evaluate_deadman(Some(10), &deadman_cond(300)), EvalResult::Ok);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p alert-evaluator deadman 2>&1 | tail -30`
Expected: compile error — `DeadmanCondition` and `evaluate_deadman` not found.

- [ ] **Step 3: Implement the minimal code**

In `services/alert-evaluator/src/evaluator.rs`, add this near the other condition structs (right after `CompositeRuleCondition`, before `calculate_burn_rate`):

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct DeadmanCondition {
    pub service_name: String,
    pub window_secs: i64,
}

pub fn evaluate_deadman(
    last_seen_secs_ago: Option<i64>,
    condition: &DeadmanCondition,
) -> EvalResult {
    match last_seen_secs_ago {
        None => EvalResult::Firing,
        Some(elapsed) if elapsed >= condition.window_secs => EvalResult::Firing,
        Some(_) => EvalResult::Ok,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p alert-evaluator deadman 2>&1 | tail -30`
Expected: `4 passed`.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add services/alert-evaluator/src/evaluator.rs
git commit -m "feat(alert-evaluator): add deadman condition evaluation logic"
```

---

### Task 2: Wire deadman evaluation into the DB/ClickHouse eval loop

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`
- Create: `services/alert-evaluator/tests/deadman_integration.rs`

**Interfaces:**
- Consumes: `DeadmanCondition`, `evaluate_deadman` (Task 1); `AlertRuleRow`, `record_firing`, `resolve_open_firing` (existing, same signatures used by `eval_threshold_rules`).
- Produces: `pub async fn eval_deadman_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()>`, called from `eval_alert_rules`.

- [ ] **Step 1: Write the failing integration test**

Create `services/alert-evaluator/tests/deadman_integration.rs`. This mirrors `services/alert-evaluator/tests/slo_burn_rate_integration.rs`'s container setup — copy its `apply_pg_migrations`, `start_postgres`, `apply_ch_migrations`, `start_clickhouse`, `make_span`, and `insert_span` helpers verbatim (same file layout, same imports), then add:

```rust
use alert_evaluator::evaluator::eval_deadman_rules;
use domain::SpanRow;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres};
use uuid::Uuid;

// ... apply_pg_migrations, start_postgres, apply_ch_migrations, start_clickhouse,
// make_span, insert_span copied from slo_burn_rate_integration.rs ...

#[tokio::test]
async fn deadman_rule_fires_when_service_never_seen() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let rule_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Checkout silent', 'deadman', 'critical', $3)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({"service_name": "checkout", "window_secs": 300}))
    .execute(&pool)
    .await
    .expect("deadman rule inserted");

    eval_deadman_rules(&pool, &ch).await.unwrap();

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(active_count, 1);
}

#[tokio::test]
async fn deadman_rule_does_not_fire_when_span_is_recent() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let rule_id = Uuid::new_v4();
    let service_name = "checkout";
    let environment = "prod";

    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Checkout silent', 'deadman', 'critical', $3)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({"service_name": service_name, "window_secs": 300}))
    .execute(&pool)
    .await
    .expect("deadman rule inserted");

    // make_span's offset_minutes=0 -> a span "now", well inside the 300s window.
    insert_span(&ch, make_span(tenant_id, service_name, environment, 0, false)).await;

    eval_deadman_rules(&pool, &ch).await.unwrap();

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(active_count, 0);
}
```

Run: `cargo test -p alert-evaluator --test deadman_integration 2>&1 | tail -40`
Expected: compile error — `eval_deadman_rules` not found.

- [ ] **Step 2: Confirm the compile failure, then implement**

In `services/alert-evaluator/src/evaluator.rs`, add this function after `eval_composite_rules` (the function ends around the `Ok(())` that closes the composite-rules loop, just before `pub async fn eval_alert_rules`):

```rust
#[derive(clickhouse::Row, serde::Deserialize)]
pub struct LastSeenRow {
    pub last_seen_unix_nano: u64,
}

pub async fn eval_deadman_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()> {
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
         auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
         FROM alert_rules WHERE alert_type = 'deadman' AND silenced = false",
    )
    .fetch_all(db)
    .await?;

    tracing::debug!(count = rules.len(), "evaluating deadman rules");

    for rule in rules {
        let cond: DeadmanCondition = match serde_json::from_value(rule.condition.clone()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    error = %e,
                    "skipping deadman rule: condition parse failed"
                );
                continue;
            }
        };

        let mut cursor = ch
            .query(
                "SELECT max(start_time_unix_nano) AS last_seen_unix_nano \
                 FROM observable.spans \
                 WHERE tenant_id = ? AND service_name = ?",
            )
            .bind(rule.tenant_id)
            .bind(&cond.service_name)
            .fetch::<LastSeenRow>()
            .map_err(|e| anyhow::anyhow!("clickhouse query error: {e}"))?;

        let last_seen_unix_nano = match cursor.next().await {
            Ok(Some(row)) if row.last_seen_unix_nano > 0 => Some(row.last_seen_unix_nano),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(rule_id = %rule.rule_id, error = %e, "skipping deadman rule: span fetch failed");
                continue;
            }
        };

        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos() as u64;
        let elapsed_secs = last_seen_unix_nano
            .map(|seen| now_ns.saturating_sub(seen) / 1_000_000_000)
            .map(|secs| secs as i64);

        match evaluate_deadman(elapsed_secs, &cond) {
            EvalResult::Firing => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    tenant_id = %rule.tenant_id,
                    rule_name = %rule.name,
                    service_name = %cond.service_name,
                    elapsed_secs = ?elapsed_secs,
                    window_secs = cond.window_secs,
                    "alert firing: deadman (no telemetry)"
                );
                let value = elapsed_secs.unwrap_or(i64::MAX) as f64;
                if let Err(e) = record_firing(db, &rule, value).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to record deadman firing");
                }
            }
            EvalResult::Ok => {
                let value = elapsed_secs.unwrap_or(0) as f64;
                if let Err(e) = resolve_open_firing(db, &rule, value).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to resolve deadman firing");
                }
            }
        }
    }

    Ok(())
}
```

Then update `eval_alert_rules` to call it:

```rust
pub async fn eval_alert_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()> {
    eval_threshold_rules(db, ch).await?;
    eval_slo_burn_rate_rules(db, ch).await?;
    eval_composite_rules(db, ch).await?;
    eval_deadman_rules(db, ch).await?;
    Ok(())
}
```

- [ ] **Step 3: Run the integration tests to verify they pass**

Run: `cargo test -p alert-evaluator --test deadman_integration 2>&1 | tail -40`
Expected: `2 passed` (requires Docker for Testcontainers — same prerequisite as the existing `slo_burn_rate_integration.rs`).

- [ ] **Step 4: Run the full alert-evaluator suite**

Run: `cargo test -p alert-evaluator 2>&1 | tail -60`
Expected: all tests pass, including the existing lifecycle/incident/http_api/slo_burn_rate integration tests (unaffected by this change).

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add services/alert-evaluator/src/evaluator.rs services/alert-evaluator/tests/deadman_integration.rs
git commit -m "feat(alert-evaluator): evaluate deadman rules against ClickHouse span recency"
```

---

### Task 3: query-api support for creating and listing deadman rules

**Files:**
- Modify: `services/query-api/src/alerts.rs`
- Modify: `services/query-api/tests/postgres_alerts_integration.rs`

**Interfaces:**
- Consumes: nothing new from other tasks (this task is independent of the evaluator).
- Produces: `CreateRuleRequest` gains `alert_type: Option<String>`, `service_name: Option<String>`, `window_secs: Option<i64>`. `create_alert_rule` accepts `alert_type == "deadman"` and inserts `alert_type = 'deadman'` rows. `list_alert_rules` includes deadman rows, surfaced as `AlertRuleItem { metric_name: <service_name>, operator: "no_data", threshold: <window_secs as f64>, .. }`. These are what Task 6 (frontend) consumes via the existing `createAlertRule`/`listAlertRules` HTTP contract — unchanged response shape, just a new value space for `operator`.

- [ ] **Step 1: Write the failing unit tests**

Add to the `#[cfg(test)] mod tests` block in `services/query-api/src/alerts.rs` (after `condition_fields_returns_none_when_threshold_not_number`):

```rust
    #[test]
    fn condition_fields_extracts_deadman_shape_as_no_data_operator() {
        let cond = serde_json::json!({"service_name": "checkout", "window_secs": 300});
        let (metric_name, operator, threshold) = condition_fields(&cond).unwrap();
        assert_eq!(metric_name, "checkout");
        assert_eq!(operator, "no_data");
        assert!((threshold - 300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn condition_fields_returns_none_for_empty_object() {
        let cond = serde_json::json!({});
        assert!(condition_fields(&cond).is_none());
    }
```

Add a separate `#[tokio::test]`-free unit test for the request validation path (still in the same `mod tests` block):

```rust
    #[tokio::test]
    async fn create_alert_rule_rejects_deadman_without_service_name() {
        // No DB needed: validation happens before any query.
        let pool = sqlx::PgPool::connect_lazy("postgres://invalid/invalid").unwrap();
        let req = CreateRuleRequest {
            name: "Silent service".into(),
            metric_name: String::new(),
            operator: String::new(),
            threshold: 0.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: Some("deadman".into()),
            service_name: None,
            window_secs: Some(300),
        };
        let err = create_alert_rule(&pool, Uuid::nil(), &req).await.unwrap_err();
        assert!(matches!(err, CreateRuleError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn create_alert_rule_rejects_deadman_with_non_positive_window() {
        let pool = sqlx::PgPool::connect_lazy("postgres://invalid/invalid").unwrap();
        let req = CreateRuleRequest {
            name: "Silent service".into(),
            metric_name: String::new(),
            operator: String::new(),
            threshold: 0.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: Some("deadman".into()),
            service_name: Some("checkout".into()),
            window_secs: Some(0),
        };
        let err = create_alert_rule(&pool, Uuid::nil(), &req).await.unwrap_err();
        assert!(matches!(err, CreateRuleError::InvalidInput(_)));
    }
```

Run: `cargo test -p query-api alerts:: 2>&1 | tail -40`
Expected: compile error — `CreateRuleRequest` has no field `alert_type`/`service_name`/`window_secs`.

- [ ] **Step 2: Implement**

In `services/query-api/src/alerts.rs`, update `CreateRuleRequest`:

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
    pub alert_type: Option<String>,
    pub service_name: Option<String>,
    pub window_secs: Option<i64>,
}
```

Update `condition_fields` to fall back to the deadman shape:

```rust
fn condition_fields(condition: &serde_json::Value) -> Option<(String, String, f64)> {
    if let (Some(metric_name), Some(operator), Some(threshold)) = (
        condition.get("metric_name").and_then(|v| v.as_str()),
        condition.get("operator").and_then(|v| v.as_str()),
        condition.get("threshold").and_then(|v| v.as_f64()),
    ) {
        return Some((metric_name.to_string(), operator.to_string(), threshold));
    }
    if let (Some(service_name), Some(window_secs)) = (
        condition.get("service_name").and_then(|v| v.as_str()),
        condition.get("window_secs").and_then(|v| v.as_f64()),
    ) {
        return Some((service_name.to_string(), "no_data".to_string(), window_secs));
    }
    None
}
```

Update `list_alert_rules`'s query filter from `WHERE r.tenant_id = $1 AND r.alert_type = 'threshold'` to `WHERE r.tenant_id = $1 AND r.alert_type IN ('threshold', 'deadman')`.

Update `create_alert_rule` to branch on `alert_type`:

```rust
pub async fn create_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateRuleRequest,
) -> Result<AlertRuleItem, CreateRuleError> {
    if req.name.trim().is_empty() {
        return Err(CreateRuleError::InvalidInput("name is required".into()));
    }

    let alert_type = req.alert_type.as_deref().unwrap_or("threshold");

    match alert_type {
        "threshold" => {
            if req.metric_name.trim().is_empty() {
                return Err(CreateRuleError::InvalidInput(
                    "metric_name is required".into(),
                ));
            }
            if !VALID_OPERATORS.contains(&req.operator.as_str()) {
                return Err(CreateRuleError::InvalidInput(format!(
                    "operator must be one of: {}",
                    VALID_OPERATORS.join(", ")
                )));
            }
            if !req.threshold.is_finite() {
                return Err(CreateRuleError::InvalidInput(
                    "threshold must be finite".into(),
                ));
            }

            let condition = serde_json::json!({
                "metric_name": req.metric_name,
                "operator": req.operator,
                "threshold": req.threshold,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);

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

            Ok(AlertRuleItem {
                rule_id,
                name: req.name.clone(),
                metric_name: req.metric_name.clone(),
                operator: req.operator.clone(),
                threshold: req.threshold,
                severity: "warning".into(),
                silenced: false,
                state: "ok".into(),
                firing: false,
                last_fired_at: None,
                notification_channels: channels,
                auto_trigger_incident: auto_trigger,
            })
        }
        "deadman" => {
            let service_name = req
                .service_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    CreateRuleError::InvalidInput("service_name is required".into())
                })?;
            let window_secs = req.window_secs.ok_or_else(|| {
                CreateRuleError::InvalidInput("window_secs is required".into())
            })?;
            if window_secs <= 0 {
                return Err(CreateRuleError::InvalidInput(
                    "window_secs must be positive".into(),
                ));
            }

            let condition = serde_json::json!({
                "service_name": service_name,
                "window_secs": window_secs,
            });
            let channels = req.notification_channels.clone().unwrap_or_default();
            let auto_trigger = req.auto_trigger_incident.unwrap_or(true);

            let rule_id: Uuid = sqlx::query_scalar(
                "INSERT INTO alert_rules \
                 (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident, runbook_url) \
                 VALUES ($1, $2, 'deadman', 'warning', $3, $4, $5, $6) \
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

            Ok(AlertRuleItem {
                rule_id,
                name: req.name.clone(),
                metric_name: service_name.to_string(),
                operator: "no_data".into(),
                threshold: window_secs as f64,
                severity: "warning".into(),
                silenced: false,
                state: "ok".into(),
                firing: false,
                last_fired_at: None,
                notification_channels: channels,
                auto_trigger_incident: auto_trigger,
            })
        }
        other => Err(CreateRuleError::InvalidInput(format!(
            "unknown alert_type: {other}"
        ))),
    }
}
```

- [ ] **Step 3: Update the existing integration test file's struct literals**

`services/query-api/tests/postgres_alerts_integration.rs` constructs `CreateRuleRequest` by struct literal in 8 places, all ending with `runbook_url: None,`. Add the three new fields after that line in every occurrence — open the file and, for each of the 8 literals, change:

```rust
        runbook_url: None,
    };
```

to:

```rust
        runbook_url: None,
        alert_type: None,
        service_name: None,
        window_secs: None,
    };
```

(Match the existing indentation at each call site — 8 spaces for the four top-level `let req = CreateRuleRequest { ... };` literals, 12 spaces for the four passed directly as `&CreateRuleRequest { ... }` arguments. `cargo fmt --all` in Step 5 will normalize anything off.)

- [ ] **Step 4: Run the unit and integration tests**

Run: `cargo test -p query-api alerts:: 2>&1 | tail -40`
Expected: all `alerts.rs` unit tests pass, including the 4 new ones.

Run: `cargo test -p query-api --test postgres_alerts_integration 2>&1 | tail -60`
Expected: all existing tests still pass (requires Docker for Testcontainers).

- [ ] **Step 5: Add a deadman-specific integration test**

Append to `services/query-api/tests/postgres_alerts_integration.rs`:

```rust
#[tokio::test]
async fn create_deadman_rule_appears_in_list_with_no_data_operator() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Checkout silent".into(),
        metric_name: String::new(),
        operator: String::new(),
        threshold: 0.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: Some("deadman".into()),
        service_name: Some("checkout".into()),
        window_secs: Some(300),
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();

    assert_eq!(created.metric_name, "checkout");
    assert_eq!(created.operator, "no_data");
    assert!((created.threshold - 300.0).abs() < f64::EPSILON);

    let rules = list_alert_rules(&pool, tenant).await.unwrap();
    assert!(
        rules.iter().any(|r| r.rule_id == created.rule_id && r.operator == "no_data"),
        "created deadman rule must appear in list with no_data operator"
    );
}

#[tokio::test]
async fn create_deadman_rule_rejects_blank_service_name() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Checkout silent".into(),
        metric_name: String::new(),
        operator: String::new(),
        threshold: 0.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: Some("deadman".into()),
        service_name: Some("   ".into()),
        window_secs: Some(300),
    };
    let err = create_alert_rule(&pool, tenant, &req).await.unwrap_err();
    assert!(matches!(err, CreateRuleError::InvalidInput(_)));
}
```

Run: `cargo test -p query-api --test postgres_alerts_integration 2>&1 | tail -60`
Expected: all tests pass, including the 2 new ones.

- [ ] **Step 6: Format and commit**

```bash
cargo fmt --all
git add services/query-api/src/alerts.rs services/query-api/tests/postgres_alerts_integration.rs
git commit -m "feat(query-api): support creating and listing deadman alert rules"
```

---

### Task 4: Frontend API client support for deadman rules

**Files:**
- Modify: `apps/frontend/src/api/alerts.ts`

**Interfaces:**
- Produces: `CreateRuleRequest` (TS) gains `alert_type?: string`, `service_name?: string`, `window_secs?: number`. Consumed by Task 5.

- [ ] **Step 1: Update the interface**

In `apps/frontend/src/api/alerts.ts`, change:

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

to:

```typescript
export interface CreateRuleRequest {
  name: string;
  metric_name: string;
  operator: string;
  threshold: number;
  notification_channels?: string[];
  auto_trigger_incident?: boolean;
  runbook_url?: string;
  alert_type?: string;
  service_name?: string;
  window_secs?: number;
}
```

No other changes needed in this file — `createAlertRule` already forwards the whole request body as JSON.

- [ ] **Step 2: Verify typecheck passes**

Run: `cd apps/frontend && npm run typecheck 2>&1 | tail -30`
Expected: no errors (this is an additive, all-optional change).

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/api/alerts.ts
git commit -m "feat(frontend): add deadman fields to CreateRuleRequest"
```

---

### Task 5: AlertsPage UI — alert type selector and deadman fields

**Files:**
- Modify: `apps/frontend/src/features/alerts/AlertsPage.tsx`

**Interfaces:**
- Consumes: `CreateRuleRequest` (Task 4).
- Produces: a working "No data" option in the create-rule form; `AlertRuleRow`'s condition label renders `no_data` rows distinctly. Task 6 tests this behavior.

- [ ] **Step 1: Add alert-type state and form branching**

In `AlertsPage.tsx`, add new state alongside the existing form state (after `const [formThreshold, setFormThreshold] = useState("");`):

```typescript
  const [formAlertType, setFormAlertType] = useState<"threshold" | "deadman">("threshold");
  const [formServiceName, setFormServiceName] = useState("");
  const [formWindowSecs, setFormWindowSecs] = useState("300");
```

Reset these in `createMutation`'s `onSuccess` alongside the other form-reset calls:

```typescript
      setFormAlertType("threshold");
      setFormServiceName("");
      setFormWindowSecs("300");
```

Replace `handleCreateSubmit` with a version that branches on `formAlertType`:

```typescript
  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (formAlertType === "deadman") {
      const windowSecs = parseInt(formWindowSecs, 10);
      if (!formServiceName.trim()) {
        setFormError("Service name is required");
        return;
      }
      if (isNaN(windowSecs) || windowSecs <= 0) {
        setFormError("Window must be a positive number of seconds");
        return;
      }
      setFormError(null);
      createMutation.mutate({
        name: formName,
        metric_name: "",
        operator: "",
        threshold: 0,
        notification_channels: selectedChannels,
        auto_trigger_incident: autoTriggerIncident,
        runbook_url: formRunbookUrl || undefined,
        alert_type: "deadman",
        service_name: formServiceName.trim(),
        window_secs: windowSecs,
      });
      return;
    }

    const threshold = parseFloat(formThreshold);
    if (isNaN(threshold)) {
      setFormError("Threshold must be a number");
      return;
    }
    setFormError(null);
    createMutation.mutate({
      name: formName,
      metric_name: formMetric,
      operator: formOperator,
      threshold,
      notification_channels: selectedChannels,
      auto_trigger_incident: autoTriggerIncident,
      runbook_url: formRunbookUrl || undefined,
      alert_type: "threshold",
    });
  };
```

- [ ] **Step 2: Add the alert-type selector and swap the form fields**

In the JSX, just before the `<div className="grid gap-3 sm:grid-cols-2">` block that contains "Rule name" and "Metric name" (inside `<Panel title="Create Threshold Rule" ...>`), add:

```tsx
                <div className="space-y-1">
                  <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="alert-type">Alert type</label>
                  <Select
                    id="alert-type"
                    value={formAlertType}
                    onChange={(e) => setFormAlertType(e.target.value as "threshold" | "deadman")}
                  >
                    <SelectOption value="threshold">Threshold metric</SelectOption>
                    <SelectOption value="deadman">No data</SelectOption>
                  </Select>
                </div>
```

Then change the existing "Rule name" / "Metric name" / "Operator" / "Threshold value" block so the metric-specific half is conditional. Replace:

```tsx
                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="rule-name">Rule name</label>
                    <Input
                      id="rule-name"
                      placeholder="e.g. High Error Rate"
                      value={formName}
                      onChange={(e) => setFormName(e.target.value)}
                      required
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="metric-name">Metric name</label>
                    <Input
                      id="metric-name"
                      placeholder="e.g. error_rate"
                      value={formMetric}
                      onChange={(e) => setFormMetric(e.target.value)}
                      required
                    />
                  </div>
                </div>

                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="operator">Operator</label>
                    <Select
                      id="operator"
                      value={formOperator}
                      onChange={(e) => setFormOperator(e.target.value)}
                    >
                      <SelectOption value="gt">&gt; (greater than)</SelectOption>
                      <SelectOption value="gte">&ge; (greater than or equal)</SelectOption>
                      <SelectOption value="lt">&lt; (less than)</SelectOption>
                      <SelectOption value="lte">&le; (less than or equal)</SelectOption>
                      <SelectOption value="eq">= (equal)</SelectOption>
                    </Select>
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="threshold">Threshold value</label>
                    <Input
                      id="threshold"
                      type="number"
                      step="any"
                      value={formThreshold}
                      onChange={(e) => setFormThreshold(e.target.value)}
                      required
                    />
                  </div>
                </div>
```

with:

```tsx
                <div className="space-y-1">
                  <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="rule-name">Rule name</label>
                  <Input
                    id="rule-name"
                    placeholder="e.g. High Error Rate"
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                    required
                  />
                </div>

                {formAlertType === "threshold" ? (
                  <>
                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="metric-name">Metric name</label>
                      <Input
                        id="metric-name"
                        placeholder="e.g. error_rate"
                        value={formMetric}
                        onChange={(e) => setFormMetric(e.target.value)}
                        required
                      />
                    </div>

                    <div className="grid gap-3 sm:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="operator">Operator</label>
                        <Select
                          id="operator"
                          value={formOperator}
                          onChange={(e) => setFormOperator(e.target.value)}
                        >
                          <SelectOption value="gt">&gt; (greater than)</SelectOption>
                          <SelectOption value="gte">&ge; (greater than or equal)</SelectOption>
                          <SelectOption value="lt">&lt; (less than)</SelectOption>
                          <SelectOption value="lte">&le; (less than or equal)</SelectOption>
                          <SelectOption value="eq">= (equal)</SelectOption>
                        </Select>
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="threshold">Threshold value</label>
                        <Input
                          id="threshold"
                          type="number"
                          step="any"
                          value={formThreshold}
                          onChange={(e) => setFormThreshold(e.target.value)}
                          required
                        />
                      </div>
                    </div>
                  </>
                ) : (
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="deadman-service">Service name</label>
                      <Input
                        id="deadman-service"
                        placeholder="e.g. checkout"
                        value={formServiceName}
                        onChange={(e) => setFormServiceName(e.target.value)}
                        required
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="deadman-window">Window (seconds)</label>
                      <Input
                        id="deadman-window"
                        type="number"
                        step="1"
                        min="1"
                        value={formWindowSecs}
                        onChange={(e) => setFormWindowSecs(e.target.value)}
                        required
                      />
                    </div>
                  </div>
                )}
```

- [ ] **Step 2: Update the table's condition label for no_data rows**

In `AlertRuleRow`, replace:

```typescript
  const conditionLabel = `${rule.operator} ${rule.threshold}`;
```

with:

```typescript
  const conditionLabel =
    rule.operator === "no_data"
      ? `No data for ${rule.threshold}s from ${rule.metric_name}`
      : `${rule.operator} ${rule.threshold}`;
```

- [ ] **Step 3: Verify typecheck passes**

Run: `cd apps/frontend && npm run typecheck 2>&1 | tail -30`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertsPage.tsx
git commit -m "feat(frontend): add No data alert type to the create-rule form"
```

---

### Task 6: AlertsPage tests for the deadman flow

**Files:**
- Create: `apps/frontend/src/features/alerts/AlertsPage.test.tsx`

**Interfaces:**
- Consumes: `AlertsPage` (Task 5), `api/alerts.ts` (`listAlertRules`, `createAlertRule`), `api/slos.ts` (`listSlos`), `api/notifications.ts` (`listNotificationChannels`) — all mocked via `vi.spyOn`.

- [ ] **Step 1: Write the test file**

Create `apps/frontend/src/features/alerts/AlertsPage.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as alertsApi from "../../api/alerts";
import * as slosApi from "../../api/slos";
import * as notificationsApi from "../../api/notifications";
import { AlertsPage } from "./AlertsPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AlertsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(slosApi, "listSlos").mockResolvedValue({ items: [] });
  vi.spyOn(notificationsApi, "listNotificationChannels").mockResolvedValue([]);
});

test("renders a no_data rule with a deadman condition label", async () => {
  vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({
    items: [
      {
        rule_id: "rule-1",
        name: "Checkout silent",
        metric_name: "checkout",
        operator: "no_data",
        threshold: 300,
        severity: "warning",
        silenced: false,
        state: "ok",
        firing: false,
        last_fired_at: null,
        notification_channels: [],
        auto_trigger_incident: true,
      },
    ],
  });

  renderPage();

  await waitFor(() =>
    expect(screen.getByText("No data for 300s from checkout")).toBeInTheDocument(),
  );
});

test("submitting the No data form sends a deadman create request", async () => {
  vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({ items: [] });
  const createSpy = vi
    .spyOn(alertsApi, "createAlertRule")
    .mockResolvedValue({
      rule_id: "rule-2",
      name: "Checkout silent",
      metric_name: "checkout",
      operator: "no_data",
      threshold: 300,
      severity: "warning",
      silenced: false,
      state: "ok",
      firing: false,
      last_fired_at: null,
      notification_channels: [],
      auto_trigger_incident: true,
    });

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));

  fireEvent.change(screen.getByLabelText("Alert type"), { target: { value: "deadman" } });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Checkout silent" },
  });
  fireEvent.change(screen.getByLabelText("Service name"), {
    target: { value: "checkout" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "300" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => expect(createSpy).toHaveBeenCalledTimes(1));
  expect(createSpy).toHaveBeenCalledWith(
    "test-tenant",
    expect.objectContaining({
      name: "Checkout silent",
      alert_type: "deadman",
      service_name: "checkout",
      window_secs: 300,
    }),
  );
});

test("No data form rejects a blank service name", async () => {
  vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({ items: [] });
  const createSpy = vi.spyOn(alertsApi, "createAlertRule");

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));
  fireEvent.change(screen.getByLabelText("Alert type"), { target: { value: "deadman" } });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Checkout silent" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "300" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() =>
    expect(screen.getByText("Service name is required")).toBeInTheDocument(),
  );
  expect(createSpy).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run the test and verify it fails first, then passes**

Run: `cd apps/frontend && npx vitest run src/features/alerts/AlertsPage.test.tsx 2>&1 | tail -60`
Expected (before Task 5's UI changes are present, if run out of order): failures locating "Alert type"/"Service name"/"Window (seconds)" labels. Since Task 5 is already implemented at this point in the plan, expected result now is: `3 passed`.

If any label text doesn't match (e.g. `getByLabelText` not finding the field), check the `htmlFor`/`id` pairing added in Task 5 Step 2 — `Input`/`Select` components associate via the `id` prop and the `<label htmlFor="...">` text content.

- [ ] **Step 3: Run the full frontend test suite**

Run: `cd apps/frontend && npm test 2>&1 | tail -60`
Expected: all tests pass, no regressions in other features.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertsPage.test.tsx
git commit -m "test(alerts-page): cover deadman alert type creation and rendering"
```

---

### Task 7: Documentation and roadmap updates

**Files:**
- Modify: `docs/agent-context.md`
- Modify: `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md`
- Move: `docs/superpowers/plans/2026-06-18-p12-s3-deadman-alert.md` → `archived/plans/2026-06-18-p12-s3-deadman-alert.md`
- Move: `docs/superpowers/specs/2026-06-18-p12-s3-deadman-alert-design.md` stays in place (specs are not archived in this repo's convention — only plans are; verify by checking whether other entries under "Completed / archived detailed plans" in `agent-context.md` reference a spec move. They don't — leave the spec where it is.)

**Interfaces:** none (docs-only task).

- [ ] **Step 1: Add a completion note to `docs/agent-context.md`**

Add a new subsection after the "Modelable Type-Mapping Migration" section (before "## Dev Environment Gotchas"):

```markdown
## Deadman Alert Type (P12-S3, completed 2026-06-18)

- `services/alert-evaluator/src/evaluator.rs` adds `eval_deadman_rules`: fires when no span has
  been received for a service within `window_secs` (including services never seen at all).
  Wired into `eval_alert_rules` alongside the threshold/SLO/composite evaluators.
- `services/query-api/src/alerts.rs` `create_alert_rule`/`list_alert_rules` support
  `alert_type = 'deadman'` by reusing the existing `AlertRuleItem` shape: deadman conditions are
  surfaced as `metric_name = service_name`, `operator = "no_data"`, `threshold = window_secs`.
  This was a deliberate choice to avoid extending the modelable-generated `AlertRuleItem` schema
  for this slice — see `docs/superpowers/specs/2026-06-18-p12-s3-deadman-alert-design.md`.
- Frontend: `AlertsPage.tsx`'s create-rule form has an "Alert type" selector ("Threshold metric"
  / "No data") that swaps in service-name/window fields; the rules table renders `no_data` rows
  as `"No data for {window}s from {service}"`.
```

Update the line in "Current Source Of Truth" that lists promotion candidates:

Find:
```
- Next promotion candidates (per that plan's §7, not yet promoted): P12-S3 Deadman alert type (quick win, small self-contained `alert-evaluator` addition) and P14-S4 Change Event API (quick win, extends the deployment-marker model). P9-S2 Error Tracking Ingestion is the largest remaining workflow gap but needs its own multi-task plan once promoted.
```

Replace with:
```
- P12-S3 Deadman alert type is complete (see "Deadman Alert Type" section below). Remaining promotion candidate (per the feature-parity plan's §7, not yet promoted): P14-S4 Change Event API (quick win, extends the deployment-marker model). P9-S2 Error Tracking Ingestion is the largest remaining workflow gap but needs its own multi-task plan once promoted.
```

Add to the "Completed / archived detailed plans" bullet list (alphabetically by date, after the `2026-06-18-frontend-design-system-modernization.md` entry):

```
  - `archived/plans/2026-06-18-p12-s3-deadman-alert.md` — P12-S3 deadman alert type: alert-evaluator span-recency check, query-api create/list support reusing the existing AlertRuleItem shape, AlertsPage "No data" rule type (COMPLETED 2026-06-18)
```

- [ ] **Step 2: Update the feature-parity plan**

In `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md`, find the `#### P12-S3: Deadman Alert Type` heading and change it to:

```markdown
#### P12-S3: Deadman Alert Type — COMPLETED 2026-06-18
```

Find the summary table row:
```
| Deadman alert type | "Is my service silent?" is asked daily | P12-S3 (promote now) |
```

Replace with:
```
| Deadman alert type | "Is my service silent?" is asked daily | P12-S3 (complete) |
```

- [ ] **Step 3: Move the plan file to archived**

```bash
git mv docs/superpowers/plans/2026-06-18-p12-s3-deadman-alert.md archived/plans/2026-06-18-p12-s3-deadman-alert.md
```

- [ ] **Step 4: Commit**

```bash
git add docs/agent-context.md docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md
git commit -m "docs: archive P12-S3 plan, update agent-context and feature-parity plan status"
```

---

### Task 8: Full verification and PR

**Files:** none (verification only).

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test --workspace 2>&1 | tail -80`
Expected: all tests pass (requires Docker running for Testcontainers tests).

- [ ] **Step 2: Run the full frontend suite**

Run: `cd apps/frontend && npm run typecheck && npm test && npm run build 2>&1 | tail -80`
Expected: all pass, build succeeds.

- [ ] **Step 3: Run local CI**

Run: `bash scripts/local-ci.sh 2>&1 | tail -100`
Expected: passes.

- [ ] **Step 4: Push and open a PR**

```bash
git push -u origin feat/p12-s3-deadman-alert
gh pr create --title "feat: P12-S3 deadman alert type" --body "$(cat <<'EOF'
## Summary
- Adds a `deadman` alert rule type: fires when a service has emitted no spans for a configurable window (including services never seen at all).
- query-api's create/list endpoints support the new type by reusing the existing AlertRuleItem shape (operator="no_data") rather than extending the modelable-generated schema — see docs/superpowers/specs/2026-06-18-p12-s3-deadman-alert-design.md.
- AlertsPage gets a "No data" option in the create-rule form.

## Test plan
- [x] cargo test --workspace
- [x] npm run typecheck && npm test && npm run build
- [x] bash scripts/local-ci.sh
EOF
)"
```
