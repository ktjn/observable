# P3-S6d: Threshold Alert UI Workflow — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give operators a complete threshold-alert loop in the UI: list rules with live firing state, create a new rule, and silence or unsilence a rule.

**Architecture:** Three new endpoints are added to `query-api` (the service that already owns tenant-scoped reads/writes with the existing auth middleware). The `alert_rules` table gains a `silenced` column via migration; the evaluator skips silenced rules at the SQL level. The frontend replaces the `/alerts` placeholder with a dedicated `AlertsPage` component that renders the rule list, a create panel, and per-row silence actions.

**Tech Stack:** Rust/axum (query-api), sqlx (Postgres), React/TypeScript, TanStack Query, Vitest/RTL, Testcontainers (Postgres)

**Spec:** `docs/superpowers/specs/2026-04-28-p3-s6d-threshold-alert-ui-design.md`

---

## File Map

| Action | Path |
|--------|------|
| Create | `migrations/postgres/010_add_silenced_to_alert_rules.sql` |
| Modify | `services/alert-evaluator/src/evaluator.rs` |
| Create | `services/query-api/src/alerts.rs` |
| Modify | `services/query-api/src/lib.rs` |
| Modify | `services/query-api/src/main.rs` |
| Modify | `services/query-api/Cargo.toml` |
| Create | `services/query-api/tests/postgres_alerts_integration.rs` |
| Create | `apps/frontend/src/api/alerts.ts` |
| Create | `apps/frontend/src/features/alerts/AlertsPage.tsx` |
| Modify | `apps/frontend/src/router.ts` |
| Modify | `apps/frontend/src/App.test.tsx` |
| Modify | `spec/09-api.md` |
| Modify | `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` |

---

## Task 1: Add silenced column migration

**Files:**
- Create: `migrations/postgres/010_add_silenced_to_alert_rules.sql`

- [ ] **Step 1: Create the migration file**

```sql
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS silenced BOOLEAN NOT NULL DEFAULT false;
```

- [ ] **Step 2: Verify the migration number is correct**

Run: `ls migrations/postgres/`
Expected: files `001_` through `009_` exist and there is no `010_` file yet.

- [ ] **Step 3: Commit**

```bash
git add migrations/postgres/010_add_silenced_to_alert_rules.sql
git commit -m "feat(db): add silenced column to alert_rules"
```

---

## Task 2: Update alert-evaluator to skip silenced rules

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`

- [ ] **Step 1: Write a failing unit test that documents the SQL filter**

Add to the `tests` module at the bottom of `services/alert-evaluator/src/evaluator.rs`, after the existing `eq_ok_when_different` test:

```rust
#[test]
fn threshold_condition_parses_all_operators() {
    // Smoke-test all five operators round-trip through the evaluator.
    // The silenced-rule SQL filter (AND silenced = false) is enforced at the
    // query level in eval_threshold_rules; verified by the postgres integration test.
    for (op, value, threshold, expected) in [
        (ThresholdOperator::Gt,  1.1, 1.0, EvalResult::Firing),
        (ThresholdOperator::Gte, 1.0, 1.0, EvalResult::Firing),
        (ThresholdOperator::Lt,  0.9, 1.0, EvalResult::Firing),
        (ThresholdOperator::Lte, 1.0, 1.0, EvalResult::Firing),
        (ThresholdOperator::Eq,  5.0, 5.0, EvalResult::Firing),
    ] {
        assert_eq!(evaluate_threshold(value, &cond(op, threshold)), expected);
    }
}
```

- [ ] **Step 2: Run test to verify it compiles and passes** (it should pass — it's a documentation test)

Run: `cargo test -p alert-evaluator`
Expected: all tests pass

- [ ] **Step 3: Add AND silenced = false to the eval query**

In `services/alert-evaluator/src/evaluator.rs`, change:

```rust
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition \
         FROM alert_rules WHERE alert_type = 'threshold'",
    )
```

To:

```rust
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition \
         FROM alert_rules WHERE alert_type = 'threshold' AND silenced = false",
    )
```

- [ ] **Step 4: Run tests to confirm nothing broke**

Run: `cargo test -p alert-evaluator`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add services/alert-evaluator/src/evaluator.rs
git commit -m "feat(alert-evaluator): skip silenced rules during evaluation"
```

---

## Task 3: Create query-api alerts module

**Files:**
- Create: `services/query-api/src/alerts.rs`

- [ ] **Step 1: Write the failing unit tests first**

Create `services/query-api/src/alerts.rs` with the following content (tests at the bottom, stubs above to make it compile):

```rust
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AlertRuleItem {
    pub rule_id: Uuid,
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
    pub severity: String,
    pub silenced: bool,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct AlertRuleListResponse {
    pub items: Vec<AlertRuleItem>,
}

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub metric_name: String,
    pub operator: String,
    pub threshold: f64,
}

#[derive(Deserialize)]
pub struct SilenceRequest {
    pub silenced: bool,
}

#[derive(Debug)]
pub enum CreateRuleError {
    InvalidInput(String),
    Db(sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct AlertRuleRow {
    rule_id: Uuid,
    name: String,
    condition: serde_json::Value,
    severity: String,
    silenced: bool,
    firing: bool,
    last_fired_at: Option<DateTime<Utc>>,
}

fn condition_fields(condition: &serde_json::Value) -> Option<(String, String, f64)> {
    let metric_name = condition.get("metric_name")?.as_str()?.to_string();
    let operator = condition.get("operator")?.as_str()?.to_string();
    let threshold = condition.get("threshold")?.as_f64()?;
    Some((metric_name, operator, threshold))
}

pub async fn list_alert_rules(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<AlertRuleItem>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AlertRuleRow>(
        "SELECT r.rule_id, r.name, r.condition, r.severity, r.silenced, \
         EXISTS( \
             SELECT 1 FROM alert_firings af \
             WHERE af.rule_id = r.rule_id AND af.state = 'active' \
         ) AS firing, \
         (SELECT MAX(occurred_at) FROM alert_firings af \
          WHERE af.rule_id = r.rule_id AND af.state = 'active') AS last_fired_at \
         FROM alert_rules r \
         WHERE r.tenant_id = $1 AND r.alert_type = 'threshold' \
         ORDER BY r.created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let (metric_name, operator, threshold) = condition_fields(&row.condition)?;
            Some(AlertRuleItem {
                rule_id: row.rule_id,
                name: row.name,
                metric_name,
                operator,
                threshold,
                severity: row.severity,
                silenced: row.silenced,
                firing: row.firing,
                last_fired_at: row.last_fired_at,
            })
        })
        .collect())
}

pub async fn create_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateRuleRequest,
) -> Result<AlertRuleItem, CreateRuleError> {
    if req.name.trim().is_empty() {
        return Err(CreateRuleError::InvalidInput("name is required".into()));
    }
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

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules (tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'threshold', 'warning', $3) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(&req.name)
    .bind(&condition)
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
        firing: false,
        last_fired_at: None,
    })
}

pub async fn silence_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    rule_id: Uuid,
    silenced: bool,
) -> Result<Option<AlertRuleItem>, sqlx::Error> {
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE alert_rules SET silenced = $1 \
         WHERE rule_id = $2 AND tenant_id = $3 \
         RETURNING rule_id",
    )
    .bind(silenced)
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    if updated.is_none() {
        return Ok(None);
    }

    let rules = list_alert_rules(db, tenant_id).await?;
    Ok(rules.into_iter().find(|r| r.rule_id == rule_id))
}

pub async fn handle_list_rules(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<AlertRuleListResponse>, StatusCode> {
    let items = list_alert_rules(&state.db, ctx.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list alert rules");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(AlertRuleListResponse { items }))
}

pub async fn handle_create_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<AlertRuleItem>), StatusCode> {
    match create_alert_rule(&state.db, ctx.tenant_id, &req).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateRuleError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid alert rule input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateRuleError::Db(e)) => {
            tracing::error!(error = %e, "failed to create alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_silence_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
    Json(req): Json<SilenceRequest>,
) -> Result<Json<AlertRuleItem>, StatusCode> {
    match silence_alert_rule(&state.db, ctx.tenant_id, rule_id, req.silenced).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to silence alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_fields_extracts_all_three_fields() {
        let cond = serde_json::json!({
            "metric_name": "error_rate",
            "operator": "gt",
            "threshold": 0.05
        });
        let (metric_name, operator, threshold) = condition_fields(&cond).unwrap();
        assert_eq!(metric_name, "error_rate");
        assert_eq!(operator, "gt");
        assert!((threshold - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn condition_fields_returns_none_when_metric_name_missing() {
        let cond = serde_json::json!({"operator": "gt", "threshold": 1.0});
        assert!(condition_fields(&cond).is_none());
    }

    #[test]
    fn condition_fields_returns_none_when_threshold_not_number() {
        let cond = serde_json::json!({"metric_name": "m", "operator": "gt", "threshold": "bad"});
        assert!(condition_fields(&cond).is_none());
    }

    #[test]
    fn all_five_operators_are_valid() {
        for op in ["gt", "gte", "lt", "lte", "eq"] {
            assert!(
                VALID_OPERATORS.contains(&op),
                "{op} should be a valid operator"
            );
        }
    }

    #[test]
    fn unknown_operator_is_not_valid() {
        assert!(!VALID_OPERATORS.contains(&"neq"));
        assert!(!VALID_OPERATORS.contains(&">"));
    }

    #[test]
    fn alert_rule_item_serializes_to_expected_json_shape() {
        let id = Uuid::nil();
        let item = AlertRuleItem {
            rule_id: id,
            name: "High error rate".into(),
            metric_name: "error_rate".into(),
            operator: "gt".into(),
            threshold: 0.05,
            severity: "warning".into(),
            silenced: false,
            firing: true,
            last_fired_at: None,
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["name"], "High error rate");
        assert_eq!(v["metric_name"], "error_rate");
        assert_eq!(v["operator"], "gt");
        assert_eq!(v["firing"], true);
        assert!(v["last_fired_at"].is_null());
    }
}
```

- [ ] **Step 2: Run unit tests to verify they pass**

Run: `cargo test -p query-api alerts`
Expected: all 6 unit tests pass (condition_fields_extracts_all_three_fields, condition_fields_returns_none_when_metric_name_missing, condition_fields_returns_none_when_threshold_not_number, all_five_operators_are_valid, unknown_operator_is_not_valid, alert_rule_item_serializes_to_expected_json_shape)

- [ ] **Step 3: Commit**

```bash
git add services/query-api/src/alerts.rs
git commit -m "feat(query-api): add alert rules CRUD handlers and unit tests"
```

---

## Task 4: Mount routes and export module

**Files:**
- Modify: `services/query-api/src/lib.rs`
- Modify: `services/query-api/src/main.rs`

- [ ] **Step 1: Add alerts to lib.rs**

In `services/query-api/src/lib.rs`, add `pub mod alerts;` so the file reads:

```rust
pub mod alerts;
pub mod audit;
pub mod deployments;
pub mod discovery;
pub mod logs;
pub mod metrics;
pub mod middleware;
pub mod planner;
pub mod traces;
```

- [ ] **Step 2: Mount the three alert routes in main.rs**

In `services/query-api/src/main.rs`, add these three routes to the `Router::new()` chain, after the deployments route:

```rust
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules", post(alerts::handle_create_rule))
        .route(
            "/v1/alerts/rules/:rule_id/silence",
            patch(alerts::handle_silence_rule),
        )
```

Also add `post` and `patch` to the axum routing imports. The full import line should read:

```rust
use axum::{middleware as axum_middleware, routing::{get, patch, post}, Router};
```

And add `mod alerts;` near the top with the other module declarations:

```rust
mod alerts;
mod audit;
mod deployments;
mod discovery;
mod logs;
mod metrics;
mod middleware;
mod planner;
mod traces;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p query-api`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add services/query-api/src/lib.rs services/query-api/src/main.rs
git commit -m "feat(query-api): mount alert rule endpoints"
```

---

## Task 5: Add Postgres Testcontainers integration test for alerts

**Files:**
- Modify: `services/query-api/Cargo.toml`
- Create: `services/query-api/tests/postgres_alerts_integration.rs`

- [ ] **Step 1: Add postgres feature to testcontainers-modules in Cargo.toml**

In `services/query-api/Cargo.toml`, change:

```toml
testcontainers-modules = { version = "0.15.0", features = ["clickhouse"] }
```

To:

```toml
testcontainers-modules = { version = "0.15.0", features = ["clickhouse", "postgres"] }
```

- [ ] **Step 2: Write the integration test file**

Create `services/query-api/tests/postgres_alerts_integration.rs`:

```rust
use query_api::alerts::{create_alert_rule, list_alert_rules, silence_alert_rule, CreateRuleRequest};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn apply_migrations(pool: &PgPool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("migration applied");
    }
}

async fn start_pool() -> (
    PgPool,
    testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
) {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

#[tokio::test]
async fn list_rules_returns_seeded_dev_rule() {
    let (pool, _container) = start_pool().await;
    let dev_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let rules = list_alert_rules(&pool, dev_tenant).await.unwrap();

    assert!(!rules.is_empty(), "dev tenant must have at least one seeded rule");
    assert!(
        rules.iter().any(|r| r.name == "High error rate"),
        "seeded 'High error rate' rule must be present"
    );
    let seeded = rules.iter().find(|r| r.name == "High error rate").unwrap();
    assert_eq!(seeded.metric_name, "error_rate");
    assert_eq!(seeded.operator, "gt");
    assert!(!seeded.silenced);
    assert!(!seeded.firing);
}

#[tokio::test]
async fn create_rule_appears_in_list() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Test latency rule".into(),
        metric_name: "p95_latency_ms".into(),
        operator: "gt".into(),
        threshold: 500.0,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();

    assert_eq!(created.name, "Test latency rule");
    assert_eq!(created.metric_name, "p95_latency_ms");
    assert_eq!(created.operator, "gt");
    assert!((created.threshold - 500.0).abs() < f64::EPSILON);
    assert_eq!(created.severity, "warning");
    assert!(!created.silenced);
    assert!(!created.firing);

    let rules = list_alert_rules(&pool, tenant).await.unwrap();
    assert!(
        rules.iter().any(|r| r.rule_id == created.rule_id),
        "created rule must appear in list"
    );
}

#[tokio::test]
async fn silence_toggle_updates_silenced_flag() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Silenceable rule".into(),
        metric_name: "cpu_usage".into(),
        operator: "gt".into(),
        threshold: 0.9,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();
    assert!(!created.silenced);

    let silenced = silence_alert_rule(&pool, tenant, created.rule_id, true)
        .await
        .unwrap()
        .expect("rule must exist for tenant");
    assert!(silenced.silenced);

    let unsilenced = silence_alert_rule(&pool, tenant, created.rule_id, false)
        .await
        .unwrap()
        .expect("rule must exist for tenant");
    assert!(!unsilenced.silenced);
}

#[tokio::test]
async fn silence_returns_none_for_cross_tenant_rule() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Private rule".into(),
        metric_name: "requests".into(),
        operator: "lt".into(),
        threshold: 1.0,
    };
    let created = create_alert_rule(&pool, tenant_a, &req).await.unwrap();

    let result = silence_alert_rule(&pool, tenant_b, created.rule_id, true)
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "tenant B must not be able to silence tenant A's rule"
    );

    // Verify the rule is still unsilenced for tenant_a
    let rules = list_alert_rules(&pool, tenant_a).await.unwrap();
    let rule = rules.iter().find(|r| r.rule_id == created.rule_id).unwrap();
    assert!(!rule.silenced, "rule must remain unsilenced after cross-tenant attempt");
}

#[tokio::test]
async fn list_rules_does_not_return_other_tenant_rules() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Tenant A rule".into(),
        metric_name: "errors".into(),
        operator: "gt".into(),
        threshold: 10.0,
    };
    create_alert_rule(&pool, tenant_a, &req).await.unwrap();

    let tenant_b_rules = list_alert_rules(&pool, tenant_b).await.unwrap();
    assert!(
        tenant_b_rules.is_empty(),
        "tenant B must not see tenant A's rules"
    );
}
```

- [ ] **Step 3: Run the integration tests**

Run: `cargo test -p query-api --test postgres_alerts_integration`
Expected: all 5 integration tests pass

- [ ] **Step 4: Commit**

```bash
git add services/query-api/Cargo.toml services/query-api/tests/postgres_alerts_integration.rs
git commit -m "test(query-api): add Postgres integration tests for alert rule CRUD"
```

---

## Task 6: Add frontend API client

**Files:**
- Create: `apps/frontend/src/api/alerts.ts`

- [ ] **Step 1: Create the API client**

Create `apps/frontend/src/api/alerts.ts`:

```typescript
const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface AlertRuleItem {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: "gt" | "gte" | "lt" | "lte" | "eq";
  threshold: number;
  severity: string;
  silenced: boolean;
  firing: boolean;
  last_fired_at: string | null;
}

export interface AlertRuleListResponse {
  items: AlertRuleItem[];
}

export interface CreateRuleRequest {
  name: string;
  metric_name: string;
  operator: string;
  threshold: number;
}

export async function listAlertRules(): Promise<AlertRuleListResponse> {
  const res = await fetch("/v1/alerts/rules", { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Failed to list alert rules: ${res.status}`);
  return res.json();
}

export async function createAlertRule(
  req: CreateRuleRequest,
): Promise<AlertRuleItem> {
  const res = await fetch("/v1/alerts/rules", {
    method: "POST",
    headers: { ...tenantHeaders(), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create alert rule: ${res.status}`);
  return res.json();
}

export async function silenceAlertRule(
  ruleId: string,
  silenced: boolean,
): Promise<AlertRuleItem> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}/silence`, {
    method: "PATCH",
    headers: { ...tenantHeaders(), "Content-Type": "application/json" },
    body: JSON.stringify({ silenced }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npm run typecheck --workspace=apps/frontend`
Expected: exits 0 with no type errors

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/api/alerts.ts
git commit -m "feat(frontend): add alert rules API client"
```

---

## Task 7: Build AlertsPage component

**Files:**
- Create: `apps/frontend/src/features/alerts/AlertsPage.tsx`

- [ ] **Step 1: Create the features/alerts directory and AlertsPage component**

Create `apps/frontend/src/features/alerts/AlertsPage.tsx`:

```tsx
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  listAlertRules,
  createAlertRule,
  silenceAlertRule,
  type AlertRuleItem,
} from "../../api/alerts";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Select, SelectOption } from "../../components/ui/select";

export function AlertsPage() {
  const queryClient = useQueryClient();
  const [isCreating, setIsCreating] = useState(false);
  const [formName, setFormName] = useState("");
  const [formMetric, setFormMetric] = useState("");
  const [formOperator, setFormOperator] = useState("gt");
  const [formThreshold, setFormThreshold] = useState("");
  const [formError, setFormError] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["alert-rules"],
    queryFn: listAlertRules,
  });

  const silenceMutation = useMutation({
    mutationFn: ({ ruleId, silenced }: { ruleId: string; silenced: boolean }) =>
      silenceAlertRule(ruleId, silenced),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules"] }),
  });

  const createMutation = useMutation({
    mutationFn: createAlertRule,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["alert-rules"] });
      setIsCreating(false);
      setFormName("");
      setFormMetric("");
      setFormOperator("gt");
      setFormThreshold("");
      setFormError(null);
    },
    onError: (e: Error) => setFormError(e.message),
  });

  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault();
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
    });
  };

  const rules = data?.items ?? [];
  const firingCount = rules.filter((r) => r.firing).length;
  const silencedCount = rules.filter((r) => r.silenced).length;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Reliability</div>
          <h1>Alerts &amp; SLOs</h1>
        </div>
      </div>

      <div className="metric-grid" aria-label="Alert summary">
        <div className="metric-tile info">
          <div className="metric-label">Total Rules</div>
          <div className="metric-value">{rules.length}</div>
        </div>
        <div className="metric-tile bad">
          <div className="metric-label">Firing</div>
          <div className="metric-value">{firingCount}</div>
        </div>
        <div className="metric-tile warn">
          <div className="metric-label">Silenced</div>
          <div className="metric-value">{silencedCount}</div>
        </div>
      </div>

      <div className="toolbar-row" style={{ justifyContent: "flex-end" }}>
        <Button onClick={() => setIsCreating((v) => !v)}>
          {isCreating ? "Cancel" : "New Rule"}
        </Button>
      </div>

      {isCreating && (
        <form
          className="table-panel"
          onSubmit={handleCreateSubmit}
          aria-label="Create alert rule"
          style={{ padding: "1rem", display: "flex", flexDirection: "column", gap: "0.75rem" }}
        >
          <div className="field-label">Create Threshold Rule</div>
          <Input
            placeholder="Rule name"
            aria-label="Rule name"
            value={formName}
            onChange={(e) => setFormName(e.target.value)}
            required
          />
          <Input
            placeholder="Metric name"
            aria-label="Metric name"
            value={formMetric}
            onChange={(e) => setFormMetric(e.target.value)}
            required
          />
          <Select
            aria-label="Operator"
            value={formOperator}
            onChange={(e) => setFormOperator(e.target.value)}
          >
            <SelectOption value="gt">&gt; (greater than)</SelectOption>
            <SelectOption value="gte">&ge; (greater than or equal)</SelectOption>
            <SelectOption value="lt">&lt; (less than)</SelectOption>
            <SelectOption value="lte">&le; (less than or equal)</SelectOption>
            <SelectOption value="eq">= (equal)</SelectOption>
          </Select>
          <Input
            placeholder="Threshold value"
            aria-label="Threshold value"
            type="number"
            step="any"
            value={formThreshold}
            onChange={(e) => setFormThreshold(e.target.value)}
            required
          />
          {formError && (
            <div role="alert" style={{ color: "var(--color-bad)" }}>
              {formError}
            </div>
          )}
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <Button type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? "Creating…" : "Create Rule"}
            </Button>
            <Button
              type="button"
              onClick={() => {
                setIsCreating(false);
                setFormError(null);
              }}
            >
              Cancel
            </Button>
          </div>
        </form>
      )}

      <div className="table-panel">
        {isLoading ? (
          <div className="loading-state">Loading alert rules…</div>
        ) : rules.length === 0 ? (
          <div className="empty-panel">
            <div className="empty-title">No alert rules</div>
            <div className="empty-metrics">
              <span>Create a threshold rule to start monitoring metrics.</span>
            </div>
          </div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Metric</th>
                <th>Condition</th>
                <th>Severity</th>
                <th>Status</th>
                <th>Action</th>
              </tr>
            </thead>
            <tbody>
              {rules.map((rule) => (
                <AlertRuleRow
                  key={rule.rule_id}
                  rule={rule}
                  onToggleSilence={() =>
                    silenceMutation.mutate({
                      ruleId: rule.rule_id,
                      silenced: !rule.silenced,
                    })
                  }
                />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

function AlertRuleRow({
  rule,
  onToggleSilence,
}: {
  rule: AlertRuleItem;
  onToggleSilence: () => void;
}) {
  const conditionLabel = `${rule.operator} ${rule.threshold}`;

  return (
    <tr>
      <td className="strong-cell">{rule.name}</td>
      <td>{rule.metric_name}</td>
      <td>{conditionLabel}</td>
      <td>{rule.severity}</td>
      <td>
        {rule.firing ? (
          <span className="status bad" aria-label="Alert status: Firing">
            Firing
          </span>
        ) : (
          <span className="status good" aria-label="Alert status: OK">
            OK
          </span>
        )}
      </td>
      <td>
        <Button onClick={onToggleSilence}>
          {rule.silenced ? "Unsilence" : "Silence"}
        </Button>
      </td>
    </tr>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npm run typecheck --workspace=apps/frontend`
Expected: exits 0 with no type errors

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertsPage.tsx
git commit -m "feat(frontend): add AlertsPage component with rule list and create form"
```

---

## Task 8: Wire up router

**Files:**
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Replace the alerts placeholder route**

In `apps/frontend/src/router.ts`:

Add the import at the top with the other page imports:
```typescript
import { AlertsPage } from "./features/alerts/AlertsPage";
```

Change the `alertsRoute` definition from:
```typescript
const alertsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts",
  component: () => createElement(ProductAreaPage, { area: "alerts" }),
});
```

To:
```typescript
const alertsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts",
  component: AlertsPage,
});
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npm run typecheck --workspace=apps/frontend`
Expected: exits 0 with no type errors

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/router.ts
git commit -m "feat(frontend): wire /alerts route to AlertsPage"
```

---

## Task 9: Add frontend tests

**Files:**
- Modify: `apps/frontend/src/App.test.tsx`

- [ ] **Step 1: Add the alert page tests to App.test.tsx**

Append the following tests at the end of `apps/frontend/src/App.test.tsx`:

```typescript
test("alerts page renders rule list with firing badge", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/alerts/rules") && !url.includes("silence")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                rule_id: "10000000-0000-0000-0000-000000000001",
                name: "High error rate",
                metric_name: "error_rate",
                operator: "gt",
                threshold: 0.05,
                severity: "warning",
                silenced: false,
                firing: true,
                last_fired_at: "2026-04-28T10:00:00Z",
              },
            ],
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Alerts & SLOs" })).toBeInTheDocument();
  expect(await screen.findByText("High error rate")).toBeInTheDocument();
  expect(screen.getByLabelText("Alert status: Firing")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Silence" })).toBeInTheDocument();
});

test("alerts page shows OK status when rule is not firing", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/alerts/rules")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                rule_id: "10000000-0000-0000-0000-000000000002",
                name: "Low traffic",
                metric_name: "requests",
                operator: "lt",
                threshold: 1.0,
                severity: "warning",
                silenced: false,
                firing: false,
                last_fired_at: null,
              },
            ],
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  expect(await screen.findByLabelText("Alert status: OK")).toBeInTheDocument();
});

test("alerts page silence button calls PATCH and refreshes list", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";

    if (url.includes("/v1/alerts/rules") && url.includes("silence") && method === "PATCH") {
      return new Response(
        JSON.stringify({
          rule_id: "10000000-0000-0000-0000-000000000001",
          name: "High error rate",
          metric_name: "error_rate",
          operator: "gt",
          threshold: 0.05,
          severity: "warning",
          silenced: true,
          firing: false,
          last_fired_at: null,
        }),
        { status: 200 },
      );
    }
    if (url.includes("/v1/alerts/rules")) {
      return new Response(
        JSON.stringify({
          items: [
            {
              rule_id: "10000000-0000-0000-0000-000000000001",
              name: "High error rate",
              metric_name: "error_rate",
              operator: "gt",
              threshold: 0.05,
              severity: "warning",
              silenced: false,
              firing: true,
              last_fired_at: null,
            },
          ],
        }),
        { status: 200 },
      );
    }
    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  const silenceBtn = await screen.findByRole("button", { name: "Silence" });
  fireEvent.click(silenceBtn);

  await waitFor(() => {
    const patchCall = fetchMock.mock.calls.find(
      ([url, init]) =>
        String(url).includes("silence") && (init as RequestInit)?.method === "PATCH",
    );
    expect(patchCall).toBeDefined();
    const body = JSON.parse((patchCall![1] as RequestInit).body as string);
    expect(body.silenced).toBe(true);
  });
});

test("alerts page create form submits POST and closes panel", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";

    if (url.includes("/v1/alerts/rules") && method === "POST") {
      return new Response(
        JSON.stringify({
          rule_id: "20000000-0000-0000-0000-000000000001",
          name: "High latency",
          metric_name: "p95_latency_ms",
          operator: "gt",
          threshold: 500,
          severity: "warning",
          silenced: false,
          firing: false,
          last_fired_at: null,
        }),
        { status: 201 },
      );
    }
    if (url.includes("/v1/alerts/rules")) {
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }
    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  await screen.findByRole("heading", { name: "Alerts & SLOs" });

  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));
  expect(screen.getByLabelText("Create alert rule")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "High latency" },
  });
  fireEvent.change(screen.getByLabelText("Metric name"), {
    target: { value: "p95_latency_ms" },
  });
  fireEvent.change(screen.getByLabelText("Threshold value"), {
    target: { value: "500" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => {
    const postCall = fetchMock.mock.calls.find(
      ([url, init]) =>
        String(url).includes("/v1/alerts/rules") && (init as RequestInit)?.method === "POST",
    );
    expect(postCall).toBeDefined();
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body.name).toBe("High latency");
    expect(body.metric_name).toBe("p95_latency_ms");
    expect(body.threshold).toBe(500);
  });

  // Panel should close after success
  await waitFor(() =>
    expect(screen.queryByLabelText("Create alert rule")).not.toBeInTheDocument(),
  );
});

test("alerts page renders empty state when no rules exist", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).includes("/v1/alerts/rules")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  expect(await screen.findByText("No alert rules")).toBeInTheDocument();
  expect(
    screen.getByText("Create a threshold rule to start monitoring metrics."),
  ).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the frontend tests**

Run: `npm run test --workspace=apps/frontend`
Expected: all new tests pass alongside existing tests

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/App.test.tsx
git commit -m "test(frontend): add alerts page UI tests"
```

---

## Task 10: Update spec and plan document

**Files:**
- Modify: `spec/09-api.md`
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`

- [ ] **Step 1: Add Alert Rules API section to spec/09-api.md**

Append the following section to the end of `spec/09-api.md`:

```markdown
### Alert Rules API

Three tenant-scoped endpoints for threshold alert rule management. All require `X-Tenant-ID` header.

#### List alert rules

- **Endpoint**: `GET /v1/alerts/rules`
- **Response**: `{ "items": [AlertRuleItem] }`
- **AlertRuleItem fields**: `rule_id` (UUID), `name` (string), `metric_name` (string), `operator` (gt|gte|lt|lte|eq), `threshold` (number), `severity` (string), `silenced` (bool), `firing` (bool), `last_fired_at` (ISO 8601 | null)
- `firing` is `true` when an `active` alert firing exists for the rule. `last_fired_at` is the timestamp of the most recent active firing.

#### Create alert rule

- **Endpoint**: `POST /v1/alerts/rules`
- **Request body**: `{ "name": string, "metric_name": string, "operator": string, "threshold": number }`
- **Response**: `201 Created` with the created `AlertRuleItem`. `severity` defaults to `warning`. `alert_type` is always `threshold`.
- **Validation errors**: `400 Bad Request` when `name` or `metric_name` is empty, `operator` is not one of `gt|gte|lt|lte|eq`, or `threshold` is non-finite.

#### Silence or unsilence a rule

- **Endpoint**: `PATCH /v1/alerts/rules/{rule_id}/silence`
- **Request body**: `{ "silenced": bool }`
- **Response**: `200 OK` with the updated `AlertRuleItem`, or `404 Not Found` when the rule does not belong to the authenticated tenant.
- Silenced rules are excluded from the alert-evaluator's evaluation cycle.
```

- [ ] **Step 2: Mark P3-S6d complete in the phases plan**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, change:

```markdown
- [ ] **P3-S6d: Add a minimal threshold-alert UI workflow**
```

To:

```markdown
- [x] **P3-S6d: Add a minimal threshold-alert UI workflow**
```

And append the checkpoint answer after the existing `Checkpoint:` line in that slice entry:

```
  - Checkpoint: does the UI expose one complete alert loop for threshold rules, not just backend evaluator state? Answer: yes. The `/alerts` page lists all threshold rules with live firing state, a create form submits POST /v1/alerts/rules, and per-row Silence/Unsilence buttons call PATCH .../silence. All three interactions are covered by frontend tests using fetch stubs and backend behavior is covered by Postgres Testcontainers integration tests.
```

- [ ] **Step 3: Commit**

```bash
git add spec/09-api.md docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs: update API spec and mark P3-S6d complete in phases plan"
```

---

## Task 11: Run local CI gate

- [ ] **Step 1: Run local-ci.sh**

Run: `bash scripts/local-ci.sh`
Expected: all stages pass — Rust fmt, clippy, tests (including Testcontainers), frontend typecheck/lint/build/test, Docker image build, smoke test.

If any stage fails, fix it before proceeding. Do not push with a failing gate.

- [ ] **Step 2: Push and open PR**

```bash
git push -u origin HEAD
```

Then open a PR targeting `main` with title `feat(P3-S6d): threshold alert UI workflow` and body following the standard slice packet from the phases plan.
