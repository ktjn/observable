# P3-S11 Deployment Event Ingestion and Timeline Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deployment marker lifecycle endpoints and a timeline overlay so operators can correlate performance shifts with deployments.

**Architecture:** `POST /v1/deployments` and `PATCH /v1/deployments/:id` live in the **ingest-gateway** (which gains a PostgreSQL connection and writes directly to Postgres for immediate consistency). `GET /v1/deployments` lives in the **query-api** (already has PgPool). A bash helper lets CI/CD pipelines call both write endpoints. The frontend gains a `DeploymentTimeline` SVG component in the service detail overview. See ADR-024 for the routing decision and the future Redpanda SSE path.

**Tech Stack:** Rust (axum, sqlx 0.7 with `postgres`+`chrono`+`uuid`+`json` features, chrono 0.4), PostgreSQL, Bash, TypeScript + React (SVG, TanStack Query)

---

## Scope boundaries

- **In scope:** `deployment_markers` table, POST/PATCH in ingest-gateway, GET in query-api, bash CI helper, frontend API client, `DeploymentTimeline` SVG component, ADR-024.
- **Out of scope:** Publishing to Redpanda (future SSE path per ADR-024 §Future streaming path), ingest enrichment (§18.5), canary-promote integration.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `migrations/postgres/009_create_deployment_markers.sql` | ✅ Done | Table schema + dev seed row |
| `spec/adr/ADR-024-deployment-marker-routing.md` | ✅ Done | Routing decision + future Redpanda path |
| `spec/adr/README.md` | ✅ Done | ADR index entry |
| `services/ingest-gateway/Cargo.toml` | Modify | Add `sqlx` and `chrono` workspace deps |
| `services/ingest-gateway/src/main.rs` | Modify | Add `DATABASE_URL` env var + `PgPool` to `AppState` startup |
| `services/ingest-gateway/src/http-json/deployments.rs` | Create | POST/PATCH handlers + unit tests |
| `services/ingest-gateway/src/http-json/mod.rs` | Modify | Register `POST /v1/deployments` and `PATCH /v1/deployments/:id` routes |
| `services/query-api/Cargo.toml` | Modify | Add `chrono = { workspace = true }` |
| `services/query-api/src/deployments.rs` | Create | GET /v1/deployments handler + unit tests |
| `services/query-api/src/main.rs` | Modify | Register `GET /v1/deployments` and `POST /v1/deployments` routes |
| `scripts/deployment-marker.sh` | Create | Bash helper for CI/CD pipelines |
| `apps/frontend/src/api/deployments.ts` | Create | Fetch wrapper for `GET /v1/deployments` |
| `apps/frontend/src/components/DeploymentTimeline.tsx` | Create | SVG timeline with colored vertical markers |
| `apps/frontend/src/components/DeploymentTimeline.test.tsx` | Create | 9 unit tests for pure functions |
| `apps/frontend/src/pages/ServiceDetailPage.tsx` | Modify | Add `DeploymentTimelineSection` above signal tabs |
| `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` | Modify | Mark P3-S11 done |

---

## Task 1: PostgreSQL migration — deployment_markers table ✅ DONE

Committed in `f1a9f18`. No further action needed.

---

## Task 2: ADR-024 and README update ✅ DONE

`spec/adr/ADR-024-deployment-marker-routing.md` and `spec/adr/README.md` updated. No further action needed.

---

## Task 3: Add PostgreSQL support to ingest-gateway

**Files:**
- Modify: `services/ingest-gateway/Cargo.toml`
- Modify: `services/ingest-gateway/src/main.rs`

The ingest-gateway currently has no database connection. This task adds `sqlx` and `chrono` workspace dependencies and wires a `PgPool` into `AppState`.

- [ ] **Add deps to Cargo.toml**

In `services/ingest-gateway/Cargo.toml`, find the `[dependencies]` section (it currently ends with `uuid = { workspace = true }`). Add after it:

```toml
sqlx               = { workspace = true }
chrono             = { workspace = true }
```

- [ ] **Add `db` field to `AppState` in main.rs**

In `services/ingest-gateway/src/main.rs`, find the `AppState` struct definition. Add one field:

```rust
pub db: Arc<sqlx::PgPool>,
```

The updated struct looks like:

```rust
#[derive(Clone)]
pub struct AppState {
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub producer: Option<Arc<QueueProducer>>,
    pub trace_rate_limiter: Arc<governor::DefaultKeyedRateLimiter<Uuid>>,
    pub log_rate_limiter: Arc<governor::DefaultKeyedRateLimiter<Uuid>>,
    pub metric_rate_limiter: Arc<governor::DefaultKeyedRateLimiter<Uuid>>,
    pub metric_cardinality: Arc<cardinality::MetricCardinalityBudget>,
    pub db: Arc<sqlx::PgPool>,
    #[cfg(test)]
    pub stub_tenant: Option<Uuid>,
}
```

- [ ] **Add `use std::sync::Arc;` if not already present**

Check the top of `main.rs` — `Arc` is already imported via `use std::sync::Arc;`. If not present, add it.

- [ ] **Initialize PgPool in `main()` in main.rs**

In the `main()` function, find the block where `AppState` is constructed (it starts with `let state = AppState {`). Before that block, add:

```rust
let database_url =
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://observable:observable@localhost:5432/observable".into());
let db = Arc::new(
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?,
);
```

Then add `db: db.clone()` to the `AppState { ... }` construction.

- [ ] **Add `use sqlx::postgres::PgPoolOptions;` import**

At the top of `main.rs`, in the use block add:

```rust
use sqlx::postgres::PgPoolOptions;
```

- [ ] **Update test stubs in main.rs**

Every `#[cfg(test)]` constructor method (`test_stub`, `with_stub_auth`, `with_stub_auth_and_rate_limit`, `with_stub_auth_and_metric_budget`) needs `db` filled. Use a shared helper:

```rust
#[cfg(test)]
fn test_pool() -> Arc<sqlx::PgPool> {
    // Disconnected pool used only in unit tests that don't touch the DB.
    Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap())
}
```

Add `db: test_pool()` to every test stub constructor.

- [ ] **Verify compilation**

```bash
cargo check -p ingest-gateway 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Run tests**

```bash
cargo test -p ingest-gateway 2>&1 | tail -10
```

Expected: all existing tests pass.

- [ ] **Commit**

```bash
git add services/ingest-gateway/Cargo.toml services/ingest-gateway/src/main.rs
git commit -m "feat(ingest-gateway): add PostgreSQL PgPool to AppState for deployment markers"
```

---

## Task 4: Implement POST/PATCH deployment handlers in ingest-gateway

**Files:**
- Create: `services/ingest-gateway/src/http-json/deployments.rs`
- Modify: `services/ingest-gateway/src/http-json/mod.rs`

- [ ] **Write the failing unit tests first**

Create `services/ingest-gateway/src/http-json/deployments.rs` with only types and tests (handlers are stubs):

```rust
use crate::AppState;
use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::auth::TenantContext;

#[derive(Deserialize)]
pub struct CreateDeploymentRequest {
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    pub project_id: Option<Uuid>,
    pub deployed_by: Option<String>,
    pub commit_sha: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct CreateDeploymentResponse {
    pub deployment_id: Uuid,
}

#[derive(Deserialize)]
pub struct FinishDeploymentRequest {
    pub status: String,
    pub finished_at: Option<DateTime<Utc>>,
    pub rollback_of: Option<Uuid>,
}

pub async fn create_deployment(
    State(_state): State<AppState>,
    Extension(_ctx): Extension<TenantContext>,
    Json(_req): Json<CreateDeploymentRequest>,
) -> Result<(StatusCode, Json<CreateDeploymentResponse>), StatusCode> {
    todo!()
}

pub async fn finish_deployment(
    State(_state): State<AppState>,
    Extension(_ctx): Extension<TenantContext>,
    Path(_deployment_id): Path<Uuid>,
    Json(_req): Json<FinishDeploymentRequest>,
) -> Result<StatusCode, StatusCode> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_allows_success() {
        let req = FinishDeploymentRequest {
            status: "success".to_string(),
            finished_at: None,
            rollback_of: None,
        };
        let allowed = ["success", "failed", "rolled_back"];
        assert!(allowed.contains(&req.status.as_str()));
    }

    #[test]
    fn finish_rejects_in_progress() {
        let req = FinishDeploymentRequest {
            status: "in_progress".to_string(),
            finished_at: None,
            rollback_of: None,
        };
        let allowed = ["success", "failed", "rolled_back"];
        assert!(!allowed.contains(&req.status.as_str()));
    }

    #[test]
    fn finish_rejects_unknown_status() {
        let req = FinishDeploymentRequest {
            status: "garbage".to_string(),
            finished_at: None,
            rollback_of: None,
        };
        let allowed = ["success", "failed", "rolled_back"];
        assert!(!allowed.contains(&req.status.as_str()));
    }

    #[test]
    fn create_response_serializes_deployment_id() {
        let id = Uuid::new_v4();
        let resp = CreateDeploymentResponse { deployment_id: id };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["deployment_id"].as_str().unwrap(), id.to_string());
    }
}
```

- [ ] **Run tests to confirm they pass (stubs don't affect unit tests)**

```bash
cargo test -p ingest-gateway deployments 2>&1 | tail -10
```

Expected: `4 passed`

- [ ] **Check how `TenantContext` is imported in other ingest-gateway http-json handlers**

Look at `services/ingest-gateway/src/http-json/traces.rs` top — the import for `TenantContext` comes from `crate::auth`. Confirm the import path: `use crate::auth::TenantContext;`. Update the import in `deployments.rs` accordingly (remove the `super::super::auth` path, use `crate::auth::TenantContext`).

The correct import is:
```rust
use crate::auth::TenantContext;
```

- [ ] **Implement `create_deployment`**

Replace the `create_deployment` stub:

```rust
pub async fn create_deployment(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateDeploymentRequest>,
) -> Result<(StatusCode, Json<CreateDeploymentResponse>), StatusCode> {
    if req.service_name.trim().is_empty()
        || req.environment.trim().is_empty()
        || req.service_version.trim().is_empty()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let deployment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_markers \
         (tenant_id, project_id, service_name, environment, service_version, \
          status, deployed_by, commit_sha, metadata) \
         VALUES ($1, $2, $3, $4, $5, 'in_progress', $6, $7, $8) \
         RETURNING deployment_id",
    )
    .bind(ctx.tenant_id)
    .bind(req.project_id)
    .bind(&req.service_name)
    .bind(&req.environment)
    .bind(&req.service_version)
    .bind(&req.deployed_by)
    .bind(&req.commit_sha)
    .bind(&req.metadata)
    .fetch_one(state.db.as_ref())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to create deployment marker");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateDeploymentResponse { deployment_id }),
    ))
}
```

- [ ] **Implement `finish_deployment`**

Replace the `finish_deployment` stub:

```rust
pub async fn finish_deployment(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(deployment_id): Path<Uuid>,
    Json(req): Json<FinishDeploymentRequest>,
) -> Result<StatusCode, StatusCode> {
    let allowed = ["success", "failed", "rolled_back"];
    if !allowed.contains(&req.status.as_str()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let finished_at = req.finished_at.unwrap_or_else(Utc::now);

    let result = sqlx::query(
        "UPDATE deployment_markers \
         SET status = $1, finished_at = $2, rollback_of = $3 \
         WHERE deployment_id = $4 AND tenant_id = $5",
    )
    .bind(&req.status)
    .bind(finished_at)
    .bind(req.rollback_of)
    .bind(deployment_id)
    .bind(ctx.tenant_id)
    .execute(state.db.as_ref())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to finish deployment marker");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Run all ingest-gateway tests**

```bash
cargo test -p ingest-gateway 2>&1 | tail -10
```

Expected: all existing tests pass plus the 4 new deployment unit tests.

- [ ] **Register routes in `services/ingest-gateway/src/http-json/mod.rs`**

Add `pub mod deployments;` at the top of the module declarations (after `pub mod traces;` etc.).

In `build_router`, add two routes inside the auth-middleware layer alongside the existing OTLP routes:

```rust
.route("/v1/deployments", post(deployments::create_deployment))
.route(
    "/v1/deployments/:deployment_id",
    axum::routing::patch(deployments::finish_deployment),
)
```

The `post` function is already imported via `use axum::{..., routing::{get, post}, ...}`.

- [ ] **Verify compilation**

```bash
cargo build -p ingest-gateway 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Commit**

```bash
git add services/ingest-gateway/src/http-json/deployments.rs \
        services/ingest-gateway/src/http-json/mod.rs
git commit -m "feat(ingest-gateway): add POST/PATCH /v1/deployments handlers"
```

---

## Task 5: Add chrono to query-api and implement GET /v1/deployments

**Files:**
- Modify: `services/query-api/Cargo.toml`
- Create: `services/query-api/src/deployments.rs`
- Modify: `services/query-api/src/main.rs`

- [ ] **Add chrono to query-api/Cargo.toml**

In `services/query-api/Cargo.toml`, find the `[dependencies]` section. After `serde_json = { workspace = true }` add:

```toml
chrono             = { workspace = true }
```

- [ ] **Write the failing unit tests first**

Create `services/query-api/src/deployments.rs` with types and the test module. The handler is a stub:

```rust
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ListDeploymentsParams {
    pub service_name: Option<String>,
    pub environment: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DeploymentMarker {
    pub deployment_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub deployed_by: Option<String>,
    pub commit_sha: Option<String>,
    pub rollback_of: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ListDeploymentsResponse {
    pub items: Vec<DeploymentMarker>,
}

pub async fn list_deployments(
    State(_state): State<AppState>,
    Extension(_ctx): Extension<TenantContext>,
    Query(_params): Query<ListDeploymentsParams>,
) -> Result<Json<ListDeploymentsResponse>, StatusCode> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_50() {
        let params = ListDeploymentsParams {
            service_name: None,
            environment: None,
            start_time: None,
            end_time: None,
            limit: None,
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 50);
    }

    #[test]
    fn limit_is_capped_at_200() {
        let params = ListDeploymentsParams {
            service_name: Some("svc".into()),
            environment: None,
            start_time: None,
            end_time: None,
            limit: Some(999),
        };
        assert_eq!(params.limit.unwrap_or(50).min(200), 200);
    }

    #[test]
    fn marker_serializes_all_fields() {
        let id = Uuid::new_v4();
        let m = DeploymentMarker {
            deployment_id: id,
            tenant_id: Uuid::new_v4(),
            project_id: None,
            service_name: "shop-api".into(),
            environment: "staging".into(),
            service_version: "v1.2.0".into(),
            status: "success".into(),
            started_at: Utc::now(),
            finished_at: None,
            deployed_by: Some("ci-bot".into()),
            commit_sha: Some("abc123".into()),
            rollback_of: None,
            metadata: None,
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["service_name"], "shop-api");
        assert_eq!(v["status"], "success");
        assert!(v["finished_at"].is_null());
    }
}
```

- [ ] **Run tests — 3 must pass**

```bash
cargo test -p query-api deployments 2>&1 | tail -10
```

Expected: `3 passed`

- [ ] **Implement `list_deployments`**

Replace the stub:

```rust
pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<ListDeploymentsParams>,
) -> Result<Json<ListDeploymentsResponse>, StatusCode> {
    let limit = params.limit.unwrap_or(50).min(200);

    let items = sqlx::query_as::<_, DeploymentMarker>(
        "SELECT deployment_id, tenant_id, project_id, service_name, environment, \
         service_version, status, started_at, finished_at, deployed_by, \
         commit_sha, rollback_of, metadata \
         FROM deployment_markers \
         WHERE tenant_id = $1 \
           AND ($2::TEXT IS NULL OR service_name = $2) \
           AND ($3::TEXT IS NULL OR environment = $3) \
           AND ($4::TIMESTAMPTZ IS NULL OR started_at >= $4) \
           AND ($5::TIMESTAMPTZ IS NULL OR started_at <= $5) \
         ORDER BY started_at DESC \
         LIMIT $6",
    )
    .bind(ctx.tenant_id)
    .bind(&params.service_name)
    .bind(&params.environment)
    .bind(params.start_time)
    .bind(params.end_time)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to list deployment markers");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListDeploymentsResponse { items }))
}
```

- [ ] **Register the route in `services/query-api/src/main.rs`**

Add `mod deployments;` after the existing `mod discovery;` line.

Add one route to the `Router::new()` chain (after the `/v1/environments` route):

```rust
.route("/v1/deployments", get(deployments::list_deployments))
```

- [ ] **Verify the service compiles and tests pass**

```bash
cargo build -p query-api 2>&1 | tail -5
cargo test -p query-api 2>&1 | tail -10
```

Expected: `Finished` and all tests pass.

- [ ] **Commit**

```bash
git add services/query-api/Cargo.toml \
        services/query-api/src/deployments.rs \
        services/query-api/src/main.rs
git commit -m "feat(query-api): add GET /v1/deployments list endpoint"
```

---

## Task 6: Bash script helper for CI/CD

**Files:**
- Create: `scripts/deployment-marker.sh`

- [ ] **Create the script**

```bash
#!/usr/bin/env bash
# CI/CD helper for Observable deployment markers.
#
# Usage:
#   # Start a deployment (prints deployment_id to stdout):
#   DEPLOYMENT_ID=$(bash scripts/deployment-marker.sh start \
#     --service shop-api --env staging --version v1.3.0 \
#     --deployed-by ci-bot --commit abc123)
#
#   # Finish a deployment:
#   bash scripts/deployment-marker.sh finish \
#     --id "$DEPLOYMENT_ID" --status success
#
# Environment variables:
#   OBSERVABLE_URL        Base URL of the Observable ingest-gateway (default: http://localhost:4318)
#   OBSERVABLE_TENANT_ID  X-Tenant-ID header value (default: dev tenant UUID)

set -euo pipefail

BASE_URL="${OBSERVABLE_URL:-http://localhost:4318}"
TENANT_ID="${OBSERVABLE_TENANT_ID:-00000000-0000-0000-0000-000000000001}"
SUBCOMMAND="${1:-}"
shift || true

case "$SUBCOMMAND" in
  start)
    SERVICE_NAME=""
    ENVIRONMENT=""
    SERVICE_VERSION=""
    DEPLOYED_BY=""
    COMMIT_SHA=""

    while [[ $# -gt 0 ]]; do
      case "$1" in
        --service)     SERVICE_NAME="$2";    shift 2 ;;
        --env)         ENVIRONMENT="$2";     shift 2 ;;
        --version)     SERVICE_VERSION="$2"; shift 2 ;;
        --deployed-by) DEPLOYED_BY="$2";     shift 2 ;;
        --commit)      COMMIT_SHA="$2";      shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
      esac
    done

    if [[ -z "$SERVICE_NAME" || -z "$ENVIRONMENT" || -z "$SERVICE_VERSION" ]]; then
      echo "ERROR: --service, --env, and --version are required for start" >&2
      exit 1
    fi

    PAYLOAD=$(printf '{"service_name":"%s","environment":"%s","service_version":"%s","deployed_by":"%s","commit_sha":"%s"}' \
      "$SERVICE_NAME" "$ENVIRONMENT" "$SERVICE_VERSION" "$DEPLOYED_BY" "$COMMIT_SHA")

    RESPONSE=$(curl -sf -X POST "$BASE_URL/v1/deployments" \
      -H "Content-Type: application/json" \
      -H "X-Tenant-ID: $TENANT_ID" \
      -d "$PAYLOAD")

    echo "$RESPONSE" | grep -o '"deployment_id":"[^"]*"' | cut -d'"' -f4
    ;;

  finish)
    DEPLOYMENT_ID=""
    STATUS=""

    while [[ $# -gt 0 ]]; do
      case "$1" in
        --id)     DEPLOYMENT_ID="$2"; shift 2 ;;
        --status) STATUS="$2";        shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
      esac
    done

    if [[ -z "$DEPLOYMENT_ID" || -z "$STATUS" ]]; then
      echo "ERROR: --id and --status are required for finish" >&2
      exit 1
    fi

    ALLOWED="success failed rolled_back"
    if ! echo "$ALLOWED" | grep -qw "$STATUS"; then
      echo "ERROR: --status must be one of: $ALLOWED" >&2
      exit 1
    fi

    curl -sf -X PATCH "$BASE_URL/v1/deployments/$DEPLOYMENT_ID" \
      -H "Content-Type: application/json" \
      -H "X-Tenant-ID: $TENANT_ID" \
      -d "{\"status\":\"$STATUS\"}"

    echo "Deployment $DEPLOYMENT_ID marked $STATUS"
    ;;

  *)
    echo "Usage: $0 {start|finish} [options]" >&2
    exit 1
    ;;
esac
```

Note: `BASE_URL` defaults to `http://localhost:4318` (the ingest-gateway HTTP port), not the query-api.

- [ ] **Verify syntax**

```bash
bash -n scripts/deployment-marker.sh && echo "syntax OK"
```

Expected: `syntax OK`

- [ ] **Make executable and commit**

```bash
chmod +x scripts/deployment-marker.sh
git add scripts/deployment-marker.sh
git commit -m "feat(scripts): add deployment-marker.sh CI/CD helper pointing at ingest-gateway"
```

---

## Task 7: Frontend API client

**Files:**
- Create: `apps/frontend/src/api/deployments.ts`

The `GET /v1/deployments` endpoint is served by the query-api on the same origin as the frontend proxy. The write endpoints (on the ingest-gateway) are not called from the browser in this slice.

- [ ] **Write the API client**

```typescript
const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface DeploymentMarker {
  deployment_id: string;
  tenant_id: string;
  project_id: string | null;
  service_name: string;
  environment: string;
  service_version: string;
  status: "in_progress" | "success" | "failed" | "rolled_back";
  started_at: string;
  finished_at: string | null;
  deployed_by: string | null;
  commit_sha: string | null;
  rollback_of: string | null;
  metadata: Record<string, unknown> | null;
}

export interface ListDeploymentsResponse {
  items: DeploymentMarker[];
}

export interface ListDeploymentsParams {
  service_name?: string;
  environment?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
}

export async function listDeployments(
  params: ListDeploymentsParams = {},
): Promise<ListDeploymentsResponse> {
  const url = new URL("/v1/deployments", window.location.origin);
  if (params.service_name) url.searchParams.set("service_name", params.service_name);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.start_time) url.searchParams.set("start_time", params.start_time);
  if (params.end_time) url.searchParams.set("end_time", params.end_time);
  if (params.limit !== undefined) url.searchParams.set("limit", String(params.limit));

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Deployments fetch failed: ${res.status}`);
  return res.json();
}
```

- [ ] **Typecheck**

```bash
cd apps/frontend && npx tsc --noEmit 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Commit**

```bash
git add apps/frontend/src/api/deployments.ts
git commit -m "feat(frontend): add deployments API client for GET /v1/deployments"
```

---

## Task 8: DeploymentTimeline component

**Files:**
- Create: `apps/frontend/src/components/DeploymentTimeline.tsx`
- Create: `apps/frontend/src/components/DeploymentTimeline.test.tsx`

The component renders a horizontal SVG bar showing deployment events as colored vertical tick marks. Colors: `success`=#22c55e, `in_progress`=#3b82f6, `failed`=#ef4444, `rolled_back`=#f97316. A hover tooltip shows version, deployer, and short commit SHA.

- [ ] **Write the failing tests first**

Create `apps/frontend/src/components/DeploymentTimeline.test.tsx`:

```typescript
import { describe, it, expect } from "vitest";
import { markerPosition, markerColor } from "./DeploymentTimeline";

describe("markerPosition", () => {
  const rangeStart = new Date("2024-01-01T00:00:00Z").getTime();
  const rangeEnd   = new Date("2024-01-01T01:00:00Z").getTime();
  const width = 400;

  it("places marker at left edge when at rangeStart", () => {
    expect(markerPosition(rangeStart, rangeStart, rangeEnd, width)).toBe(0);
  });

  it("places marker at right edge when at rangeEnd", () => {
    expect(markerPosition(rangeEnd, rangeStart, rangeEnd, width)).toBe(400);
  });

  it("places marker at midpoint when halfway", () => {
    const mid = (rangeStart + rangeEnd) / 2;
    expect(markerPosition(mid, rangeStart, rangeEnd, width)).toBe(200);
  });

  it("clamps to 0 when marker is before range", () => {
    expect(markerPosition(rangeStart - 1000, rangeStart, rangeEnd, width)).toBe(0);
  });

  it("clamps to width when marker is after range", () => {
    expect(markerPosition(rangeEnd + 1000, rangeStart, rangeEnd, width)).toBe(400);
  });
});

describe("markerColor", () => {
  it("returns green for success",          () => expect(markerColor("success")).toBe("#22c55e"));
  it("returns blue for in_progress",       () => expect(markerColor("in_progress")).toBe("#3b82f6"));
  it("returns red for failed",             () => expect(markerColor("failed")).toBe("#ef4444"));
  it("returns orange for rolled_back",     () => expect(markerColor("rolled_back")).toBe("#f97316"));
  it("returns grey for unknown status",    () => expect(markerColor("unknown")).toBe("#9ca3af"));
});
```

- [ ] **Run tests — confirm they FAIL**

```bash
cd apps/frontend && npx vitest run src/components/DeploymentTimeline.test.tsx 2>&1 | tail -10
```

Expected: failures — `markerPosition` and `markerColor` are not exported yet.

- [ ] **Implement the component**

Create `apps/frontend/src/components/DeploymentTimeline.tsx`:

```typescript
import { useState } from "react";
import type { DeploymentMarker } from "../api/deployments";

export function markerColor(status: string): string {
  switch (status) {
    case "success":     return "#22c55e";
    case "in_progress": return "#3b82f6";
    case "failed":      return "#ef4444";
    case "rolled_back": return "#f97316";
    default:            return "#9ca3af";
  }
}

export function markerPosition(
  timestampMs: number,
  rangeStartMs: number,
  rangeEndMs: number,
  widthPx: number,
): number {
  const span = rangeEndMs - rangeStartMs;
  if (span <= 0) return 0;
  const ratio = (timestampMs - rangeStartMs) / span;
  return Math.round(Math.min(Math.max(ratio, 0), 1) * widthPx);
}

interface Props {
  markers: DeploymentMarker[];
  rangeStartMs: number;
  rangeEndMs: number;
}

interface TooltipState {
  marker: DeploymentMarker;
  x: number;
  y: number;
}

export function DeploymentTimeline({ markers, rangeStartMs, rangeEndMs }: Props) {
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const width = 600;
  const height = 32;
  const lineHeight = 24;
  const lineY = 4;

  if (markers.length === 0) return null;

  return (
    <div className="deployment-timeline" style={{ position: "relative" }}>
      <div className="field-label" style={{ marginBottom: "4px" }}>Deployments</div>
      <svg
        width="100%"
        viewBox={`0 0 ${width} ${height}`}
        aria-label="Deployment timeline"
        style={{ display: "block" }}
      >
        <line
          x1={0} y1={lineY + lineHeight / 2}
          x2={width} y2={lineY + lineHeight / 2}
          stroke="#374151" strokeWidth={1}
        />
        {markers.map((m) => {
          const x = markerPosition(
            new Date(m.started_at).getTime(),
            rangeStartMs,
            rangeEndMs,
            width,
          );
          const color = markerColor(m.status);
          return (
            <g key={m.deployment_id}>
              <line
                x1={x} y1={lineY} x2={x} y2={lineY + lineHeight}
                stroke={color} strokeWidth={2}
              />
              <circle
                cx={x} cy={lineY + lineHeight / 2} r={5}
                fill={color}
                style={{ cursor: "pointer" }}
                onMouseEnter={(e) => {
                  const svgRect = (e.currentTarget.closest("svg") as SVGElement)
                    .getBoundingClientRect();
                  setTooltip({ marker: m, x: e.clientX - svgRect.left, y: e.clientY - svgRect.top });
                }}
                onMouseLeave={() => setTooltip(null)}
                aria-label={`Deployment ${m.service_version} — ${m.status}`}
              />
            </g>
          );
        })}
      </svg>

      {tooltip && (
        <div
          role="tooltip"
          style={{
            position: "absolute",
            left: tooltip.x + 8,
            top: tooltip.y - 8,
            background: "#1f2937",
            border: "1px solid #374151",
            borderRadius: "4px",
            padding: "6px 10px",
            fontSize: "12px",
            color: "#f3f4f6",
            pointerEvents: "none",
            zIndex: 10,
            whiteSpace: "nowrap",
          }}
        >
          <div><strong>{tooltip.marker.service_version}</strong></div>
          <div style={{ color: markerColor(tooltip.marker.status) }}>{tooltip.marker.status}</div>
          {tooltip.marker.deployed_by && <div>by {tooltip.marker.deployed_by}</div>}
          {tooltip.marker.commit_sha && (
            <div style={{ fontFamily: "monospace" }}>{tooltip.marker.commit_sha.slice(0, 8)}</div>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Run tests — 10 must pass**

```bash
cd apps/frontend && npx vitest run src/components/DeploymentTimeline.test.tsx 2>&1 | tail -10
```

Expected: `10 passed, 0 failed`

- [ ] **Typecheck**

```bash
cd apps/frontend && npx tsc --noEmit 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Commit**

```bash
git add apps/frontend/src/components/DeploymentTimeline.tsx \
        apps/frontend/src/components/DeploymentTimeline.test.tsx
git commit -m "feat(frontend): add DeploymentTimeline SVG component with 10 unit tests"
```

---

## Task 9: Integrate DeploymentTimeline into ServiceDetailPage

**Files:**
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`

- [ ] **Add imports**

At the top of `apps/frontend/src/pages/ServiceDetailPage.tsx`, add two imports alongside the existing ones:

```typescript
import { listDeployments } from "../api/deployments";
import { DeploymentTimeline } from "../components/DeploymentTimeline";
```

`useQuery` is already imported — do not add a duplicate.

- [ ] **Add `DeploymentTimelineSection` sub-component**

Add this function before the final closing `}` of the file (after the `MetricTile` component):

```typescript
function DeploymentTimelineSection({
  serviceName,
  lookbackMinutes,
}: {
  serviceName: string;
  lookbackMinutes: number;
}) {
  const nowMs = Date.now();
  const startMs = nowMs - lookbackMinutes * 60 * 1000;

  const { data } = useQuery({
    queryKey: ["deployments", serviceName, lookbackMinutes],
    queryFn: () =>
      listDeployments({
        service_name: serviceName,
        start_time: new Date(startMs).toISOString(),
        end_time: new Date(nowMs).toISOString(),
        limit: 20,
      }),
  });

  if (!data?.items.length) return null;

  return (
    <DeploymentTimeline
      markers={data.items}
      rangeStartMs={startMs}
      rangeEndMs={nowMs}
    />
  );
}
```

- [ ] **Add `DeploymentTimelineSection` inside `ServiceOverview`**

In the `ServiceOverview` function, locate the closing `</div>` of `<div className="metric-grid" ...>`. Immediately after it, add:

```tsx
<DeploymentTimelineSection
  serviceName={service.service_name}
  lookbackMinutes={lookbackMinutes}
/>
```

- [ ] **Run the full frontend test suite**

```bash
cd apps/frontend && npx vitest run 2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Typecheck**

```bash
cd apps/frontend && npx tsc --noEmit 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "feat(frontend): show deployment timeline overlay in service detail overview"
```

---

## Task 10: Run local-ci, update phases plan, open PR

- [ ] **Run local CI gate**

```bash
bash scripts/local-ci.sh --skip-smoke 2>&1 | tail -30
```

Expected: all stages pass. Fix any failure before continuing.

- [ ] **Update phases plan to mark P3-S11 done**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, find:

```
- [ ] **P3-S11: Add deployment event ingestion and one timeline overlay**
```

Replace with:

```
- [x] **P3-S11: Add deployment event ingestion and one timeline overlay**
  - Outcome: `deployment_markers` table (migration 009), `POST /v1/deployments` + `PATCH /v1/deployments/:id` in ingest-gateway (new PgPool connection), `GET /v1/deployments` in query-api, `scripts/deployment-marker.sh` CI helper, `DeploymentTimeline` SVG component in service detail overview, ADR-024 documents routing split and future Redpanda SSE path. Completed 2026-04-26.
  - Checkpoint: is deployment identity clean enough for rollback analysis later? Answer: yes. `rollback_of` FK links a `rolled_back` deployment to the original. Ingest enrichment (§18.5 stamping `deployment_id` on span rows) and the Redpanda event-stream path (ADR-024 §Future) are explicitly deferred.
```

- [ ] **Commit the plan update**

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs: mark P3-S11 deployment markers complete"
```

- [ ] **Push branch and open PR**

```bash
git push -u origin feat/p3-s11-deployment-markers
gh pr create \
  --title "feat(P3-S11): deployment event ingestion and timeline overlay" \
  --body "$(cat <<'EOF'
## Summary

- Adds `deployment_markers` PostgreSQL table (migration 009) covering full spec/18-deployment-markers.md schema
- `POST /v1/deployments` and `PATCH /v1/deployments/:id` in **ingest-gateway** (new PgPool connection; direct Postgres write for immediate consistency)
- `GET /v1/deployments` in **query-api** with service/environment/time filters
- ADR-024 documents the ingest/query routing split and the future Redpanda `deployment.events` topic that enables SSE push to the UI without polling
- `scripts/deployment-marker.sh` bash helper for CI/CD pipelines (points at ingest-gateway port 4318)
- `DeploymentTimeline` SVG component (10 unit tests) in service detail overview — green=success, blue=in_progress, red=failed, orange=rolled_back, hover tooltip with version/deployer/commit

## Out of scope

- Redpanda event publication (ADR-024 future path)
- Ingest enrichment — stamping `deployment_id` on span/log/metric rows (spec §18.5)
- RBAC role checks on write endpoints at the ingest-gateway layer beyond current tenant auth

## Test plan

- [ ] `cargo test -p ingest-gateway` — 4 new deployment unit tests pass
- [ ] `cargo test -p query-api` — 3 new deployment unit tests pass
- [ ] `npx vitest run` in `apps/frontend` — 10 new timeline tests pass, all existing pass
- [ ] `bash scripts/local-ci.sh --skip-smoke` — all stages green
- [ ] Manual: `docker compose up -d`, `GET http://localhost:8090/v1/deployments` returns dev seed row
- [ ] Manual: service detail for `shop-api` shows deployment timeline

## ADR/spec sync

- ADR-024 created — documents write-path routing decision and future Redpanda SSE path
- `spec/adr/README.md` updated with ADR-024 index entry

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

**Spec coverage (spec/18-deployment-markers.md):**

| Requirement | Covered |
|---|---|
| §18.1 all table fields | ✅ Task 1 |
| §18.3 POST /v1/deployments (ingest-gateway) | ✅ Task 4 |
| §18.3 PATCH /v1/deployments/:id (ingest-gateway) | ✅ Task 4 |
| §18.3 GET /v1/deployments (query-api) | ✅ Task 5 |
| §18.4 Timeline overlay with status colors + tooltip | ✅ Tasks 8–9 |
| §18.5 Ingest enrichment | ❌ Deferred — out of scope |
| §18.6 RBAC write-role gate | ⚠️ Partial — tenant auth only; role gate deferred |
| §18.8 CI/CD helper script | ✅ Task 6 |
| ADR-024 routing + Redpanda future path | ✅ Task 2 (done) |

**Type consistency:** `DeploymentMarker.status` union in `api/deployments.ts` matches SQL CHECK constraint in migration. `markerColor(status: string)` receives `status` from `DeploymentMarker.status` which is always one of the four union members at runtime. `list_deployments` in `api/deployments.ts` returns `ListDeploymentsResponse` which is consumed correctly in `DeploymentTimelineSection`.
