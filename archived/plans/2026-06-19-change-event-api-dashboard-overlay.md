# Change Event API and Dashboard Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add a generic, non-deploy "change event" concept (config changes, feature-flag toggles, migrations, incidents, etc.) with `POST /v1/events/changes` ingestion, `GET /v1/events/changes` querying, vertical dashed-marker overlays on the one live time-series chart that already supports deployment-marker overlays, and a filterable Change Events explorer page.

**Architecture:** New, independent `change_events` PostgreSQL table (not a replacement for `deployment_markers` — it covers events deployments don't). Follows the exact split already established for deployment markers: creation lives on `ingest-gateway`'s platform port (4321, `auth_middleware`-gated, for CI/feature-flag/config systems to call), listing lives on `query-api` (tenant-scoped read path). The frontend overlay target is `apps/frontend/src/components/ui/time-series-graph.tsx`'s `TimeSeriesGraph`, which already renders `deploymentMarkers` as dashed vertical lines with tooltips inside `ServiceDetailPage.tsx`'s `ResponseTimeGraphSection` — the only chart in the codebase with real marker-overlay rendering today. (The Dashboards grid at `DashboardDetailPage.tsx` renders time-series query results as a plain HTML table via `VisualizationPanel`'s `TimeseriesTable`, not a chart — there's nothing to overlay a line onto there, so it's out of scope for this slice; see the plan's final task for the explicit scope note.) A new top-level `/change-events` explorer page lists events with plain dropdown/date-range filters (not the NLQ filter bar — this is a control-plane PostgreSQL table like deployments, not an NLQ-queryable ClickHouse signal).

**Tech Stack:** Rust (axum, sqlx, PostgreSQL), React 19 + TanStack Router/Query, Tailwind v4, Vitest, Testcontainers.

## Global Constraints

- Follow the existing `deployment_markers` create/list split exactly: create on `ingest-gateway`'s platform router (`services/ingest-gateway/src/http-json/mod.rs`'s `build_platform_router`, behind `auth_middleware`), list on `query-api`'s router (`services/query-api/src/main.rs`).
- No new `.mdl` model: `deployment_markers` itself is fully hand-written in Rust and TypeScript despite 10 other domains being on modelable (per ADR-032 — deployments was never migrated). `change_events` follows the same precedent; do not create `models/change_events.mdl` in this slice.
- Frontend filtering surfaces normally use the shared NLQ query input (`docs/agent-context.md` standing constraint) — this does NOT apply here. Change events, like deployments, are a PostgreSQL control-plane table, not an NLQ-queryable ClickHouse signal. Use plain filters, matching `ServiceDeploymentsTab.tsx`'s pattern.
- Every backend slice touching PostgreSQL needs a Testcontainers integration test or a stated reason one isn't applicable (roadmap §1 rule 5). `query-api`'s `list_change_events` gets one (mirrors `services/query-api/tests/postgres_alerts_integration.rs`, seeding via raw SQL). `ingest-gateway`'s `create_change_event` does NOT get one — same precedent as `create_deployment` (also untested via Testcontainers): the insert is a single trivial parameterized `INSERT`, and the `ingest-gateway` binary's modules (`deployments`, soon `change_events`) are private to `main.rs`, not exposed via `lib.rs` (only `deployment_registry` and `readyz` are `pub mod`), so a Testcontainers test would require restructuring module visibility for two trivial inserts — out of scope. Request-validation is covered by unit tests instead, matching `services/ingest-gateway/src/deployments.rs`'s existing pattern.
- `cargo fmt --all` after every Rust edit, before staging (recurring project requirement).
- Keep the PR(s) small; this plan's tasks are designed to each be a reviewable, independently-committable unit, but ship as one PR per the roadmap's "small standalone slice" sizing unless the reviewer wants it split.

---

## Task 1: PostgreSQL migration + spec update

**Files:**
- Create: `migrations/postgres/032_create_change_events.sql`
- Modify: `spec/18-deployment-markers.md`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `change_events` table, consumed by Tasks 2-4.

- [x] **Step 1: Write the migration**

Create `migrations/postgres/032_create_change_events.sql`:

```sql
CREATE TABLE IF NOT EXISTS change_events (
    change_event_id UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    project_id      UUID        REFERENCES projects(id) ON DELETE SET NULL,
    event_type      TEXT        NOT NULL CHECK (event_type IN ('config_change', 'feature_flag', 'migration', 'incident', 'other')),
    service_name    TEXT,
    environment     TEXT        NOT NULL,
    title           TEXT        NOT NULL,
    description     TEXT,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source          TEXT,
    created_by      TEXT,
    metadata        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_change_events_tenant ON change_events (tenant_id);
CREATE INDEX IF NOT EXISTS idx_change_events_tenant_service ON change_events (tenant_id, service_name);
CREATE INDEX IF NOT EXISTS idx_change_events_occurred_at ON change_events (occurred_at);
```

`service_name` is nullable (unlike `deployment_markers.service_name`) because some change events are tenant/environment-wide rather than service-scoped (e.g. a global feature-flag rollout). `event_type` deliberately excludes `'deploy'` — deployments keep using `deployment_markers`; this table is for everything else, per the roadmap item's "non-deploy change event types" framing.

- [x] **Step 2: Apply it locally and verify**

Run: `docker compose up -d postgres postgres-setup` then check the table exists:
```bash
docker compose exec postgres psql -U postgres -d observable -c "\d change_events"
```
Expected: column list matches the migration above. Tear down after: `docker compose down`.

- [x] **Step 3: Extend the spec**

In `spec/18-deployment-markers.md`, rename the file's H1 conceptually by adding a new `### 18.9` section after `### 18.8 Automation and Tooling` (do not rename the file or the existing sections — only append):

```markdown
### 18.9 Generic Change Events

Not every operationally-relevant change is a deployment. The `change_events` table
(distinct from `deployment_markers`) covers config changes, feature-flag toggles,
schema migrations, and ad-hoc incident annotations — anything teams want correlated
against telemetry that isn't a service deploy.

| Field | Type | Required | Notes |
|---|---|---|---|
| change_event_id | UUID | yes | Unique identifier for the event. |
| tenant_id | UUID | yes | Partitioning key for multi-tenancy. |
| project_id | UUID | no | Project boundary, if applicable. |
| event_type | enum | yes | `config_change`, `feature_flag`, `migration`, `incident`, `other`. |
| service_name | string | no | Omitted for tenant/environment-wide events. |
| environment | string | yes | Target environment. |
| title | string | yes | Short human-readable summary. |
| description | string | no | Longer free-text detail. |
| occurred_at | timestamp | yes | When the change took effect. |
| source | string | no | Originating system, e.g. `launchdarkly`, `ci`, `manual`. |
| created_by | string | no | Identity of the user or system that recorded the event. |
| metadata | JSON | no | Arbitrary key-value context. |

**Ingestion API**: `POST /v1/events/changes` on the ingest-gateway Platform API port
(4321), behind the same auth as `POST /v1/deployments`. Returns `change_event_id`.

**Query API**: `GET /v1/events/changes` on the Query API, filters `service_name`,
`environment`, `event_type`, `start_time`, `end_time`; `limit` default 50, max 200.

**UI Visualization**: change events render as dashed vertical markers alongside
deployment markers on the same service-level time-series chart
(`apps/frontend/src/components/ui/time-series-graph.tsx`), distinguished by a
diamond marker shape (vs. deployments' triangle) and per-`event_type` color, with
a hover tooltip showing `title`, `event_type`, and `source`. A dedicated
`/change-events` explorer page lists and filters events independent of any chart.

**Retention**: follows the same Warm/Cold policy as deployment markers (§18.7).

**RBAC**: same roles as the Deployment API (§18.6) — `Member`/`ProjectAdmin`/`TenantAdmin`
for `POST`, all roles including `Viewer` for `GET`.
```

- [x] **Step 4: Commit**

```bash
git add migrations/postgres/032_create_change_events.sql spec/18-deployment-markers.md
git commit -m "feat(db): add change_events table for non-deploy change tracking"
```

---

## Task 2: query-api list endpoint

**Files:**
- Create: `services/query-api/src/change_events.rs`
- Modify: `services/query-api/src/main.rs:23` (add `mod change_events;` alphabetically after `audit`) and the route chain (add after the `/v1/deployments` line, ~line 108)
- Modify: `services/query-api/src/lib.rs` (add `pub mod change_events;` alphabetically after `audit`)

**Interfaces:**
- Consumes: `change_events` table (Task 1).
- Produces: `list_change_events(pool: &PgPool, tenant_id: Uuid, params: ListChangeEventsParams) -> Result<Vec<ChangeEvent>, sqlx::Error>` — a plain pool-level function (not just the axum handler) so Task 3's integration test can call it directly, matching `alerts::list_alert_rules`'s shape. `ChangeEvent` and `ListChangeEventsParams` are `pub`.

- [x] **Step 1: Write `services/query-api/src/change_events.rs`**

```rust
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ListChangeEventsParams {
    pub service_name: Option<String>,
    pub environment: Option<String>,
    pub event_type: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ChangeEvent {
    pub change_event_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,
    pub event_type: String,
    pub service_name: Option<String>,
    pub environment: String,
    pub title: String,
    pub description: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub source: Option<String>,
    pub created_by: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ListChangeEventsResponse {
    pub items: Vec<ChangeEvent>,
}

pub async fn list_change_events(
    pool: &PgPool,
    tenant_id: Uuid,
    params: ListChangeEventsParams,
) -> Result<Vec<ChangeEvent>, sqlx::Error> {
    let limit = params.limit.unwrap_or(50).min(200);

    sqlx::query_as::<_, ChangeEvent>(
        "SELECT change_event_id, tenant_id, project_id, event_type, service_name, \
         environment, title, description, occurred_at, source, created_by, metadata \
         FROM change_events \
         WHERE tenant_id = $1 \
           AND ($2::TEXT IS NULL OR service_name = $2) \
           AND ($3::TEXT IS NULL OR environment = $3) \
           AND ($4::TEXT IS NULL OR event_type = $4) \
           AND ($5::TIMESTAMPTZ IS NULL OR occurred_at >= $5) \
           AND ($6::TIMESTAMPTZ IS NULL OR occurred_at <= $6) \
         ORDER BY occurred_at DESC \
         LIMIT $7",
    )
    .bind(tenant_id)
    .bind(&params.service_name)
    .bind(&params.environment)
    .bind(&params.event_type)
    .bind(params.start_time)
    .bind(params.end_time)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn handle_list_change_events(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<ListChangeEventsParams>,
) -> Result<Json<ListChangeEventsResponse>, StatusCode> {
    let items = list_change_events(&state.db, ctx.tenant_id, params)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list change events");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ListChangeEventsResponse { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_50() {
        let params = ListChangeEventsParams {
            service_name: None,
            environment: None,
            event_type: None,
            start_time: None,
            end_time: None,
            limit: None,
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 50);
    }

    #[test]
    fn limit_is_capped_at_200() {
        let params = ListChangeEventsParams {
            service_name: None,
            environment: None,
            event_type: None,
            start_time: None,
            end_time: None,
            limit: Some(999),
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 200);
    }

    #[test]
    fn event_serializes_all_fields() {
        let id = Uuid::new_v4();
        let e = ChangeEvent {
            change_event_id: id,
            tenant_id: Uuid::new_v4(),
            project_id: None,
            event_type: "feature_flag".into(),
            service_name: Some("checkout".into()),
            environment: "production".into(),
            title: "Enabled new-checkout-flow".into(),
            description: None,
            occurred_at: Utc::now(),
            source: Some("launchdarkly".into()),
            created_by: None,
            metadata: None,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["event_type"], "feature_flag");
        assert_eq!(v["service_name"], "checkout");
        assert!(v["description"].is_null());
    }
}
```

- [x] **Step 2: Register the module and route**

In `services/query-api/src/main.rs`, add `mod change_events;` to the `mod` list (insert after `mod audit;`, before `mod config;` — alphabetical order matches the existing list).

Also add `pub mod change_events;` to `services/query-api/src/lib.rs` at the same alphabetical position (after `pub mod audit;`, before `pub mod config;`) — `lib.rs` mirrors `main.rs`'s module list exactly and is what Task 3's integration test imports from (confirmed: `lib.rs` currently lists `admin_members, alerts, audit, config, dashboards, deployments, discovery, incidents, llm_adapter, logs, mcp_query, mcp_tools, metrics, middleware, notifications, observability, planner, reliability, schemas, slos, sql_templates, tenants, tokens, traces, usage`, all `pub mod`).

Add the route in the same `Router::new()` chain that has `.route("/v1/deployments", get(deployments::list_deployments))` (~line 108), immediately after it:

```rust
        .route("/v1/events/changes", get(change_events::handle_list_change_events))
```

- [x] **Step 3: Run unit tests**

Run: `cargo test -p query-api change_events`
Expected: 3 tests pass (`default_limit_is_50`, `limit_is_capped_at_200`, `event_serializes_all_fields`).

- [x] **Step 4: Format and commit**

```bash
cargo fmt --all
git add services/query-api/src/change_events.rs services/query-api/src/main.rs services/query-api/src/lib.rs
git commit -m "feat(query-api): add GET /v1/events/changes list endpoint"
```

---

## Task 3: query-api Testcontainers integration test

**Files:**
- Create: `services/query-api/tests/postgres_change_events_integration.rs`

**Interfaces:**
- Consumes: `query_api::change_events::{list_change_events, ListChangeEventsParams}` (Task 2, must be `pub`).
- Produces: nothing other tasks depend on.

- [x] **Step 1: Write the test file**

```rust
use chrono::Utc;
use query_api::change_events::{ListChangeEventsParams, list_change_events};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
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
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

async fn seed_event(
    pool: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
    service_name: Option<&str>,
    environment: &str,
    title: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO change_events (tenant_id, event_type, service_name, environment, title) \
         VALUES ($1, $2, $3, $4, $5) RETURNING change_event_id",
    )
    .bind(tenant_id)
    .bind(event_type)
    .bind(service_name)
    .bind(environment)
    .bind(title)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn empty_params() -> ListChangeEventsParams {
    ListChangeEventsParams {
        service_name: None,
        environment: None,
        event_type: None,
        start_time: None,
        end_time: None,
        limit: None,
    }
}

#[tokio::test]
async fn list_returns_seeded_event() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    let id = seed_event(&pool, tenant, "feature_flag", Some("checkout"), "production", "Enabled new flow").await;

    let items = list_change_events(&pool, tenant, empty_params()).await.unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].change_event_id, id);
    assert_eq!(items[0].event_type, "feature_flag");
    assert_eq!(items[0].service_name, Some("checkout".to_string()));
}

#[tokio::test]
async fn list_does_not_return_other_tenant_events() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    seed_event(&pool, tenant_a, "config_change", None, "staging", "Tenant A change").await;

    let tenant_b_items = list_change_events(&pool, tenant_b, empty_params()).await.unwrap();

    assert!(tenant_b_items.is_empty(), "tenant B must not see tenant A's change events");
}

#[tokio::test]
async fn list_filters_by_service_name() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    seed_event(&pool, tenant, "migration", Some("checkout"), "production", "Checkout schema migration").await;
    seed_event(&pool, tenant, "migration", Some("billing"), "production", "Billing schema migration").await;

    let items = list_change_events(
        &pool,
        tenant,
        ListChangeEventsParams { service_name: Some("checkout".into()), ..empty_params() },
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Checkout schema migration");
}

#[tokio::test]
async fn list_filters_by_event_type() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    seed_event(&pool, tenant, "incident", Some("checkout"), "production", "Incident annotation").await;
    seed_event(&pool, tenant, "config_change", Some("checkout"), "production", "Config change").await;

    let items = list_change_events(
        &pool,
        tenant,
        ListChangeEventsParams { event_type: Some("incident".into()), ..empty_params() },
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Incident annotation");
}

#[tokio::test]
async fn list_respects_limit_cap() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    for i in 0..5 {
        seed_event(&pool, tenant, "other", None, "production", &format!("Event {i}")).await;
    }

    let items = list_change_events(
        &pool,
        tenant,
        ListChangeEventsParams { limit: Some(2), ..empty_params() },
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn list_orders_by_occurred_at_descending() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    let older = seed_event(&pool, tenant, "other", None, "production", "Older").await;
    sqlx::query("UPDATE change_events SET occurred_at = NOW() - INTERVAL '1 hour' WHERE change_event_id = $1")
        .bind(older)
        .execute(&pool)
        .await
        .unwrap();
    seed_event(&pool, tenant, "other", None, "production", "Newer").await;

    let items = list_change_events(&pool, tenant, empty_params()).await.unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Newer");
    assert_eq!(items[1].title, "Older");
}

#[allow(unused)]
fn _use_utc() -> chrono::DateTime<Utc> {
    Utc::now()
}
```

- [x] **Step 2: Confirm `change_events` is reachable from integration tests**

Task 2 Step 2 already added `pub mod change_events;` to `services/query-api/src/lib.rs`, which is what this test imports (`query_api::change_events::{...}`). No further action needed here — this step is just the checkpoint before running the test.

- [x] **Step 3: Run the integration test**

Run: `cargo test -p query-api --test postgres_change_events_integration`
Expected: all 6 tests pass (requires Docker running for Testcontainers).

- [x] **Step 4: Format and commit**

```bash
cargo fmt --all
git add services/query-api/tests/postgres_change_events_integration.rs
git commit -m "test(query-api): add Testcontainers coverage for change_events listing"
```

---

## Task 4: ingest-gateway create endpoint

**Files:**
- Create: `services/ingest-gateway/src/change_events.rs`
- Modify: `services/ingest-gateway/src/main.rs:1-9` (add `mod change_events;`)
- Modify: `services/ingest-gateway/src/http-json/mod.rs:19,97-102` (import and route)

**Interfaces:**
- Consumes: `change_events` table (Task 1), `crate::AppState`, `crate::auth::TenantContext` (existing).
- Produces: `POST /v1/events/changes` on the platform router. Nothing else depends on this within the plan.

- [x] **Step 1: Write `services/ingest-gateway/src/change_events.rs`**

```rust
use crate::AppState;
use crate::auth::TenantContext;
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const ALLOWED_EVENT_TYPES: &[&str] =
    &["config_change", "feature_flag", "migration", "incident", "other"];

#[derive(Deserialize)]
pub struct CreateChangeEventRequest {
    pub event_type: String,
    pub environment: String,
    pub title: String,
    pub service_name: Option<String>,
    pub description: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub created_by: Option<String>,
    pub project_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct CreateChangeEventResponse {
    pub change_event_id: Uuid,
}

pub async fn create_change_event(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateChangeEventRequest>,
) -> Result<(StatusCode, Json<CreateChangeEventResponse>), StatusCode> {
    if !ALLOWED_EVENT_TYPES.contains(&req.event_type.as_str()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if req.environment.trim().is_empty() || req.title.trim().is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let occurred_at = req.occurred_at.unwrap_or_else(Utc::now);

    let change_event_id: Uuid = sqlx::query_scalar(
        "INSERT INTO change_events \
         (tenant_id, project_id, event_type, service_name, environment, title, \
          description, occurred_at, source, created_by, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         RETURNING change_event_id",
    )
    .bind(ctx.tenant_id)
    .bind(req.project_id)
    .bind(&req.event_type)
    .bind(&req.service_name)
    .bind(&req.environment)
    .bind(&req.title)
    .bind(&req.description)
    .bind(occurred_at)
    .bind(&req.source)
    .bind(&req.created_by)
    .bind(&req.metadata)
    .fetch_one(state.db.as_ref())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to create change event");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateChangeEventResponse { change_event_id }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_known_event_types() {
        for t in ALLOWED_EVENT_TYPES {
            assert!(ALLOWED_EVENT_TYPES.contains(t));
        }
    }

    #[test]
    fn rejects_unknown_event_type() {
        assert!(!ALLOWED_EVENT_TYPES.contains(&"deploy"));
        assert!(!ALLOWED_EVENT_TYPES.contains(&"garbage"));
    }

    #[test]
    fn create_response_serializes_change_event_id() {
        let id = Uuid::new_v4();
        let resp = CreateChangeEventResponse { change_event_id: id };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["change_event_id"].as_str().unwrap(), id.to_string());
    }
}
```

- [x] **Step 2: Register the module**

In `services/ingest-gateway/src/main.rs`, add `mod change_events;` to the `mod` list at the top (insert after `mod cardinality;`, before `mod deployment_registry;` — alphabetical order matches the existing list of `auth, cardinality, deployment_registry, deployments, grpc, http_json, queue, readyz`, so it lands between `cardinality` and `deployment_registry`).

- [x] **Step 3: Wire the route**

In `services/ingest-gateway/src/http-json/mod.rs`:
- Change line 19's import from `use crate::{AppState, auth, deployments};` to `use crate::{AppState, auth, change_events, deployments};`
- In `build_platform_router` (the `authenticated` router, ~line 97-102), add after the existing `/v1/deployments` POST route:

```rust
        .route("/v1/events/changes", post(change_events::create_change_event))
```

- [x] **Step 4: Run unit tests**

Run: `cargo test -p ingest-gateway change_events`
Expected: 3 tests pass.

- [x] **Step 5: Format and commit**

```bash
cargo fmt --all
git add services/ingest-gateway/src/change_events.rs services/ingest-gateway/src/main.rs services/ingest-gateway/src/http-json/mod.rs
git commit -m "feat(ingest-gateway): add POST /v1/events/changes ingestion endpoint"
```

---

## Task 5: Frontend API client

**Files:**
- Create: `apps/frontend/src/api/changeEvents.ts`

**Interfaces:**
- Consumes: `GET /v1/events/changes` (Task 2).
- Produces: `ChangeEvent`, `ListChangeEventsResponse`, `ListChangeEventsParams` types and `listChangeEvents(tenantId, params)` function — consumed by Tasks 6 and 7.

- [x] **Step 1: Write `apps/frontend/src/api/changeEvents.ts`**

```typescript
function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type ChangeEventType = "config_change" | "feature_flag" | "migration" | "incident" | "other";

export interface ChangeEvent {
  change_event_id: string;
  tenant_id: string;
  project_id: string | null;
  event_type: ChangeEventType;
  service_name: string | null;
  environment: string;
  title: string;
  description: string | null;
  occurred_at: string;
  source: string | null;
  created_by: string | null;
  metadata: Record<string, unknown> | null;
}

export interface ListChangeEventsResponse {
  items: ChangeEvent[];
}

export interface ListChangeEventsParams {
  service_name?: string;
  environment?: string;
  event_type?: ChangeEventType;
  start_time?: string;
  end_time?: string;
  limit?: number;
}

export async function listChangeEvents(
  tenantId: string,
  params: ListChangeEventsParams = {},
): Promise<ListChangeEventsResponse> {
  const url = new URL("/v1/events/changes", window.location.origin);
  if (params.service_name) url.searchParams.set("service_name", params.service_name);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.event_type) url.searchParams.set("event_type", params.event_type);
  if (params.start_time) url.searchParams.set("start_time", params.start_time);
  if (params.end_time) url.searchParams.set("end_time", params.end_time);
  if (params.limit !== undefined) url.searchParams.set("limit", String(params.limit));

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Change events fetch failed: ${res.status}`);
  return res.json();
}
```

- [x] **Step 2: Commit**

```bash
git add apps/frontend/src/api/changeEvents.ts
git commit -m "feat(frontend): add change-events API client"
```

---

## Task 6: Generalize the chart overlay

**Files:**
- Modify: `apps/frontend/src/components/ui/time-series-graph.tsx`
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx` (the `ResponseTimeGraphSection` function, ~lines 337-411)
- Modify: `apps/frontend/src/components/ui/time-series-graph.test.tsx` (add coverage)

**Interfaces:**
- Consumes: `ChangeEvent`, `listChangeEvents` (Task 5); existing `DeploymentMarker`, `markerColor`, `markerPosition` (unchanged, from `apps/frontend/src/api/deployments.ts` and `apps/frontend/src/components/DeploymentTimeline.tsx`).
- Produces: `TimeSeriesGraph`'s new `changeEvents` prop — nothing later depends on this beyond Task 7's explorer page being a separate, unrelated consumer of the same API client.

- [x] **Step 1: Add a `changeEvents` prop and renderer to `TimeSeriesGraph`**

In `apps/frontend/src/components/ui/time-series-graph.tsx`, add the import and a color helper near the top (after the existing `markerColor`/`markerPosition` import on line 3):

```typescript
import type { ChangeEvent } from "../../api/changeEvents";
```

Add this function near `toY` (after line 68, before `buildPolylinePoints`):

```typescript
export function changeEventColor(eventType: ChangeEvent["event_type"]): string {
  switch (eventType) {
    case "incident":       return "#ef4444";
    case "feature_flag":   return "#8b5cf6";
    case "config_change":  return "#3b82f6";
    case "migration":      return "#f97316";
    default:               return "#9ca3af";
  }
}
```

Add `changeEvents` to `TimeSeriesGraphProps` (after `deploymentMarkers?: DeploymentMarker[];` on line 21):

```typescript
  changeEvents?: ChangeEvent[];
```

Add `changeEvents = []` to the destructured props (after `deploymentMarkers = [],` on line 92):

```typescript
  changeEvents = [],
```

Add a tooltip state for change events near `deployTooltip` (after line 104):

```typescript
  const [changeEventTooltip, setChangeEventTooltip] = useState<ChangeEvent | null>(null);
```

Add a render block for change events in the `<svg>`, immediately after the existing `{deploymentMarkers.map(...)}` block (after line 278, before the hover-line block):

```tsx
          {changeEvents.map((ev) => {
            const x = markerPosition(
              new Date(ev.occurred_at).getTime(),
              rangeStartMs,
              rangeEndMs,
              width,
            );
            const color = changeEventColor(ev.event_type);
            return (
              <g key={ev.change_event_id}>
                <line
                  x1={x} y1={PLOT_TOP}
                  x2={x} y2={plotBottom}
                  stroke={color}
                  strokeWidth={1}
                  strokeDasharray="3 2"
                  opacity={0.7}
                />
                <polygon
                  points={`${x},${PLOT_TOP - 3} ${x + 3},${PLOT_TOP} ${x},${PLOT_TOP + 3} ${x - 3},${PLOT_TOP}`}
                  fill={color}
                  onMouseEnter={() => setChangeEventTooltip(ev)}
                  onMouseLeave={() => setChangeEventTooltip(null)}
                  style={{ cursor: "default" }}
                  aria-label={`${ev.event_type}: ${ev.title}`}
                />
              </g>
            );
          })}
```

Add a tooltip render block immediately after the existing `{deployTooltip && (...)}` block (after line 355):

```tsx
        {changeEventTooltip && (
          <div role="tooltip" className="deployment-timeline-tooltip">
            <div><strong>{changeEventTooltip.title}</strong></div>
            <div>{changeEventTooltip.event_type}</div>
            {changeEventTooltip.source && <div>via {changeEventTooltip.source}</div>}
          </div>
        )}
```

The diamond marker shape (vs. the deployment triangle) and per-`event_type` color (vs. per-deployment-status color) give visual distinction when both overlays are present at once, matching the spec's §18.9 UI Visualization note from Task 1.

- [x] **Step 2: Wire change events into `ServiceDetailPage.tsx`**

In `apps/frontend/src/pages/ServiceDetailPage.tsx`, add the import (alongside the existing `import { listDeployments } from "../api/deployments";` on line 4):

```typescript
import { listChangeEvents } from "../api/changeEvents";
```

In `ResponseTimeGraphSection`, add a second query after the existing `deploymentData` query (after line 370):

```typescript
  const { data: changeEventData } = useQuery({
    queryKey: ["change-events", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      listChangeEvents(tenantId, {
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
    ...liveViewQueryOptions,
  });
```

Pass it to the chart (alongside the existing `deploymentMarkers={deploymentData?.items ?? []}` on line 402):

```typescript
      changeEvents={changeEventData?.items ?? []}
```

This query filters by `service_name`, which excludes tenant/environment-wide change events (`service_name = null`). That's an intentional, documented limitation for this slice — the service-detail chart is service-scoped, so showing only this service's events plus global ones would need an `OR service_name IS NULL` server-side option not in Task 2's filter set. State this explicitly in the PR description rather than silently narrowing; it's an acceptable v1 gap, not a bug, since the `/change-events` explorer page (Task 7) is unfiltered by default and shows everything.

- [x] **Step 3: Add a unit test for the new overlay**

In `apps/frontend/src/components/ui/time-series-graph.test.tsx`, add this `describe` block after the existing `"TimeSeriesGraph brush"` block:

```typescript
describe("TimeSeriesGraph change events", () => {
  it("renders a marker for each change event", () => {
    const { container } = render(
      <TimeSeriesGraph
        series={[]}
        rangeStartMs={1000}
        rangeEndMs={2000}
        ariaLabel="test graph"
        changeEvents={[
          {
            change_event_id: "ce-1",
            tenant_id: "t1",
            project_id: null,
            event_type: "feature_flag",
            service_name: "checkout",
            environment: "production",
            title: "Enabled new flow",
            description: null,
            occurred_at: new Date(1500).toISOString(),
            source: "launchdarkly",
            created_by: null,
            metadata: null,
          },
        ]}
      />
    );
    expect(container.querySelector('[aria-label="feature_flag: Enabled new flow"]')).not.toBeNull();
  });
});
```

- [x] **Step 4: Run frontend tests**

Run: `npm test -- time-series-graph`
Expected: all tests pass, including the new one.

- [x] **Step 5: Run typecheck**

Run: `npm run typecheck`
Expected: no errors.

- [x] **Step 6: Commit**

```bash
git add apps/frontend/src/components/ui/time-series-graph.tsx apps/frontend/src/components/ui/time-series-graph.test.tsx apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "feat(frontend): overlay change events on the service response-time chart"
```

---

## Task 7: Change Events explorer page

**Files:**
- Create: `apps/frontend/src/features/changeEvents/ChangeEventsPage.tsx`
- Create: `apps/frontend/src/features/changeEvents/ChangeEventsPage.test.tsx`
- Modify: `apps/frontend/src/router.ts` (add route)
- Modify: `apps/frontend/src/components/AppShell.tsx` (add nav entry)

**Interfaces:**
- Consumes: `listChangeEvents`, `ChangeEvent`, `ChangeEventType` (Task 5).
- Produces: nothing other tasks depend on (terminal UI task).

- [x] **Step 1: Write `apps/frontend/src/features/changeEvents/ChangeEventsPage.tsx`**

```tsx
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listChangeEvents, type ChangeEvent, type ChangeEventType } from "../../api/changeEvents";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Select, SelectOption } from "../../components/ui/select";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

const EVENT_TYPES: ChangeEventType[] = ["config_change", "feature_flag", "migration", "incident", "other"];

function eventTypeTone(eventType: ChangeEventType): "good" | "bad" | "warn" | "info" {
  switch (eventType) {
    case "incident":      return "bad";
    case "feature_flag":  return "info";
    case "migration":     return "warn";
    default:              return "info";
  }
}

export default function ChangeEventsPage() {
  const { tenantId } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();
  const [serviceFilter, setServiceFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState<ChangeEventType | "all">("all");

  const { data, isLoading, error } = useQuery({
    queryKey: ["change-events-explorer", tenantId, fromMs, toMs, serviceFilter, typeFilter],
    queryFn: () =>
      listChangeEvents(tenantId, {
        service_name: serviceFilter || undefined,
        event_type: typeFilter === "all" ? undefined : typeFilter,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 200,
      }),
  });

  const items = useMemo(() => data?.items ?? [], [data]);

  if (isLoading) return <LoadingState>Loading change events...</LoadingState>;
  if (error) return <div className="signal-empty">Change events could not be loaded.</div>;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
          <h1>Change Events</h1>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <input
          type="search"
          value={serviceFilter}
          onChange={(e) => setServiceFilter(e.target.value)}
          placeholder="Filter by service…"
          aria-label="Filter by service"
          className="min-w-[180px] border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
        />
        <Select
          value={typeFilter}
          onChange={(e) => setTypeFilter(e.target.value as ChangeEventType | "all")}
          aria-label="Filter by event type"
        >
          <SelectOption value="all">All types</SelectOption>
          {EVENT_TYPES.map((t) => (
            <SelectOption key={t} value={t}>{t}</SelectOption>
          ))}
        </Select>
      </div>

      {items.length === 0 ? (
        <EmptyState
          title="No change events found"
          description="No change events match the current filters and time range."
        />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm" aria-label="Change events">
            <thead>
              <tr className="border-b text-left">
                <th className="pb-2 pr-4">Title</th>
                <th className="pb-2 pr-4">Type</th>
                <th className="pb-2 pr-4">Service</th>
                <th className="pb-2 pr-4">Environment</th>
                <th className="pb-2 pr-4">Occurred</th>
                <th className="pb-2 pr-4">Source</th>
              </tr>
            </thead>
            <tbody>
              {items.map((ev: ChangeEvent) => (
                <tr key={ev.change_event_id} className="modern-table-row">
                  <td className="py-2 pr-4">
                    <div className="font-semibold text-[var(--text-strong)]">{ev.title}</div>
                    {ev.description && (
                      <div className="text-xs text-[var(--muted)]">{ev.description}</div>
                    )}
                  </td>
                  <td className="py-2 pr-4">
                    <Badge tone={eventTypeTone(ev.event_type)}>{ev.event_type}</Badge>
                  </td>
                  <td className="py-2 pr-4">{ev.service_name ?? "—"}</td>
                  <td className="py-2 pr-4">{ev.environment}</td>
                  <td className="py-2 pr-4 text-[var(--muted)]">
                    {formatTimestamp(new Date(ev.occurred_at).getTime() * 1_000_000, format)}
                  </td>
                  <td className="py-2 pr-4 text-[var(--muted)]">{ev.source ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
```

- [x] **Step 2: Write `apps/frontend/src/features/changeEvents/ChangeEventsPage.test.tsx`**

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import ChangeEventsPage from "./ChangeEventsPage";
import * as changeEventsApi from "../../api/changeEvents";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "tenant-1" }),
}));
vi.mock("../../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({ fromMs: 0, toMs: 1000 }),
}));

function renderPage() {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <ChangeEventsPage />
    </QueryClientProvider>,
  );
}

describe("ChangeEventsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders an empty state when no events are returned", async () => {
    vi.spyOn(changeEventsApi, "listChangeEvents").mockResolvedValue({ items: [] });
    renderPage();
    await waitFor(() => expect(screen.getByText("No change events found")).toBeInTheDocument());
  });

  it("renders a row for each returned event", async () => {
    vi.spyOn(changeEventsApi, "listChangeEvents").mockResolvedValue({
      items: [
        {
          change_event_id: "ce-1",
          tenant_id: "tenant-1",
          project_id: null,
          event_type: "incident",
          service_name: "checkout",
          environment: "production",
          title: "Payment gateway flapping",
          description: null,
          occurred_at: new Date(500).toISOString(),
          source: "manual",
          created_by: "oncall",
          metadata: null,
        },
      ],
    });
    renderPage();
    await waitFor(() => expect(screen.getByText("Payment gateway flapping")).toBeInTheDocument());
    expect(screen.getByText("checkout")).toBeInTheDocument();
  });
});
```

- [x] **Step 3: Register the route**

In `apps/frontend/src/router.ts`:
- Add the import (alongside other page imports, after `import OnboardingPage from "./pages/OnboardingPage";` on line 31):

```typescript
import ChangeEventsPage from "./features/changeEvents/ChangeEventsPage";
```

- Add the route definition (after `metricsSearchRoute`'s definition, ~line 236):

```typescript
const changeEventsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/change-events",
  component: ChangeEventsPage,
});
```

- Add it to `addChildren([...])` (after `metricsSearchRoute,` ~line 294):

```typescript
    changeEventsRoute,
```

- [x] **Step 4: Add the nav entry**

In `apps/frontend/src/components/AppShell.tsx`, add `"change-events"` to the `signals` children array (after `{ id: "metrics", label: "Metrics", to: "/metrics" },` ~line 50):

```typescript
        { id: "change-events", label: "Change Events", to: "/change-events" },
```

- [x] **Step 5: Run frontend tests and typecheck**

Run: `npm test -- ChangeEventsPage`
Expected: both tests pass.

Run: `npm run typecheck && npm run build`
Expected: no errors.

- [x] **Step 6: Commit**

```bash
git add apps/frontend/src/features/changeEvents apps/frontend/src/router.ts apps/frontend/src/components/AppShell.tsx
git commit -m "feat(frontend): add change events explorer page and nav entry"
```

---

## Task 8: Roadmap and agent-context housekeeping

**Files:**
- Modify: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`
- Modify: `docs/agent-context.md`
- Move: this plan file to `archived/plans/`

**Interfaces:**
- Consumes: completion of Tasks 1-7.
- Produces: nothing (terminal documentation task).

- [x] **Step 1: Check off the roadmap item**

In `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`, change the Tier 1 line:

```markdown
- [x] **Change Event API and Dashboard Overlay** (was P14-S4) — `POST /v1/events/changes`, vertical
```
to
```markdown
- [x] **Change Event API and Dashboard Overlay** (was P14-S4) — `POST /v1/events/changes`, vertical
```

- [x] **Step 2: Add an agent-context.md entry**

In `docs/agent-context.md`, add a new `## Change Events (P14-S4, completed 2026-06-19)` section after the `## Deadman Alert Type` section, summarizing: the new `change_events` table (distinct from `deployment_markers`), the create/list split (ingest-gateway platform port / query-api), the `TimeSeriesGraph` `changeEvents` overlay prop and its known limitation (service-scoped queries exclude tenant-wide events with `service_name = null`), and the new `/change-events` explorer route. Also append `archived/plans/2026-06-19-change-event-api-dashboard-overlay.md` to the "Completed / archived detailed plans" bullet list.

- [x] **Step 3: Archive this plan**

```bash
git mv docs/superpowers/plans/2026-06-19-change-event-api-dashboard-overlay.md archived/plans/2026-06-19-change-event-api-dashboard-overlay.md
```
Mark every checkbox above `[x]` in the archived copy before committing.

- [x] **Step 4: Final verification**

Run: `bash scripts/local-ci.sh`
Expected: passes end-to-end (Rust fmt/clippy/tests, frontend typecheck/test/build, smoke test).

- [x] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md docs/agent-context.md archived/plans/2026-06-19-change-event-api-dashboard-overlay.md
git commit -m "docs: close out change-event API and dashboard overlay roadmap item"
```
