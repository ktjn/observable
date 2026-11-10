//! Tenant administration of auth-owned data, served by auth-service itself: ingestion tokens
//! (`auth.api_keys`) and tenant members (`auth.users`, `auth.user_tenant_roles`,
//! `auth.user_sessions`). Every route needs a session whose role in the selected tenant
//! (`X-Tenant-ID`, default the session tenant) is `tenant_admin`. The tenant always comes from
//! the verified session, never from the request body or path.
//!
//! GET    /v1/tokens                          list the tenant's tokens
//! POST   /v1/tokens                          create; the plaintext is returned once
//! DELETE /v1/tokens/{id}                     soft-revoke
//! POST   /v1/tokens/{id}/renew               rotate the key and clear revoked_at
//! POST   /v1/tokens/{id}/restore             clear revoked_at, keeping the key
//! DELETE /v1/tokens/{id}/permanent           hard-delete a revoked token
//! GET    /v1/admin/members                   list members
//! POST   /v1/admin/members                   add a known user by email
//! PUT    /v1/admin/members/{user_id}/role    change a member's role (never one's own)
//! DELETE /v1/admin/members/{user_id}         remove a member and revoke their sessions
//! POST   /v1/admin/members/{user_id}/revoke-sessions
//!
//! Each mutation and its `credential_audit_log` row commit together; an audit failure fails
//! the request.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post, put},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::oidc::{OidcState, resolve_session};
use crate::validate::sha256_hex;

const MEMBER_ROLES: &[&str] = &["tenant_admin", "member", "viewer"];

pub fn router() -> Router<OidcState> {
    Router::new()
        .route("/v1/tokens", get(list_tokens).post(create_token))
        .route("/v1/tokens/{id}", delete(revoke_token))
        .route("/v1/tokens/{id}/renew", post(renew_token))
        .route("/v1/tokens/{id}/restore", post(restore_token))
        .route("/v1/tokens/{id}/permanent", delete(delete_token))
        .route("/v1/admin/members", get(list_members).post(add_member))
        .route("/v1/admin/members/{user_id}/role", put(update_role))
        .route("/v1/admin/members/{user_id}", delete(remove_member))
        .route(
            "/v1/admin/members/{user_id}/revoke-sessions",
            post(revoke_sessions),
        )
}

struct Admin {
    tenant_id: Uuid,
    user_id: Uuid,
}

/// The caller's verified session, restricted to `tenant_admin` of the selected tenant.
async fn tenant_admin(state: &OidcState, headers: &HeaderMap) -> Result<Admin, StatusCode> {
    let token = match observable_auth::extract_session_cookie(headers) {
        Some(cookie) => cookie,
        None => observable_auth::extract_bearer_token(headers)
            .map_err(StatusCode::from)?
            .ok_or(StatusCode::UNAUTHORIZED)?,
    };
    let requested = match observable_auth::extract_tenant_id_header(headers) {
        Ok(id) => Some(id),
        Err(observable_auth::AuthError::BadRequest) => return Err(StatusCode::BAD_REQUEST),
        Err(_) => None,
    };
    let session = resolve_session(state, &token, requested).await?;
    let admin = Admin {
        tenant_id: session
            .tenant_id
            .parse()
            .map_err(|_| StatusCode::UNAUTHORIZED)?,
        user_id: session
            .user_id
            .parse()
            .map_err(|_| StatusCode::UNAUTHORIZED)?,
    };
    // The current membership decides, not the role captured in the session at login, so a
    // demoted or removed admin loses access immediately.
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(admin.user_id)
    .bind(admin.tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(store_error)?;
    if role.as_deref() != Some("tenant_admin") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(admin)
}

fn store_error(e: sqlx::Error) -> StatusCode {
    tracing::error!(error = %e, "tenant admin store error");
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    action: &str,
    admin: &Admin,
) -> Result<(), StatusCode> {
    sqlx::query(
        "INSERT INTO credential_audit_log (action, outcome, credential_hash, tenant_id, auth_method) \
         VALUES ($1, 'allow', $2, $3, 'tenant_admin')",
    )
    .bind(action)
    .bind(admin.user_id.to_string())
    .bind(admin.tenant_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(store_error)
}

// ── Tokens ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct TokenRecord {
    pub id: Uuid,
    pub name: String,
    pub environment: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked: bool,
}

#[derive(Serialize)]
pub struct TokenList {
    pub tokens: Vec<TokenRecord>,
}

#[derive(Deserialize)]
pub struct CreateToken {
    pub name: String,
    pub environment: String,
}

#[derive(Serialize)]
pub struct IssuedToken {
    #[serde(flatten)]
    pub token: TokenRecord,
    /// Shown once, never stored.
    pub plaintext: String,
}

fn new_key() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let plaintext: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let hash = sha256_hex(&plaintext);
    (plaintext, hash)
}

macro_rules! token_columns {
    () => {
        "id, name, environment, created_at, revoked_at IS NOT NULL AS revoked"
    };
}

async fn list_tokens(
    State(state): State<OidcState>,
    headers: HeaderMap,
) -> Result<Json<TokenList>, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    let tokens = sqlx::query_as::<_, TokenRecord>(concat!(
        "SELECT ",
        token_columns!(),
        " FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC"
    ))
    .bind(admin.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(store_error)?;
    Ok(Json(TokenList { tokens }))
}

async fn create_token(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Json(body): Json<CreateToken>,
) -> Result<Json<IssuedToken>, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    let (name, environment) = (body.name.trim(), body.environment.trim());
    if name.is_empty() || environment.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let (plaintext, hash) = new_key();
    let mut tx = state.db.begin().await.map_err(store_error)?;
    let token = sqlx::query_as::<_, TokenRecord>(concat!(
        "INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) \
         VALUES ($1, $2, $3, $4, 'member') RETURNING ",
        token_columns!()
    ))
    .bind(admin.tenant_id)
    .bind(&hash)
    .bind(name)
    .bind(environment)
    .fetch_one(&mut *tx)
    .await
    .map_err(store_error)?;
    audit(&mut tx, "token_create", &admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok(Json(IssuedToken { token, plaintext }))
}

/// Runs a single-row token mutation and its audit; no matching row is 404.
async fn mutate_token(
    state: &OidcState,
    admin: &Admin,
    sql: &'static str,
    id: Uuid,
    action: &str,
) -> Result<StatusCode, StatusCode> {
    let mut tx = state.db.begin().await.map_err(store_error)?;
    let rows = sqlx::query(sql)
        .bind(id)
        .bind(admin.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(store_error)?
        .rows_affected();
    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(&mut tx, action, admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_token(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    mutate_token(
        &state,
        &admin,
        "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
        id,
        "token_revoke",
    )
    .await
}

async fn restore_token(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    mutate_token(
        &state,
        &admin,
        "UPDATE api_keys SET revoked_at = NULL WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NOT NULL",
        id,
        "token_restore",
    )
    .await
}

async fn delete_token(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    mutate_token(
        &state,
        &admin,
        "DELETE FROM api_keys WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NOT NULL",
        id,
        "token_delete",
    )
    .await
}

async fn renew_token(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<IssuedToken>, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    let (plaintext, hash) = new_key();
    let mut tx = state.db.begin().await.map_err(store_error)?;
    let token = sqlx::query_as::<_, TokenRecord>(concat!(
        "UPDATE api_keys SET key_hash = $3, revoked_at = NULL \
         WHERE id = $1 AND tenant_id = $2 RETURNING ",
        token_columns!()
    ))
    .bind(id)
    .bind(admin.tenant_id)
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_error)?
    .ok_or(StatusCode::NOT_FOUND)?;
    audit(&mut tx, "token_renew", &admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok(Json(IssuedToken { token, plaintext }))
}

// ── Members ───────────────────────────────────────────────────────────────────

/// Mirrors `admin.Member@1` in `models/admin.mdl`.
#[derive(Serialize, sqlx::FromRow)]
pub struct MemberRecord {
    pub user_id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct MemberList {
    pub members: Vec<MemberRecord>,
}

#[derive(Deserialize)]
pub struct AddMember {
    pub email: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateRole {
    pub role: String,
}

fn valid_role(role: &str) -> Result<(), StatusCode> {
    if MEMBER_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(StatusCode::UNPROCESSABLE_ENTITY)
    }
}

async fn list_members(
    State(state): State<OidcState>,
    headers: HeaderMap,
) -> Result<Json<MemberList>, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    let members = sqlx::query_as::<_, MemberRecord>(
        "SELECT u.id AS user_id, u.email, u.name, utr.role, utr.created_at AS joined_at \
         FROM user_tenant_roles utr JOIN users u ON u.id = utr.user_id \
         WHERE utr.tenant_id = $1 ORDER BY utr.created_at ASC",
    )
    .bind(admin.tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(store_error)?;
    Ok(Json(MemberList { members }))
}

async fn add_member(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Json(body): Json<AddMember>,
) -> Result<(StatusCode, Json<MemberRecord>), StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    valid_role(&body.role)?;
    let mut tx = state.db.begin().await.map_err(store_error)?;
    let member = sqlx::query_as::<_, MemberRecord>(
        "WITH target AS (SELECT id, email, name FROM users WHERE email = $1), \
         upserted AS ( \
           INSERT INTO user_tenant_roles (user_id, tenant_id, role) \
           SELECT id, $2, $3 FROM target \
           ON CONFLICT (user_id, tenant_id) DO UPDATE SET role = EXCLUDED.role \
           RETURNING user_id, role, created_at) \
         SELECT t.id AS user_id, t.email, t.name, u.role, u.created_at AS joined_at \
         FROM target t JOIN upserted u ON u.user_id = t.id",
    )
    .bind(&body.email)
    .bind(admin.tenant_id)
    .bind(&body.role)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_error)?
    .ok_or(StatusCode::NOT_FOUND)?;
    audit(&mut tx, "member_add", &admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(member)))
}

async fn update_role(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(body): Json<UpdateRole>,
) -> Result<StatusCode, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    if admin.user_id == user_id {
        return Err(StatusCode::FORBIDDEN);
    }
    valid_role(&body.role)?;
    let mut tx = state.db.begin().await.map_err(store_error)?;
    let rows =
        sqlx::query("UPDATE user_tenant_roles SET role = $1 WHERE user_id = $2 AND tenant_id = $3")
            .bind(&body.role)
            .bind(user_id)
            .bind(admin.tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(store_error)?
            .rows_affected();
    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(&mut tx, "member_role_update", &admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_user_sessions(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Result<(), StatusCode> {
    sqlx::query(
        "UPDATE user_sessions SET revoked_at = now() \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(store_error)
}

async fn remove_member(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    let mut tx = state.db.begin().await.map_err(store_error)?;
    // Lock the tenant's admin rows so two concurrent removals cannot drop the last admin.
    let admins: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM user_tenant_roles WHERE tenant_id = $1 AND role = 'tenant_admin' FOR UPDATE",
    )
    .bind(admin.tenant_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(store_error)?;
    if admins.contains(&user_id) && admins.len() <= 1 {
        return Err(StatusCode::FORBIDDEN);
    }
    revoke_user_sessions(&mut tx, user_id, admin.tenant_id).await?;
    let rows = sqlx::query("DELETE FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2")
        .bind(user_id)
        .bind(admin.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(store_error)?
        .rows_affected();
    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    audit(&mut tx, "member_remove", &admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_sessions(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let admin = tenant_admin(&state, &headers).await?;
    let mut tx = state.db.begin().await.map_err(store_error)?;
    revoke_user_sessions(&mut tx, user_id, admin.tenant_id).await?;
    audit(&mut tx, "member_sessions_revoke", &admin).await?;
    tx.commit().await.map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}
