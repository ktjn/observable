// Ingestion token management endpoints.
//
// Tokens (`api_keys`) are the authoritative source for environments and tenants.
// An environment exists because a token was issued for it (per ADR-028).
// The plaintext token is generated here, returned once, and never stored — only
// its SHA-256 hash is persisted.
//
// GET    /v1/tokens                  — list all active + revoked tokens for the caller's tenant
// POST   /v1/tokens                  — create a new token; returns plaintext once
// DELETE /v1/tokens/:id              — soft-revoke (sets revoked_at)
// POST   /v1/tokens/:id/renew        — rotate key (new hash, clears revoked_at); returns new plaintext once
// POST   /v1/tokens/:id/restore      — un-revoke (clears revoked_at, preserves existing hash)
// DELETE /v1/tokens/:id/permanent    — hard-delete the row

use crate::AdminServiceAppState;
use crate::middleware::auth::{TenantContext, require_admin};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TokenRecord {
    pub id: Uuid,
    pub name: String,
    pub tenant_name: String,
    pub environment: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked: bool,
}

#[derive(Serialize)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenRecord>,
}

#[derive(Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub environment: String,
}

#[derive(Serialize)]
pub struct CreateTokenResponse {
    #[serde(flatten)]
    pub token: TokenRecord,
    /// Plaintext token value — shown once, never stored.
    pub plaintext: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /v1/tokens — list all tokens for the caller's tenant.
pub async fn list_tokens(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<TokenListResponse>, StatusCode> {
    require_admin(&ctx)?;
    let rows = sqlx::query!(
        r#"
        SELECT
            ak.id,
            ak.name,
            t.name AS tenant_name,
            ak.environment,
            ak.created_at,
            ak.revoked_at
        FROM api_keys ak
        JOIN tenants t ON t.id = ak.tenant_id
        WHERE ak.tenant_id = $1
        ORDER BY ak.created_at DESC
        "#,
        ctx.tenant_id
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to list tokens");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let tokens = rows
        .into_iter()
        .map(|r| TokenRecord {
            id: r.id,
            name: r.name,
            tenant_name: r.tenant_name,
            environment: r.environment,
            created_at: r.created_at,
            revoked: r.revoked_at.is_some(),
        })
        .collect();

    Ok(Json(TokenListResponse { tokens }))
}

/// POST /v1/tokens — create a new ingestion token.
/// Generates a secure random 32-byte hex token, stores its SHA-256 hash,
/// and returns the plaintext once in the response.
pub async fn create_token(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, StatusCode> {
    require_admin(&ctx)?;
    if body.name.trim().is_empty() || body.environment.trim().is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Generate 32 cryptographically random bytes → 64-char hex string.
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let plaintext: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    // SHA-256 hash for storage — same scheme as seeded dev tokens.
    let hash: String = Sha256::digest(plaintext.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let row = sqlx::query!(
        r#"
        INSERT INTO api_keys (tenant_id, key_hash, name, environment, role)
        VALUES ($1, $2, $3, $4, 'member')
        RETURNING id, name, environment, created_at
        "#,
        ctx.tenant_id,
        hash,
        body.name.trim(),
        body.environment.trim(),
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to create token");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Fetch tenant name for the response.
    let tenant_name: String =
        sqlx::query_scalar!("SELECT name FROM tenants WHERE id = $1", ctx.tenant_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or_else(|_| ctx.tenant_id.to_string());

    Ok(Json(CreateTokenResponse {
        token: TokenRecord {
            id: row.id,
            name: row.name,
            tenant_name,
            environment: row.environment,
            created_at: row.created_at,
            revoked: false,
        },
        plaintext,
    }))
}

/// DELETE /v1/tokens/:id — soft-revoke a token by setting revoked_at.
pub async fn revoke_token(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    let result = sqlx::query!(
        r#"
        UPDATE api_keys
        SET revoked_at = now()
        WHERE id = $1
          AND tenant_id = $2
          AND revoked_at IS NULL
        "#,
        id,
        ctx.tenant_id,
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to revoke token");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /v1/tokens/:id/renew — rotate the token: generate a new plaintext,
/// update the stored hash, and clear revoked_at if set.
/// Returns the new plaintext once; the old value is immediately invalidated.
pub async fn renew_token(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<CreateTokenResponse>, StatusCode> {
    require_admin(&ctx)?;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let plaintext: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let hash: String = Sha256::digest(plaintext.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let row = sqlx::query!(
        r#"
        UPDATE api_keys
        SET key_hash = $3, revoked_at = NULL
        WHERE id = $1
          AND tenant_id = $2
        RETURNING id, name, environment, created_at
        "#,
        id,
        ctx.tenant_id,
        hash,
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to renew token");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let tenant_name: String =
        sqlx::query_scalar!("SELECT name FROM tenants WHERE id = $1", ctx.tenant_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or_else(|_| ctx.tenant_id.to_string());

    Ok(Json(CreateTokenResponse {
        token: TokenRecord {
            id: row.id,
            name: row.name,
            tenant_name,
            environment: row.environment,
            created_at: row.created_at,
            revoked: false,
        },
        plaintext,
    }))
}

/// POST /v1/tokens/:id/restore — un-revoke a token by clearing revoked_at.
/// The existing key hash (and therefore the original token value) is preserved.
pub async fn restore_token(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    let result = sqlx::query!(
        r#"
        UPDATE api_keys
        SET revoked_at = NULL
        WHERE id = $1
          AND tenant_id = $2
          AND revoked_at IS NOT NULL
        "#,
        id,
        ctx.tenant_id,
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to restore token");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /v1/tokens/:id/permanent — hard-delete a token row.
/// Only permitted for revoked tokens; active tokens must be revoked first.
pub async fn delete_token(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    let result = sqlx::query!(
        r#"
        DELETE FROM api_keys
        WHERE id = $1
          AND tenant_id = $2
          AND revoked_at IS NOT NULL
        "#,
        id,
        ctx.tenant_id,
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to delete token");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}
