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

#[derive(Serialize, sqlx::FromRow)]
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

    let members = sqlx::query_as::<_, MemberRecord>(
        r#"
        SELECT u.id AS user_id, u.email, u.name, utr.role, utr.created_at AS joined_at
        FROM user_tenant_roles utr
        JOIN users u ON u.id = utr.user_id
        WHERE utr.tenant_id = $1
        ORDER BY utr.created_at ASC
        "#,
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to list members");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(MemberListResponse { members }))
}

/// POST /v1/admin/members — add a user (by email) to the tenant
pub async fn handle_add_member(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<MemberRecord>), StatusCode> {
    require_admin(&ctx)?;

    // 1. Look up the user by email.
    #[derive(sqlx::FromRow)]
    struct UserRow {
        id: Uuid,
        email: String,
        name: Option<String>,
    }

    let user = sqlx::query_as::<_, UserRow>("SELECT id, email, name FROM users WHERE email = $1")
        .bind(&body.email)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "failed to look up user by email");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 2. Upsert into user_tenant_roles; return the created_at timestamp.
    let joined_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        r#"
        INSERT INTO user_tenant_roles (user_id, tenant_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, tenant_id) DO UPDATE SET role = EXCLUDED.role
        RETURNING created_at
        "#,
    )
    .bind(user.id)
    .bind(ctx.tenant_id)
    .bind(&body.role)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to upsert member");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let record = MemberRecord {
        user_id: user.id,
        email: user.email,
        name: user.name,
        role: body.role,
        joined_at,
    };

    Ok((StatusCode::CREATED, Json(record)))
}

/// PUT /v1/admin/members/:user_id/role — stub (implemented in Task 3)
pub async fn handle_update_role(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(_user_id): Path<Uuid>,
    Json(_body): Json<UpdateRoleRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// DELETE /v1/admin/members/:user_id — stub (implemented in Task 4)
pub async fn handle_remove_member(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(_user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// POST /v1/admin/members/:user_id/revoke-sessions — stub (implemented in Task 5)
pub async fn handle_revoke_sessions(
    State(_state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(_user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    Err(StatusCode::NOT_IMPLEMENTED)
}
