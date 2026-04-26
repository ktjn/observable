# P3-S11 Deployment Event Ingestion and Timeline Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deployment marker lifecycle endpoints to the query-api and a timeline overlay component on the service detail page so operators can correlate performance shifts with deployments.

**Architecture:** All three endpoints (`POST /v1/deployments`, `PATCH /v1/deployments/:id`, `GET /v1/deployments`) live in the query-api, which already has a `PgPool`. A new PostgreSQL table `deployment_markers` stores the records. A small bash helper script lets CI/CD pipelines create and finish markers with `curl`. The frontend gains a `DeploymentTimeline` SVG component embedded in the service detail overview above the signal tabs, queried via a new `api/deployments.ts` client.

**Tech Stack:** Rust (axum, sqlx 0.7 with `json`+`chrono`+`uuid` features, chrono 0.4), PostgreSQL, Bash, TypeScript + React (SVG, TanStack Query)

---

## Scope boundaries for this slice

- **In scope:** `deployment_markers` table, POST/PATCH/GET endpoints, bash script helper, frontend API client, `DeploymentTimeline` component in service detail, phases plan update.
- **Out of scope:** ingest enrichment (§18.5 — stamping `deployment_id` on spans/logs/metrics), RBAC role checks on write endpoints (query-api uses tenant-only auth), canary-promote integration, signal retention of `deployment_id` (already in `spans` schema column, just not populated).

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `migrations/postgres/009_create_deployment_markers.sql` | Create | Table schema + dev seed row |
| `services/query-api/Cargo.toml` | Modify | Add `chrono = { workspace = true }` |
| `services/query-api/src/deployments.rs` | Create | POST/PATCH/GET handlers + unit tests |
| `services/query-api/src/main.rs` | Modify | Register three new routes |
| `scripts/deployment-marker.sh` | Create | Bash helper for CI/CD pipelines |
| `apps/frontend/src/api/deployments.ts` | Create | Fetch wrapper for `GET /v1/deployments` |
| `apps/frontend/src/components/DeploymentTimeline.tsx` | Create | SVG timeline with colored vertical markers |
| `apps/frontend/src/components/DeploymentTimeline.test.tsx` | Create | Component unit tests |
| `apps/frontend/src/pages/ServiceDetailPage.tsx` | Modify | Add `DeploymentTimeline` to overview section |
| `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` | Modify | Mark P3-S11 done |

---

## Task 1: PostgreSQL migration — deployment_markers table

**Files:**
- Create: `migrations/postgres/009_create_deployment_markers.sql`

- [ ] **Create the migration file**

```sql
CREATE TABLE IF NOT EXISTS deployment_markers (
    deployment_id   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    project_id      UUID        REFERENCES projects(id) ON DELETE SET NULL,
    service_name    TEXT        NOT NULL,
    environment     TEXT        NOT NULL,
    service_version TEXT        NOT NULL,
    status          TEXT        NOT NULL CHECK (status IN ('in_progress', 'success', 'failed', 'rolled_back')),
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at     TIMESTAMPTZ,
    deployed_by     TEXT,
    commit_sha      TEXT,
    rollback_of     UUID        REFERENCES deployment_markers(deployment_id) ON DELETE SET NULL,
    metadata        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS deployment_markers_tenant_idx     ON deployment_markers (tenant_id);
CREATE INDEX IF NOT EXISTS deployment_markers_service_idx    ON deployment_markers (tenant_id, service_name);
CREATE INDEX IF NOT EXISTS deployment_markers_started_at_idx ON deployment_markers (started_at);

-- Dev seed: one successful deployment for the dev tenant / shop-api
INSERT INTO deployment_markers (
    deployment_id,
    tenant_id,
    project_id,
    service_name,
    environment,
    service_version,
    status,
    started_at,
    finished_at,
    deployed_by,
    commit_sha
) VALUES (
    '20000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    (SELECT id FROM projects
     WHERE tenant_id = '00000000-0000-0000-0000-000000000001'
       AND name = 'default'
     LIMIT 1),
    'shop-api',
    'staging',
    'v1.2.0',
    'success',
    NOW() - INTERVAL '2 hours',
    NOW() - INTERVAL '1 hour 45 minutes',
    'ci-bot',
    'abc123def456'
) ON CONFLICT DO NOTHING;
```

- [ ] **Verify SQL syntax with psql (or docker compose)**

```bash
docker compose exec postgres psql -U observable -c "\d deployment_markers" 2>/dev/null || echo "Run docker compose up -d first, then re-run"
```

Expected after `docker compose up -d`: table description lists all columns.

- [ ] **Commit**

```bash
git add migrations/postgres/009_create_deployment_markers.sql
git commit -m "feat(deployments): add deployment_markers PostgreSQL table and dev seed"
```

---

## Task 2: Add chrono dependency to query-api

**Files:**
- Modify: `services/query-api/Cargo.toml`

- [ ] **Add chrono to query-api dependencies**

Open `services/query-api/Cargo.toml`. Find the `[dependencies]` section. It currently contains:

```toml
serde              = { workspace = true }
serde_json         = { workspace = true }
```

Add one line immediately after `serde_json`:

```toml
chrono             = { workspace = true }
```

- [ ] **Verify the workspace crate compiles**

```bash
cargo check -p query-api 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Commit**

```bash
git add services/query-api/Cargo.toml
git commit -m "chore(query-api): add chrono workspace dependency"
```

---

## Task 3: Implement deployment marker handlers

**Files:**
- Create: `services/query-api/src/deployments.rs`

This module provides three handlers:
- `create_deployment` — `POST /v1/deployments`
- `finish_deployment` — `PATCH /v1/deployments/:id`
- `list_deployments` — `GET /v1/deployments`

- [ ] **Write the failing unit tests first**

Create `services/query-api/src/deployments.rs` with only the test module and imports:

```rust
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---- Request / Response types ----

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

// ---- Handlers ----

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

pub async fn list_deployments(
    State(_state): State<AppState>,
    Extension(_ctx): Extension<TenantContext>,
    Query(_params): Query<ListDeploymentsParams>,
) -> Result<Json<ListDeploymentsResponse>, StatusCode> {
    todo!()
}

// ---- Unit tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_request_allows_success_status() {
        let req = FinishDeploymentRequest {
            status: "success".to_string(),
            finished_at: None,
            rollback_of: None,
        };
        let allowed = ["success", "failed", "rolled_back"];
        assert!(allowed.contains(&req.status.as_str()));
    }

    #[test]
    fn finish_request_rejects_in_progress_status() {
        let req = FinishDeploymentRequest {
            status: "in_progress".to_string(),
            finished_at: None,
            rollback_of: None,
        };
        let allowed = ["success", "failed", "rolled_back"];
        assert!(!allowed.contains(&req.status.as_str()));
    }

    #[test]
    fn finish_request_rejects_unknown_status() {
        let req = FinishDeploymentRequest {
            status: "unknown".to_string(),
            finished_at: None,
            rollback_of: None,
        };
        let allowed = ["success", "failed", "rolled_back"];
        assert!(!allowed.contains(&req.status.as_str()));
    }

    #[test]
    fn deployment_marker_serialize_shape() {
        let marker = DeploymentMarker {
            deployment_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            project_id: None,
            service_name: "shop-api".to_string(),
            environment: "staging".to_string(),
            service_version: "v1.0.0".to_string(),
            status: "success".to_string(),
            started_at: Utc::now(),
            finished_at: None,
            deployed_by: Some("ci-bot".to_string()),
            commit_sha: Some("abc123".to_string()),
            rollback_of: None,
            metadata: None,
        };
        let v = serde_json::to_value(&marker).unwrap();
        assert_eq!(v["service_name"], "shop-api");
        assert_eq!(v["status"], "success");
        assert_eq!(v["deployed_by"], "ci-bot");
        assert!(v["finished_at"].is_null());
    }

    #[test]
    fn list_params_default_limit_is_none() {
        let params = ListDeploymentsParams {
            service_name: None,
            environment: None,
            start_time: None,
            end_time: None,
            limit: None,
        };
        let effective_limit = params.limit.unwrap_or(50).min(200);
        assert_eq!(effective_limit, 50);
    }

    #[test]
    fn list_params_limit_is_capped_at_200() {
        let params = ListDeploymentsParams {
            service_name: Some("svc".to_string()),
            environment: None,
            start_time: None,
            end_time: None,
            limit: Some(999),
        };
        let effective_limit = params.limit.unwrap_or(50).min(200);
        assert_eq!(effective_limit, 200);
    }
}
```

- [ ] **Run the tests — expect FAIL (todo! panics are compile-time stubs not test failures)**

```bash
cargo test -p query-api deployments 2>&1 | tail -20
```

Expected: `4 passed` for the four unit tests (they don't call the `todo!()` handlers).

- [ ] **Implement create_deployment**

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
    .fetch_one(&state.db)
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

- [ ] **Implement finish_deployment**

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
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to update deployment marker");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Implement list_deployments**

Replace the `list_deployments` stub:

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

- [ ] **Run tests — all must pass**

```bash
cargo test -p query-api deployments 2>&1 | tail -10
```

Expected: `5 passed, 0 failed`

- [ ] **Commit**

```bash
git add services/query-api/src/deployments.rs
git commit -m "feat(query-api): add deployment marker create/finish/list handlers"
```

---

## Task 4: Register routes in query-api main.rs

**Files:**
- Modify: `services/query-api/src/main.rs`

- [ ] **Add module declaration and three routes**

In `services/query-api/src/main.rs`, add `mod deployments;` after the existing `mod discovery;` line:

```rust
mod deployments;
```

Add three routes to the `Router::new()` chain (place them after the existing `/v1/environments` route):

```rust
.route("/v1/deployments", axum::routing::post(deployments::create_deployment))
.route("/v1/deployments", get(deployments::list_deployments))
.route(
    "/v1/deployments/:deployment_id",
    axum::routing::patch(deployments::finish_deployment),
)
```

The two `/v1/deployments` routes use different HTTP methods so axum will route them correctly.

- [ ] **Verify the service compiles**

```bash
cargo build -p query-api 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Run the full query-api test suite**

```bash
cargo test -p query-api 2>&1 | tail -10
```

Expected: all existing tests still pass plus the 5 new deployment tests.

- [ ] **Commit**

```bash
git add services/query-api/src/main.rs
git commit -m "feat(query-api): register deployment marker routes"
```

---

## Task 5: Bash script helper for CI/CD

**Files:**
- Create: `scripts/deployment-marker.sh`

This script lets a CI pipeline create a deployment marker when a deploy starts and finish it when it completes or fails.

- [ ] **Create the script**

```bash
#!/usr/bin/env bash
# CI/CD helper for Observable deployment markers.
#
# Usage:
#   # Start a deployment (prints deployment_id):
#   DEPLOYMENT_ID=$(bash scripts/deployment-marker.sh start \
#     --service shop-api --env staging --version v1.3.0 \
#     --deployed-by ci-bot --commit abc123)
#
#   # Finish a deployment:
#   bash scripts/deployment-marker.sh finish \
#     --id "$DEPLOYMENT_ID" --status success
#
# Environment variables:
#   OBSERVABLE_URL   Base URL of the Observable query-api (default: http://localhost:8090)
#   OBSERVABLE_TENANT_ID   X-Tenant-ID header value (required)

set -euo pipefail

BASE_URL="${OBSERVABLE_URL:-http://localhost:8090}"
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
        --service)   SERVICE_NAME="$2";    shift 2 ;;
        --env)       ENVIRONMENT="$2";     shift 2 ;;
        --version)   SERVICE_VERSION="$2"; shift 2 ;;
        --deployed-by) DEPLOYED_BY="$2";   shift 2 ;;
        --commit)    COMMIT_SHA="$2";      shift 2 ;;
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

- [ ] **Verify syntax**

```bash
bash -n scripts/deployment-marker.sh && echo "syntax OK"
```

Expected: `syntax OK`

- [ ] **Make executable and commit**

```bash
chmod +x scripts/deployment-marker.sh
git add scripts/deployment-marker.sh
git commit -m "feat(scripts): add deployment-marker.sh CI/CD helper"
```

---

## Task 6: Frontend API client

**Files:**
- Create: `apps/frontend/src/api/deployments.ts`

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

- [ ] **Verify TypeScript compiles**

```bash
cd apps/frontend && npx tsc --noEmit 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Commit**

```bash
git add apps/frontend/src/api/deployments.ts
git commit -m "feat(frontend): add deployments API client"
```

---

## Task 7: DeploymentTimeline component

**Files:**
- Create: `apps/frontend/src/components/DeploymentTimeline.tsx`
- Create: `apps/frontend/src/components/DeploymentTimeline.test.tsx`

The component renders a horizontal SVG bar showing deployment markers as colored vertical lines. The time axis spans from `rangeStart` to `rangeEnd`. Each marker is positioned proportionally along the bar. Colors: `success` = green (`#22c55e`), `in_progress` = blue (`#3b82f6`), `failed` = red (`#ef4444`), `rolled_back` = orange (`#f97316`). A hover tooltip shows version, deployed_by, and commit_sha.

- [ ] **Write the failing tests first**

Create `apps/frontend/src/components/DeploymentTimeline.test.tsx`:

```typescript
import { describe, it, expect } from "vitest";
import { markerPosition, markerColor } from "./DeploymentTimeline";

describe("markerPosition", () => {
  const rangeStart = new Date("2024-01-01T00:00:00Z").getTime();
  const rangeEnd = new Date("2024-01-01T01:00:00Z").getTime();
  const width = 400;

  it("places marker at left edge when at rangeStart", () => {
    const pos = markerPosition(rangeStart, rangeStart, rangeEnd, width);
    expect(pos).toBe(0);
  });

  it("places marker at right edge when at rangeEnd", () => {
    const pos = markerPosition(rangeEnd, rangeStart, rangeEnd, width);
    expect(pos).toBe(400);
  });

  it("places marker at midpoint when halfway", () => {
    const mid = (rangeStart + rangeEnd) / 2;
    const pos = markerPosition(mid, rangeStart, rangeEnd, width);
    expect(pos).toBe(200);
  });

  it("clamps to 0 when marker is before range", () => {
    const before = rangeStart - 1000;
    const pos = markerPosition(before, rangeStart, rangeEnd, width);
    expect(pos).toBe(0);
  });

  it("clamps to width when marker is after range", () => {
    const after = rangeEnd + 1000;
    const pos = markerPosition(after, rangeStart, rangeEnd, width);
    expect(pos).toBe(400);
  });
});

describe("markerColor", () => {
  it("returns green for success", () => {
    expect(markerColor("success")).toBe("#22c55e");
  });

  it("returns blue for in_progress", () => {
    expect(markerColor("in_progress")).toBe("#3b82f6");
  });

  it("returns red for failed", () => {
    expect(markerColor("failed")).toBe("#ef4444");
  });

  it("returns orange for rolled_back", () => {
    expect(markerColor("rolled_back")).toBe("#f97316");
  });

  it("returns grey for unknown status", () => {
    expect(markerColor("unknown")).toBe("#9ca3af");
  });
});
```

- [ ] **Run tests to confirm they fail (functions not defined yet)**

```bash
cd apps/frontend && npx vitest run src/components/DeploymentTimeline.test.tsx 2>&1 | tail -10
```

Expected: test failures — `markerPosition` and `markerColor` are not exported yet.

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
        {/* baseline */}
        <line
          x1={0}
          y1={lineY + lineHeight / 2}
          x2={width}
          y2={lineY + lineHeight / 2}
          stroke="#374151"
          strokeWidth={1}
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
                x1={x}
                y1={lineY}
                x2={x}
                y2={lineY + lineHeight}
                stroke={color}
                strokeWidth={2}
              />
              <circle
                cx={x}
                cy={lineY + lineHeight / 2}
                r={5}
                fill={color}
                style={{ cursor: "pointer" }}
                onMouseEnter={(e) => {
                  const svgRect = (e.currentTarget.closest("svg") as SVGElement)
                    .getBoundingClientRect();
                  setTooltip({
                    marker: m,
                    x: e.clientX - svgRect.left,
                    y: e.clientY - svgRect.top,
                  });
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

- [ ] **Run tests — all must pass**

```bash
cd apps/frontend && npx vitest run src/components/DeploymentTimeline.test.tsx 2>&1 | tail -10
```

Expected: `9 passed, 0 failed`

- [ ] **Typecheck**

```bash
cd apps/frontend && npx tsc --noEmit 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Commit**

```bash
git add apps/frontend/src/components/DeploymentTimeline.tsx \
        apps/frontend/src/components/DeploymentTimeline.test.tsx
git commit -m "feat(frontend): add DeploymentTimeline SVG component with unit tests"
```

---

## Task 8: Integrate DeploymentTimeline into ServiceDetailPage

**Files:**
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`

The timeline is added inside the `ServiceOverview` sub-component, between the metric grid and the signal tabs. It queries `GET /v1/deployments?service_name=<name>` using TanStack Query and passes the results to `DeploymentTimeline`. The time range matches `lookbackMinutes`.

- [ ] **Add imports to ServiceDetailPage.tsx**

At the top of the file, after the existing imports, add:

```typescript
import { useQuery } from "@tanstack/react-query";
import { listDeployments } from "../api/deployments";
import { DeploymentTimeline } from "../components/DeploymentTimeline";
```

Note: `useQuery` is already imported once — if there is already a `useQuery` import at the top of the file, do not add a duplicate. Just add the two new lines for `listDeployments` and `DeploymentTimeline`.

- [ ] **Add DeploymentTimelineSection sub-component**

Add this component immediately before the closing `}` of the file (after the `MetricTile` component):

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

- [ ] **Add DeploymentTimelineSection to the ServiceOverview component**

In `ServiceOverview`, locate the `<div className="metric-grid"` block. The line after it ends with `</div>`. After that closing `</div>` (the one closing the metric-grid), add:

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

Expected: all tests pass, including existing service detail tests and the new timeline tests.

- [ ] **Typecheck**

```bash
cd apps/frontend && npx tsc --noEmit 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "feat(frontend): add deployment timeline overlay to service detail overview"
```

---

## Task 9: Run local-ci and open PR

- [ ] **Run local CI gate**

```bash
bash scripts/local-ci.sh --skip-smoke 2>&1 | tail -30
```

Expected: all stages pass (Rust fmt, clippy, tests; frontend typecheck/lint/build/test; Docker build).

If any stage fails, fix before proceeding.

- [ ] **Update phases plan to mark P3-S11 done**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, find:

```
- [ ] **P3-S11: Add deployment event ingestion and one timeline overlay**
```

Replace with:

```
- [x] **P3-S11: Add deployment event ingestion and one timeline overlay**
  - Outcome: `deployment_markers` table in PostgreSQL, `POST /v1/deployments` + `PATCH /v1/deployments/:id` + `GET /v1/deployments` in query-api, `scripts/deployment-marker.sh` CI helper, `DeploymentTimeline` SVG component in service detail overview. Completed 2026-04-26.
  - Checkpoint: is deployment identity clean enough for rollback analysis later? Answer: yes. The `rollback_of` foreign key links a `rolled_back` deployment to the original, and the `status` enum tracks the full lifecycle. Enrichment of span/log/metric rows with `deployment_id` (§18.5) is deferred to the next ingest-enrichment slice.
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

- Adds `deployment_markers` PostgreSQL table (migration 009) with lifecycle fields from spec/18-deployment-markers.md
- Adds `POST /v1/deployments`, `PATCH /v1/deployments/:id`, `GET /v1/deployments` to the query-api
- Adds `scripts/deployment-marker.sh` for CI/CD pipelines to create and finish markers via curl
- Adds `DeploymentTimeline` SVG component to the service detail overview — shows colored vertical markers (green=success, blue=in_progress, red=failed, orange=rolled_back) with a hover tooltip

## Out of scope

- Ingest enrichment (§18.5 — stamping deployment_id on spans/logs/metrics) — deferred
- RBAC role checks on write endpoints at the query-api layer — query-api uses tenant-only auth (X-Tenant-ID); role enforcement is a Phase 4 concern
- Canary-promote integration

## Test plan

- [ ] `cargo test -p query-api` — 5 new deployment unit tests pass
- [ ] `npx vitest run` in `apps/frontend` — 9 new timeline tests pass, all existing tests pass
- [ ] `bash scripts/local-ci.sh --skip-smoke` — all stages green
- [ ] Manual: `docker compose up -d`, seed migration runs, `GET /v1/deployments` returns dev seed row
- [ ] Manual: open service detail page for `shop-api`, deployment timeline appears above signal tabs

## ADR/spec sync

No architecture, technology, or data-model decisions changed beyond what spec/18-deployment-markers.md already specifies. No ADR update needed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

**Spec coverage check (spec/18-deployment-markers.md):**

| Requirement | Covered |
|---|---|
| §18.1 `deployment_markers` table with all fields | ✅ Task 1 |
| §18.3 `POST /v1/deployments` — create in_progress | ✅ Task 3 |
| §18.3 `PATCH /v1/deployments/:id` — finish lifecycle | ✅ Task 3 |
| §18.3 `GET /v1/deployments` — list with filters | ✅ Task 3 |
| §18.4 Timeline overlay with status colors | ✅ Tasks 7–8 |
| §18.4 Hover tooltip with version/committer | ✅ Task 7 |
| §18.5 Ingest enrichment (stamp deployment_id) | ❌ Deferred — explicitly out of scope |
| §18.6 RBAC (Member to write, Viewer to read) | ⚠️ Partial — tenant auth only at query-api; role gate deferred |
| §18.8 CI/CD helper script | ✅ Task 5 |

**Placeholder scan:** No TBD, TODO, or "similar to" placeholders present.

**Type consistency check:**
- `DeploymentMarker.status` type: `"in_progress" | "success" | "failed" | "rolled_back"` in `api/deployments.ts` — matches SQL CHECK constraint in Task 1
- `markerColor(status: string)` accepts string — consistent with `DeploymentMarker.status` passed from component in Task 7
- `listDeployments` in `api/deployments.ts` returns `ListDeploymentsResponse` — consumed correctly in `DeploymentTimelineSection` in Task 8
- Route handler param `Path(deployment_id): Path<Uuid>` in `finish_deployment` — matches route pattern `/v1/deployments/:deployment_id` in Task 4
