# P5-S1 Incident Timeline with Source Links — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make incident timelines human-readable and link each alert-fired event to a new AlertRuleDetailPage that shows the rule's condition and firing history.

**Architecture:** The backend gains `rule_name` on `IncidentDetailResponse` (LEFT JOIN), enriched event messages in the alert evaluator, and a new `GET /v1/alerts/rules/{rule_id}` endpoint. The frontend gains source links on the incident timeline and a new `AlertRuleDetailPage` at `/alerts/$ruleId`.

**Tech Stack:** Rust/axum (query-api, alert-evaluator), sqlx (PostgreSQL), React 19 + TanStack Router + TanStack Query, Vitest/RTL (frontend tests), Testcontainers (backend integration tests).

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Modify | `services/query-api/src/incidents.rs` | Add `rule_name` field via LEFT JOIN |
| Modify | `services/alert-evaluator/src/evaluator.rs` | Enrich `alert_fired`/`alert_resolved` messages |
| Modify | `services/query-api/src/alerts.rs` | Add `get_alert_rule` + `handle_get_rule` |
| Modify | `services/query-api/src/main.rs` | Register new `GET /v1/alerts/rules/{rule_id}` route |
| Modify | `services/query-api/tests/http_api_integration.rs` | Four new integration tests |
| Modify | `apps/frontend/src/api/incidents.ts` | Add `rule_name` to `IncidentDetailResponse` |
| Modify | `apps/frontend/src/api/alerts.ts` | Add `FiringItem`, `AlertRuleDetailResponse`, `getAlertRule` |
| Modify | `apps/frontend/src/features/incidents/IncidentDetailPage.tsx` | Source link + runbook URL |
| Create | `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx` | Unit tests |
| Create | `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx` | New page |
| Create | `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx` | Unit tests |
| Modify | `apps/frontend/src/router.ts` | Register `/alerts/$ruleId` route |
| Modify | `docs/agent-context.md` | Note new endpoint and route |
| Modify | `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` | Mark P5-S1 complete |

---

## Task 1: Create branch and write backend integration tests (failing)

**Files:**
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Create the feature branch**

```bash
git checkout -b feat/p5-s1-incident-timeline
git push -u origin feat/p5-s1-incident-timeline
```

- [ ] **Step 2: Add four failing integration tests at the bottom of `http_api_integration.rs`**

Append this block before the final `}` closing the file (after the last existing test):

```rust
#[tokio::test]
async fn get_incident_detail_includes_rule_name() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let app = build_app_with_pg(ch, pg.clone());
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'CPU High', 'threshold', 'critical', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":90}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'CPU spike', 'critical', 'triggered', 'dedup-rule-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&pg)
    .await
    .expect("incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["rule_name"], "CPU High");
}

#[tokio::test]
async fn get_incident_detail_rule_name_null_when_no_rule() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let app = build_app_with_pg(ch, pg.clone());
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Manual incident', 'warning', 'triggered', 'dedup-norule')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&pg)
    .await
    .expect("incident inserted");

    let response = app
        .oneshot(dev_request("GET", &format!("/v1/incidents/{incident_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert!(body["rule_name"].is_null());
}

#[tokio::test]
async fn get_alert_rule_returns_detail_with_firings() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'High Error Rate', 'threshold', 'critical', \
                 '{\"metric_name\":\"error_rate\",\"operator\":\"gt\",\"threshold\":0.05}', \
                 '{}', false) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&pg)
    .await
    .expect("rule inserted");

    for state in ["active", "resolved"] {
        sqlx::query(
            "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(rule_id)
        .bind(tenant)
        .bind(state)
        .bind(0.08_f64)
        .execute(&pg)
        .await
        .expect("firing inserted");
    }

    let app = build_app_with_pg(ch, pg.clone());
    let response = app
        .oneshot(dev_request("GET", &format!("/v1/alerts/rules/{rule_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "High Error Rate");
    assert_eq!(body["severity"], "critical");
    assert_eq!(body["alert_type"], "threshold");
    let firings = body["firings"].as_array().unwrap();
    assert_eq!(firings.len(), 2);
}

#[tokio::test]
async fn get_alert_rule_returns_404_for_wrong_tenant() {
    let (ch, _ch_container) = start_clickhouse().await;
    let (pg, _pg_container) = start_postgres().await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Other Tenant Rule', 'threshold', 'warning', \
                 '{\"metric_name\":\"m\",\"operator\":\"gt\",\"threshold\":1.0}', \
                 '{}', false) \
         RETURNING rule_id",
    )
    .bind(Uuid::new_v4()) // different tenant — NOT DEV_TENANT_ID
    .fetch_one(&pg)
    .await
    .expect("rule inserted");

    // Request is authenticated as DEV_TENANT_ID
    let app = build_app_with_pg(ch, pg.clone());
    let response = app
        .oneshot(dev_request("GET", &format!("/v1/alerts/rules/{rule_id}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 3: Run the new tests to verify they fail**

```bash
cd services/query-api
cargo test --test http_api_integration get_incident_detail_includes_rule_name 2>&1 | tail -20
cargo test --test http_api_integration get_alert_rule_returns_detail_with_firings 2>&1 | tail -20
```

Expected: compile error or test failure — `rule_name` field doesn't exist yet, `handle_get_rule` doesn't exist yet.

- [ ] **Step 4: Commit the failing tests**

```bash
git add services/query-api/tests/http_api_integration.rs
git commit -m "test(issue): reproduce P5-S1 gaps — rule_name on incident detail, alert rule detail endpoint"
```

---

## Task 2: Add `rule_name` to incident detail response

**Files:**
- Modify: `services/query-api/src/incidents.rs`

- [ ] **Step 1: Add `rule_name` to `IncidentDetailRow`**

Find this struct in `incidents.rs`:

```rust
#[derive(sqlx::FromRow)]
struct IncidentDetailRow {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    dedup_key: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    triggered_by_rule_id: Option<Uuid>,
    runbook_url: Option<String>,
}
```

Replace it with:

```rust
#[derive(sqlx::FromRow)]
struct IncidentDetailRow {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    dedup_key: String,
    triggered_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    triggered_by_rule_id: Option<Uuid>,
    runbook_url: Option<String>,
    rule_name: Option<String>,
}
```

- [ ] **Step 2: Add `rule_name` to `IncidentDetailResponse`**

Find:

```rust
#[derive(Serialize)]
pub struct IncidentDetailResponse {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub dedup_key: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
    pub runbook_url: Option<String>,
    pub timeline: Vec<IncidentEventItem>,
}
```

Replace with:

```rust
#[derive(Serialize)]
pub struct IncidentDetailResponse {
    pub incident_id: Uuid,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub dedup_key: String,
    pub triggered_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub triggered_by_rule_id: Option<Uuid>,
    pub runbook_url: Option<String>,
    pub rule_name: Option<String>,
    pub timeline: Vec<IncidentEventItem>,
}
```

- [ ] **Step 3: Update the SQL query to LEFT JOIN alert_rules**

Find the query in `get_incident`:

```rust
    let row: Option<IncidentDetailRow> = sqlx::query_as(
        "SELECT incident_id, title, severity, status, dedup_key, triggered_at, resolved_at, triggered_by_rule_id, runbook_url \
         FROM incidents \
         WHERE incident_id = $1 AND tenant_id = $2",
    )
```

Replace with:

```rust
    let row: Option<IncidentDetailRow> = sqlx::query_as(
        "SELECT i.incident_id, i.title, i.severity, i.status, i.dedup_key, \
                i.triggered_at, i.resolved_at, i.triggered_by_rule_id, i.runbook_url, \
                r.name AS rule_name \
         FROM incidents i \
         LEFT JOIN alert_rules r ON i.triggered_by_rule_id = r.rule_id \
         WHERE i.incident_id = $1 AND i.tenant_id = $2",
    )
```

- [ ] **Step 4: Propagate `rule_name` in the return value**

Find the `Ok(Some(IncidentDetailResponse { ... }))` block and add `rule_name: row.rule_name,` to it:

```rust
    Ok(Some(IncidentDetailResponse {
        incident_id: row.incident_id,
        title: row.title,
        severity: row.severity,
        status: row.status,
        dedup_key: row.dedup_key,
        triggered_at: row.triggered_at,
        resolved_at: row.resolved_at,
        triggered_by_rule_id: row.triggered_by_rule_id,
        runbook_url: row.runbook_url,
        rule_name: row.rule_name,
        timeline,
    }))
```

- [ ] **Step 5: Run cargo fmt, then run the two incident tests**

```bash
cargo fmt --all
cd services/query-api
cargo test --test http_api_integration get_incident_detail_includes_rule_name 2>&1 | tail -10
cargo test --test http_api_integration get_incident_detail_rule_name_null_when_no_rule 2>&1 | tail -10
```

Expected: both tests PASS.

- [ ] **Step 6: Commit**

```bash
git add services/query-api/src/incidents.rs
git commit -m "feat(incidents): add rule_name to incident detail response via LEFT JOIN"
```

---

## Task 3: Enrich alert evaluator event messages

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`

- [ ] **Step 1: Enrich the `alert_fired` message in `upsert_incident_from_firing`**

Find this query in `upsert_incident_from_firing`:

```rust
    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'alert_fired', 'system', $2)",
    )
    .bind(incident_id)
    .bind(format!("Firing {firing_id}"))
    .execute(db)
    .await?;
```

Replace `.bind(format!("Firing {firing_id}"))` with:

```rust
    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'alert_fired', 'system', $2)",
    )
    .bind(incident_id)
    .bind(format!("{} fired: value={:.2}", rule.name, value))
    .execute(db)
    .await?;
```

- [ ] **Step 2: Add `value: f64` parameter to `resolve_incident_for_firing`**

Find the function signature:

```rust
async fn resolve_incident_for_firing(
    db: &sqlx::PgPool,
    rule: &AlertRuleRow,
    firing_id: Uuid,
) -> anyhow::Result<()> {
```

Replace with:

```rust
async fn resolve_incident_for_firing(
    db: &sqlx::PgPool,
    rule: &AlertRuleRow,
    firing_id: Uuid,
    value: f64,
) -> anyhow::Result<()> {
```

- [ ] **Step 3: Enrich the `alert_resolved` message in `resolve_incident_for_firing`**

Find:

```rust
        sqlx::query(
            "INSERT INTO incident_events (incident_id, event_type, actor, message) \
             VALUES ($1, 'alert_resolved', 'system', $2)",
        )
        .bind(incident_id)
        .bind(format!("Firing {firing_id} resolved"))
        .execute(db)
        .await?;
```

Replace `.bind(format!("Firing {firing_id} resolved"))` with:

```rust
        sqlx::query(
            "INSERT INTO incident_events (incident_id, event_type, actor, message) \
             VALUES ($1, 'alert_resolved', 'system', $2)",
        )
        .bind(incident_id)
        .bind(format!("{} resolved: value={:.2}", rule.name, value))
        .execute(db)
        .await?;
```

- [ ] **Step 4: Update the call site in `resolve_open_firing` to pass `value`**

Find the call in `resolve_open_firing`:

```rust
        if let Err(e) = resolve_incident_for_firing(db, rule, *firing_id).await {
```

Replace with:

```rust
        if let Err(e) = resolve_incident_for_firing(db, rule, *firing_id, value).await {
```

- [ ] **Step 5: Run cargo fmt and evaluator unit tests**

```bash
cargo fmt --all
cd services/alert-evaluator
cargo test 2>&1 | tail -10
```

Expected: all existing unit tests PASS (they don't assert on message text).

- [ ] **Step 6: Commit**

```bash
git add services/alert-evaluator/src/evaluator.rs
git commit -m "feat(alert-evaluator): enrich incident event messages with rule name and value"
```

---

## Task 4: Add `GET /v1/alerts/rules/{rule_id}` endpoint

**Files:**
- Modify: `services/query-api/src/alerts.rs`
- Modify: `services/query-api/src/main.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Add new types to `alerts.rs`**

Add these structs after the existing `AlertRuleListResponse` struct:

```rust
#[derive(Serialize)]
pub struct FiringItem {
    pub firing_id: Uuid,
    pub state: String,
    pub value: Option<f64>,
    pub occurred_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

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
}

#[derive(sqlx::FromRow)]
struct AlertRuleDetailRow {
    rule_id: Uuid,
    name: String,
    severity: String,
    alert_type: String,
    condition: serde_json::Value,
    silenced: bool,
    firing: bool,
}

#[derive(sqlx::FromRow)]
struct FiringRow {
    firing_id: Uuid,
    state: String,
    value: Option<f64>,
    occurred_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 2: Add `get_alert_rule` function to `alerts.rs`**

Add this function before the existing `handle_list_rules`:

```rust
pub async fn get_alert_rule(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    rule_id: Uuid,
) -> Result<Option<AlertRuleDetailResponse>, sqlx::Error> {
    let row: Option<AlertRuleDetailRow> = sqlx::query_as(
        "SELECT r.rule_id, r.name, r.severity, r.alert_type, r.condition, r.silenced, \
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

    let Some(row) = row else {
        return Ok(None);
    };

    let firings: Vec<FiringRow> = sqlx::query_as(
        "SELECT firing_id, state, value, occurred_at, resolved_at \
         FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 \
         ORDER BY occurred_at DESC \
         LIMIT 20",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .fetch_all(db)
    .await?;

    Ok(Some(AlertRuleDetailResponse {
        rule_id: row.rule_id,
        name: row.name,
        severity: row.severity,
        alert_type: row.alert_type,
        condition: row.condition,
        silenced: row.silenced,
        firing: row.firing,
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
}

pub async fn handle_get_rule(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(rule_id): Path<Uuid>,
) -> Result<Json<AlertRuleDetailResponse>, StatusCode> {
    match get_alert_rule(&state.db, ctx.tenant_id, rule_id).await {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get alert rule");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 3: Register the route in `main.rs`**

Find the existing alert routes block:

```rust
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules", post(alerts::handle_create_rule))
        .route(
            "/v1/alerts/rules/{rule_id}/silence",
            patch(alerts::handle_silence_rule),
        )
```

Add the new route immediately after `/v1/alerts/rules` POST:

```rust
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules", post(alerts::handle_create_rule))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .route(
            "/v1/alerts/rules/{rule_id}/silence",
            patch(alerts::handle_silence_rule),
        )
```

- [ ] **Step 4: Register the route in `build_app_with_pg` in the integration test file**

Find:

```rust
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
```

Add below it:

```rust
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
```

- [ ] **Step 5: Run cargo fmt, then run the two alert rule tests**

```bash
cargo fmt --all
cd services/query-api
cargo test --test http_api_integration get_alert_rule_returns_detail_with_firings 2>&1 | tail -10
cargo test --test http_api_integration get_alert_rule_returns_404_for_wrong_tenant 2>&1 | tail -10
```

Expected: both tests PASS.

- [ ] **Step 6: Run all four new integration tests together**

```bash
cargo test --test http_api_integration get_incident_detail_includes_rule_name 2>&1 | tail -5
cargo test --test http_api_integration get_incident_detail_rule_name_null_when_no_rule 2>&1 | tail -5
cargo test --test http_api_integration get_alert_rule_returns_detail_with_firings 2>&1 | tail -5
cargo test --test http_api_integration get_alert_rule_returns_404_for_wrong_tenant 2>&1 | tail -5
```

Expected: all four PASS.

- [ ] **Step 7: Commit**

```bash
git add services/query-api/src/alerts.rs services/query-api/src/main.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(alerts): add GET /v1/alerts/rules/:rule_id endpoint with firing history"
```

---

## Task 5: Update frontend API types

**Files:**
- Modify: `apps/frontend/src/api/incidents.ts`
- Modify: `apps/frontend/src/api/alerts.ts`

- [ ] **Step 1: Add `rule_name` to `IncidentDetailResponse` in `incidents.ts`**

Find:

```ts
export interface IncidentDetailResponse {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  dedup_key: string;
  triggered_at: string;
  resolved_at: string | null;
  triggered_by_rule_id: string | null;
  runbook_url: string | null;
  timeline: IncidentEventItem[];
}
```

Replace with:

```ts
export interface IncidentDetailResponse {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  dedup_key: string;
  triggered_at: string;
  resolved_at: string | null;
  triggered_by_rule_id: string | null;
  runbook_url: string | null;
  rule_name: string | null;
  timeline: IncidentEventItem[];
}
```

- [ ] **Step 2: Add `FiringItem`, `AlertRuleDetailResponse`, and `getAlertRule` to `alerts.ts`**

Append to the end of `apps/frontend/src/api/alerts.ts`:

```ts
export interface FiringItem {
  firing_id: string;
  state: "pending" | "active" | "resolved";
  value: number | null;
  occurred_at: string;
  resolved_at: string | null;
}

export interface AlertRuleDetailResponse {
  rule_id: string;
  name: string;
  severity: string;
  alert_type: string;
  condition: Record<string, unknown>;
  silenced: boolean;
  firing: boolean;
  firings: FiringItem[];
}

export async function getAlertRule(
  tenantId: string,
  ruleId: string,
): Promise<AlertRuleDetailResponse> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Failed to get alert rule: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 3: Run frontend typecheck**

```bash
cd apps/frontend
npm run typecheck
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/api/incidents.ts apps/frontend/src/api/alerts.ts
git commit -m "feat(frontend/api): add rule_name to incident response, add getAlertRule and types"
```

---

## Task 6: Update `IncidentDetailPage` with source links and runbook URL

**Files:**
- Modify: `apps/frontend/src/features/incidents/IncidentDetailPage.tsx`
- Create: `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx`

- [ ] **Step 1: Write the failing test file first**

Create `apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as incidentsApi from "../../api/incidents";
import { IncidentDetailPage } from "./IncidentDetailPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useParams: () => ({ incidentId: "inc-1" }),
    Link: ({
      children,
      to,
      params,
      ...props
    }: {
      children?: React.ReactNode;
      to: string;
      params?: Record<string, string>;
    } & React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
      <a href={params ? `${to}/${Object.values(params)[0]}` : to} {...props}>
        {children}
      </a>
    ),
  };
});

const baseDetail: incidentsApi.IncidentDetailResponse = {
  incident_id: "inc-1",
  title: "CPU spike",
  severity: "critical",
  status: "triggered",
  dedup_key: "rule-abc",
  triggered_at: "2026-05-18T10:00:00Z",
  resolved_at: null,
  triggered_by_rule_id: "rule-abc",
  runbook_url: null,
  rule_name: "High CPU Alert",
  timeline: [
    {
      event_time: "2026-05-18T10:00:01Z",
      event_type: "triggered",
      actor: "system",
      message: "Alert rule transitioned to active",
    },
    {
      event_time: "2026-05-18T10:00:05Z",
      event_type: "alert_fired",
      actor: "system",
      message: "High CPU Alert fired: value=95.30",
    },
  ],
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <IncidentDetailPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders timeline events with humanized labels", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue(baseDetail);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1, name: "CPU spike" }));
  expect(screen.getByText("triggered")).toBeInTheDocument();
  expect(screen.getByText("alert fired")).toBeInTheDocument();
});

test("renders view rule link on alert_fired when triggered_by_rule_id is set", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue(baseDetail);
  renderPage();
  await waitFor(() => screen.getByText("→ View rule"));
  expect(screen.getByText("→ View rule")).toBeInTheDocument();
});

test("does not render view rule link when triggered_by_rule_id is null", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue({
    ...baseDetail,
    triggered_by_rule_id: null,
  });
  renderPage();
  await waitFor(() => screen.getByText("alert fired"));
  expect(screen.queryByText("→ View rule")).not.toBeInTheDocument();
});

test("renders runbook_url as link when present", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue({
    ...baseDetail,
    runbook_url: "https://runbooks.example.com/cpu-high",
  });
  renderPage();
  await waitFor(() => screen.getByText("https://runbooks.example.com/cpu-high"));
  const link = screen.getByRole("link", { name: "https://runbooks.example.com/cpu-high" });
  expect(link).toHaveAttribute("href", "https://runbooks.example.com/cpu-high");
});

test("does not render Runbook section when runbook_url is null", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue(baseDetail);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1 }));
  expect(screen.queryByText("Runbook")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run the tests — verify they fail**

```bash
cd apps/frontend
npm test -- IncidentDetailPage.test.tsx 2>&1 | tail -20
```

Expected: tests fail because `→ View rule` link does not exist yet.

- [ ] **Step 3: Replace `IncidentDetailPage.tsx` with the updated version**

Replace the entire file with:

```tsx
import { Link, useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getIncident, type IncidentEventItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":    return "bad";
    case "acknowledged": return "warn";
    case "resolved":     return "good";
    default:             return "neutral";
  }
}

function eventGlyph(eventType: string): string {
  switch (eventType) {
    case "triggered":         return "▸";
    case "alert_fired":       return "!";
    case "alert_resolved":    return "✓";
    case "acknowledged":      return "◎";
    case "comment":           return "·";
    case "status_change":     return "→";
    case "deployment_linked": return "↑";
    default:                  return "·";
  }
}

const LINKED_EVENT_TYPES = new Set(["alert_fired", "alert_resolved"]);

export function IncidentDetailPage() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();
  const { incidentId } = useParams({ from: "/incidents/$incidentId" });

  const { data, isLoading } = useQuery({
    queryKey: ["incident", tenantId, incidentId],
    queryFn: () => getIncident(tenantId, incidentId),
  });

  if (isLoading) {
    return <LoadingState>Loading incident...</LoadingState>;
  }

  if (!data) {
    return <Panel>Incident not found.</Panel>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Incident</div>
          <h1>{data.title}</h1>
        </div>
        <div className="flex items-center gap-2">
          <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
          <Badge tone={statusColor(data.status)}>{data.status}</Badge>
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <div className="field-label">Triggered</div>
            <div className="mt-1">{formatTimestamp(isoToNs(data.triggered_at), format)}</div>
          </div>
          <div>
            <div className="field-label">Resolved</div>
            <div className="mt-1">
              {data.resolved_at
                ? formatTimestamp(isoToNs(data.resolved_at), format)
                : "—"}
            </div>
          </div>
          {data.runbook_url && (
            <div className="col-span-2">
              <div className="field-label">Runbook</div>
              <div className="mt-1">
                <a
                  href={data.runbook_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[var(--brand)] hover:underline"
                >
                  {data.runbook_url}
                </a>
              </div>
            </div>
          )}
        </div>
      </Panel>

      <Panel>
        <h3 className="text-sm font-semibold mb-3">Timeline</h3>
        <div className="space-y-3">
          {data.timeline.map((event: IncidentEventItem, idx: number) => {
            const showLink =
              LINKED_EVENT_TYPES.has(event.event_type) &&
              data.triggered_by_rule_id !== null;
            return (
              <div key={idx} className="flex gap-3">
                <div className="font-mono text-base leading-none text-[var(--muted)] w-4 flex-shrink-0">
                  {eventGlyph(event.event_type)}
                </div>
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium capitalize">
                      {event.event_type.replace(/_/g, " ")}
                    </span>
                    <span className="text-xs text-[var(--muted)]">
                      {formatTimestamp(isoToNs(event.event_time), format)}
                    </span>
                  </div>
                  {event.message && (
                    <p className="text-sm text-[var(--muted)] mt-0.5">{event.message}</p>
                  )}
                  <div className="flex items-center gap-3">
                    <p className="text-xs text-[var(--muted)]">by {event.actor}</p>
                    {showLink && (
                      <Link
                        to="/alerts/$ruleId"
                        params={{ ruleId: data.triggered_by_rule_id! }}
                        className="text-xs text-[var(--brand)] hover:underline"
                      >
                        → View rule
                      </Link>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
          {data.timeline.length === 0 && (
            <p className="text-sm text-[var(--muted)]">No timeline events.</p>
          )}
        </div>
      </Panel>
    </section>
  );
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cd apps/frontend
npm test -- IncidentDetailPage.test.tsx 2>&1 | tail -15
```

Expected: all 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/incidents/IncidentDetailPage.tsx \
        apps/frontend/src/features/incidents/IncidentDetailPage.test.tsx
git commit -m "feat(incidents): add source links and runbook URL to incident timeline"
```

---

## Task 7: Create `AlertRuleDetailPage`

**Files:**
- Create: `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`
- Create: `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`

- [ ] **Step 1: Write the failing test file**

Create `apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as alertsApi from "../../api/alerts";
import { AlertRuleDetailPage } from "./AlertRuleDetailPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useParams: () => ({ ruleId: "rule-1" }),
  };
});

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
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AlertRuleDetailPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders rule name and severity", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue(sampleRule);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1, name: "High CPU Alert" }));
  expect(screen.getByText("critical")).toBeInTheDocument();
  expect(screen.getByText("threshold")).toBeInTheDocument();
});

test("renders condition summary for threshold rule", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue(sampleRule);
  renderPage();
  await waitFor(() => screen.getByText("cpu_usage > 90"));
});

test("renders firings table with correct row count", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue(sampleRule);
  renderPage();
  await waitFor(() => screen.getByRole("table", { name: "Firing history" }));
  // 1 header row + 2 data rows
  expect(screen.getAllByRole("row")).toHaveLength(3);
});

test("renders empty state when no firings", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue({ ...sampleRule, firings: [] });
  renderPage();
  await waitFor(() => screen.getByText("No firings recorded."));
});
```

- [ ] **Step 2: Run the tests — verify they fail**

```bash
cd apps/frontend
npm test -- AlertRuleDetailPage.test.tsx 2>&1 | tail -10
```

Expected: fail — `AlertRuleDetailPage` does not exist yet.

- [ ] **Step 3: Create `AlertRuleDetailPage.tsx`**

Create `apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx`:

```tsx
import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getAlertRule, type FiringItem } from "../../api/alerts";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function stateColor(state: string): "bad" | "warn" | "good" | "neutral" {
  switch (state) {
    case "active":   return "bad";
    case "pending":  return "warn";
    case "resolved": return "good";
    default:         return "neutral";
  }
}

function conditionSummary(condition: Record<string, unknown>): string {
  const { metric_name, operator, threshold, slo_id } = condition;
  if (slo_id) return `SLO burn-rate (${slo_id})`;
  if (metric_name && operator && threshold !== undefined) {
    const opSymbol: Record<string, string> = {
      gt: ">", gte: "≥", lt: "<", lte: "≤", eq: "=",
    };
    return `${metric_name} ${opSymbol[operator as string] ?? operator} ${threshold}`;
  }
  return JSON.stringify(condition);
}

export function AlertRuleDetailPage() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();
  const { ruleId } = useParams({ from: "/alerts/$ruleId" });

  const { data, isLoading } = useQuery({
    queryKey: ["alertRule", tenantId, ruleId],
    queryFn: () => getAlertRule(tenantId, ruleId),
  });

  if (isLoading) {
    return <LoadingState>Loading alert rule...</LoadingState>;
  }

  if (!data) {
    return <Panel>Alert rule not found.</Panel>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Alert Rule</div>
          <h1>{data.name}</h1>
        </div>
        <div className="flex items-center gap-2">
          <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
          {data.silenced && <Badge tone="neutral">silenced</Badge>}
          {data.firing && <Badge tone="bad">firing</Badge>}
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <div className="field-label">Type</div>
            <div className="mt-1 font-mono">{data.alert_type}</div>
          </div>
          <div>
            <div className="field-label">Condition</div>
            <div className="mt-1 font-mono">{conditionSummary(data.condition)}</div>
          </div>
        </div>
      </Panel>

      <Panel eyebrow="Last 20 firings">
        {data.firings.length === 0 ? (
          <EmptyState title="No firings recorded." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm" aria-label="Firing history">
              <thead>
                <tr className="border-b text-left">
                  <th className="pb-2 pr-4">State</th>
                  <th className="pb-2 pr-4">Value</th>
                  <th className="pb-2 pr-4">Occurred At</th>
                  <th className="pb-2 pr-4">Resolved At</th>
                </tr>
              </thead>
              <tbody>
                {data.firings.map((firing: FiringItem) => (
                  <tr key={firing.firing_id} className="modern-table-row">
                    <td className="py-2 pr-4">
                      <Badge tone={stateColor(firing.state)}>{firing.state}</Badge>
                    </td>
                    <td className="py-2 pr-4 font-mono">
                      {firing.value !== null ? firing.value.toFixed(2) : "—"}
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">
                      {formatTimestamp(isoToNs(firing.occurred_at), format)}
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">
                      {firing.resolved_at
                        ? formatTimestamp(isoToNs(firing.resolved_at), format)
                        : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </section>
  );
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cd apps/frontend
npm test -- AlertRuleDetailPage.test.tsx 2>&1 | tail -15
```

Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertRuleDetailPage.tsx \
        apps/frontend/src/features/alerts/AlertRuleDetailPage.test.tsx
git commit -m "feat(alerts): add AlertRuleDetailPage with firing history"
```

---

## Task 8: Wire the router

**Files:**
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Add the import and route to `router.ts`**

Add this import after the existing `AlertsPage` import at line 6:

```ts
import { AlertRuleDetailPage } from "./features/alerts/AlertRuleDetailPage";
```

Add this route constant after `alertsRoute`:

```ts
const alertRuleDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/alerts/$ruleId",
  component: AlertRuleDetailPage,
});
```

Add `alertRuleDetailRoute` to `routeTree.addChildren([...])` immediately after `alertsRoute`:

```ts
    alertsRoute,
    alertRuleDetailRoute,
```

- [ ] **Step 2: Run typecheck**

```bash
cd apps/frontend
npm run typecheck
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/router.ts
git commit -m "feat(router): add /alerts/\$ruleId route for AlertRuleDetailPage"
```

---

## Task 9: Update docs and roadmap

**Files:**
- Modify: `docs/agent-context.md`
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`

- [ ] **Step 1: Update `docs/agent-context.md`**

Find the "Active detailed implementation plan" line and update it to reference the P5-S1 plan, then add a note about the new API endpoint and route. Find the section that lists active routes or backend gotchas and add:

```
- `GET /v1/alerts/rules/{rule_id}` — returns AlertRuleDetailResponse with recent firings (20 max). Added in P5-S1.
- `IncidentDetailResponse` now includes `rule_name: Option<String>` via LEFT JOIN on alert_rules. Added in P5-S1.
- Frontend route `/alerts/$ruleId` renders AlertRuleDetailPage. Added in P5-S1.
```

- [ ] **Step 2: Mark P5-S1 complete in `2026-05-07-remaining-roadmap-plan.md`**

Find:

```markdown
- [ ] **P5-S1: Add incident timeline for one alert source**
```

Replace with:

```markdown
- [x] **P5-S1: Add incident timeline for one alert source** (COMPLETED 2026-05-18)
```

Add a completion note below it:

```markdown
  - Completion: `IncidentDetailResponse` includes `rule_name` via LEFT JOIN; alert evaluator writes human-readable messages (`{name} fired: value={value:.2}`); `GET /v1/alerts/rules/{rule_id}` returns rule detail + 20 recent firings; `AlertRuleDetailPage` at `/alerts/$ruleId`; incident timeline links to rule detail on `alert_fired`/`alert_resolved` events.
```

- [ ] **Step 3: Commit**

```bash
git add docs/agent-context.md docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md
git commit -m "docs: mark P5-S1 complete, update agent context with new endpoint and route"
```

---

## Task 10: Run local CI and open PR

- [ ] **Step 1: Run local CI**

```bash
bash scripts/local-ci.sh --skip-smoke
```

Expected: frontend typecheck, lint, build, and tests all pass; Rust fmt, clippy, unit tests, and Docker build all pass.

If any check fails, fix the failure before continuing. Do not skip or bypass checks.

- [ ] **Step 2: Open the pull request**

```bash
gh pr create \
  --title "feat(p5-s1): incident timeline with source links to alert rule detail" \
  --body "$(cat <<'EOF'
## Summary

- Adds `rule_name` to `IncidentDetailResponse` via LEFT JOIN on `alert_rules` (no schema migration needed)
- Enriches `alert_fired`/`alert_resolved` incident event messages with rule name and value (e.g. `High CPU Alert fired: value=95.30`)
- New `GET /v1/alerts/rules/{rule_id}` endpoint returning rule detail + 20 most recent firings
- New `AlertRuleDetailPage` at `/alerts/$ruleId` showing rule metadata and firing history table
- Incident timeline events link to the rule detail page on `alert_fired` and `alert_resolved` events
- Renders `runbook_url` on the incident detail page when present

Closes P5-S1 in the roadmap.

## Verification

Four new Testcontainers integration tests in `http_api_integration.rs`:
- `get_incident_detail_includes_rule_name`
- `get_incident_detail_rule_name_null_when_no_rule`
- `get_alert_rule_returns_detail_with_firings`
- `get_alert_rule_returns_404_for_wrong_tenant`

Nine new frontend unit tests across `IncidentDetailPage.test.tsx` and `AlertRuleDetailPage.test.tsx`.

Local CI: `bash scripts/local-ci.sh --skip-smoke` — all checks pass.

## Rollback

All backend changes are additive. Revert the three changed Rust files and two new frontend files. No database migration required.

## ADR / Spec Sync

No new architectural decisions. Within existing incident model (ADR-008). `docs/agent-context.md` updated.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Remove `in-progress` label from roadmap item if using GitHub Issues workflow**

```bash
# Only if this was tracked as a GitHub issue:
# gh issue edit <NUMBER> --remove-label "in-progress" --add-label "ready-for-review"
```

---

## Self-Review

**Spec coverage check:**
- ✅ `rule_name` on `IncidentDetailResponse` — Task 2
- ✅ Enriched `alert_fired`/`alert_resolved` messages — Task 3
- ✅ `GET /v1/alerts/rules/{rule_id}` endpoint — Task 4
- ✅ `AlertRuleDetailPage` at `/alerts/$ruleId` — Task 7 (router) + Task 7 (page)
- ✅ Source link on timeline events — Task 6
- ✅ `runbook_url` on `IncidentDetailPage` — Task 6
- ✅ HTTP integration tests — Task 1 (written first) + Task 4 (wired)
- ✅ Frontend unit tests — Tasks 6 and 7
- ✅ `docs/agent-context.md` update — Task 9
- ✅ Roadmap plan update — Task 9

**Type consistency check:**
- `FiringItem` defined in `alerts.rs` (Rust) and `alerts.ts` (TS) — field names match
- `AlertRuleDetailResponse` field names consistent across Rust struct, TS interface, and test fixtures
- `rule_name: Option<String>` (Rust) → `rule_name: string | null` (TS) — correct mapping
- `getAlertRule` used in `AlertRuleDetailPage.tsx` matches function name in `alerts.ts`
- Route param `$ruleId` in router matches `useParams({ from: "/alerts/$ruleId" })` in page
