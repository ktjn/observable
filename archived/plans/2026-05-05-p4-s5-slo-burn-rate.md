# P4-S5 SLO Burn-Rate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one service-level SLO definition workflow and one multi-window burn-rate alert path.

**Architecture:** Keep the first SLO slice service-scoped and span-backed: query-api owns SLO definition CRUD and state readback in PostgreSQL, while alert-evaluator reuses the existing `alert_rules` dispatch loop for `alert_type = 'slo_burn_rate'`. Burn-rate evaluation reads ClickHouse spans for tenant, service, environment, and two windows, writes normal `alert_firings`, and exposes enough API/frontend state for operators to see SLO health.

**Tech Stack:** Rust, Axum, SQLx/PostgreSQL, ClickHouse, Testcontainers, React 19, TanStack Query, existing UI primitives, MSW/fetch-test patterns, `tower::ServiceExt::oneshot`.

---

## File Structure

- Create: `migrations/postgres/018_create_slo_definitions.sql` for service-level SLO definitions and one seeded dev SLO.
- Create: `services/query-api/src/slos.rs` for SLO request/response types, create/list helpers, paired burn-rate rule creation, firing-state readback, and HTTP handlers.
- Modify: `services/query-api/src/main.rs` to register `/v1/slos` routes.
- Modify: `services/query-api/src/lib.rs` to export `slos` for integration tests.
- Modify: `services/query-api/tests/postgres_alerts_integration.rs` or create `services/query-api/tests/postgres_slos_integration.rs` for PostgreSQL SLO definition coverage.
- Modify: `services/query-api/tests/http_api_integration.rs` for handler-path coverage via `tower::ServiceExt::oneshot`.
- Modify: `services/alert-evaluator/src/evaluator.rs` to add burn-rate condition parsing, window math, ClickHouse span queries, and dispatch from threshold + SLO rules.
- Create: `services/alert-evaluator/tests/slo_burn_rate_integration.rs` for PostgreSQL + ClickHouse Testcontainers coverage.
- Create: `apps/frontend/src/api/slos.ts` for typed SLO API calls.
- Modify: `apps/frontend/src/features/alerts/AlertsPage.tsx` to show SLO health and create one availability SLO.
- Modify: `apps/frontend/src/App.test.tsx` or create `apps/frontend/src/features/alerts/AlertsPage.test.tsx` for the frontend workflow.
- Modify: `apps/frontend/e2e/accessibility.spec.ts` only if the SLO panel introduces a materially new major view state.
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` after implementation to mark P4-S5 complete.
- Modify: `docs/agent-context.md` after implementation if the next active detailed-plan pointer changes.

## Slice Contract

Source spec: `spec/07-alerting-slo.md §12.1-§12.3`, `spec/14-domain-model.md §5`, `spec/10-process.md §17 Phase 4`, ADR-002, ADR-003, ADR-004, ADR-007, ADR-013.
Phase: 4.
Parent phase item: Add SLO definitions, burn-rate calculations, and burn-rate alerts.
Acceptance target: one tenant can create or read a service availability SLO, alert-evaluator can evaluate a fast+slow burn-rate condition from real ClickHouse span data, and the Alerts & SLOs UI shows SLO health state.
User/operator outcome: an operator can see whether a service is within its error budget and receive an active alert firing when both burn-rate windows exceed configured thresholds.
Files or modules expected to change: query-api, alert-evaluator, PostgreSQL migrations, frontend alerts feature, focused integration tests, active plan docs.
Out of scope: latency/throughput SLOs, synthetic-check SLOs, incident creation, notification routing, alert inhibition, Prometheus rule import, query federation, and customer-facing SLA/legal reporting.
Verification: PostgreSQL Testcontainers for SLO definition persistence, ClickHouse + PostgreSQL Testcontainers for burn-rate evaluation, HTTP integration tests for new handler paths, frontend tests for list/create/health states, accessibility check if the view changes materially, and `bash scripts/local-ci.sh` before push.
Baseline: run `cargo test -p query-api postgres_alerts_integration --test postgres_alerts_integration`, `cargo test -p alert-evaluator`, and `npm test -- Alerts` in `apps/frontend` before implementation to capture current alert behavior.
New errors introduced: none.
Telemetry impact: burn-rate evaluator emits structured logs with `rule_id`, `slo_id`, `tenant_id`, `service_name`, `environment`, `fast_burn_rate`, `slow_burn_rate`, thresholds, and firing outcome.
Auth/tenancy impact: every SLO API call uses `TenantContext`; every SQL query filters by `tenant_id`; cross-tenant reads and mutations return no data or 404.
Data retention or migration impact: one additive PostgreSQL table; no ClickHouse schema change; burn-rate accuracy is bounded by hot span retention.
Rollback path: remove or disable SLO burn-rate alert rules; reverting the migration removes SLO definitions and seeded rule only if no later slice depends on them.
ADR/spec sync: no ADR change expected because this implements existing SLO and alerting scope; update specs only if the implementation changes SLO fields, burn-rate formula, or alert state semantics.
Checkpoint question: are error budget semantics now reliable enough for customer use?
Next smallest slice: P5-S2 notification routing, so active burn-rate alerts reach an operator channel.

---

### Task 1: Add SLO Definition Persistence And Query-API Models

**Files:**
- Create: `migrations/postgres/018_create_slo_definitions.sql`
- Create: `services/query-api/src/slos.rs`
- Modify: `services/query-api/src/lib.rs`
- Test: `services/query-api/tests/postgres_slos_integration.rs`

- [ ] **Step 1: Write the migration**

Create `migrations/postgres/018_create_slo_definitions.sql`:

```sql
CREATE TABLE IF NOT EXISTS slo_definitions (
    slo_id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                UUID        NOT NULL,
    service_name             TEXT        NOT NULL,
    environment              TEXT        NOT NULL,
    sli_type                 TEXT        NOT NULL CHECK (sli_type IN ('availability')),
    target                   DOUBLE PRECISION NOT NULL CHECK (target > 0 AND target < 1),
    window_days              INTEGER     NOT NULL CHECK (window_days > 0),
    burn_rate_fast_threshold DOUBLE PRECISION NOT NULL CHECK (burn_rate_fast_threshold > 0),
    burn_rate_slow_threshold DOUBLE PRECISION NOT NULL CHECK (burn_rate_slow_threshold > 0),
    description              TEXT        NOT NULL DEFAULT '',
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, service_name, environment, sli_type)
);

CREATE INDEX IF NOT EXISTS slo_definitions_tenant_service_idx
    ON slo_definitions (tenant_id, service_name, environment);

INSERT INTO slo_definitions (
    slo_id, tenant_id, service_name, environment, sli_type, target, window_days,
    burn_rate_fast_threshold, burn_rate_slow_threshold, description
) VALUES (
    '20000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    'checkout',
    'prod',
    'availability',
    0.999,
    30,
    14.4,
    1.0,
    'Checkout availability SLO'
) ON CONFLICT DO NOTHING;

INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) VALUES (
    '20000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'Checkout availability burn rate',
    'slo_burn_rate',
    'critical',
    '{"slo_id":"20000000-0000-0000-0000-000000000001","fast_window_minutes":60,"slow_window_minutes":360}'
) ON CONFLICT DO NOTHING;
```

- [ ] **Step 2: Add failing persistence tests**

Create `services/query-api/tests/postgres_slos_integration.rs` using the same migration helper shape as `postgres_alerts_integration.rs`. Add tests:

```rust
#[tokio::test]
async fn list_slos_returns_seeded_dev_slo() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let slos = query_api::slos::list_slos(&pool, tenant).await.unwrap();

    assert!(slos.iter().any(|s| s.service_name == "checkout"));
    let checkout = slos.iter().find(|s| s.service_name == "checkout").unwrap();
    assert_eq!(checkout.environment, "prod");
    assert_eq!(checkout.sli_type, "availability");
    assert!((checkout.target - 0.999).abs() < f64::EPSILON);
}
```

Expected: FAIL because `query_api::slos` does not exist yet.

- [ ] **Step 3: Implement the minimal SLO module**

Create `services/query-api/src/slos.rs` with:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SloDefinitionItem {
    pub slo_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub sli_type: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: String,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSloRequest {
    pub service_name: String,
    pub environment: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: Option<String>,
}

pub async fn list_slos(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<SloDefinitionItem>, sqlx::Error> {
    sqlx::query_as(
        "SELECT slo_id, service_name, environment, sli_type, target, window_days, \
         burn_rate_fast_threshold, burn_rate_slow_threshold, description, \
         EXISTS( \
             SELECT 1 FROM alert_rules ar \
             JOIN alert_firings af ON af.rule_id = ar.rule_id \
             WHERE ar.tenant_id = slo_definitions.tenant_id \
               AND ar.alert_type = 'slo_burn_rate' \
               AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
               AND af.state = 'active' \
         ) AS firing, \
         (SELECT MAX(af.occurred_at) FROM alert_rules ar \
          JOIN alert_firings af ON af.rule_id = ar.rule_id \
          WHERE ar.tenant_id = slo_definitions.tenant_id \
            AND ar.alert_type = 'slo_burn_rate' \
            AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
            AND af.state = 'active') AS last_fired_at, \
         created_at, updated_at \
         FROM slo_definitions WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await
}
```

Export `pub mod slos;` from `services/query-api/src/lib.rs`.

- [ ] **Step 4: Run the focused test**

Run:

```bash
cargo test -p query-api --test postgres_slos_integration -- --nocapture
```

Expected: PASS for seeded SLO readback.

- [ ] **Step 5: Commit**

Run:

```bash
git add migrations/postgres/018_create_slo_definitions.sql services/query-api/src/slos.rs services/query-api/src/lib.rs services/query-api/tests/postgres_slos_integration.rs
git commit -m "Add SLO definition persistence"
```

---

### Task 2: Add SLO HTTP API And Handler Integration Tests

**Files:**
- Modify: `services/query-api/src/slos.rs`
- Modify: `services/query-api/src/main.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Add failing HTTP integration tests**

In `services/query-api/tests/http_api_integration.rs`, add handler-path tests for:

```rust
#[tokio::test]
async fn post_slo_creates_tenant_scoped_definition() {
    // Build the real app with Postgres + ClickHouse fixtures.
    // POST /v1/slos with X-Tenant-ID.
    // Assert 201 and response.service_name == "checkout".
}

#[tokio::test]
async fn get_slos_does_not_return_other_tenant_definitions() {
    // Insert or create tenant A SLO.
    // GET /v1/slos as tenant B.
    // Assert 200 and empty items.
}
```

Expected: FAIL because routes and handlers are missing.

- [ ] **Step 2: Implement validation and create helper**

Extend `services/query-api/src/slos.rs`:

```rust
#[derive(Debug)]
pub enum CreateSloError {
    InvalidInput(String),
    Db(sqlx::Error),
}

pub fn validate_create_slo(req: &CreateSloRequest) -> Result<(), String> {
    if req.service_name.trim().is_empty() {
        return Err("service_name is required".into());
    }
    if req.environment.trim().is_empty() {
        return Err("environment is required".into());
    }
    if !(req.target > 0.0 && req.target < 1.0) {
        return Err("target must be between 0 and 1".into());
    }
    if req.window_days <= 0 {
        return Err("window_days must be positive".into());
    }
    if req.burn_rate_fast_threshold <= 0.0 || req.burn_rate_slow_threshold <= 0.0 {
        return Err("burn rate thresholds must be positive".into());
    }
    Ok(())
}
```

Add `create_slo()` that:

1. Starts a PostgreSQL transaction.
2. Inserts `sli_type = 'availability'`, binds `tenant_id`, and returns the inserted `slo_id`.
3. Inserts a paired `alert_rules.alert_type = 'slo_burn_rate'` rule with:

```rust
let condition = serde_json::json!({
    "slo_id": slo_id,
    "fast_window_minutes": 60,
    "slow_window_minutes": 360,
});
```

4. Commits the transaction.
5. Calls `list_slos()` and returns the newly inserted SLO item so the response includes `firing` and `last_fired_at`.

- [ ] **Step 3: Implement handlers**

Add:

```rust
#[derive(Serialize)]
pub struct SloListResponse {
    pub items: Vec<SloDefinitionItem>,
}

pub async fn handle_list_slos(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<SloListResponse>, StatusCode> {
    let items = list_slos(&state.db, ctx.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list SLO definitions");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SloListResponse { items }))
}

pub async fn handle_create_slo(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateSloRequest>,
) -> Result<(StatusCode, Json<SloDefinitionItem>), StatusCode> {
    match create_slo(&state.db, ctx.tenant_id, &req).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateSloError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid SLO input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateSloError::Db(e)) => {
            tracing::error!(error = %e, "failed to create SLO definition");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

Use existing imports from `alerts.rs`: `TenantContext`, `AppState`, `axum::{extract::State, Extension, Json}`, and `StatusCode`.

- [ ] **Step 4: Register routes**

In `services/query-api/src/main.rs`, add:

```rust
mod slos;
```

and inside the authenticated router:

```rust
.route("/v1/slos", get(slos::handle_list_slos))
.route("/v1/slos", post(slos::handle_create_slo))
```

- [ ] **Step 5: Run focused checks**

Run:

```bash
cargo test -p query-api --test postgres_slos_integration -- --nocapture
cargo test -p query-api --test http_api_integration post_slo -- --nocapture
cargo test -p query-api --test http_api_integration get_slos -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add services/query-api/src/slos.rs services/query-api/src/main.rs services/query-api/tests/http_api_integration.rs
git commit -m "Add tenant-scoped SLO API"
```

---

### Task 3: Add Burn-Rate Evaluation In Alert Evaluator

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`
- Create: `services/alert-evaluator/tests/slo_burn_rate_integration.rs`

- [ ] **Step 1: Add unit tests for burn-rate math**

Add tests in `services/alert-evaluator/src/evaluator.rs`:

```rust
#[test]
fn burn_rate_uses_error_rate_over_error_budget() {
    let burn = calculate_burn_rate(10, 1_000, 0.999, 60, 30);
    assert!((burn - 10.0).abs() < 0.001);
}

#[test]
fn multi_window_requires_fast_and_slow_to_fire() {
    let cond = SloBurnRateCondition {
        slo_id: Uuid::nil(),
        fast_window_minutes: 60,
        slow_window_minutes: 360,
    };
    assert_eq!(evaluate_burn_rate(15.0, 2.0, 14.4, 1.0, &cond), EvalResult::Firing);
    assert_eq!(evaluate_burn_rate(15.0, 0.5, 14.4, 1.0, &cond), EvalResult::Ok);
}
```

Expected: FAIL until the burn-rate types/functions exist.

- [ ] **Step 2: Add condition and query row types**

Add:

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct SloBurnRateCondition {
    pub slo_id: Uuid,
    pub fast_window_minutes: u64,
    pub slow_window_minutes: u64,
}

#[derive(sqlx::FromRow)]
pub struct SloRuleRow {
    pub rule_id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub condition: serde_json::Value,
}

#[derive(sqlx::FromRow)]
pub struct SloDefinitionRow {
    pub slo_id: Uuid,
    pub tenant_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
}

#[derive(clickhouse::Row, serde::Deserialize)]
pub struct SpanCountRow {
    pub total_count: u64,
    pub bad_count: u64,
}
```

- [ ] **Step 3: Implement pure burn-rate helpers**

Add:

```rust
pub fn calculate_burn_rate(
    bad_events: u64,
    total_events: u64,
    target: f64,
    _window_minutes: u64,
    _slo_window_days: i32,
) -> f64 {
    if bad_events == 0 || total_events == 0 {
        return 0.0;
    }
    let error_budget_fraction = 1.0 - target;
    let observed_error_fraction = bad_events as f64 / total_events as f64;
    observed_error_fraction / error_budget_fraction
}

pub fn evaluate_burn_rate(
    fast_burn_rate: f64,
    slow_burn_rate: f64,
    fast_threshold: f64,
    slow_threshold: f64,
    _condition: &SloBurnRateCondition,
) -> EvalResult {
    if fast_burn_rate >= fast_threshold && slow_burn_rate >= slow_threshold {
        EvalResult::Firing
    } else {
        EvalResult::Ok
    }
}
```

- [ ] **Step 4: Add integration test with real PostgreSQL and ClickHouse**

Create `services/alert-evaluator/tests/slo_burn_rate_integration.rs` that:

1. Starts PostgreSQL and applies `migrations/postgres`.
2. Starts ClickHouse `24.3` and applies `migrations/clickhouse`.
3. Inserts one SLO definition and one `alert_rules.alert_type = 'slo_burn_rate'` rule.
4. Inserts spans for the same tenant/service/environment into `observable.spans`, with enough `status_code = 'ERROR'` rows in both the 1h and 6h windows to exceed thresholds.
5. Calls `eval_slo_burn_rate_rules(&pool, &ch).await`.
6. Asserts one `alert_firings` row exists for the rule with `state = 'active'`.

Expected test name:

```rust
#[tokio::test]
async fn slo_burn_rate_rule_fires_when_fast_and_slow_windows_burn() {
    // fixture, inserts, eval, assertion
}
```

- [ ] **Step 5: Implement evaluator query path**

Add `eval_slo_burn_rate_rules()` that:

1. Reads unsilenced `alert_type = 'slo_burn_rate'` rules.
2. Parses `SloBurnRateCondition`.
3. Loads the tenant-scoped `slo_definitions` row by `slo_id`.
4. Queries ClickHouse twice with this SQL shape:

```sql
SELECT
  count() AS total_count,
  countIf(status_code = 'ERROR') AS bad_count
FROM observable.spans
WHERE tenant_id = ?
  AND service_name = ?
  AND environment = ?
  AND start_time_unix_nano >= ?
```

5. Computes fast and slow burn rates.
6. Inserts `alert_firings` with `value = fast_burn_rate` only when both windows fire.
7. Logs skipped malformed rules and missing SLOs without failing the full cycle.

- [ ] **Step 6: Dispatch both evaluator types**

Rename the worker log from threshold-only to alert evaluation and make the cycle call:

```rust
pub async fn eval_alert_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()> {
    eval_threshold_rules(db, ch).await?;
    eval_slo_burn_rate_rules(db, ch).await?;
    Ok(())
}
```

`start_eval_worker()` must call `eval_alert_rules()`.

- [ ] **Step 7: Run focused evaluator checks**

Run:

```bash
cargo test -p alert-evaluator burn_rate
cargo test -p alert-evaluator --test slo_burn_rate_integration -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit**

Run:

```bash
git add services/alert-evaluator/src/evaluator.rs services/alert-evaluator/tests/slo_burn_rate_integration.rs
git commit -m "Add SLO burn-rate evaluation"
```

---

### Task 4: Surface SLO Health In Alerts UI

**Files:**
- Create: `apps/frontend/src/api/slos.ts`
- Modify: `apps/frontend/src/api/alerts.ts`
- Modify: `apps/frontend/src/features/alerts/AlertsPage.tsx`
- Modify: `apps/frontend/src/App.test.tsx` or create `apps/frontend/src/features/alerts/AlertsPage.test.tsx`

- [ ] **Step 1: Add frontend API types**

Create `apps/frontend/src/api/slos.ts`:

```typescript
function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export interface SloDefinitionItem {
  slo_id: string;
  service_name: string;
  environment: string;
  sli_type: "availability";
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description: string;
  firing: boolean;
  last_fired_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface SloListResponse {
  items: SloDefinitionItem[];
}

export interface CreateSloRequest {
  service_name: string;
  environment: string;
  target: number;
  window_days: number;
  burn_rate_fast_threshold: number;
  burn_rate_slow_threshold: number;
  description?: string;
}

export async function listSlos(tenantId: string): Promise<SloListResponse> {
  const res = await fetch("/v1/slos", { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list SLOs: ${res.status}`);
  return res.json();
}

export async function createSlo(
  tenantId: string,
  req: CreateSloRequest,
): Promise<SloDefinitionItem> {
  const res = await fetch("/v1/slos", {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create SLO: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 2: Add failing UI tests**

Add tests that:

```typescript
test("alerts page renders SLO health cards", async () => {
  // mock /v1/alerts/rules and /v1/slos
  // navigate to /alerts
  // expect "Checkout availability" and "99.9%" to be visible
});

test("alerts page creates an availability SLO", async () => {
  // open SLO create panel
  // fill service_name, environment, target
  // submit
  // assert POST /v1/slos body includes target: 0.999
});
```

Expected: FAIL until `AlertsPage` calls the SLO API.

- [ ] **Step 3: Extend `AlertsPage`**

Use existing `Panel`, `MetricCard`, `Input`, `Button`, and `Toolbar` components. Add a second query:

```typescript
const { data: sloData, isLoading: isLoadingSlos } = useQuery({
  queryKey: ["slos", tenantId],
  queryFn: () => listSlos(tenantId),
});
```

Render a SLO section below the alert summary with service, environment, target percentage, window, thresholds, and `firing`/OK badge. Keep text compact; do not introduce a new page or route.

- [ ] **Step 4: Add create SLO form**

Add a separate "New SLO" action that posts:

```typescript
{
  service_name: formSloService,
  environment: formSloEnvironment,
  target,
  window_days: 30,
  burn_rate_fast_threshold: 14.4,
  burn_rate_slow_threshold: 1.0,
  description: formSloDescription
}
```

Invalidate `["slos", tenantId]` on success.

- [ ] **Step 5: Run frontend tests**

Run:

```bash
cd apps/frontend
npm test -- Alerts
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add apps/frontend/src/api/slos.ts apps/frontend/src/features/alerts/AlertsPage.tsx apps/frontend/src/App.test.tsx
git commit -m "Show SLO health in alerts UI"
```

---

### Task 5: Verify And Update Planning State

**Files:**
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`
- Modify: `docs/agent-context.md`

- [ ] **Step 1: Run focused checks**

Run:

```bash
cargo test -p query-api --test postgres_slos_integration -- --nocapture
cargo test -p query-api --test http_api_integration post_slo -- --nocapture
cargo test -p alert-evaluator burn_rate
cargo test -p alert-evaluator --test slo_burn_rate_integration -- --nocapture
cd apps/frontend && npm test -- Alerts
```

Expected: PASS.

- [ ] **Step 2: Run mandatory code gate**

Run from repo root:

```bash
bash scripts/local-ci.sh
```

Expected: PASS. If an environment limitation requires a documented skip flag, record the exact skipped stage and replacement signal in the PR body.

- [ ] **Step 3: Update active roadmap**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, mark P4-S5 complete with:

```markdown
- [x] **P4-S5: Add SLO definition and one burn-rate alert**
  - Outcome: one service-level availability SLO can be created/read, alert-evaluator evaluates an `slo_burn_rate` rule using fast and slow ClickHouse span windows, and the Alerts & SLOs UI shows SLO health.
  - Checkpoint: are error budget semantics now reliable enough for customer use? Answer: yes for service-level availability SLOs backed by hot span data and multi-window burn-rate alerts. Latency, synthetic, incident, and notification behavior remain follow-up slices.
```

- [ ] **Step 4: Update active detailed plan pointer**

Set `docs/agent-context.md` to the next detailed plan state:

```markdown
- Active detailed implementation plan: none; write the next plan for **P5-S2** (notification routing integration) based on `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` before starting.
```

- [ ] **Step 5: Run documentation hygiene**

Run:

```bash
git diff --check
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md docs/agent-context.md
git commit -m "Update plan after SLO burn-rate slice"
```

---

## Verification Plan

Required for implementation PRs:

```bash
cargo test -p query-api --test postgres_slos_integration -- --nocapture
cargo test -p query-api --test http_api_integration post_slo -- --nocapture
cargo test -p alert-evaluator burn_rate
cargo test -p alert-evaluator --test slo_burn_rate_integration -- --nocapture
cd apps/frontend && npm test -- Alerts
bash scripts/local-ci.sh
```

Documentation-only edits to this plan are exempt from `bash scripts/local-ci.sh`, but must run:

```bash
git diff --check
```

## ADR/Spec Synchronization

This plan implements existing roadmap and domain-model scope from `spec/07-alerting-slo.md`, `spec/14-domain-model.md`, and `spec/10-process.md`. No ADR update is expected if implementation keeps the slice to service-level availability SLOs, uses existing PostgreSQL control-plane state, reads hot ClickHouse spans, preserves existing alert firing semantics, and adds Testcontainers coverage for PostgreSQL and ClickHouse. Update ADR/spec files in the same PR if implementation changes SLO fields, burn-rate formula, alert state semantics, tenant/auth model, or storage/query architecture.
