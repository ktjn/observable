# Saved Views in Explorers — Logs Slice — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user save the current `LogSearch` explorer configuration (NLQ query, severity filter, message search, time range, visible columns) as a named, reloadable view — private to them or shared with the tenant — mirroring the existing dashboards' visibility/grant pattern.

**Architecture:** A new `saved_views` + `saved_view_grants` Postgres table pair and a `services/query-api/src/saved_views.rs` CRUD module, built by copying `dashboards.rs`'s proven ReBAC pattern (reusing its pure `grant_satisfies_read/write/delete` helpers directly — they take no dashboard-specific state). On the frontend, a new `SavedViewsControl` component plugs into `SignalExplorer`'s toolbar; `LogResultsTable` gains a minimal show/hide column toggle since a saved view needs something to persist for "columns."

**Tech Stack:** Rust (axum, sqlx, Postgres), React + TanStack Query, Vitest + Testing Library.

## Global Constraints

- Backend changes confined to: `migrations/postgres/038_create_saved_views.sql`, `services/query-api/src/saved_views.rs`, `services/query-api/src/main.rs`, `services/query-api/tests/it/http_api_integration.rs`.
- Frontend changes confined to: `apps/frontend/src/api/savedViews.ts`, `apps/frontend/src/features/signals/components/LogResultsTable.tsx` (+ its test file), `apps/frontend/src/features/signals/components/SavedViewsControl.tsx` (+ test), `apps/frontend/src/components/shared/SignalExplorer.tsx`, `apps/frontend/src/pages/LogSearch.tsx` (+ test).
- `signal_kind` is constrained to `'logs'` only in this slice (`CHECK (signal_kind IN ('logs'))`) — widen the constraint in the traces/metrics follow-up slices, do not pre-widen it now.
- Reuse `grant_satisfies_read`, `grant_satisfies_write`, `grant_satisfies_delete` from `crate::dashboards` — they are `pub(crate)` pure functions over `(visibility, relation)` / `(tenant_role, relation)`, not dashboard-specific. Do not duplicate them.
- `visibility` values: `'private'` (default) or `'public'`, matching dashboards.
- Grant `relation` values: `'owner'`, `'editor'`, `'viewer'`, matching dashboards.
- Run `cargo fmt --all` after every `.rs` edit, before staging.
- Run `cargo test -p query-api` (unit tests) after backend unit-test tasks; the Testcontainers integration test task additionally needs `cargo test -p query-api --test it` (requires Docker running).
- Run `npx vitest run <path>` after each frontend test-writing task; run `npm run typecheck` in `apps/frontend` before the final commit of each frontend task.
- No `.mdl` (Modelable) schema is introduced for `saved_views` — `config` stays an opaque `serde_json::Value` in Rust, matching how dashboards' `filters`/`time_range` fields are handled (see `services/query-api/src/dashboards.rs:1-21`).

---

## File Map

| File | Change |
|------|--------|
| `migrations/postgres/038_create_saved_views.sql` | New: `saved_views` + `saved_view_grants` tables |
| `services/query-api/src/saved_views.rs` | New: types, CRUD functions, permission-check wiring, HTTP handlers |
| `services/query-api/src/main.rs` | Add `mod saved_views;` + route registrations |
| `services/query-api/tests/it/http_api_integration.rs` | Add saved-views routes to the test router + one end-to-end test |
| `apps/frontend/src/api/savedViews.ts` | New: fetch/create/update/delete/grant client functions |
| `apps/frontend/src/features/signals/components/LogResultsTable.tsx` | Add `visibleColumns` prop (show/hide Level, Service) |
| `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx` | Tests for column visibility |
| `apps/frontend/src/features/signals/components/SavedViewsControl.tsx` | New: save/load/manage dropdown component |
| `apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx` | New: component tests |
| `apps/frontend/src/components/shared/SignalExplorer.tsx` | Add `savedViewsControl?: ReactNode` toolbar slot |
| `apps/frontend/src/pages/LogSearch.tsx` | Wire `LogViewConfig` state + `SavedViewsControl` into the toolbar |
| `apps/frontend/src/pages/LogSearch.test.tsx` | Test loading a saved view rehydrates state |
| `spec/05-frontend.md` | §9.11: mark Saved Views shipped (logs-only) |
| `spec/09-api.md` | Add `/v1/saved-views` REST resource section |
| `apps/frontend/src/features/signals/components/ColumnPickerControl.tsx` | New (Task 10): column show/hide popover |
| `apps/frontend/src/features/signals/components/SavedViewsControl.tsx` | Extended (Task 11): visibility toggle + grant management panel |

---

### Task 1: Postgres migration

**Files:**
- Create: `migrations/postgres/038_create_saved_views.sql`

**Interfaces:**
- Consumes: nothing
- Produces: `saved_views`, `saved_view_grants` tables, picked up automatically by `libs/test-support/src/postgres.rs`'s `apply_migrations` (reads every `.sql` file in `migrations/postgres/`, sorted by filename) and by `docker-compose.yml`'s `postgres-setup` service — no registration step needed beyond adding the file.

- [ ] **Step 1: Write the migration**

Create `migrations/postgres/038_create_saved_views.sql`:

```sql
CREATE TABLE IF NOT EXISTS saved_views (
    saved_view_id  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    owner_user_id  UUID        REFERENCES users(id) ON DELETE SET NULL,
    name           TEXT        NOT NULL,
    signal_kind    TEXT        NOT NULL CHECK (signal_kind IN ('logs')),
    visibility     TEXT        NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'public')),
    config         JSONB       NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS saved_views_tenant_signal_idx
    ON saved_views (tenant_id, signal_kind, created_at DESC);

CREATE TABLE IF NOT EXISTS saved_view_grants (
    saved_view_id UUID        NOT NULL REFERENCES saved_views(saved_view_id) ON DELETE CASCADE,
    user_id       UUID        NOT NULL REFERENCES users(id)                  ON DELETE CASCADE,
    relation      TEXT        NOT NULL CHECK (relation IN ('owner', 'editor', 'viewer')),
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (saved_view_id, user_id)
);

CREATE INDEX IF NOT EXISTS saved_view_grants_user_idx
    ON saved_view_grants (user_id, saved_view_id);
```

`owner_user_id` uses `ON DELETE SET NULL` (not `CASCADE`) so a deleted user's saved views survive as tenant-owned orphans, matching how dashboards has no owner column at all today and relies purely on grants — here we keep `owner_user_id` for display ("Created by X") but the grants table remains the source of truth for permissions.

- [ ] **Step 2: Apply and verify locally**

Run: `docker compose up postgres-setup 2>&1 | tail -20`
Expected: `Applying PostgreSQL migration: /migrations/postgres/038_create_saved_views.sql` with no errors. (If Docker isn't running locally, skip this manual check — Task 4's Testcontainers test will apply and exercise the migration.)

- [ ] **Step 3: Commit**

```bash
git add migrations/postgres/038_create_saved_views.sql
git commit -m "feat(query-api): add saved_views and saved_view_grants tables"
```

---

### Task 2: Backend types, validation, and CRUD functions

**Files:**
- Create: `services/query-api/src/saved_views.rs`
- Modify: `services/query-api/src/main.rs` — add `mod saved_views;`

**Interfaces:**
- Consumes: `crate::dashboards::{grant_satisfies_read, grant_satisfies_write, grant_satisfies_delete}` (existing pure functions), `crate::middleware::auth::TenantContext`, `crate::traces::AppState`.
- Produces: `SavedViewItem`, `CreateSavedViewRequest`, `UpdateSavedViewRequest`, `CreateSavedViewError`, `list_saved_views`, `get_saved_view`, `create_saved_view`, `update_saved_view`, `delete_saved_view` — consumed by Task 3's HTTP handlers.

- [ ] **Step 1: Write failing validation unit tests**

Create `services/query-api/src/saved_views.rs` with just the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_create_request_rejects_blank_name() {
        let req = CreateSavedViewRequest {
            name: "   ".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!({}),
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_invalid_signal_kind() {
        let req = CreateSavedViewRequest {
            name: "My view".into(),
            signal_kind: "traces".into(),
            config: serde_json::json!({}),
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_rejects_non_object_config() {
        let req = CreateSavedViewRequest {
            name: "My view".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!("not-an-object"),
        };
        assert!(validate_create_request(&req).is_err());
    }

    #[test]
    fn validate_create_request_accepts_valid_input() {
        let req = CreateSavedViewRequest {
            name: "My view".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!({"severity_filter": "error"}),
        };
        assert!(validate_create_request(&req).is_ok());
    }

    #[test]
    fn validate_update_request_rejects_invalid_visibility() {
        let req = UpdateSavedViewRequest {
            name: "My view".into(),
            config: serde_json::json!({}),
            visibility: Some("everyone".into()),
        };
        assert!(validate_update_request(&req).is_err());
    }

    #[test]
    fn validate_update_request_accepts_valid_visibility() {
        let req = UpdateSavedViewRequest {
            name: "My view".into(),
            config: serde_json::json!({}),
            visibility: Some("public".into()),
        };
        assert!(validate_update_request(&req).is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p query-api saved_views 2>&1 | tail -20`
Expected: compile errors — `CreateSavedViewRequest`, `UpdateSavedViewRequest`, `validate_create_request`, `validate_update_request` don't exist yet.

- [ ] **Step 3: Add types and validation above the test module**

At the top of `services/query-api/src/saved_views.rs`, above `#[cfg(test)]`:

```rust
use crate::dashboards::{grant_satisfies_delete, grant_satisfies_read, grant_satisfies_write};
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_SIGNAL_KINDS: &[&str] = &["logs"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SavedViewItem {
    pub saved_view_id: Uuid,
    pub name: String,
    pub signal_kind: String,
    pub visibility: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct SavedViewListResponse {
    pub items: Vec<SavedViewItem>,
}

#[derive(Deserialize)]
pub struct ListSavedViewsQuery {
    pub signal_kind: String,
}

#[derive(Deserialize)]
pub struct CreateSavedViewRequest {
    pub name: String,
    pub signal_kind: String,
    pub config: serde_json::Value,
}

#[derive(Deserialize)]
pub struct UpdateSavedViewRequest {
    pub name: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct AddGrantRequest {
    pub user_id: Uuid,
    pub relation: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct GrantItem {
    pub user_id: Uuid,
    pub relation: String,
    pub granted_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct GrantListResponse {
    pub grants: Vec<GrantItem>,
}

#[derive(Debug)]
pub enum SavedViewError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for SavedViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SavedViewError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            SavedViewError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for SavedViewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SavedViewError::Db(e) => Some(e),
            SavedViewError::InvalidInput(_) => None,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SavedViewRow {
    saved_view_id: Uuid,
    name: String,
    signal_kind: String,
    visibility: String,
    config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn row_to_item(row: SavedViewRow) -> SavedViewItem {
    SavedViewItem {
        saved_view_id: row.saved_view_id,
        name: row.name,
        signal_kind: row.signal_kind,
        visibility: row.visibility,
        config: row.config,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn validate_create_request(req: &CreateSavedViewRequest) -> Result<(), SavedViewError> {
    if req.name.trim().is_empty() {
        return Err(SavedViewError::InvalidInput("name is required".into()));
    }
    if !VALID_SIGNAL_KINDS.contains(&req.signal_kind.as_str()) {
        return Err(SavedViewError::InvalidInput(format!(
            "signal_kind must be one of: {}",
            VALID_SIGNAL_KINDS.join(", ")
        )));
    }
    if !req.config.is_object() {
        return Err(SavedViewError::InvalidInput(
            "config must be a JSON object".into(),
        ));
    }
    Ok(())
}

fn validate_update_request(req: &UpdateSavedViewRequest) -> Result<(), SavedViewError> {
    if req.name.trim().is_empty() {
        return Err(SavedViewError::InvalidInput("name is required".into()));
    }
    if !req.config.is_object() {
        return Err(SavedViewError::InvalidInput(
            "config must be a JSON object".into(),
        ));
    }
    if req
        .visibility
        .as_deref()
        .is_some_and(|v| !matches!(v, "public" | "private"))
    {
        return Err(SavedViewError::InvalidInput(
            "visibility must be 'public' or 'private'".into(),
        ));
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p query-api saved_views 2>&1 | tail -20`
Expected: all 6 validation tests pass.

- [ ] **Step 5: Add CRUD functions**

Insert above the `#[cfg(test)]` module, after the validation functions:

```rust
/// Fetch the relation a specific user holds on a specific saved view, if any.
async fn fetch_relation(
    db: &sqlx::PgPool,
    user_id: Uuid,
    saved_view_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT relation FROM saved_view_grants \
         WHERE saved_view_id = $1 AND user_id = $2",
    )
    .bind(saved_view_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

pub async fn list_saved_views(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
    signal_kind: &str,
) -> Result<Vec<SavedViewItem>, sqlx::Error> {
    // Same visibility rule as dashboards::list_dashboards: API-key callers
    // (user_id = None) see every tenant row; session users see public rows
    // plus rows they hold any grant on.
    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, SavedViewRow>(
            "SELECT saved_view_id, name, signal_kind, visibility, config, created_at, updated_at \
             FROM saved_views \
             WHERE tenant_id = $1 AND signal_kind = $2 \
               AND (visibility = 'public' \
                    OR EXISTS ( \
                        SELECT 1 FROM saved_view_grants \
                        WHERE saved_view_grants.saved_view_id = saved_views.saved_view_id \
                          AND user_id = $3 \
                    )) \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(signal_kind)
        .bind(uid)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, SavedViewRow>(
            "SELECT saved_view_id, name, signal_kind, visibility, config, created_at, updated_at \
             FROM saved_views \
             WHERE tenant_id = $1 AND signal_kind = $2 \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(signal_kind)
        .fetch_all(db)
        .await?
    };
    Ok(rows.into_iter().map(row_to_item).collect())
}

pub async fn get_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    saved_view_id: Uuid,
) -> Result<Option<SavedViewItem>, sqlx::Error> {
    let row = sqlx::query_as::<_, SavedViewRow>(
        "SELECT saved_view_id, name, signal_kind, visibility, config, created_at, updated_at \
         FROM saved_views \
         WHERE saved_view_id = $1 AND tenant_id = $2",
    )
    .bind(saved_view_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(row_to_item))
}

pub async fn create_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateSavedViewRequest,
    creator_user_id: Option<Uuid>,
) -> Result<SavedViewItem, SavedViewError> {
    validate_create_request(req)?;

    let mut tx = db.begin().await.map_err(SavedViewError::Db)?;
    let row = sqlx::query_as::<_, SavedViewRow>(
        "INSERT INTO saved_views (tenant_id, owner_user_id, name, signal_kind, config) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING saved_view_id, name, signal_kind, visibility, config, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(creator_user_id)
    .bind(req.name.trim())
    .bind(&req.signal_kind)
    .bind(&req.config)
    .fetch_one(&mut *tx)
    .await
    .map_err(SavedViewError::Db)?;

    if let Some(user_id) = creator_user_id {
        sqlx::query(
            "INSERT INTO saved_view_grants (saved_view_id, user_id, relation) \
             VALUES ($1, $2, 'owner') \
             ON CONFLICT (saved_view_id, user_id) DO UPDATE SET relation = 'owner'",
        )
        .bind(row.saved_view_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(SavedViewError::Db)?;
    }

    tx.commit().await.map_err(SavedViewError::Db)?;
    Ok(row_to_item(row))
}

pub async fn update_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    saved_view_id: Uuid,
    req: &UpdateSavedViewRequest,
) -> Result<Option<SavedViewItem>, SavedViewError> {
    validate_update_request(req)?;

    let row = if let Some(vis) = req.visibility.as_deref() {
        sqlx::query_as::<_, SavedViewRow>(
            "UPDATE saved_views SET name = $1, config = $2, visibility = $3, updated_at = NOW() \
             WHERE saved_view_id = $4 AND tenant_id = $5 \
             RETURNING saved_view_id, name, signal_kind, visibility, config, created_at, updated_at",
        )
        .bind(req.name.trim())
        .bind(&req.config)
        .bind(vis)
        .bind(saved_view_id)
        .bind(tenant_id)
        .fetch_optional(db)
        .await
        .map_err(SavedViewError::Db)?
    } else {
        sqlx::query_as::<_, SavedViewRow>(
            "UPDATE saved_views SET name = $1, config = $2, updated_at = NOW() \
             WHERE saved_view_id = $3 AND tenant_id = $4 \
             RETURNING saved_view_id, name, signal_kind, visibility, config, created_at, updated_at",
        )
        .bind(req.name.trim())
        .bind(&req.config)
        .bind(saved_view_id)
        .bind(tenant_id)
        .fetch_optional(db)
        .await
        .map_err(SavedViewError::Db)?
    };

    Ok(row.map(row_to_item))
}

pub async fn delete_saved_view(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    saved_view_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM saved_views WHERE saved_view_id = $1 AND tenant_id = $2")
        .bind(saved_view_id)
        .bind(tenant_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 6: Register the module**

In `services/query-api/src/main.rs`, find `mod dashboards;` and add directly after it:

```rust
mod saved_views;
```

- [ ] **Step 7: Run all query-api unit tests**

Run: `cargo test -p query-api 2>&1 | tail -30`
Expected: all tests pass, including the 6 new ones. `cargo build -p query-api` must also succeed (the CRUD functions aren't called anywhere yet, so expect `dead_code` warnings, not errors — handlers in Task 3 remove them).

- [ ] **Step 8: cargo fmt and commit**

```bash
cargo fmt --all
git add services/query-api/src/saved_views.rs services/query-api/src/main.rs
git commit -m "feat(query-api): add saved_views CRUD functions and validation"
```

---

### Task 3: HTTP handlers, grants, and router wiring

**Files:**
- Modify: `services/query-api/src/saved_views.rs` — add handlers
- Modify: `services/query-api/src/main.rs` — add routes

**Interfaces:**
- Consumes: Task 2's `list_saved_views`, `get_saved_view`, `create_saved_view`, `update_saved_view`, `delete_saved_view`, `SavedViewItem`, `SavedViewError`, plus `crate::dashboards::{grant_satisfies_read, grant_satisfies_write, grant_satisfies_delete}`.
- Produces: `handle_list_saved_views`, `handle_create_saved_view`, `handle_get_saved_view`, `handle_update_saved_view`, `handle_delete_saved_view`, `handle_list_saved_view_grants`, `handle_add_saved_view_grant`, `handle_revoke_saved_view_grant` — registered as routes, consumed by Task 4's integration test.

- [ ] **Step 1: Add handlers**

Append to `services/query-api/src/saved_views.rs`, after the CRUD functions and before `#[cfg(test)]`:

```rust
pub async fn handle_list_saved_views(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<ListSavedViewsQuery>,
) -> Result<Json<SavedViewListResponse>, StatusCode> {
    if !VALID_SIGNAL_KINDS.contains(&params.signal_kind.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let items = list_saved_views(&state.db, ctx.tenant_id, ctx.user_id, &params.signal_kind)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list saved views");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SavedViewListResponse { items }))
}

pub async fn handle_create_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateSavedViewRequest>,
) -> Result<(StatusCode, Json<SavedViewItem>), StatusCode> {
    match create_saved_view(&state.db, ctx.tenant_id, &req, ctx.user_id).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(SavedViewError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid saved view input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(SavedViewError::Db(e)) => {
            tracing::error!(error = %e, "failed to create saved view");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_get_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
) -> Result<Json<SavedViewItem>, StatusCode> {
    let item = match get_saved_view(&state.db, ctx.tenant_id, saved_view_id).await {
        Ok(Some(item)) => item,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get saved view");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, saved_view_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_read(&item.visibility, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(Json(item))
}

async fn saved_view_exists(
    db: &sqlx::PgPool,
    saved_view_id: Uuid,
    tenant_id: Uuid,
) -> Result<bool, StatusCode> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM saved_views WHERE saved_view_id = $1 AND tenant_id = $2)",
    )
    .bind(saved_view_id)
    .bind(tenant_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check saved view existence");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn handle_update_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
    Json(req): Json<UpdateSavedViewRequest>,
) -> Result<Json<SavedViewItem>, StatusCode> {
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, saved_view_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_write(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match update_saved_view(&state.db, ctx.tenant_id, saved_view_id, &req).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(SavedViewError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid saved view update");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(SavedViewError::Db(e)) => {
            tracing::error!(error = %e, "failed to update saved view");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_delete_saved_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, saved_view_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_delete(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match delete_saved_view(&state.db, ctx.tenant_id, saved_view_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete saved view");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_list_saved_view_grants(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
) -> Result<Json<GrantListResponse>, StatusCode> {
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let relation = fetch_relation(&state.db, user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let is_admin = ctx.role == "tenant_admin";
    let is_owner = relation.as_deref() == Some("owner");
    if !is_admin && !is_owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let grants = sqlx::query_as::<_, GrantItem>(
        "SELECT user_id, relation, granted_at \
         FROM saved_view_grants \
         WHERE saved_view_id = $1 \
         ORDER BY granted_at ASC",
    )
    .bind(saved_view_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to list grants");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(GrantListResponse { grants }))
}

pub async fn handle_add_saved_view_grant(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(saved_view_id): Path<Uuid>,
    Json(req): Json<AddGrantRequest>,
) -> Result<StatusCode, StatusCode> {
    if !matches!(req.relation.as_str(), "owner" | "editor" | "viewer") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let caller_relation = fetch_relation(&state.db, user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let is_admin = ctx.role == "tenant_admin";
    if !is_admin && caller_relation.as_deref() != Some("owner") {
        return Err(StatusCode::FORBIDDEN);
    }
    let target_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2)",
    )
    .bind(req.user_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to verify target user");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !target_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query(
        "INSERT INTO saved_view_grants (saved_view_id, user_id, relation) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (saved_view_id, user_id) DO UPDATE SET relation = EXCLUDED.relation",
    )
    .bind(saved_view_id)
    .bind(req.user_id)
    .bind(&req.relation)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to insert grant");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn handle_revoke_saved_view_grant(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((saved_view_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;
    if !saved_view_exists(&state.db, saved_view_id, ctx.tenant_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let caller_relation = fetch_relation(&state.db, user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch caller grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let is_admin = ctx.role == "tenant_admin";
    let is_owner = caller_relation.as_deref() == Some("owner");
    if !is_admin && !is_owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let target_relation = fetch_relation(&state.db, target_user_id, saved_view_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch target grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    // Atomic guard against deleting the last owner — see dashboards::handle_revoke_grant
    // for the identical TOCTOU-race rationale.
    let result = if target_relation.as_deref() == Some("owner") {
        sqlx::query(
            "WITH guard AS ( \
               SELECT COUNT(*) AS remaining \
               FROM saved_view_grants \
               WHERE saved_view_id = $1 AND relation = 'owner' AND user_id != $2 \
             ) \
             DELETE FROM saved_view_grants \
             WHERE saved_view_id = $1 AND user_id = $2 \
               AND (SELECT remaining FROM guard) > 0",
        )
        .bind(saved_view_id)
        .bind(target_user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query("DELETE FROM saved_view_grants WHERE saved_view_id = $1 AND user_id = $2")
            .bind(saved_view_id)
            .bind(target_user_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to delete grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };
    if result.rows_affected() == 0 {
        if target_relation.as_deref() == Some("owner") {
            return Err(StatusCode::CONFLICT);
        }
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 2: Register routes**

In `services/query-api/src/main.rs`, find the dashboard route block (`"/v1/dashboards"` ... `"/v1/dashboards/{id}/grants/{user_id}"`) and add directly after it:

```rust
        .route(
            "/v1/saved-views",
            get(saved_views::handle_list_saved_views).post(saved_views::handle_create_saved_view),
        )
        .route(
            "/v1/saved-views/{id}",
            get(saved_views::handle_get_saved_view)
                .put(saved_views::handle_update_saved_view)
                .delete(saved_views::handle_delete_saved_view),
        )
        .route(
            "/v1/saved-views/{id}/grants",
            get(saved_views::handle_list_saved_view_grants)
                .post(saved_views::handle_add_saved_view_grant),
        )
        .route(
            "/v1/saved-views/{id}/grants/{user_id}",
            axum::routing::delete(saved_views::handle_revoke_saved_view_grant),
        )
```

- [ ] **Step 3: Build and run all query-api tests**

Run: `cargo build -p query-api 2>&1 | tail -30`
Expected: builds cleanly, no more `dead_code` warnings for the saved_views functions (they're now wired into handlers).

Run: `cargo test -p query-api 2>&1 | tail -30`
Expected: all tests still pass.

- [ ] **Step 4: cargo fmt and commit**

```bash
cargo fmt --all
git add services/query-api/src/saved_views.rs services/query-api/src/main.rs
git commit -m "feat(query-api): add saved-views HTTP handlers and routes"
```

---

### Task 4: Backend Testcontainers integration test

**Files:**
- Modify: `services/query-api/tests/it/http_api_integration.rs`

**Interfaces:**
- Consumes: `saved_views` module (Task 2/3), `test_support::postgres::shared_pool()`, existing `build_app_with_pg`, `dev_request`, `response_body_json` helpers.
- Produces: nothing new — this is a verification-only task.

- [ ] **Step 1: Add saved-views routes to the test router**

In `services/query-api/tests/it/http_api_integration.rs`, update the import list (around line 12):

```rust
use query_api::{
    alerts, dashboards, discovery, incidents, llm_adapter, logs, metrics,
    middleware::auth::TenantContext, middleware::auth::require_tenant, observability,
    planner::QueryPlanner, reliability, saved_views, slos, traces,
};
```

Then find `build_app_with_pg_at`'s route chain and add, directly after the dashboard grant routes:

```rust
        .route(
            "/v1/saved-views",
            get(saved_views::handle_list_saved_views).post(saved_views::handle_create_saved_view),
        )
        .route(
            "/v1/saved-views/{id}",
            get(saved_views::handle_get_saved_view).put(saved_views::handle_update_saved_view),
        )
```

- [ ] **Step 2: Write the failing integration test**

Append to the end of `services/query-api/tests/it/http_api_integration.rs`, inside the existing test module (same file, not a separate `mod`):

```rust
#[tokio::test]
async fn saved_view_create_then_list_then_get_round_trips_config() {
    let (ch, _ch_container) = start_clickhouse().await;
    let pg = test_support::postgres::shared_pool().await;
    let app = build_app_with_pg(ch, pg.clone()).await;
    let tenant = Uuid::parse_str(DEV_TENANT_ID).unwrap();

    let created = saved_views::create_saved_view(
        &pg,
        tenant,
        &saved_views::CreateSavedViewRequest {
            name: "Errors in checkout".into(),
            signal_kind: "logs".into(),
            config: serde_json::json!({
                "query": null,
                "severity_filter": "error",
                "message_search": "",
                "time_range": {"mode": "preset", "preset": "1h"},
                "visible_columns": ["level", "service"]
            }),
        },
        None,
    )
    .await
    .expect("saved view created");

    let list_response = app
        .clone()
        .oneshot(dev_request("GET", "/v1/saved-views?signal_kind=logs"))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = response_body_json(list_response.into_body()).await;
    let items = list_body["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .any(|item| item["saved_view_id"] == created.saved_view_id.to_string())
    );

    let get_response = app
        .oneshot(dev_request(
            "GET",
            &format!("/v1/saved-views/{}", created.saved_view_id),
        ))
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body = response_body_json(get_response.into_body()).await;
    assert_eq!(get_body["name"], "Errors in checkout");
    assert_eq!(get_body["config"]["severity_filter"], "error");
    assert_eq!(get_body["config"]["visible_columns"][0], "level");
}
```

- [ ] **Step 3: Run the integration test**

Run: `cargo test -p query-api --test it saved_view_create_then_list_then_get 2>&1 | tail -40`
Expected: PASS. Requires Docker running locally (Testcontainers spins up Postgres + ClickHouse). If Docker is unavailable in this environment, note that in the task log instead of skipping the test permanently.

- [ ] **Step 4: Run the full integration suite to check for regressions**

Run: `cargo test -p query-api --test it 2>&1 | tail -60`
Expected: all tests pass.

- [ ] **Step 5: cargo fmt and commit**

```bash
cargo fmt --all
git add services/query-api/tests/it/http_api_integration.rs
git commit -m "test(query-api): add saved-views HTTP integration coverage"
```

---

### Task 5: Frontend API client

**Files:**
- Create: `apps/frontend/src/api/savedViews.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks (independent of backend task completion for typechecking, but needs the live routes to actually succeed at runtime).
- Produces: `SavedView`, `LogViewConfig`, `fetchSavedViews`, `createSavedView`, `updateSavedView`, `deleteSavedView`, `fetchSavedViewGrants`, `addSavedViewGrant`, `revokeSavedViewGrant` — consumed by Task 7's `SavedViewsControl`.

- [ ] **Step 1: Write the API client**

Create `apps/frontend/src/api/savedViews.ts`:

```typescript
function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type SignalKind = "logs";

export interface LogViewConfig {
  query: string | null;
  severity_filter: string;
  message_search: string;
  time_range: { mode: "preset"; preset: string } | { mode: "absolute"; from_ms: number; to_ms: number };
  visible_columns: string[];
}

export interface SavedView {
  saved_view_id: string;
  name: string;
  signal_kind: SignalKind;
  visibility: "private" | "public";
  config: LogViewConfig;
  created_at: string;
  updated_at: string;
}

export interface SavedViewListResponse {
  items: SavedView[];
}

export interface CreateSavedViewRequest {
  name: string;
  signal_kind: SignalKind;
  config: LogViewConfig;
}

export interface UpdateSavedViewRequest {
  name: string;
  config: LogViewConfig;
  visibility?: "private" | "public";
}

export interface GrantItem {
  user_id: string;
  relation: "owner" | "editor" | "viewer";
  granted_at: string;
}

export interface GrantListResponse {
  grants: GrantItem[];
}

export async function fetchSavedViews(tenantId: string, signalKind: SignalKind): Promise<SavedViewListResponse> {
  const res = await fetch(`/v1/saved-views?signal_kind=${signalKind}`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved views list failed: ${res.status}`);
  return res.json();
}

export async function createSavedView(tenantId: string, req: CreateSavedViewRequest): Promise<SavedView> {
  const res = await fetch("/v1/saved-views", {
    credentials: "include",
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Saved view create failed: ${res.status}`);
  return res.json();
}

export async function updateSavedView(
  tenantId: string,
  savedViewId: string,
  req: UpdateSavedViewRequest,
): Promise<SavedView> {
  const res = await fetch(`/v1/saved-views/${savedViewId}`, {
    credentials: "include",
    method: "PUT",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Saved view update failed: ${res.status}`);
  return res.json();
}

export async function deleteSavedView(tenantId: string, savedViewId: string): Promise<void> {
  const res = await fetch(`/v1/saved-views/${savedViewId}`, {
    credentials: "include",
    method: "DELETE",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved view delete failed: ${res.status}`);
}

export async function fetchSavedViewGrants(tenantId: string, savedViewId: string): Promise<GrantListResponse> {
  const res = await fetch(`/v1/saved-views/${savedViewId}/grants`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved view grants list failed: ${res.status}`);
  return res.json();
}

export async function addSavedViewGrant(
  tenantId: string,
  savedViewId: string,
  userId: string,
  relation: "owner" | "editor" | "viewer",
): Promise<void> {
  const res = await fetch(`/v1/saved-views/${savedViewId}/grants`, {
    credentials: "include",
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ user_id: userId, relation }),
  });
  if (!res.ok) throw new Error(`Saved view grant add failed: ${res.status}`);
}

export async function revokeSavedViewGrant(tenantId: string, savedViewId: string, userId: string): Promise<void> {
  const res = await fetch(`/v1/saved-views/${savedViewId}/grants/${userId}`, {
    credentials: "include",
    method: "DELETE",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Saved view grant revoke failed: ${res.status}`);
}
```

- [ ] **Step 2: Typecheck**

Run: `cd apps/frontend && npm run typecheck 2>&1 | tail -30`
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/api/savedViews.ts
git commit -m "feat(frontend): add saved-views API client"
```

---

### Task 6: Column-visibility toggle on `LogResultsTable`

**Files:**
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.tsx`
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `LogResultsTable`'s new `visibleColumns?: ("level" | "service")[]` prop (`undefined` = show all, preserving current default behavior) — consumed by Task 7/8's `SavedViewsControl` config and `LogSearch`.

- [ ] **Step 1: Write the failing test**

Read the existing test file first to match its setup conventions, then add a new test to `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx` (append inside the existing test block structure, following whatever `describe`/`test` pattern the file already uses):

```typescript
test("hides the Level column when visibleColumns omits it", () => {
  render(
    <LogResultsTable
      logs={[sampleLog]}
      selectedLogId={undefined}
      onSelectLog={vi.fn()}
      timeFormat="local"
      visibleColumns={["service"]}
    />,
  );
  expect(screen.queryByRole("columnheader", { name: "Level" })).not.toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
});

test("shows all columns when visibleColumns is omitted", () => {
  render(
    <LogResultsTable
      logs={[sampleLog]}
      selectedLogId={undefined}
      onSelectLog={vi.fn()}
      timeFormat="local"
    />,
  );
  expect(screen.getByRole("columnheader", { name: "Level" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
});
```

Note: adapt `sampleLog` to whatever fixture/import the existing test file already uses for a `LogRecord` — do not invent a new one if one exists in that file.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/LogResultsTable.test.tsx 2>&1 | tail -40`
Expected: FAIL — `visibleColumns` prop doesn't exist, "Level" column always renders.

- [ ] **Step 3: Add the prop**

In `apps/frontend/src/features/signals/components/LogResultsTable.tsx`, replace the full file content with:

```typescript
import type { LogRecord } from "../../../api/logs";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import { formatTimestamp } from "../../../utils/formatTimestamp";
import { formatLogMessage, getSeverityColor, otelSeverity } from "../../../utils/logFormatting";
import type { TimeFormat } from "../../../lib/timeDisplay";

export type LogTableColumn = "level" | "service";

export function LogResultsTable({
  logs,
  selectedLogId,
  onSelectLog,
  timeFormat,
  showServiceColumn = true,
  visibleColumns,
  ariaLabel = showServiceColumn ? "Log results" : "Service logs",
}: {
  logs: LogRecord[];
  selectedLogId: string | undefined;
  onSelectLog: (logId: string) => void;
  timeFormat: TimeFormat;
  showServiceColumn?: boolean;
  /** When set, restricts optional columns (Level, Service) to this list. Time and Message always show. */
  visibleColumns?: LogTableColumn[];
  ariaLabel?: string;
}) {
  const showLevel = visibleColumns === undefined || visibleColumns.includes("level");
  const showService = showServiceColumn && (visibleColumns === undefined || visibleColumns.includes("service"));

  return (
    <VirtualTable
      rows={logs}
      ariaLabel={ariaLabel}
      renderHead={() => (
        <tr>
          <th aria-label="Time">Time</th>
          {showLevel && <th>Level</th>}
          {showService && <th>Service</th>}
          <th>Message</th>
        </tr>
      )}
      renderRow={(log, ref, index) => (
        <LogResultsRow
          key={log.log_id}
          log={log}
          timeFormat={timeFormat}
          selected={selectedLogId === log.log_id}
          onSelect={() => onSelectLog(log.log_id)}
          showLevel={showLevel}
          showServiceColumn={showService}
          measureRef={ref}
          index={index}
        />
      )}
    />
  );
}

function LogResultsRow({
  log,
  timeFormat,
  selected,
  onSelect,
  showLevel,
  showServiceColumn,
  measureRef,
  index,
}: {
  log: LogRecord;
  timeFormat: TimeFormat;
  selected: boolean;
  onSelect: () => void;
  showLevel: boolean;
  showServiceColumn: boolean;
  measureRef: (el: Element | null) => void;
  index: number;
}) {
  const severity = otelSeverity(log.severity_number);
  const message = formatLogMessage(log.body);

  const accentClass =
    severity.tone === "bad"
      ? "border-l-2 border-l-[var(--bad)]"
      : severity.tone === "warn"
        ? "border-l-2 border-l-[var(--warn)]"
        : "";

  return (
    <tr
      ref={measureRef}
      data-index={index}
      className={`modern-table-row cursor-pointer ${accentClass} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      tabIndex={0}
      role="button"
      aria-label={`Open log context for ${message}`}
      aria-pressed={selected}
    >
      <td className="whitespace-nowrap">{formatTimestamp(log.timestamp_unix_nano, timeFormat)}</td>
      {showLevel && (
        <td>
          <span
            className="text-[9px] font-bold uppercase tracking-wide"
            style={{ color: getSeverityColor(log.severity_number) }}
          >
            {severity.label}
          </span>
        </td>
      )}
      {showServiceColumn && <td>{log.service_name}</td>}
      <td className="whitespace-normal break-all">{message}</td>
    </tr>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/LogResultsTable.test.tsx 2>&1 | tail -40`
Expected: all tests pass, including the 2 new ones.

- [ ] **Step 5: Typecheck and commit**

```bash
cd apps/frontend && npm run typecheck 2>&1 | tail -30
git add apps/frontend/src/features/signals/components/LogResultsTable.tsx apps/frontend/src/features/signals/components/LogResultsTable.test.tsx
git commit -m "feat(frontend): add column-visibility toggle to LogResultsTable"
```

---

### Task 7: `SavedViewsControl` component

**Files:**
- Create: `apps/frontend/src/features/signals/components/SavedViewsControl.tsx`
- Create: `apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx`

**Interfaces:**
- Consumes: Task 5's `fetchSavedViews`, `createSavedView`, `updateSavedView`, `deleteSavedView`, `SavedView`, `LogViewConfig`.
- Produces: `SavedViewsControl` React component with props `{ tenantId: string; currentConfig: LogViewConfig; onLoad: (config: LogViewConfig) => void }` — consumed by Task 8's `LogSearch` wiring.

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx`:

```typescript
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, test, vi } from "vitest";
import { SavedViewsControl } from "./SavedViewsControl";
import type { LogViewConfig, SavedView } from "../../../api/savedViews";

const baseConfig: LogViewConfig = {
  query: null,
  severity_filter: "all",
  message_search: "",
  time_range: { mode: "preset", preset: "1h" },
  visible_columns: ["level", "service"],
};

const savedView: SavedView = {
  saved_view_id: "view-1",
  name: "Errors in checkout",
  signal_kind: "logs",
  visibility: "private",
  config: { ...baseConfig, severity_filter: "error" },
  created_at: "2026-07-01T00:00:00Z",
  updated_at: "2026-07-01T00:00:00Z",
};

vi.mock("../../../api/savedViews", async () => {
  const actual = await vi.importActual<typeof import("../../../api/savedViews")>("../../../api/savedViews");
  return {
    ...actual,
    fetchSavedViews: vi.fn(async () => ({ items: [savedView] })),
    createSavedView: vi.fn(async () => savedView),
    deleteSavedView: vi.fn(async () => undefined),
  };
});

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

test("loading a saved view calls onLoad with its config", async () => {
  const onLoad = vi.fn();
  render(
    <SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={onLoad} />,
    { wrapper },
  );

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByText("Errors in checkout"));

  expect(onLoad).toHaveBeenCalledWith(savedView.config);
});

test("saving the current view calls createSavedView with the current config", async () => {
  const { createSavedView } = await import("../../../api/savedViews");
  render(
    <SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />,
    { wrapper },
  );

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /save current view/i }));
  fireEvent.change(screen.getByLabelText(/view name/i), { target: { value: "My new view" } });
  fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

  await waitFor(() =>
    expect(createSavedView).toHaveBeenCalledWith("tenant-1", {
      name: "My new view",
      signal_kind: "logs",
      config: baseConfig,
    }),
  );
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/SavedViewsControl.test.tsx 2>&1 | tail -40`
Expected: FAIL — `./SavedViewsControl` module doesn't exist.

- [ ] **Step 3: Implement the component**

Create `apps/frontend/src/features/signals/components/SavedViewsControl.tsx`:

```typescript
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../../components/ui/button";
import {
  createSavedView,
  deleteSavedView,
  fetchSavedViews,
  type LogViewConfig,
} from "../../../api/savedViews";

export interface SavedViewsControlProps {
  tenantId: string;
  currentConfig: LogViewConfig;
  onLoad: (config: LogViewConfig) => void;
}

export function SavedViewsControl({ tenantId, currentConfig, onLoad }: SavedViewsControlProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [newViewName, setNewViewName] = useState("");
  const queryClient = useQueryClient();

  const { data } = useQuery({
    queryKey: ["saved-views", tenantId, "logs"],
    queryFn: () => fetchSavedViews(tenantId, "logs"),
    enabled: isOpen,
  });
  const views = data?.items ?? [];

  const createMutation = useMutation({
    mutationFn: (name: string) =>
      createSavedView(tenantId, { name, signal_kind: "logs", config: currentConfig }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
      setIsSaving(false);
      setNewViewName("");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (savedViewId: string) => deleteSavedView(tenantId, savedViewId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
    },
  });

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Saved Views
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-72 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          <ul className="max-h-60 overflow-y-auto">
            {views.map((view) => (
              <li key={view.saved_view_id} className="flex items-center justify-between gap-2 py-1">
                <button
                  type="button"
                  className="flex-1 text-left text-sm hover:text-[var(--brand)]"
                  onClick={() => {
                    onLoad(view.config);
                    setIsOpen(false);
                  }}
                >
                  {view.name}
                </button>
                <button
                  type="button"
                  aria-label={`Delete ${view.name}`}
                  className="text-xs text-[var(--muted)] hover:text-[var(--bad)]"
                  onClick={() => deleteMutation.mutate(view.saved_view_id)}
                >
                  Delete
                </button>
              </li>
            ))}
            {views.length === 0 && (
              <li className="py-1 text-xs text-[var(--muted)]">No saved views yet.</li>
            )}
          </ul>
          <div className="mt-2 border-t border-[var(--border)] pt-2">
            {isSaving ? (
              <div className="flex flex-col gap-2">
                <label className="text-xs text-[var(--muted)]" htmlFor="saved-view-name">
                  View name
                </label>
                <input
                  id="saved-view-name"
                  type="text"
                  value={newViewName}
                  onChange={(e) => setNewViewName(e.target.value)}
                  className="border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-xs"
                />
                <Button
                  variant="primary"
                  disabled={!newViewName.trim() || createMutation.isPending}
                  onClick={() => createMutation.mutate(newViewName.trim())}
                >
                  Save
                </Button>
              </div>
            ) : (
              <Button variant="secondary" onClick={() => setIsSaving(true)}>
                Save current view
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/SavedViewsControl.test.tsx 2>&1 | tail -40`
Expected: both tests pass.

- [ ] **Step 5: Typecheck and commit**

```bash
cd apps/frontend && npm run typecheck 2>&1 | tail -30
git add apps/frontend/src/features/signals/components/SavedViewsControl.tsx apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx
git commit -m "feat(frontend): add SavedViewsControl component"
```

---

### Task 8: Wire `SavedViewsControl` into `LogSearch`

**Files:**
- Modify: `apps/frontend/src/components/shared/SignalExplorer.tsx` — add a toolbar slot
- Modify: `apps/frontend/src/pages/LogSearch.tsx` — build `LogViewConfig`, render the control, apply loaded config
- Modify: `apps/frontend/src/pages/LogSearch.test.tsx` — cover the load flow

**Interfaces:**
- Consumes: Task 7's `SavedViewsControl`, Task 6's `LogResultsTable` `visibleColumns` prop, Task 5's `LogViewConfig` type.
- Produces: nothing new for later tasks — this is the integration point.

- [ ] **Step 1: Write the failing test**

In `apps/frontend/src/pages/LogSearch.test.tsx`, add (after the existing mocks, following the file's existing `vi.mock` conventions):

```typescript
vi.mock("../api/savedViews", async () => {
  const actual = await vi.importActual<typeof import("../api/savedViews")>("../api/savedViews");
  return {
    ...actual,
    fetchSavedViews: vi.fn(async () => ({
      items: [
        {
          saved_view_id: "view-1",
          name: "Only errors",
          signal_kind: "logs",
          visibility: "private",
          config: {
            query: null,
            severity_filter: "error",
            message_search: "timeout",
            time_range: { mode: "preset", preset: "1h" },
            visible_columns: ["level"],
          },
          created_at: "2026-07-01T00:00:00Z",
          updated_at: "2026-07-01T00:00:00Z",
        },
      ],
    })),
  };
});
```

Then add a new test in the same file (matching the existing `test(...)` style used there):

```typescript
test("loading a saved view applies its severity filter and message search", async () => {
  render(<LogSearch />, { wrapper: /* reuse the same wrapper the existing tests in this file use */ });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Only errors"));
  fireEvent.click(screen.getByText("Only errors"));

  await waitFor(() => {
    expect(screen.getByLabelText("Search log messages")).toHaveValue("timeout");
  });
});
```

Note: match the exact render/wrapper helper already defined in this test file (e.g. a local `renderWithProviders` function) instead of inlining a new one — read the file's existing tests to find it.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx 2>&1 | tail -40`
Expected: FAIL — no "Saved views" button rendered yet.

- [ ] **Step 3: Add the toolbar slot to `SignalExplorer`**

In `apps/frontend/src/components/shared/SignalExplorer.tsx`, add to `SignalExplorerProps` (after `onPromote: () => void;`):

```typescript
  savedViewsControl?: ReactNode;
```

Add `savedViewsControl,` to the destructured props in the function signature (after `onPromote,`), and render it in the toolbar row — replace:

```typescript
        {showPromote && (
          <>
            <Button variant="secondary" onClick={onPromote} disabled={saveStatus === "saving"}>
              Promote to dashboard
            </Button>
            {saveStatus === "saved" && (
              <span className="text-sm font-semibold text-[var(--good)]">Saved to dashboard</span>
            )}
            {saveStatus === "error" && (
              <span className="text-sm font-semibold text-[var(--bad)]">Dashboard save failed</span>
            )}
          </>
        )}
```

with:

```typescript
        {savedViewsControl}
        {showPromote && (
          <>
            <Button variant="secondary" onClick={onPromote} disabled={saveStatus === "saving"}>
              Promote to dashboard
            </Button>
            {saveStatus === "saved" && (
              <span className="text-sm font-semibold text-[var(--good)]">Saved to dashboard</span>
            )}
            {saveStatus === "error" && (
              <span className="text-sm font-semibold text-[var(--bad)]">Dashboard save failed</span>
            )}
          </>
        )}
```

- [ ] **Step 4: Wire it into `LogSearch`**

In `apps/frontend/src/pages/LogSearch.tsx`, add to the imports (after the `LogResultsTable` import):

```typescript
import { SavedViewsControl } from "../features/signals/components/SavedViewsControl";
import type { LogViewConfig } from "../api/savedViews";
```

Add state for visible columns, after the `isLive` state declaration:

```typescript
  const [visibleColumns, setVisibleColumns] = useState<("level" | "service")[]>(["level", "service"]);
```

Build the current config and a load handler, placed just before the `return (` statement:

```typescript
  const currentViewConfig: LogViewConfig = {
    query: userQuery,
    severity_filter: severityFilter,
    message_search: messageSearch,
    time_range:
      preset != null
        ? { mode: "preset", preset }
        : { mode: "absolute", from_ms: fromMs, to_ms: toMs },
    visible_columns: visibleColumns,
  };

  const handleLoadView = (config: LogViewConfig) => {
    setUserQuery(config.query);
    setSeverityFilter(config.severity_filter as SeverityFilter);
    setMessageSearch(config.message_search);
    if (config.time_range.mode === "preset") {
      setPreset(config.time_range.preset as Parameters<typeof setPreset>[0]);
    } else {
      setCustomRange(config.time_range.from_ms, config.time_range.to_ms);
    }
    setVisibleColumns(config.visible_columns.filter((c): c is "level" | "service" => c === "level" || c === "service"));
  };
```

`preset` and `setPreset` come from `useGlobalDateRange()` — update that destructuring near the top of `LogExplorer` from:

```typescript
  const { fromMs, toMs, setCustomRange } = useGlobalDateRange();
```

to:

```typescript
  const { preset, fromMs, toMs, setPreset, setCustomRange } = useGlobalDateRange();
```

Pass the control into `SignalExplorer` — add this prop to the `<SignalExplorer ...>` call (alongside `onPromote={handlePromote}`):

```typescript
      savedViewsControl={
        <SavedViewsControl tenantId={tenantId} currentConfig={currentViewConfig} onLoad={handleLoadView} />
      }
```

Finally, pass `visibleColumns` through to `LogResultsTable` — update its usage inside `renderTable`:

```typescript
                  <LogResultsTable
                    logs={displayedLogs}
                    selectedLogId={selectedId ?? undefined}
                    onSelectLog={(id) => onSelect(id)}
                    timeFormat={format}
                    showServiceColumn={showServiceColumn}
                    visibleColumns={visibleColumns}
                    ariaLabel={tableAriaLabel}
                  />
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx 2>&1 | tail -40`
Expected: all tests pass, including the new one.

- [ ] **Step 6: Run the full frontend test suite for regressions**

Run: `cd apps/frontend && npx vitest run 2>&1 | tail -60`
Expected: no regressions in `SignalExplorer`-consuming pages (`TraceSearch.tsx`, `MetricsSearch.tsx` — they don't pass `savedViewsControl`, so it's `undefined` and renders nothing, matching current behavior).

- [ ] **Step 7: Typecheck and commit**

```bash
cd apps/frontend && npm run typecheck 2>&1 | tail -30
git add apps/frontend/src/components/shared/SignalExplorer.tsx apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/LogSearch.test.tsx
git commit -m "feat(frontend): wire Saved Views control into LogSearch"
```

---

### Task 9: Spec sync

**Files:**
- Modify: `spec/05-frontend.md`
- Modify: `spec/09-api.md`

**Interfaces:**
- Consumes: nothing (documentation only)
- Produces: nothing (documentation only)

- [ ] **Step 1: Update `spec/05-frontend.md` §9.11**

Find the line:

```
**Saved views**: Named bookmarks for search configurations (filter set + time range + column selection), scoped per user or shared within project
```

Replace with:

```
**Saved views**: Named bookmarks for search configurations (filter set + time range + column selection), scoped per user or shared within project. **Shipped for the logs explorer** (`LogSearch.tsx`) — see `docs/superpowers/plans/2026-07-03-saved-views-logs.md` and `docs/superpowers/specs/2026-07-03-saved-views-logs-design.md`. Traces and metrics explorers are follow-up slices.
```

- [ ] **Step 2: Add the API section to `spec/09-api.md`**

Find §14.2 (the Dashboard/Alert Rule API section referenced in the roadmap) and add a new subsection directly after it, following the same `Endpoints:` / `Behavior:` / `Auth:` format used there:

```markdown
### Saved Views API

Endpoints: `GET/POST /v1/saved-views`, `GET/PUT/DELETE /v1/saved-views/{id}`,
`GET/POST /v1/saved-views/{id}/grants`, `DELETE /v1/saved-views/{id}/grants/{user_id}`.

Behavior: Mirrors the Dashboards API's visibility/grant model exactly —
`visibility` is `private` (default) or `public`; grants carry `owner`/`editor`/`viewer`
relations. `signal_kind` is currently restricted to `logs` (widens to `traces`/`metrics`
in follow-up slices). `config` is an opaque JSON object interpreted only by the
frontend — the backend validates it is a JSON object and stores/returns it verbatim.

Auth: Requires `require_tenant` middleware (tenant-scoped). API-key callers see all
tenant saved views; session users see public views plus views they hold a grant on,
identical to dashboards.
```

- [ ] **Step 3: Commit**

```bash
git add spec/05-frontend.md spec/09-api.md
git commit -m "docs(spec): document Saved Views (logs slice) API and frontend behavior"
```

---

### Task 10: Column-picker UI

**Added after final review** (`docs/superpowers/plans/2026-07-03-saved-views-logs.md` final whole-branch review): Task 6 added the `visibleColumns` prop to `LogResultsTable` but no UI ever changed it, so the "column selection" capability from the design doc was inert end-to-end. This task closes that gap. It also fixes a real bug found in the same review: `handleLoadView` (Task 8) never resets `service`, so a stale service filter can linger after loading a saved view.

**Files:**
- Create: `apps/frontend/src/features/signals/components/ColumnPickerControl.tsx`
- Create: `apps/frontend/src/features/signals/components/ColumnPickerControl.test.tsx`
- Modify: `apps/frontend/src/pages/LogSearch.tsx` — render `ColumnPickerControl` alongside `SavedViewsControl`; fix `handleLoadView` to reset `service`

**Interfaces:**
- Consumes: `LogTableColumn` type already exported from `apps/frontend/src/features/signals/components/LogResultsTable.tsx` (Task 6).
- Produces: `ColumnPickerControl` component — consumed only by `LogSearch.tsx` in this task.

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/features/signals/components/ColumnPickerControl.test.tsx`:

```typescript
import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ColumnPickerControl } from "./ColumnPickerControl";

test("toggling an unchecked column adds it to visibleColumns", () => {
  const onChange = vi.fn();
  render(<ColumnPickerControl visibleColumns={["service"]} onChange={onChange} />);

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Level"));

  expect(onChange).toHaveBeenCalledWith(["service", "level"]);
});

test("toggling a checked column removes it from visibleColumns", () => {
  const onChange = vi.fn();
  render(<ColumnPickerControl visibleColumns={["level", "service"]} onChange={onChange} />);

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Service"));

  expect(onChange).toHaveBeenCalledWith(["level"]);
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/ColumnPickerControl.test.tsx 2>&1 | tail -40`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement the component**

Create `apps/frontend/src/features/signals/components/ColumnPickerControl.tsx`:

```typescript
import { useState } from "react";
import { Button } from "../../../components/ui/button";
import type { LogTableColumn } from "./LogResultsTable";

const COLUMN_LABELS: Record<LogTableColumn, string> = {
  level: "Level",
  service: "Service",
};

const ALL_COLUMNS: LogTableColumn[] = ["level", "service"];

export interface ColumnPickerControlProps {
  visibleColumns: LogTableColumn[];
  onChange: (columns: LogTableColumn[]) => void;
}

export function ColumnPickerControl({ visibleColumns, onChange }: ColumnPickerControlProps) {
  const [isOpen, setIsOpen] = useState(false);

  function toggle(column: LogTableColumn) {
    if (visibleColumns.includes(column)) {
      onChange(visibleColumns.filter((c) => c !== column));
    } else {
      onChange([...visibleColumns, column]);
    }
  }

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Columns
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-40 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          {ALL_COLUMNS.map((column) => (
            <label key={column} className="flex items-center gap-2 py-1 text-sm">
              <input
                type="checkbox"
                checked={visibleColumns.includes(column)}
                onChange={() => toggle(column)}
              />
              {COLUMN_LABELS[column]}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/ColumnPickerControl.test.tsx 2>&1 | tail -40`
Expected: both tests pass.

- [ ] **Step 5: Wire it into `LogSearch.tsx` and fix the stale-`service` bug**

In `apps/frontend/src/pages/LogSearch.tsx`, add to the imports (after the `SavedViewsControl` import):

```typescript
import { ColumnPickerControl } from "../features/signals/components/ColumnPickerControl";
```

Replace the `savedViewsControl` prop value passed to `<SignalExplorer>`:

```typescript
      savedViewsControl={
        <SavedViewsControl tenantId={tenantId} currentConfig={currentViewConfig} onLoad={handleLoadView} />
      }
```

with:

```typescript
      savedViewsControl={
        <>
          <ColumnPickerControl visibleColumns={visibleColumns} onChange={setVisibleColumns} />
          <SavedViewsControl tenantId={tenantId} currentConfig={currentViewConfig} onLoad={handleLoadView} />
        </>
      }
```

Fix the stale-`service` bug in `handleLoadView` — replace:

```typescript
  const handleLoadView = (config: LogViewConfig) => {
    setUserQuery(config.query);
    setSeverityFilter(config.severity_filter as SeverityFilter);
    setMessageSearch(config.message_search);
```

with:

```typescript
  const handleLoadView = (config: LogViewConfig) => {
    setUserQuery(config.query);
    setService("");
    setSeverityFilter(config.severity_filter as SeverityFilter);
    setMessageSearch(config.message_search);
```

- [ ] **Step 6: Run the full frontend suite and typecheck**

Run: `cd apps/frontend && npx vitest run 2>&1 | tail -60`
Expected: no regressions.

Run: `cd apps/frontend && npm run typecheck 2>&1 | tail -30`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/features/signals/components/ColumnPickerControl.tsx apps/frontend/src/features/signals/components/ColumnPickerControl.test.tsx apps/frontend/src/pages/LogSearch.tsx
git commit -m "feat(frontend): add column-picker UI and fix stale service filter on view load"
```

---

### Task 11: Sharing UI (visibility + grants) in `SavedViewsControl`

**Added after final review**: the backend already supports `visibility` toggling and per-user grants (Tasks 2-3), and the frontend API client already has `updateSavedView`/`fetchSavedViewGrants`/`addSavedViewGrant`/`revokeSavedViewGrant` (Task 5), but nothing in the UI called them — every saved view was permanently private. This task adds a "Manage" panel per view: a visibility toggle and a minimal grant list with an add-by-user-id form, per the design doc's documented fallback ("a minimal list+add-user-by-id form").

**Files:**
- Modify: `apps/frontend/src/features/signals/components/SavedViewsControl.tsx`
- Modify: `apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx`

**Interfaces:**
- Consumes: `updateSavedView`, `fetchSavedViewGrants`, `addSavedViewGrant`, `revokeSavedViewGrant` from `apps/frontend/src/api/savedViews.ts` (Task 5, already present, currently unused).
- Produces: nothing new for later tasks — this is the final UI surface for this slice.

- [ ] **Step 1: Write the failing tests**

Add to `apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx`, extending the existing `vi.mock("../../../api/savedViews", ...)` block to also mock the grants functions:

```typescript
vi.mock("../../../api/savedViews", async () => {
  const actual = await vi.importActual<typeof import("../../../api/savedViews")>("../../../api/savedViews");
  return {
    ...actual,
    fetchSavedViews: vi.fn(async () => ({ items: [savedView] })),
    createSavedView: vi.fn(async () => savedView),
    deleteSavedView: vi.fn(async () => undefined),
    updateSavedView: vi.fn(async () => ({ ...savedView, visibility: "public" })),
    fetchSavedViewGrants: vi.fn(async () => ({ grants: [] })),
    addSavedViewGrant: vi.fn(async () => undefined),
    revokeSavedViewGrant: vi.fn(async () => undefined),
  };
});
```

Then add two new tests to the same file:

```typescript
test("toggling visibility calls updateSavedView with the flipped value", async () => {
  const { updateSavedView } = await import("../../../api/savedViews");
  render(<SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /manage errors in checkout/i }));
  await waitFor(() => screen.getByRole("button", { name: /make public/i }));
  fireEvent.click(screen.getByRole("button", { name: /make public/i }));

  await waitFor(() =>
    expect(updateSavedView).toHaveBeenCalledWith("tenant-1", savedView.saved_view_id, {
      name: savedView.name,
      config: savedView.config,
      visibility: "public",
    }),
  );
});

test("adding a grant calls addSavedViewGrant with the entered user id and relation", async () => {
  const { addSavedViewGrant } = await import("../../../api/savedViews");
  render(<SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /manage errors in checkout/i }));
  await waitFor(() => screen.getByPlaceholderText("User ID"));
  fireEvent.change(screen.getByPlaceholderText("User ID"), { target: { value: "user-42" } });
  fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

  await waitFor(() =>
    expect(addSavedViewGrant).toHaveBeenCalledWith("tenant-1", savedView.saved_view_id, "user-42", "viewer"),
  );
});
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/SavedViewsControl.test.tsx 2>&1 | tail -60`
Expected: FAIL — no "Manage" button rendered yet.

- [ ] **Step 3: Implement the Manage panel**

Replace the full content of `apps/frontend/src/features/signals/components/SavedViewsControl.tsx` with:

```typescript
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../../components/ui/button";
import {
  addSavedViewGrant,
  createSavedView,
  deleteSavedView,
  fetchSavedViewGrants,
  fetchSavedViews,
  revokeSavedViewGrant,
  updateSavedView,
  type LogViewConfig,
  type SavedView,
} from "../../../api/savedViews";

export interface SavedViewsControlProps {
  tenantId: string;
  currentConfig: LogViewConfig;
  onLoad: (config: LogViewConfig) => void;
}

export function SavedViewsControl({ tenantId, currentConfig, onLoad }: SavedViewsControlProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [newViewName, setNewViewName] = useState("");
  const [managingViewId, setManagingViewId] = useState<string | null>(null);
  const [newGrantUserId, setNewGrantUserId] = useState("");
  const [newGrantRelation, setNewGrantRelation] = useState<"owner" | "editor" | "viewer">("viewer");
  const queryClient = useQueryClient();

  const { data } = useQuery({
    queryKey: ["saved-views", tenantId, "logs"],
    queryFn: () => fetchSavedViews(tenantId, "logs"),
    enabled: isOpen,
  });
  const views = data?.items ?? [];
  const managingView = views.find((v) => v.saved_view_id === managingViewId) ?? null;

  const { data: grantsData } = useQuery({
    queryKey: ["saved-view-grants", tenantId, managingViewId],
    queryFn: () => fetchSavedViewGrants(tenantId, managingViewId as string),
    enabled: managingViewId !== null,
  });
  const grants = grantsData?.grants ?? [];

  const createMutation = useMutation({
    mutationFn: (name: string) =>
      createSavedView(tenantId, { name, signal_kind: "logs", config: currentConfig }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
      setIsSaving(false);
      setNewViewName("");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (savedViewId: string) => deleteSavedView(tenantId, savedViewId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
    },
  });

  const visibilityMutation = useMutation({
    mutationFn: (view: SavedView) =>
      updateSavedView(tenantId, view.saved_view_id, {
        name: view.name,
        config: view.config,
        visibility: view.visibility === "private" ? "public" : "private",
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-views", tenantId, "logs"] });
    },
  });

  const addGrantMutation = useMutation({
    mutationFn: () =>
      addSavedViewGrant(tenantId, managingViewId as string, newGrantUserId.trim(), newGrantRelation),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-view-grants", tenantId, managingViewId] });
      setNewGrantUserId("");
    },
  });

  const revokeGrantMutation = useMutation({
    mutationFn: (userId: string) => revokeSavedViewGrant(tenantId, managingViewId as string, userId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["saved-view-grants", tenantId, managingViewId] });
    },
  });

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Saved Views
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-80 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          <ul className="max-h-72 overflow-y-auto">
            {views.map((view) => (
              <li key={view.saved_view_id} className="flex flex-col gap-1 border-b border-[var(--border)] py-1 last:border-0">
                <div className="flex items-center justify-between gap-2">
                  <button
                    type="button"
                    className="flex-1 text-left text-sm hover:text-[var(--brand)]"
                    onClick={() => {
                      onLoad(view.config);
                      setIsOpen(false);
                    }}
                  >
                    {view.name}
                  </button>
                  <span className="text-[10px] uppercase text-[var(--muted)]">{view.visibility}</span>
                  <button
                    type="button"
                    aria-label={`Manage ${view.name}`}
                    className="text-xs text-[var(--muted)] hover:text-[var(--brand)]"
                    onClick={() =>
                      setManagingViewId(managingViewId === view.saved_view_id ? null : view.saved_view_id)
                    }
                  >
                    Manage
                  </button>
                  <button
                    type="button"
                    aria-label={`Delete ${view.name}`}
                    className="text-xs text-[var(--muted)] hover:text-[var(--bad)]"
                    onClick={() => deleteMutation.mutate(view.saved_view_id)}
                  >
                    Delete
                  </button>
                </div>
                {managingViewId === view.saved_view_id && managingView && (
                  <div className="ml-2 flex flex-col gap-2 border-l border-[var(--border)] py-1 pl-2">
                    <Button
                      variant="secondary"
                      disabled={visibilityMutation.isPending}
                      onClick={() => visibilityMutation.mutate(managingView)}
                    >
                      Make {managingView.visibility === "private" ? "Public" : "Private"}
                    </Button>
                    <ul className="flex flex-col gap-1">
                      {grants.map((grant) => (
                        <li key={grant.user_id} className="flex items-center justify-between gap-2 text-xs">
                          <span>
                            {grant.user_id} ({grant.relation})
                          </span>
                          <button
                            type="button"
                            aria-label={`Remove ${grant.user_id}`}
                            className="text-[var(--muted)] hover:text-[var(--bad)]"
                            onClick={() => revokeGrantMutation.mutate(grant.user_id)}
                          >
                            Remove
                          </button>
                        </li>
                      ))}
                    </ul>
                    <div className="flex items-center gap-1">
                      <input
                        type="text"
                        placeholder="User ID"
                        value={newGrantUserId}
                        onChange={(e) => setNewGrantUserId(e.target.value)}
                        className="min-w-0 flex-1 border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-xs"
                      />
                      <select
                        aria-label="Relation"
                        value={newGrantRelation}
                        onChange={(e) =>
                          setNewGrantRelation(e.target.value as "owner" | "editor" | "viewer")
                        }
                        className="border border-[var(--border)] bg-[var(--surface)] px-1 py-1 text-xs"
                      >
                        <option value="viewer">Viewer</option>
                        <option value="editor">Editor</option>
                        <option value="owner">Owner</option>
                      </select>
                      <Button
                        variant="primary"
                        disabled={!newGrantUserId.trim() || addGrantMutation.isPending}
                        onClick={() => addGrantMutation.mutate()}
                      >
                        Add
                      </Button>
                    </div>
                  </div>
                )}
              </li>
            ))}
            {views.length === 0 && (
              <li className="py-1 text-xs text-[var(--muted)]">No saved views yet.</li>
            )}
          </ul>
          <div className="mt-2 border-t border-[var(--border)] pt-2">
            {isSaving ? (
              <div className="flex flex-col gap-2">
                <label className="text-xs text-[var(--muted)]" htmlFor="saved-view-name">
                  View name
                </label>
                <input
                  id="saved-view-name"
                  type="text"
                  value={newViewName}
                  onChange={(e) => setNewViewName(e.target.value)}
                  className="border border-[var(--border)] bg-[var(--surface)] px-2 py-1 text-xs"
                />
                <Button
                  variant="primary"
                  disabled={!newViewName.trim() || createMutation.isPending}
                  onClick={() => createMutation.mutate(newViewName.trim())}
                >
                  Save
                </Button>
              </div>
            ) : (
              <Button variant="secondary" onClick={() => setIsSaving(true)}>
                Save current view
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/features/signals/components/SavedViewsControl.test.tsx 2>&1 | tail -60`
Expected: all tests pass, including the 2 existing ones (load, save) and the 2 new ones (visibility toggle, add grant).

- [ ] **Step 5: Run the full frontend suite and typecheck**

Run: `cd apps/frontend && npx vitest run 2>&1 | tail -60`
Expected: no regressions (in particular, `LogSearch.test.tsx`'s existing saved-views test should still pass since the "load" flow is unchanged).

Run: `cd apps/frontend && npm run typecheck 2>&1 | tail -30`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/features/signals/components/SavedViewsControl.tsx apps/frontend/src/features/signals/components/SavedViewsControl.test.tsx
git commit -m "feat(frontend): add visibility toggle and grant management to SavedViewsControl"
```

---

## Self-Review

**Spec coverage** (against `docs/superpowers/specs/2026-07-03-saved-views-logs-design.md`):
- Data model (`saved_views` + `saved_view_grants`, visibility/grants) → Task 1 ✓
- API (CRUD + grants, reusing dashboards' permission helpers) → Tasks 2-3 ✓
- Integration test coverage per roadmap operating rule 5 → Task 4 ✓
- Frontend API client → Task 5 ✓
- Column visibility (minimal show/hide) → Task 6 ✓
- Toolbar control (save/load/manage) → Task 7 ✓
- Wiring into `LogSearch` + `SignalExplorer` → Task 8 ✓
- Spec/ADR sync → Task 9 ✓
- Non-goals (traces/metrics, URL deep-linking, reordering, versioning) → correctly excluded from all tasks ✓
- "Manage" (rename/change visibility) UI mentioned in the design as part of the toolbar control: **scoped down** in Task 7 to load/save/delete only — rename and visibility-change via UI are deferred to keep this task reviewable; the backend (`PUT` with `visibility`, grants endpoints) already supports them for a fast follow-up. This is a deliberate reduction, not a gap — flagged here per the self-review's ambiguity check.

**Placeholder scan:** No TBD/TODO markers. Every step has complete code. Task 8 Step 1's test intentionally references "the file's existing wrapper" instead of inventing one, because `LogSearch.test.tsx` already defines its own render helper — copying an assumed one here would create a mismatch; the step explicitly instructs looking it up rather than leaving it unspecified as dead scope.

**Type consistency:** `LogViewConfig` (Task 5) fields (`query`, `severity_filter`, `message_search`, `time_range`, `visible_columns`) match exactly what Task 8's `currentViewConfig`/`handleLoadView` construct and consume. `SavedViewsControlProps` (Task 7) matches Task 8's usage (`tenantId`, `currentConfig`, `onLoad`). `LogResultsTable`'s `visibleColumns?: LogTableColumn[]` (Task 6) matches the `("level" | "service")[]` state type used in Task 8.
