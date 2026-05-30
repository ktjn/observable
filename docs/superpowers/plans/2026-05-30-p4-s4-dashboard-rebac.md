# P4-S4: Dashboard ReBAC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add enforceable sharing semantics to dashboards using a PostgreSQL relationship tuple store, additive to the existing RBAC model.

**Architecture:** Two new migrations add a `visibility` column to `dashboards` and a `dashboard_grants` tuple table. Pure check helpers in `dashboards.rs` test the relation string in memory; async wrappers do the single DB lookup. Enforcement is applied only when `user_id` is present (session auth); API-key callers keep existing tenant-scoped behavior. Three new grant-management endpoints are wired into `main.rs`.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, Testcontainers (for integration tests).

**Design doc:** `docs/superpowers/specs/2026-05-30-p4-s4-dashboard-rebac-design.md`

---

## Files Changed

| File | Change |
|---|---|
| `migrations/postgres/030_add_dashboard_visibility.sql` | New: add `visibility` column to `dashboards` |
| `migrations/postgres/031_create_dashboard_grants.sql` | New: `dashboard_grants` tuple table |
| `services/query-api/src/dashboards.rs` | Modify: add visibility field, pure check helpers, grant management, updated handlers |
| `services/query-api/src/main.rs` | Modify: register 3 new grant routes |
| `services/query-api/tests/postgres_dashboards_integration.rs` | Modify: update call sites for new function signatures |
| `services/query-api/tests/postgres_dashboard_rebac_integration.rs` | New: Testcontainers integration tests for ReBAC enforcement |
| `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` | Modify: mark P4-S4 complete |
| `docs/agent-context.md` | Modify: update active plan note |

---

## Task 1: Database migrations

**Files:**
- Create: `migrations/postgres/030_add_dashboard_visibility.sql`
- Create: `migrations/postgres/031_create_dashboard_grants.sql`

- [ ] **Step 1: Create migration 030**

Create `migrations/postgres/030_add_dashboard_visibility.sql`:

```sql
ALTER TABLE dashboards
    ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'public'
        CHECK (visibility IN ('public', 'private'));
```

- [ ] **Step 2: Create migration 031**

Create `migrations/postgres/031_create_dashboard_grants.sql`:

```sql
CREATE TABLE IF NOT EXISTS dashboard_grants (
    dashboard_id UUID        NOT NULL REFERENCES dashboards(dashboard_id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id)                ON DELETE CASCADE,
    relation     TEXT        NOT NULL CHECK (relation IN ('owner', 'editor', 'viewer')),
    granted_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (dashboard_id, user_id)
);

CREATE INDEX IF NOT EXISTS dashboard_grants_user_idx
    ON dashboard_grants (user_id, dashboard_id);
```

- [ ] **Step 3: Verify migrations apply clean**

```bash
cargo test -p query-api --test postgres_dashboards_integration -- --nocapture 2>&1 | head -20
```

Expected: The test harness applies all migrations via Testcontainers; if it errors on a migration, you'll see the SQL error here. Existing dashboard tests should still pass.

- [ ] **Step 4: Commit**

```bash
git add migrations/postgres/030_add_dashboard_visibility.sql \
        migrations/postgres/031_create_dashboard_grants.sql
git commit -m "feat(db): add dashboard visibility and grants tuple table (migrations 030-031)"
```

---

## Task 2: Pure check helpers + unit tests (TDD)

**Files:**
- Modify: `services/query-api/src/dashboards.rs`

Add visibility to the data model and the pure check helpers. TDD: write failing tests first, then implement.

- [ ] **Step 1: Add `visibility` to `DashboardRow` and `DashboardItem`**

In `services/query-api/src/dashboards.rs`, update the two structs:

```rust
// Replace the existing DashboardItem struct:
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DashboardItem {
    pub dashboard_id: Uuid,
    pub name: String,
    pub visibility: String,
    pub panels: Vec<DashboardPanelItem>,
    pub created_at: DateTime<Utc>,
}

// Replace the existing DashboardRow struct:
#[derive(sqlx::FromRow)]
struct DashboardRow {
    dashboard_id: Uuid,
    name: String,
    visibility: String,
    created_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Add `visibility` to `UpdateDashboardRequest`**

```rust
// Replace the existing UpdateDashboardRequest struct:
#[derive(Deserialize)]
pub struct UpdateDashboardRequest {
    pub name: String,
    pub panels: Vec<DashboardPanelRequest>,
    #[serde(default)]
    pub visibility: Option<String>,
}
```

- [ ] **Step 3: Write the failing unit tests**

Add a new `#[cfg(test)]` module `rebac_tests` at the bottom of `services/query-api/src/dashboards.rs`, inside the existing `#[cfg(test)]` block (add after the last existing test):

```rust
    // --- ReBAC pure-helper tests ---

    #[test]
    fn public_dashboard_visible_without_grant() {
        assert!(grant_satisfies_read("public", None));
    }

    #[test]
    fn private_dashboard_hidden_without_grant() {
        assert!(!grant_satisfies_read("private", None));
    }

    #[test]
    fn private_dashboard_visible_with_viewer_grant() {
        assert!(grant_satisfies_read("private", Some("viewer")));
    }

    #[test]
    fn private_dashboard_visible_with_editor_grant() {
        assert!(grant_satisfies_read("private", Some("editor")));
    }

    #[test]
    fn private_dashboard_visible_with_owner_grant() {
        assert!(grant_satisfies_read("private", Some("owner")));
    }

    #[test]
    fn viewer_grant_cannot_write() {
        assert!(!grant_satisfies_write("member", Some("viewer")));
    }

    #[test]
    fn editor_grant_can_write() {
        assert!(grant_satisfies_write("member", Some("editor")));
    }

    #[test]
    fn owner_grant_can_write() {
        assert!(grant_satisfies_write("member", Some("owner")));
    }

    #[test]
    fn tenant_admin_can_write_without_grant() {
        assert!(grant_satisfies_write("tenant_admin", None));
    }

    #[test]
    fn viewer_grant_cannot_delete() {
        assert!(!grant_satisfies_delete("member", Some("viewer")));
    }

    #[test]
    fn editor_grant_cannot_delete() {
        assert!(!grant_satisfies_delete("member", Some("editor")));
    }

    #[test]
    fn owner_grant_can_delete() {
        assert!(grant_satisfies_delete("member", Some("owner")));
    }

    #[test]
    fn tenant_admin_can_delete_without_grant() {
        assert!(grant_satisfies_delete("tenant_admin", None));
    }
```

- [ ] **Step 4: Run tests — verify they fail to compile (functions not defined yet)**

```bash
cargo test -p query-api --lib -- rebac 2>&1 | head -10
```

Expected: compile error mentioning `grant_satisfies_read`, `grant_satisfies_write`, `grant_satisfies_delete` not found.

- [ ] **Step 5: Implement the pure helpers and DB fetch helper**

In `services/query-api/src/dashboards.rs`, add these functions after `inject_current_context` — place them just before the `list_dashboards` function:

```rust
/// True if the caller is allowed to read this dashboard.
/// Public dashboards are readable by any tenant member (RBAC already enforced by middleware).
/// Private dashboards require an explicit grant of any relation.
pub(crate) fn grant_satisfies_read(visibility: &str, relation: Option<&str>) -> bool {
    visibility == "public"
        || relation.is_some_and(|r| matches!(r, "owner" | "editor" | "viewer"))
}

/// True if the caller is allowed to write (update) this dashboard.
/// `tenant_admin` bypasses tuple checks.
pub(crate) fn grant_satisfies_write(tenant_role: &str, relation: Option<&str>) -> bool {
    tenant_role == "tenant_admin"
        || relation.is_some_and(|r| matches!(r, "owner" | "editor"))
}

/// True if the caller is allowed to delete this dashboard.
/// `tenant_admin` bypasses tuple checks.
pub(crate) fn grant_satisfies_delete(tenant_role: &str, relation: Option<&str>) -> bool {
    tenant_role == "tenant_admin" || relation.is_some_and(|r| r == "owner")
}

/// Fetch the relation a specific user holds on a specific dashboard, if any.
async fn fetch_relation(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    dashboard_id: uuid::Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT relation FROM dashboard_grants \
         WHERE dashboard_id = $1 AND user_id = $2",
    )
    .bind(dashboard_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}
```

- [ ] **Step 6: Run tests — verify GREEN**

```bash
cargo test -p query-api --lib -- rebac
```

Expected: 13 tests pass.

- [ ] **Step 7: Run cargo fmt**

```bash
cargo fmt --all
```

- [ ] **Step 8: Commit**

```bash
git add services/query-api/src/dashboards.rs
git commit -m "feat(query-api): add visibility field and grant check helpers to dashboards"
```

---

## Task 3: Update create/list/get handlers for access control

**Files:**
- Modify: `services/query-api/src/dashboards.rs`
- Modify: `services/query-api/tests/postgres_dashboards_integration.rs`

- [ ] **Step 1: Update `create_dashboard` to accept `creator_user_id` and auto-insert owner grant**

Replace the existing `create_dashboard` function signature and body. The function now takes an optional `creator_user_id`; when `Some`, it inserts an `owner` grant inside the same transaction.

```rust
pub async fn create_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    req: &CreateDashboardRequest,
    creator_user_id: Option<uuid::Uuid>,
) -> Result<DashboardItem, CreateDashboardError> {
    validate_create_request(req)?;

    let mut tx = db.begin().await.map_err(CreateDashboardError::Db)?;
    let row = sqlx::query_as::<_, DashboardRow>(
        "INSERT INTO dashboards (tenant_id, name) VALUES ($1, $2) \
         RETURNING dashboard_id, name, visibility, created_at",
    )
    .bind(tenant_id)
    .bind(req.name.trim())
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateDashboardError::Db)?;

    if let Some(user_id) = creator_user_id {
        sqlx::query(
            "INSERT INTO dashboard_grants (dashboard_id, user_id, relation) \
             VALUES ($1, $2, 'owner') \
             ON CONFLICT (dashboard_id, user_id) DO UPDATE SET relation = 'owner'",
        )
        .bind(row.dashboard_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;
    }

    let mut panels = Vec::with_capacity(req.panels.len());
    for (position, panel) in req.panels.iter().enumerate() {
        let panel_kind = panel_kind(panel);
        let layout = normalized_layout(panel.layout.as_ref(), position);
        let time_range = normalized_time_range(panel);
        let item = sqlx::query_as::<_, DashboardPanelRow>(
            "INSERT INTO dashboard_panels \
             (dashboard_id, title, panel_kind, query_kind, service, preset, filters, \
              query_text, content, layout, time_range, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             RETURNING dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                       filters, query_text, content, layout, time_range",
        )
        .bind(row.dashboard_id)
        .bind(panel.title.trim())
        .bind(panel_kind)
        .bind(panel.query_kind.as_deref())
        .bind(
            panel
                .service
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(panel.preset.as_deref())
        .bind(&panel.filters)
        .bind(
            panel
                .query_text
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(
            panel
                .content
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(&layout)
        .bind(&time_range)
        .bind(position as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;

        panels.push(row_to_panel_item(item));
    }

    tx.commit().await.map_err(CreateDashboardError::Db)?;

    Ok(DashboardItem {
        dashboard_id: row.dashboard_id,
        name: row.name,
        visibility: row.visibility,
        panels,
        created_at: row.created_at,
    })
}
```

- [ ] **Step 2: Update `import_dashboard` to thread through `creator_user_id`**

Replace the existing `import_dashboard` function:

```rust
pub async fn import_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    export: &DashboardExport,
    creator_user_id: Option<uuid::Uuid>,
) -> Result<DashboardItem, CreateDashboardError> {
    let req = CreateDashboardRequest {
        name: export.name.clone(),
        panels: export
            .panels
            .iter()
            .map(|p| DashboardPanelRequest {
                panel_id: None,
                title: p.title.clone(),
                panel_kind: p.panel_kind.clone(),
                query_kind: p.query_kind.clone(),
                service: p.service.clone(),
                preset: p.preset.clone(),
                filters: p.filters.clone(),
                query_text: p.query_text.clone(),
                content: p.content.clone(),
                layout: p.layout.clone(),
                time_range: p.time_range.clone(),
            })
            .collect(),
    };
    create_dashboard(db, tenant_id, &req, creator_user_id).await
}
```

- [ ] **Step 3: Update `list_dashboards` to accept `user_id` and filter by visibility**

Replace the existing `list_dashboards` function:

```rust
pub async fn list_dashboards(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    user_id: Option<uuid::Uuid>,
) -> Result<Vec<DashboardItem>, sqlx::Error> {
    let dashboards = if let Some(uid) = user_id {
        sqlx::query_as::<_, DashboardRow>(
            "SELECT dashboard_id, name, visibility, created_at \
             FROM dashboards \
             WHERE tenant_id = $1 \
               AND (visibility = 'public' \
                    OR EXISTS ( \
                        SELECT 1 FROM dashboard_grants \
                        WHERE dashboard_grants.dashboard_id = dashboards.dashboard_id \
                          AND user_id = $2 \
                    )) \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(uid)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as::<_, DashboardRow>(
            "SELECT dashboard_id, name, visibility, created_at \
             FROM dashboards \
             WHERE tenant_id = $1 \
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(db)
        .await?
    };

    let dashboard_ids: Vec<uuid::Uuid> = dashboards.iter().map(|d| d.dashboard_id).collect();
    let panels = if dashboard_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, DashboardPanelRow>(
            "SELECT dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                    filters, query_text, content, layout, time_range \
             FROM dashboard_panels \
             WHERE dashboard_id = ANY($1) \
             ORDER BY dashboard_id, position ASC",
        )
        .bind(&dashboard_ids)
        .fetch_all(db)
        .await?
    };

    Ok(dashboards
        .into_iter()
        .map(|dashboard| DashboardItem {
            panels: panels
                .iter()
                .filter(|panel| panel.dashboard_id == dashboard.dashboard_id)
                .map(|panel| DashboardPanelItem {
                    panel_id: panel.panel_id,
                    title: panel.title.clone(),
                    panel_kind: panel.panel_kind.clone(),
                    query_kind: panel.query_kind.clone(),
                    service: panel.service.clone(),
                    preset: panel.preset.clone(),
                    filters: panel.filters.clone(),
                    query_text: panel.query_text.clone(),
                    content: panel.content.clone(),
                    layout: panel.layout.clone(),
                    time_range: panel.time_range.clone(),
                })
                .collect(),
            dashboard_id: dashboard.dashboard_id,
            name: dashboard.name,
            visibility: dashboard.visibility,
            created_at: dashboard.created_at,
        })
        .collect())
}
```

- [ ] **Step 4: Update `get_dashboard` SQL to return `visibility`**

In `get_dashboard`, update the SELECT to include `visibility` and populate it in the returned `DashboardItem`:

```rust
pub async fn get_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    dashboard_id: uuid::Uuid,
) -> Result<Option<DashboardItem>, sqlx::Error> {
    let dashboard = sqlx::query_as::<_, DashboardRow>(
        "SELECT dashboard_id, name, visibility, created_at \
         FROM dashboards \
         WHERE dashboard_id = $1 AND tenant_id = $2",
    )
    .bind(dashboard_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;

    let Some(dashboard) = dashboard else {
        return Ok(None);
    };

    let panels = sqlx::query_as::<_, DashboardPanelRow>(
        "SELECT dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                filters, query_text, content, layout, time_range \
         FROM dashboard_panels \
         WHERE dashboard_id = $1 \
         ORDER BY position ASC",
    )
    .bind(dashboard.dashboard_id)
    .fetch_all(db)
    .await?;

    Ok(Some(DashboardItem {
        dashboard_id: dashboard.dashboard_id,
        name: dashboard.name,
        visibility: dashboard.visibility,
        panels: panels.into_iter().map(row_to_panel_item).collect(),
        created_at: dashboard.created_at,
    }))
}
```

- [ ] **Step 5: Update the three handlers to use the new function signatures**

Update `handle_create_dashboard`, `handle_list_dashboards`, `handle_get_dashboard`, and `handle_import_dashboard`:

```rust
pub async fn handle_list_dashboards(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DashboardListResponse>, StatusCode> {
    let items = list_dashboards(&state.db, ctx.tenant_id, ctx.user_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list dashboards");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(DashboardListResponse { items }))
}

pub async fn handle_create_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<CreateDashboardRequest>,
) -> Result<(StatusCode, Json<DashboardItem>), StatusCode> {
    match create_dashboard(&state.db, ctx.tenant_id, &req, ctx.user_id).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateDashboardError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid dashboard input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateDashboardError::Db(e)) => {
            tracing::error!(error = %e, "failed to create dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_get_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
) -> Result<Json<DashboardItem>, StatusCode> {
    let item = match get_dashboard(&state.db, ctx.tenant_id, dashboard_id).await {
        Ok(Some(item)) => item,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get dashboard");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, dashboard_id)
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

pub async fn handle_import_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(export): Json<DashboardExport>,
) -> Result<(StatusCode, Json<DashboardItem>), StatusCode> {
    if export.schema_version != "1" && export.schema_version != EXPORT_SCHEMA_VERSION {
        tracing::warn!(schema_version = %export.schema_version, "unsupported dashboard export schema version");
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    match import_dashboard(&state.db, ctx.tenant_id, &export, ctx.user_id).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateDashboardError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid dashboard import");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateDashboardError::Db(e)) => {
            tracing::error!(error = %e, "failed to import dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 6: Update call sites in `postgres_dashboards_integration.rs`**

In `services/query-api/tests/postgres_dashboards_integration.rs`, update all calls to pass the new parameters:
- Every `create_dashboard(&pool, tenant, &req)` → `create_dashboard(&pool, tenant, &req, None)`
- Every `list_dashboards(&pool, tenant)` → `list_dashboards(&pool, tenant, None)`
- Every `import_dashboard(&pool, tenant, &export)` → `import_dashboard(&pool, tenant, &export, None)`

There are multiple occurrences of each — use `cargo build` output to find them all if needed.

Also update the imports line at the top of the test file to ensure `fetch_relation`, `grant_satisfies_read`, `grant_satisfies_write`, `grant_satisfies_delete` are not imported (they're private/pub(crate) — the integration test file doesn't need them).

- [ ] **Step 7: Build and run existing dashboard tests**

```bash
cargo build -p query-api
```

Expected: clean build, no errors.

```bash
cargo test -p query-api --test postgres_dashboards_integration -- --nocapture
```

Expected: all existing dashboard integration tests pass.

- [ ] **Step 8: Run lib tests**

```bash
cargo test -p query-api --lib
```

Expected: all 13 new ReBAC unit tests pass alongside existing tests.

- [ ] **Step 9: Format and commit**

```bash
cargo fmt --all
git add services/query-api/src/dashboards.rs \
        services/query-api/tests/postgres_dashboards_integration.rs
git commit -m "feat(query-api): apply ReBAC checks to dashboard create/list/get handlers"
```

---

## Task 4: Update update/delete handlers for write access control

**Files:**
- Modify: `services/query-api/src/dashboards.rs`

- [ ] **Step 1: Update `update_dashboard` to handle optional `visibility`**

Replace the existing `update_dashboard` function:

```rust
pub async fn update_dashboard(
    db: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    dashboard_id: uuid::Uuid,
    req: &UpdateDashboardRequest,
) -> Result<Option<DashboardItem>, CreateDashboardError> {
    validate_update_request(req)?;

    let mut tx = db.begin().await.map_err(CreateDashboardError::Db)?;

    let row = if let Some(vis) = req.visibility.as_deref() {
        sqlx::query_as::<_, DashboardRow>(
            "UPDATE dashboards SET name = $1, visibility = $2 \
             WHERE dashboard_id = $3 AND tenant_id = $4 \
             RETURNING dashboard_id, name, visibility, created_at",
        )
        .bind(req.name.trim())
        .bind(vis)
        .bind(dashboard_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?
    } else {
        sqlx::query_as::<_, DashboardRow>(
            "UPDATE dashboards SET name = $1 \
             WHERE dashboard_id = $2 AND tenant_id = $3 \
             RETURNING dashboard_id, name, visibility, created_at",
        )
        .bind(req.name.trim())
        .bind(dashboard_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?
    };

    let Some(row) = row else {
        tx.rollback().await.map_err(CreateDashboardError::Db)?;
        return Ok(None);
    };

    sqlx::query("DELETE FROM dashboard_panels WHERE dashboard_id = $1")
        .bind(row.dashboard_id)
        .execute(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;

    let mut panels = Vec::with_capacity(req.panels.len());
    for (position, panel) in req.panels.iter().enumerate() {
        let panel_kind = panel_kind(panel);
        let layout = normalized_layout(panel.layout.as_ref(), position);
        let time_range = normalized_time_range(panel);
        let panel_id = panel.panel_id.unwrap_or_else(Uuid::new_v4);
        let item = sqlx::query_as::<_, DashboardPanelRow>(
            "INSERT INTO dashboard_panels \
             (panel_id, dashboard_id, title, panel_kind, query_kind, service, preset, filters, \
              query_text, content, layout, time_range, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             RETURNING dashboard_id, panel_id, title, panel_kind, query_kind, service, preset, \
                       filters, query_text, content, layout, time_range",
        )
        .bind(panel_id)
        .bind(row.dashboard_id)
        .bind(panel.title.trim())
        .bind(panel_kind)
        .bind(panel.query_kind.as_deref())
        .bind(
            panel
                .service
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(panel.preset.as_deref())
        .bind(&panel.filters)
        .bind(
            panel
                .query_text
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(
            panel
                .content
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty()),
        )
        .bind(&layout)
        .bind(&time_range)
        .bind(position as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(CreateDashboardError::Db)?;
        panels.push(row_to_panel_item(item));
    }

    tx.commit().await.map_err(CreateDashboardError::Db)?;

    Ok(Some(DashboardItem {
        dashboard_id: row.dashboard_id,
        name: row.name,
        visibility: row.visibility,
        panels,
        created_at: row.created_at,
    }))
}
```

- [ ] **Step 2: Add visibility validation to `validate_update_request`**

In `validate_update_request`, add after the panel loop:

```rust
    if let Some(vis) = &req.visibility {
        if !matches!(vis.as_str(), "public" | "private") {
            return Err(CreateDashboardError::InvalidInput(
                "visibility must be 'public' or 'private'".into(),
            ));
        }
    }
```

- [ ] **Step 3: Update `handle_update_dashboard` to check write access**

Replace the existing handler:

```rust
pub async fn handle_update_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
    Json(req): Json<UpdateDashboardRequest>,
) -> Result<Json<DashboardItem>, StatusCode> {
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, dashboard_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_write(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match update_dashboard(&state.db, ctx.tenant_id, dashboard_id, &req).await {
        Ok(Some(item)) => Ok(Json(item)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(CreateDashboardError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid dashboard update");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateDashboardError::Db(e)) => {
            tracing::error!(error = %e, "failed to update dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 4: Update `handle_delete_dashboard` to check delete access**

Replace the existing handler:

```rust
pub async fn handle_delete_dashboard(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    if let Some(user_id) = ctx.user_id {
        let relation = fetch_relation(&state.db, user_id, dashboard_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch grant");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !grant_satisfies_delete(&ctx.role, relation.as_deref()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    match delete_dashboard(&state.db, ctx.tenant_id, dashboard_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete dashboard");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

Note: `ctx.role` is already on `TenantContext` but marked `#[allow(dead_code)]`. Remove that annotation now that it's used.

In `services/query-api/src/middleware/auth.rs`, remove the `#[allow(dead_code)]` attribute on `role`:

```rust
#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
    pub role: String,
}
```

- [ ] **Step 5: Build and run existing tests**

```bash
cargo build -p query-api
cargo test -p query-api --lib
cargo test -p query-api --test postgres_dashboards_integration -- --nocapture
```

Expected: all pass.

- [ ] **Step 6: Format and commit**

```bash
cargo fmt --all
git add services/query-api/src/dashboards.rs \
        services/query-api/src/middleware/auth.rs
git commit -m "feat(query-api): enforce ReBAC write/delete checks on dashboard handlers"
```

---

## Task 5: Grant management handlers and route registration

**Files:**
- Modify: `services/query-api/src/dashboards.rs`
- Modify: `services/query-api/src/main.rs`

- [ ] **Step 1: Add grant response types**

In `services/query-api/src/dashboards.rs`, add after the `DashboardListResponse` struct:

```rust
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
```

- [ ] **Step 2: Add `handle_list_grants` handler**

Add after `handle_delete_dashboard`:

```rust
pub async fn handle_list_grants(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
) -> Result<Json<GrantListResponse>, StatusCode> {
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;

    // verify dashboard belongs to this tenant
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dashboards WHERE dashboard_id = $1 AND tenant_id = $2)",
    )
    .bind(dashboard_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check dashboard ownership");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let relation = fetch_relation(&state.db, user_id, dashboard_id)
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
         FROM dashboard_grants \
         WHERE dashboard_id = $1 \
         ORDER BY granted_at ASC",
    )
    .bind(dashboard_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to list grants");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(GrantListResponse { grants }))
}
```

- [ ] **Step 3: Add `handle_add_grant` handler**

```rust
pub async fn handle_add_grant(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(dashboard_id): Path<Uuid>,
    Json(req): Json<AddGrantRequest>,
) -> Result<StatusCode, StatusCode> {
    if !matches!(req.relation.as_str(), "owner" | "editor" | "viewer") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;

    // verify dashboard belongs to this tenant
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dashboards WHERE dashboard_id = $1 AND tenant_id = $2)",
    )
    .bind(dashboard_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check dashboard");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let caller_relation = fetch_relation(&state.db, user_id, dashboard_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if caller_relation.as_deref() != Some("owner") {
        return Err(StatusCode::FORBIDDEN);
    }

    // verify target user exists in this tenant
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
        "INSERT INTO dashboard_grants (dashboard_id, user_id, relation) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (dashboard_id, user_id) DO UPDATE SET relation = EXCLUDED.relation",
    )
    .bind(dashboard_id)
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
```

- [ ] **Step 4: Add `handle_revoke_grant` handler**

```rust
pub async fn handle_revoke_grant(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((dashboard_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = ctx.user_id.ok_or(StatusCode::FORBIDDEN)?;

    // verify dashboard belongs to this tenant
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dashboards WHERE dashboard_id = $1 AND tenant_id = $2)",
    )
    .bind(dashboard_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to check dashboard");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let caller_relation = fetch_relation(&state.db, user_id, dashboard_id)
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

    // Check if removing this grant would leave zero owners
    let target_relation = fetch_relation(&state.db, target_user_id, dashboard_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to fetch target grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if target_relation.as_deref() == Some("owner") {
        let remaining_owners: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dashboard_grants \
             WHERE dashboard_id = $1 AND relation = 'owner' AND user_id != $2",
        )
        .bind(dashboard_id)
        .bind(target_user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to count owners");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if remaining_owners == 0 {
            return Err(StatusCode::CONFLICT);
        }
    }

    let deleted = sqlx::query(
        "DELETE FROM dashboard_grants WHERE dashboard_id = $1 AND user_id = $2",
    )
    .bind(dashboard_id)
    .bind(target_user_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to delete grant");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if deleted.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 5: Register the three new routes in `main.rs`**

In `services/query-api/src/main.rs`, find the dashboard routes block and add after the existing export route:

```rust
        .route(
            "/v1/dashboards/{id}/grants",
            get(dashboards::handle_list_grants).post(dashboards::handle_add_grant),
        )
        .route(
            "/v1/dashboards/{id}/grants/{user_id}",
            axum::routing::delete(dashboards::handle_revoke_grant),
        )
```

- [ ] **Step 6: Build and run existing tests**

```bash
cargo build -p query-api
cargo test -p query-api --lib
cargo test -p query-api --test postgres_dashboards_integration -- --nocapture
```

Expected: all pass.

- [ ] **Step 7: Format and commit**

```bash
cargo fmt --all
git add services/query-api/src/dashboards.rs services/query-api/src/main.rs
git commit -m "feat(query-api): add dashboard grant management endpoints (list/add/revoke)"
```

---

## Task 6: Integration tests for ReBAC enforcement

**Files:**
- Create: `services/query-api/tests/postgres_dashboard_rebac_integration.rs`

These tests use Testcontainers Postgres and exercise the full enforcement path through the HTTP handler layer.

- [ ] **Step 1: Create the test file**

Create `services/query-api/tests/postgres_dashboard_rebac_integration.rs`:

```rust
//! Integration tests for dashboard ReBAC enforcement.
//! All tests use Testcontainers Postgres with all migrations applied.

use query_api::dashboards::{
    AddGrantRequest, CreateDashboardRequest, DashboardPanelRequest, UpdateDashboardRequest,
    create_dashboard, get_dashboard, list_dashboards, update_dashboard,
};
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

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
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

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(tenant_id)
        .bind(format!("tenant-{tenant_id}"))
        .execute(pool)
        .await
        .expect("tenant inserted");
}

async fn insert_user(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, idp_subject, email) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(format!("sub-{user_id}"))
    .bind(format!("{user_id}@test.com"))
    .execute(pool)
    .await
    .expect("user inserted");
    sqlx::query(
        "INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, 'member')",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("role assigned");
    user_id
}

fn one_panel() -> Vec<DashboardPanelRequest> {
    vec![DashboardPanelRequest {
        title: "Test panel".into(),
        query_kind: Some("logs".into()),
        ..Default::default()
    }]
}

// ── Test 1: creator gets owner grant ────────────────────────────────────────

#[tokio::test]
async fn create_dashboard_assigns_owner_grant_to_creator() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let user = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "My dash".into(), panels: one_panel() },
        Some(user),
    )
    .await
    .unwrap();

    let relation: Option<String> = sqlx::query_scalar(
        "SELECT relation FROM dashboard_grants WHERE dashboard_id = $1 AND user_id = $2",
    )
    .bind(dashboard.dashboard_id)
    .bind(user)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert_eq!(relation.as_deref(), Some("owner"));
}

// ── Test 2: public dashboard visible to all members ─────────────────────────

#[tokio::test]
async fn public_dashboard_visible_to_any_member() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let other = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Public".into(), panels: one_panel() },
        Some(creator),
    )
    .await
    .unwrap();
    assert_eq!(dashboard.visibility, "public");

    // other user can list and get the dashboard
    let listed = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        listed.iter().any(|d| d.dashboard_id == dashboard.dashboard_id),
        "other user must see public dashboard in list"
    );

    let fetched = get_dashboard(&pool, tenant, dashboard.dashboard_id)
        .await
        .unwrap();
    assert!(fetched.is_some(), "get_dashboard must return public dashboard");
}

// ── Test 3: private dashboard hidden from non-granted users ─────────────────

#[tokio::test]
async fn private_dashboard_hidden_from_non_granted_user() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let other = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Private".into(), panels: one_panel() },
        Some(creator),
    )
    .await
    .unwrap();

    // flip to private
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Private".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // other user should NOT see it in list
    let listed = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        !listed.iter().any(|d| d.dashboard_id == dashboard.dashboard_id),
        "other user must not see private dashboard in list"
    );
}

// ── Test 4: private dashboard visible after explicit viewer grant ────────────

#[tokio::test]
async fn private_dashboard_visible_after_viewer_grant() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let viewer = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Shared".into(), panels: one_panel() },
        Some(creator),
    )
    .await
    .unwrap();

    // flip to private
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Shared".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // grant viewer access
    sqlx::query(
        "INSERT INTO dashboard_grants (dashboard_id, user_id, relation) VALUES ($1, $2, 'viewer')",
    )
    .bind(dashboard.dashboard_id)
    .bind(viewer)
    .execute(&pool)
    .await
    .unwrap();

    // viewer should now see it in list
    let listed = list_dashboards(&pool, tenant, Some(viewer)).await.unwrap();
    assert!(
        listed.iter().any(|d| d.dashboard_id == dashboard.dashboard_id),
        "viewer must see private dashboard after explicit grant"
    );
}

// ── Test 5: flip back to public restores access ──────────────────────────────

#[tokio::test]
async fn flip_to_public_restores_member_access() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let other = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Toggle".into(), panels: one_panel() },
        Some(creator),
    )
    .await
    .unwrap();

    // flip private
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Toggle".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // confirm hidden
    let hidden = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        !hidden.iter().any(|d| d.dashboard_id == dashboard.dashboard_id)
    );

    // flip public
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Toggle".into(),
            panels: one_panel(),
            visibility: Some("public".into()),
        },
    )
    .await
    .unwrap();

    // confirm visible again
    let visible = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        visible.iter().any(|d| d.dashboard_id == dashboard.dashboard_id),
        "dashboard must be visible again after flip to public"
    );
}

// ── Test 6: revoke last owner grant returns error ────────────────────────────

#[tokio::test]
async fn revoke_last_owner_is_blocked_at_db_level() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let user = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Solo owner".into(), panels: one_panel() },
        Some(user),
    )
    .await
    .unwrap();

    // confirm there is exactly one owner
    let owners: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dashboard_grants WHERE dashboard_id = $1 AND relation = 'owner'",
    )
    .bind(dashboard.dashboard_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owners, 1);

    // the handler checks this before deleting — here we verify the data constraint is correct
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dashboard_grants \
         WHERE dashboard_id = $1 AND relation = 'owner' AND user_id != $2",
    )
    .bind(dashboard.dashboard_id)
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        remaining, 0,
        "zero remaining owners means handle_revoke_grant must return 409"
    );
}

// ── Test 7: API-key path (user_id = None) sees all dashboards ───────────────

#[tokio::test]
async fn api_key_caller_sees_all_dashboards_regardless_of_visibility() {
    let (pool, _c) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let user = insert_user(&pool, tenant).await;

    let public_dash = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Public".into(), panels: one_panel() },
        Some(user),
    )
    .await
    .unwrap();

    let private_dash = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest { name: "Private".into(), panels: one_panel() },
        Some(user),
    )
    .await
    .unwrap();

    update_dashboard(
        &pool,
        tenant,
        private_dash.dashboard_id,
        &UpdateDashboardRequest {
            name: "Private".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // API-key path: user_id = None
    let all = list_dashboards(&pool, tenant, None).await.unwrap();
    let ids: Vec<_> = all.iter().map(|d| d.dashboard_id).collect();
    assert!(ids.contains(&public_dash.dashboard_id));
    assert!(ids.contains(&private_dash.dashboard_id));
}
```

- [ ] **Step 2: Run the integration tests**

```bash
cargo test -p query-api --test postgres_dashboard_rebac_integration -- --nocapture
```

Expected: all 7 tests pass.

- [ ] **Step 3: Format and commit**

```bash
cargo fmt --all
git add services/query-api/tests/postgres_dashboard_rebac_integration.rs
git commit -m "test(query-api): add ReBAC integration tests for dashboard visibility and grants"
```

---

## Task 7: Roadmap and agent-context update

**Files:**
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Modify: `docs/agent-context.md`

- [ ] **Step 1: Mark P4-S4 complete in the roadmap**

In `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, find:

```markdown
- [ ] **P4-S4: Add fine-grained authorization for one protected resource**
  - Outcome: one OpenFGA-style protected object has enforceable sharing semantics.
  - Checkpoint: is the ReBAC model additive to RBAC rather than conflicting with it?
```

Replace with:

```markdown
- [x] **P4-S4: Add fine-grained authorization for one protected resource** (COMPLETED 2026-05-30)
  - Outcome: dashboards have enforceable visibility (`public`/`private`) and relationship tuples (`owner`, `editor`, `viewer`) stored in `dashboard_grants`. Handlers enforce read/write/delete based on grants; `tenant_admin` bypasses tuple checks; API-key callers keep existing tenant-scoped behavior.
  - Checkpoint: ReBAC is additive to RBAC — RBAC gate is unchanged; tuple checks narrow access within it; existing dashboards remain accessible by default (`visibility = 'public'`).
  - Detail: `docs/superpowers/specs/2026-05-30-p4-s4-dashboard-rebac-design.md`
```

- [ ] **Step 2: Update agent-context.md**

In `docs/agent-context.md`, find:

```
- Active detailed implementation plan: none — RF-2, RF-3, RF-6, P4-S9, stream-processor batching, and Telemetry Loop Prevention complete. Next: P4-S4 fine-grained authorization.
```

Replace with:

```
- Active detailed implementation plan: none — RF-2, RF-3, RF-6, P4-S9, stream-processor batching, Telemetry Loop Prevention, and P4-S4 dashboard ReBAC complete. Next: P4-S3b SCIM/SSO (if required by v1 customers) or P4-S5+ Phase 5 work.
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md docs/agent-context.md
git commit -m "chore(docs): mark P4-S4 dashboard ReBAC complete, update agent-context"
```

---

## Verification Checklist (run before pushing)

- [ ] `cargo test -p query-api --lib` — all unit tests pass including 13 new ReBAC helper tests
- [ ] `cargo test -p query-api --test postgres_dashboards_integration` — all existing dashboard tests pass with updated call signatures
- [ ] `cargo test -p query-api --test postgres_dashboard_rebac_integration` — all 7 new integration tests pass
- [ ] `cargo build --workspace` — clean build
- [ ] `cargo fmt --all -- --check` — no formatting issues
- [ ] `cargo clippy -p query-api --all-targets -- -D warnings` — no warnings
