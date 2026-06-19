# Admin Console Member Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `/admin/members` page that lets tenant admins list, invite (by email), change roles, remove, and revoke sessions for members of the current tenant.

**Architecture:** Five new Axum handlers in `services/query-api/src/admin_members.rs` gated to `tenant_admin` role. A new `MemberManagementPage` in `apps/frontend/src/features/admin/` with five corresponding API functions and React Query mutations. Wired into the existing router and `AdminSurfaceNav`.

**Tech Stack:** Rust/Axum/SQLx (backend), React 19 / TanStack Query / TanStack Router (frontend), Vitest/RTL (frontend tests), Testcontainers/tower::ServiceExt (backend tests).

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `services/query-api/src/admin_members.rs` | **Create** | All five handlers + request/response types |
| `services/query-api/src/main.rs` | **Modify** | Register five new routes |
| `services/query-api/tests/http_api_integration.rs` | **Modify** | Integration tests for all five endpoints |
| `apps/frontend/src/api/admin-members.ts` | **Create** | Five typed API fetch functions |
| `apps/frontend/src/features/admin/MemberManagementPage.tsx` | **Create** | Full page with read + write UI |
| `apps/frontend/src/pages/AdminMembersPage.tsx` | **Create** | Re-export shim |
| `apps/frontend/src/features/admin/AdminSurfaceNav.tsx` | **Modify** | Add Members nav entry |
| `apps/frontend/src/router.ts` | **Modify** | Register `/admin/members` route |
| `apps/frontend/src/pages/AdminPage.test.tsx` | **Modify** | Add render test for `/admin/members` |
| `apps/frontend/src/features/admin/MemberManagementPage.test.tsx` | **Create** | Mutation behavior tests |

---

## Task 1: Backend — admin_members.rs skeleton + GET /v1/admin/members

**Files:**
- Create: `services/query-api/src/admin_members.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Write the failing integration test**

Add to the bottom of `services/query-api/tests/http_api_integration.rs`:

```rust
// ── Admin members API ───────────────────────────────────────────────────────

fn build_admin_members_app(db: PgPool) -> (Router, Uuid, Uuid) {
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = traces::AppState {
        ch,
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(observability::QueryApiMetrics::new()),
    };
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let caller_id = Uuid::new_v4();
    let app = Router::new()
        .route("/v1/admin/members", get(query_api::admin_members::handle_list_members))
        .route("/v1/admin/members", post(query_api::admin_members::handle_add_member))
        .route("/v1/admin/members/:user_id/role", axum::routing::put(query_api::admin_members::handle_update_role))
        .route("/v1/admin/members/:user_id", axum::routing::delete(query_api::admin_members::handle_remove_member))
        .route("/v1/admin/members/:user_id/revoke-sessions", post(query_api::admin_members::handle_revoke_sessions))
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(caller_id),
            role: "tenant_admin".into(),
        }))
        .layer(axum::Extension(db))
        .with_state(state);
    (app, tenant_id, caller_id)
}

async fn seed_user(db: &PgPool, email: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, idp_subject, email, name) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(format!("idp|{email}"))
    .bind(email)
    .bind(email.split('@').next().unwrap_or("user"))
    .execute(db)
    .await
    .expect("user inserted");
    user_id
}

async fn seed_member(db: &PgPool, user_id: Uuid, tenant_id: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(role)
    .execute(db)
    .await
    .expect("member inserted");
}

#[tokio::test]
async fn list_members_returns_tenant_members() {
    let (_ch_container, pg, _pg_container) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());

    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/members")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    let members = body["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|m| m["email"] == "bob@example.com"));
    assert!(members.iter().any(|m| m["role"] == "member"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

```
cd services/query-api
cargo test list_members_returns_tenant_members 2>&1 | tail -20
```

Expected: compile error — `query_api::admin_members` does not exist yet.

- [ ] **Step 3: Create admin_members.rs with the list handler**

Create `services/query-api/src/admin_members.rs`:

```rust
// Admin console — tenant member management.
//
// All handlers require the caller to have role "tenant_admin".
// The tenant_id is always sourced from TenantContext, never from the request.
//
// GET    /v1/admin/members                    — list all members
// POST   /v1/admin/members                    — add by email
// PUT    /v1/admin/members/:user_id/role       — update role
// DELETE /v1/admin/members/:user_id            — remove member + revoke sessions
// POST   /v1/admin/members/:user_id/revoke-sessions — revoke sessions only

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MemberRecord {
    pub user_id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct MemberListResponse {
    pub members: Vec<MemberRecord>,
}

#[derive(Deserialize)]
pub struct AddMemberRequest {
    pub email: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,
}

// ── Role guard helper ─────────────────────────────────────────────────────────

fn require_admin(ctx: &TenantContext) -> Result<(), StatusCode> {
    if ctx.role != "tenant_admin" {
        Err(StatusCode::FORBIDDEN)
    } else {
        Ok(())
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /v1/admin/members
pub async fn handle_list_members(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<MemberListResponse>, StatusCode> {
    require_admin(&ctx)?;

    let rows = sqlx::query!(
        r#"
        SELECT u.id AS user_id, u.email, u.name, utr.role, utr.created_at AS joined_at
        FROM user_tenant_roles utr
        JOIN users u ON u.id = utr.user_id
        WHERE utr.tenant_id = $1
        ORDER BY utr.created_at ASC
        "#,
        ctx.tenant_id
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to list members");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let members = rows
        .into_iter()
        .map(|r| MemberRecord {
            user_id: r.user_id,
            email: r.email,
            name: r.name,
            role: r.role,
            joined_at: r.joined_at,
        })
        .collect();

    Ok(Json(MemberListResponse { members }))
}

/// POST /v1/admin/members — placeholder stubs (implemented in later tasks)
pub async fn handle_add_member(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(_body): Json<AddMemberRequest>,
) -> Result<Json<MemberRecord>, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// PUT /v1/admin/members/:user_id/role — placeholder stub
pub async fn handle_update_role(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(_user_id): Path<Uuid>,
    Json(_body): Json<UpdateRoleRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// DELETE /v1/admin/members/:user_id — placeholder stub
pub async fn handle_remove_member(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(_user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// POST /v1/admin/members/:user_id/revoke-sessions — placeholder stub
pub async fn handle_revoke_sessions(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(_user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}
```

- [ ] **Step 4: Add module declaration to lib.rs**

In `services/query-api/src/lib.rs`, add to the module list:

```rust
pub mod admin_members;
```

- [ ] **Step 5: Run the test to verify it passes**

```
cd services/query-api
cargo test list_members_returns_tenant_members 2>&1 | tail -10
```

Expected: `test list_members_returns_tenant_members ... ok`

- [ ] **Step 6: Run cargo fmt**

```
cargo fmt --all
```

- [ ] **Step 7: Commit**

```
git add services/query-api/src/admin_members.rs services/query-api/src/lib.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(admin-members): add GET /v1/admin/members handler + integration test"
```

---

## Task 2: Backend — POST /v1/admin/members (add by email)

**Files:**
- Modify: `services/query-api/src/admin_members.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Write the failing tests**

Add to `http_api_integration.rs` after the `list_members` test:

```rust
#[tokio::test]
async fn add_member_by_email_succeeds_for_known_user() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    // A user exists in the `users` table but is NOT yet a member of this tenant.
    seed_user(&pg, "newuser@example.com").await;

    let body = serde_json::json!({ "email": "newuser@example.com", "role": "member" });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/members")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["email"], "newuser@example.com");
    assert_eq!(body["role"], "member");
}

#[tokio::test]
async fn add_member_returns_404_for_unknown_email() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    let body = serde_json::json!({ "email": "nobody@example.com", "role": "member" });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/members")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run to verify they fail**

```
cd services/query-api
cargo test add_member 2>&1 | grep -E "FAILED|error"
```

Expected: tests fail with `NOT_IMPLEMENTED`.

- [ ] **Step 3: Implement handle_add_member**

Replace the stub `handle_add_member` in `admin_members.rs`:

```rust
pub async fn handle_add_member(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<MemberRecord>), StatusCode> {
    require_admin(&ctx)?;

    // Look up the user by email.
    let user = sqlx::query!(
        "SELECT id, email, name FROM users WHERE email = $1",
        body.email
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to look up user by email");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let joined_at = sqlx::query_scalar!(
        r#"
        INSERT INTO user_tenant_roles (user_id, tenant_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, tenant_id) DO UPDATE SET role = EXCLUDED.role
        RETURNING created_at
        "#,
        user.id,
        ctx.tenant_id,
        body.role
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to insert member");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(MemberRecord {
            user_id: user.id,
            email: user.email,
            name: user.name,
            role: body.role,
            joined_at,
        }),
    ))
}
```

- [ ] **Step 4: Run tests to verify they pass**

```
cd services/query-api
cargo test add_member 2>&1 | tail -10
```

Expected: both `add_member` tests pass.

- [ ] **Step 5: Run cargo fmt and commit**

```
cargo fmt --all
git add services/query-api/src/admin_members.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(admin-members): implement POST /v1/admin/members (add by email)"
```

---

## Task 3: Backend — PUT /v1/admin/members/:user_id/role (with self-demotion guard)

**Files:**
- Modify: `services/query-api/src/admin_members.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn update_role_changes_member_role() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    let body = serde_json::json!({ "role": "viewer" });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/members/{bob_id}/role"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let row = sqlx::query!("SELECT role FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2", bob_id, tenant_id)
        .fetch_one(&pg)
        .await
        .unwrap();
    assert_eq!(row.role, "viewer");
}

#[tokio::test]
async fn update_role_returns_403_for_self() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    let body = serde_json::json!({ "role": "member" });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/members/{caller_id}/role"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run to verify they fail**

```
cd services/query-api
cargo test update_role 2>&1 | grep -E "FAILED|error"
```

- [ ] **Step 3: Implement handle_update_role**

Replace the stub:

```rust
pub async fn handle_update_role(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;

    // Prevent self-demotion.
    if ctx.user_id == Some(user_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows_affected = sqlx::query!(
        "UPDATE user_tenant_roles SET role = $1 WHERE user_id = $2 AND tenant_id = $3",
        body.role,
        user_id,
        ctx.tenant_id
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to update member role");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .rows_affected();

    if rows_affected == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 4: Run tests and verify they pass**

```
cd services/query-api
cargo test update_role 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```
cargo fmt --all
git add services/query-api/src/admin_members.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(admin-members): implement PUT role with self-demotion guard"
```

---

## Task 4: Backend — DELETE /v1/admin/members/:user_id (last-admin guard + session revocation)

**Files:**
- Modify: `services/query-api/src/admin_members.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn remove_member_deletes_row_and_revokes_sessions() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    // Seed a session for bob.
    sqlx::query(
        "INSERT INTO user_sessions (user_id, tenant_id, environment, issued_at, expires_at) \
         VALUES ($1, $2, 'prod', now(), now() + interval '1 hour')",
    )
    .bind(bob_id)
    .bind(tenant_id)
    .execute(&pg)
    .await
    .unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/admin/members/{bob_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Membership row removed.
    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        bob_id, tenant_id
    )
    .fetch_one(&pg)
    .await
    .unwrap()
    .unwrap_or(0);
    assert_eq!(count, 0);

    // Session revoked.
    let revoked: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND revoked_at IS NOT NULL",
        bob_id
    )
    .fetch_one(&pg)
    .await
    .unwrap()
    .unwrap_or(0);
    assert_eq!(revoked, 1);
}

#[tokio::test]
async fn remove_last_admin_returns_403() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    // Only one admin — the caller.
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    // Try to remove the only admin (caller_id, but as a target).
    // We use bob here as the target — but bob isn't an admin, so let's
    // instead promote bob to admin and remove caller.
    sqlx::query!("UPDATE user_tenant_roles SET role = 'tenant_admin' WHERE user_id = $1", bob_id)
        .execute(&pg)
        .await
        .unwrap();
    sqlx::query!("UPDATE user_tenant_roles SET role = 'member' WHERE user_id = $1", caller_id)
        .execute(&pg)
        .await
        .unwrap();

    // Now bob is the only admin. Try to remove bob.
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/admin/members/{bob_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run to verify they fail**

```
cd services/query-api
cargo test remove_member 2>&1 | grep -E "FAILED|error"
```

- [ ] **Step 3: Implement handle_remove_member**

Replace the stub:

```rust
pub async fn handle_remove_member(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;

    // Guard: cannot remove the last tenant_admin.
    let admin_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM user_tenant_roles \
         WHERE tenant_id = $1 AND role = 'tenant_admin'",
        ctx.tenant_id
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to count admins");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or(0);

    let target_is_admin: bool = sqlx::query_scalar!(
        "SELECT role = 'tenant_admin' FROM user_tenant_roles \
         WHERE user_id = $1 AND tenant_id = $2",
        user_id,
        ctx.tenant_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to fetch target role");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .flatten()
    .unwrap_or(false);

    if target_is_admin && admin_count <= 1 {
        return Err(StatusCode::FORBIDDEN);
    }

    // Revoke sessions and remove membership in one transaction.
    let mut tx = state.db.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query!(
        "UPDATE user_sessions SET revoked_at = now() \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked_at IS NULL AND expires_at > now()",
        user_id,
        ctx.tenant_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to revoke sessions on remove");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rows = sqlx::query!(
        "DELETE FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        user_id,
        ctx.tenant_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to delete member");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .rows_affected();

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 4: Run tests and verify they pass**

```
cd services/query-api
cargo test remove_member 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```
cargo fmt --all
git add services/query-api/src/admin_members.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(admin-members): implement DELETE with last-admin guard and session revocation"
```

---

## Task 5: Backend — POST /v1/admin/members/:user_id/revoke-sessions + non-admin 403 tests

**Files:**
- Modify: `services/query-api/src/admin_members.rs`
- Modify: `services/query-api/tests/http_api_integration.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn revoke_sessions_marks_all_active_sessions_revoked() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    // Two active sessions for bob.
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO user_sessions (user_id, tenant_id, environment, issued_at, expires_at) \
             VALUES ($1, $2, 'prod', now(), now() + interval '1 hour')",
        )
        .bind(bob_id)
        .bind(tenant_id)
        .execute(&pg)
        .await
        .unwrap();
    }

    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/admin/members/{bob_id}/revoke-sessions"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let revoked: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND revoked_at IS NOT NULL",
        bob_id
    )
    .fetch_one(&pg)
    .await
    .unwrap()
    .unwrap_or(0);
    assert_eq!(revoked, 2);
}

#[tokio::test]
async fn admin_members_returns_403_for_non_admin() {
    let (_ch_c, pg, _pg_c) = {
        let (ch, ch_c) = start_clickhouse().await;
        let (pg, pg_c) = start_postgres().await;
        (ch_c, pg, pg_c)
    };
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let caller_id = Uuid::new_v4();
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = traces::AppState {
        ch,
        db: pg.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(observability::QueryApiMetrics::new()),
    };
    let app = Router::new()
        .route("/v1/admin/members", get(query_api::admin_members::handle_list_members))
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(caller_id),
            role: "member".into(), // non-admin
        }))
        .layer(axum::Extension(pg))
        .with_state(state);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/members")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run to verify they fail**

```
cd services/query-api
cargo test revoke_sessions admin_members_returns_403 2>&1 | grep -E "FAILED|error"
```

- [ ] **Step 3: Implement handle_revoke_sessions**

Replace the stub:

```rust
pub async fn handle_revoke_sessions(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;

    sqlx::query!(
        "UPDATE user_sessions SET revoked_at = now() \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked_at IS NULL AND expires_at > now()",
        user_id,
        ctx.tenant_id
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to revoke sessions");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 4: Run all admin_members tests**

```
cd services/query-api
cargo test --test http_api_integration -- members sessions admin_members 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 5: Commit**

```
cargo fmt --all
git add services/query-api/src/admin_members.rs services/query-api/tests/http_api_integration.rs
git commit -m "feat(admin-members): implement revoke-sessions + non-admin 403 test"
```

---

## Task 6: Backend — Register routes in main.rs

**Files:**
- Modify: `services/query-api/src/main.rs`

- [ ] **Step 1: Add import and five routes**

In `services/query-api/src/main.rs`, add the import at the top with the other module imports:

```rust
use axum::routing::delete;
```

(Already present if dashboards uses it — check first; skip if already imported.)

Add in the `Router::new()` chain after the existing dashboard grant routes (around line 131):

```rust
        .route("/v1/admin/members", get(admin_members::handle_list_members))
        .route("/v1/admin/members", post(admin_members::handle_add_member))
        .route(
            "/v1/admin/members/:user_id/role",
            axum::routing::put(admin_members::handle_update_role),
        )
        .route(
            "/v1/admin/members/:user_id",
            delete(admin_members::handle_remove_member),
        )
        .route(
            "/v1/admin/members/:user_id/revoke-sessions",
            post(admin_members::handle_revoke_sessions),
        )
```

- [ ] **Step 2: Build to verify it compiles**

```
cd services/query-api
cargo build 2>&1 | grep -E "error|warning" | grep -v "warning:" | head -10
```

Expected: clean build (zero errors).

- [ ] **Step 3: Commit**

```
cargo fmt --all
git add services/query-api/src/main.rs
git commit -m "feat(admin-members): register five admin member routes in main.rs"
```

---

## Task 7: Frontend — api/admin-members.ts

**Files:**
- Create: `apps/frontend/src/api/admin-members.ts`

- [ ] **Step 1: Write the API module**

Create `apps/frontend/src/api/admin-members.ts`:

```typescript
// Admin console — tenant member management API.
//
// GET    /v1/admin/members                    → MemberListResponse
// POST   /v1/admin/members                    → MemberRecord (201)
// PUT    /v1/admin/members/:userId/role       → 204
// DELETE /v1/admin/members/:userId            → 204
// POST   /v1/admin/members/:userId/revoke-sessions → 204

export interface MemberRecord {
  user_id: string;
  email: string;
  name: string | null;
  role: "tenant_admin" | "member" | "viewer";
  joined_at: string;
}

export interface MemberListResponse {
  members: MemberRecord[];
}

export type TenantRole = "tenant_admin" | "member" | "viewer";

export async function listMembers(tenantId: string): Promise<MemberListResponse> {
  const res = await fetch("/v1/admin/members", {
    credentials: "include",
    headers: { "X-Tenant-ID": tenantId },
  });
  if (!res.ok) throw new Error(`listMembers failed: ${res.status}`);
  return res.json() as Promise<MemberListResponse>;
}

export async function addMember(
  tenantId: string,
  body: { email: string; role: TenantRole },
): Promise<MemberRecord> {
  const res = await fetch("/v1/admin/members", {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json", "X-Tenant-ID": tenantId },
    body: JSON.stringify(body),
  });
  if (res.status === 404) throw new Error("EMAIL_NOT_FOUND");
  if (!res.ok) throw new Error(`addMember failed: ${res.status}`);
  return res.json() as Promise<MemberRecord>;
}

export async function updateMemberRole(
  tenantId: string,
  userId: string,
  role: TenantRole,
): Promise<void> {
  const res = await fetch(`/v1/admin/members/${encodeURIComponent(userId)}/role`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json", "X-Tenant-ID": tenantId },
    body: JSON.stringify({ role }),
  });
  if (res.status === 403) throw new Error("SELF_DEMOTION");
  if (!res.ok) throw new Error(`updateMemberRole failed: ${res.status}`);
}

export async function removeMember(tenantId: string, userId: string): Promise<void> {
  const res = await fetch(`/v1/admin/members/${encodeURIComponent(userId)}`, {
    method: "DELETE",
    credentials: "include",
    headers: { "X-Tenant-ID": tenantId },
  });
  if (res.status === 403) throw new Error("LAST_ADMIN");
  if (!res.ok) throw new Error(`removeMember failed: ${res.status}`);
}

export async function revokeMemberSessions(tenantId: string, userId: string): Promise<void> {
  const res = await fetch(
    `/v1/admin/members/${encodeURIComponent(userId)}/revoke-sessions`,
    {
      method: "POST",
      credentials: "include",
      headers: { "X-Tenant-ID": tenantId },
    },
  );
  if (!res.ok) throw new Error(`revokeSessions failed: ${res.status}`);
}
```

- [ ] **Step 2: Type-check**

```
cd apps/frontend
npx tsc --noEmit 2>&1 | grep -i "admin-members\|error" | head -20
```

Expected: no errors relating to `admin-members.ts`.

- [ ] **Step 3: Commit**

```
git add apps/frontend/src/api/admin-members.ts
git commit -m "feat(admin-members): add typed API client for member management"
```

---

## Task 8: Frontend — MemberManagementPage.tsx

**Files:**
- Create: `apps/frontend/src/features/admin/MemberManagementPage.tsx`

- [ ] **Step 1: Write the page component**

Create `apps/frontend/src/features/admin/MemberManagementPage.tsx`:

```tsx
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  type MemberRecord,
  type TenantRole,
  addMember,
  listMembers,
  removeMember,
  revokeMemberSessions,
  updateMemberRole,
} from "../../api/admin-members";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { TablePanel } from "../../components/ui/table-panel";
import { useAuth } from "../../hooks/useAuth";
import { useTenantContext } from "../../hooks/useTenantContext";
import { roleTone, roleLabel } from "./admin-utils";
import { AdminSurfaceNav } from "./AdminSurfaceNav";

const ROLES: TenantRole[] = ["tenant_admin", "member", "viewer"];

export function MemberManagementPage() {
  const { tenantId } = useTenantContext();
  const { data: me } = useAuth();
  const qc = useQueryClient();

  const [addEmail, setAddEmail] = useState("");
  const [addRole, setAddRole] = useState<TenantRole>("member");
  const [addError, setAddError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["admin-members", tenantId],
    queryFn: () => listMembers(tenantId),
    enabled: !!tenantId,
  });

  const myMembership = me?.tenants?.find((t) => t.tenant_id === tenantId);
  const isAdmin = myMembership?.role === "tenant_admin";

  const invalidate = () => qc.invalidateQueries({ queryKey: ["admin-members", tenantId] });

  const addMutation = useMutation({
    mutationFn: (vars: { email: string; role: TenantRole }) =>
      addMember(tenantId, vars),
    onSuccess: () => {
      setAddEmail("");
      setAddError(null);
      setStatusMsg("Member added.");
      void invalidate();
    },
    onError: (err: Error) => {
      if (err.message === "EMAIL_NOT_FOUND") {
        setAddError("No account found for that email.");
      } else {
        setAddError("Failed to add member. Please try again.");
      }
    },
  });

  const roleMutation = useMutation({
    mutationFn: (vars: { userId: string; role: TenantRole }) =>
      updateMemberRole(tenantId, vars.userId, vars.role),
    onSuccess: () => { setStatusMsg("Role updated."); void invalidate(); },
    onError: (err: Error) => {
      if (err.message === "SELF_DEMOTION") {
        setStatusMsg("You cannot change your own role.");
      } else {
        setStatusMsg("Failed to update role.");
      }
    },
  });

  const removeMutation = useMutation({
    mutationFn: (userId: string) => removeMember(tenantId, userId),
    onSuccess: () => { setStatusMsg("Member removed."); void invalidate(); },
    onError: (err: Error) => {
      if (err.message === "LAST_ADMIN") {
        setStatusMsg("Cannot remove the last admin from a tenant.");
      } else {
        setStatusMsg("Failed to remove member.");
      }
    },
  });

  const revokeMutation = useMutation({
    mutationFn: (userId: string) => revokeMemberSessions(tenantId, userId),
    onSuccess: () => { setStatusMsg("Sessions revoked."); void invalidate(); },
    onError: () => setStatusMsg("Failed to revoke sessions."),
  });

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    setAddError(null);
    if (!addEmail.trim()) { setAddError("Email is required."); return; }
    addMutation.mutate({ email: addEmail.trim(), role: addRole });
  }

  function handleRemove(member: MemberRecord) {
    if (!confirm(`Remove ${member.email} from this tenant?`)) return;
    removeMutation.mutate(member.user_id);
  }

  if (isLoading) return <LoadingState>Loading members…</LoadingState>;

  const members = data?.members ?? [];

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Members</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            Manage who has access to this tenant. Only tenant admins can make changes.
          </p>
        </div>
      </div>

      <AdminSurfaceNav />

      {statusMsg && (
        <p className="text-sm text-[var(--text)]" role="status">
          {statusMsg}
        </p>
      )}

      {isAdmin && (
        <Panel title="Add member" eyebrow="Invite">
          <form onSubmit={handleAdd} className="flex flex-wrap items-end gap-3">
            <div className="flex flex-col gap-1">
              <label className="field-label" htmlFor="add-email">Email</label>
              <input
                id="add-email"
                type="email"
                className="input w-64"
                placeholder="user@example.com"
                value={addEmail}
                onChange={(e) => setAddEmail(e.target.value)}
                disabled={addMutation.isPending}
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="field-label" htmlFor="add-role">Role</label>
              <select
                id="add-role"
                className="input"
                value={addRole}
                onChange={(e) => setAddRole(e.target.value as TenantRole)}
                disabled={addMutation.isPending}
              >
                {ROLES.map((r) => (
                  <option key={r} value={r}>{roleLabel(r)}</option>
                ))}
              </select>
            </div>
            <button
              type="submit"
              className="button-primary"
              disabled={addMutation.isPending}
            >
              {addMutation.isPending ? "Adding…" : "Add"}
            </button>
            {addError && (
              <p className="w-full text-xs text-[var(--error)]">{addError}</p>
            )}
          </form>
        </Panel>
      )}

      <Panel title={`Members (${members.length})`} eyebrow="RBAC">
        {members.length === 0 ? (
          <EmptyState title="No members" description="No users are assigned to this tenant." />
        ) : (
          <TablePanel>
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-[var(--surface-muted)] text-[var(--muted)]">
                <tr>
                  <th scope="col" className="px-3 py-2 font-semibold">Name / Email</th>
                  <th scope="col" className="px-3 py-2 font-semibold">Role</th>
                  {isAdmin && <th scope="col" className="px-3 py-2 font-semibold">Actions</th>}
                </tr>
              </thead>
              <tbody>
                {members.map((member) => {
                  const isSelf = member.user_id === me?.user_id;
                  return (
                    <tr key={member.user_id}>
                      <td className="px-3 py-2">
                        <div className="font-medium text-[var(--text-strong)]">
                          {member.name ?? member.email}
                          {isSelf && (
                            <span className="ml-2 text-[11px] uppercase tracking-wide text-[var(--muted)]">
                              you
                            </span>
                          )}
                        </div>
                        {member.name && (
                          <div className="text-xs text-[var(--muted)]">{member.email}</div>
                        )}
                      </td>
                      <td className="px-3 py-2">
                        {isAdmin && !isSelf ? (
                          <select
                            aria-label={`Role for ${member.email}`}
                            className="input text-sm"
                            value={member.role}
                            onChange={(e) =>
                              roleMutation.mutate({
                                userId: member.user_id,
                                role: e.target.value as TenantRole,
                              })
                            }
                            disabled={roleMutation.isPending}
                          >
                            {ROLES.map((r) => (
                              <option key={r} value={r}>{roleLabel(r)}</option>
                            ))}
                          </select>
                        ) : (
                          <Badge tone={roleTone(member.role)}>{roleLabel(member.role)}</Badge>
                        )}
                      </td>
                      {isAdmin && (
                        <td className="px-3 py-2">
                          <div className="flex items-center gap-3">
                            <button
                              className="text-xs text-[var(--muted)] hover:text-[var(--text)] transition-colors"
                              onClick={() => revokeMutation.mutate(member.user_id)}
                              disabled={revokeMutation.isPending}
                              aria-label={`Revoke sessions for ${member.email}`}
                            >
                              Revoke sessions
                            </button>
                            {!isSelf && (
                              <button
                                className="text-xs text-[var(--error)] hover:opacity-80 transition-opacity"
                                onClick={() => handleRemove(member)}
                                disabled={removeMutation.isPending}
                                aria-label={`Remove ${member.email}`}
                              >
                                ×
                              </button>
                            )}
                          </div>
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </TablePanel>
        )}
      </Panel>
    </section>
  );
}
```

- [ ] **Step 2: Type-check**

```
cd apps/frontend
npx tsc --noEmit 2>&1 | grep -i "MemberManagement\|error" | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```
git add apps/frontend/src/features/admin/MemberManagementPage.tsx
git commit -m "feat(admin-members): add MemberManagementPage with full read/write UI"
```

---

## Task 9: Frontend — Route wiring

**Files:**
- Create: `apps/frontend/src/pages/AdminMembersPage.tsx`
- Modify: `apps/frontend/src/features/admin/AdminSurfaceNav.tsx`
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Create the page shim**

Create `apps/frontend/src/pages/AdminMembersPage.tsx`:

```tsx
export { MemberManagementPage as default } from "../features/admin/MemberManagementPage";
```

- [ ] **Step 2: Update AdminSurfaceNav**

In `apps/frontend/src/features/admin/AdminSurfaceNav.tsx`, update `sections`:

```ts
const sections: AdminSection[] = [
  { to: "/admin", label: "Overview" },
  { to: "/admin/config", label: "Tenant configuration" },
  { to: "/admin/members", label: "Members" },
  { to: "/admin/fleet", label: "Fleet management" },
  { to: "/admin/identity", label: "Identity" },
];
```

- [ ] **Step 3: Register the route in router.ts**

In `apps/frontend/src/router.ts`, add after the `adminFleetRoute` definition:

```ts
import AdminMembersPage from "./pages/AdminMembersPage";
```

(Add this with the other admin imports at the top of the file.)

Then add the route definition after `adminFleetRoute`:

```ts
const adminMembersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/members",
  component: AdminMembersPage,
});
```

Then add `adminMembersRoute` to the `routeTree` array where the other admin routes are registered:

```ts
    adminRoute,
    identitySettingsRoute,
    adminConfigRoute,
    adminFleetRoute,
    adminMembersRoute,   // ← add this line
```

- [ ] **Step 4: Type-check the whole frontend**

```
cd apps/frontend
npx tsc --noEmit 2>&1 | tail -20
```

Expected: no errors.

- [ ] **Step 5: Commit**

```
git add apps/frontend/src/pages/AdminMembersPage.tsx \
        apps/frontend/src/features/admin/AdminSurfaceNav.tsx \
        apps/frontend/src/router.ts
git commit -m "feat(admin-members): wire /admin/members route and nav entry"
```

---

## Task 10: Frontend — Tests

**Files:**
- Modify: `apps/frontend/src/pages/AdminPage.test.tsx`
- Create: `apps/frontend/src/features/admin/MemberManagementPage.test.tsx`

- [ ] **Step 1: Add a render test for /admin/members in AdminPage.test.tsx**

In the `vi.fn(async (input: RequestInfo | URL) => {` mock inside `beforeEach`, ensure the mock handles `/v1/admin/members`. The existing mock returns `{ items: [] }` for unknown URLs — members needs `{ members: [] }`. Add this branch before the final fallback:

```ts
      if (url.includes("/v1/admin/members")) {
        return new Response(JSON.stringify({ members: [] }), { status: 200 });
      }
```

Then add a new test at the bottom of `AdminPage.test.tsx`:

```ts
test("renders the members management page at /admin/members", async () => {
  window.history.pushState({}, "", "/admin/members");

  render(<App />);

  await screen.findByRole("heading", { name: "Members" });
  expect(screen.getByRole("link", { name: "Members" })).toBeInTheDocument();
  expect(screen.getByText("Manage who has access to this tenant.")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the existing + new AdminPage tests**

```
cd apps/frontend
npx vitest run src/pages/AdminPage.test.tsx 2>&1 | tail -20
```

Expected: all tests pass (including the new one).

- [ ] **Step 3: Write MemberManagementPage.test.tsx**

Create `apps/frontend/src/features/admin/MemberManagementPage.test.tsx`:

```tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const TENANT_ID = "00000000-0000-0000-0000-000000000002";
const ALICE_ID = "aaaaaaaa-0000-0000-0000-000000000001";
const BOB_ID = "bbbbbbbb-0000-0000-0000-000000000001";

const BASE_MEMBERS = [
  { user_id: ALICE_ID, email: "alice@example.com", name: "Alice", role: "tenant_admin", joined_at: "2026-01-01T00:00:00Z" },
  { user_id: BOB_ID,   email: "bob@example.com",   name: "Bob",   role: "member",       joined_at: "2026-01-02T00:00:00Z" },
];

let App: typeof import("../../App").default;

function stubFetch(overrides: Record<string, () => Response> = {}) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      for (const [pattern, handler] of Object.entries(overrides)) {
        if (url.includes(pattern)) return handler();
      }
      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: ALICE_ID,
            email: "alice@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "tenant_admin" }],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/admin/members")) {
        return new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 });
      }
      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ environments: [] }), { status: 200 });
      }
      if (url.includes("/v1/tenants/usage-report")) {
        return new Response(JSON.stringify({
          tenant_id: TENANT_ID, from: "", to: "",
          telemetry_summary: { spans: 0, logs: 0, metric_points: 0, metric_series_created: 0 },
          control_plane_summary: { query_reads: 0, query_rows: 0, credential_checks: 0, credential_allows: 0, credential_denies: 0 },
          estimated_cost_index: 0,
        }), { status: 200 });
      }
      if (url.includes("/v1/tenants")) {
        return new Response(JSON.stringify({ tenants: [{ id: TENANT_ID, name: "observable" }] }), { status: 200 });
      }
      return new Response(JSON.stringify({}), { status: 200 });
    }),
  );
}

beforeEach(async () => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/admin/members");
  vi.resetModules();
  stubFetch();
  ({ default: App } = await import("../../App"));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

test("renders member list with roles", async () => {
  render(<App />);
  await screen.findByRole("heading", { name: "Members" });
  expect(screen.getByText("Alice")).toBeInTheDocument();
  expect(screen.getByText("Bob")).toBeInTheDocument();
  expect(screen.getByText("Members (2)")).toBeInTheDocument();
});

test("shows add form for tenant_admin", async () => {
  render(<App />);
  await screen.findByRole("heading", { name: "Members" });
  expect(screen.getByLabelText("Email")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
});

test("shows inline error when email not found", async () => {
  stubFetch({
    "/v1/admin/members": () =>
      new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 }),
  });
  // Override POST to return 404.
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.includes("/v1/auth/me")) {
      return new Response(JSON.stringify({ user_id: ALICE_ID, email: "alice@example.com", tenants: [{ tenant_id: TENANT_ID, role: "tenant_admin" }] }), { status: 200 });
    }
    if (url.includes("/v1/admin/members") && (!init?.method || init.method === "GET")) {
      return new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 });
    }
    if (url.includes("/v1/admin/members") && init?.method === "POST") {
      return new Response(JSON.stringify({ error: "not found" }), { status: 404 });
    }
    if (url.includes("/v1/tenants")) return new Response(JSON.stringify({ tenants: [{ id: TENANT_ID, name: "observable" }] }), { status: 200 });
    return new Response(JSON.stringify({}), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  vi.resetModules();
  ({ default: App } = await import("../../App"));

  render(<App />);
  await screen.findByRole("heading", { name: "Members" });
  fireEvent.change(screen.getByLabelText("Email"), { target: { value: "nobody@example.com" } });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  await waitFor(() =>
    expect(screen.getByText("No account found for that email.")).toBeInTheDocument(),
  );
});

test("hides mutation controls for non-admin user", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: BOB_ID,
            email: "bob@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "member" }],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/admin/members")) {
        return new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 });
      }
      if (url.includes("/v1/tenants")) return new Response(JSON.stringify({ tenants: [{ id: TENANT_ID, name: "observable" }] }), { status: 200 });
      return new Response(JSON.stringify({}), { status: 200 });
    }),
  );
  vi.resetModules();
  ({ default: App } = await import("../../App"));

  render(<App />);
  await screen.findByRole("heading", { name: "Members" });
  // Add form not present for non-admin.
  expect(screen.queryByLabelText("Email")).not.toBeInTheDocument();
  // Remove buttons not present.
  expect(screen.queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
});
```

- [ ] **Step 4: Run all new tests**

```
cd apps/frontend
npx vitest run src/features/admin/MemberManagementPage.test.tsx 2>&1 | tail -20
```

Expected: all four tests pass.

- [ ] **Step 5: Run full frontend test suite to check for regressions**

```
cd apps/frontend
npx vitest run 2>&1 | tail -10
```

Expected: all existing tests continue to pass.

- [ ] **Step 6: Commit**

```
git add apps/frontend/src/pages/AdminPage.test.tsx \
        apps/frontend/src/features/admin/MemberManagementPage.test.tsx
git commit -m "test(admin-members): add render and mutation tests for MemberManagementPage"
```

---

## Task 11: Final verification and PR

- [ ] **Step 1: Run the full backend test suite**

```
cd services/query-api
cargo test 2>&1 | tail -20
```

Expected: all tests pass including the new admin_members tests.

- [ ] **Step 2: Run local-ci.sh (if Docker is available)**

```
bash scripts/local-ci.sh 2>&1 | tail -30
```

Expected: passes.

- [ ] **Step 3: Push branch and open PR**

```
git push -u origin HEAD
gh pr create \
  --title "feat(admin): member management — list, invite, re-role, remove, revoke sessions" \
  --body "$(cat <<'EOF'
## Summary
- Adds `/admin/members` tab to the admin console
- Five new backend endpoints under `GET/POST/PUT/DELETE /v1/admin/members` gated to `tenant_admin` role
- Frontend page with add-by-email form, inline role selects, remove with confirm, revoke-sessions per row
- Non-admins see the read-only member list with all mutation controls hidden
- Guards: self-demotion blocked (PUT role), last-admin lockout (DELETE), session cascade on remove

## Test plan
- [ ] Backend: `cargo test --test http_api_integration -- members sessions` — all pass
- [ ] Frontend: `npx vitest run src/features/admin/MemberManagementPage.test.tsx` — all pass
- [ ] Manual: navigate to `/admin/members` as tenant_admin, add a member, change role, remove
- [ ] Manual: navigate as non-admin — confirm controls are hidden

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
