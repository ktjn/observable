use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
    pub role: String,
}

/// Row returned from the api_keys lookup.
#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    tenant_id: Uuid,
    role: String,
    revoked_at: Option<DateTime<Utc>>,
}

/// Middleware that accepts either:
///   1. `Authorization: Bearer <api-key>` + `X-Tenant-ID` — verified against
///      the `api_keys` table (existing SDK / CLI path).
///   2. `Cookie: session=<jwt>` — forwarded to auth-service
///      `POST /internal/validate-session` (browser / UI path after OIDC login).
///
/// Extensions required in the tower stack (via `.layer(axum::Extension(...))`):
///   - `PgPool`          — for API-key lookups
///   - `Arc<String>`     — the auth-service base URL (for session validation)
pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    // --- Synchronous extraction (no await) ---
    let db = req.extensions().get::<PgPool>().cloned().ok_or_else(|| {
        tracing::error!("PgPool not found in request extensions — misconfigured middleware stack");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let auth_service_url = req.extensions().get::<Arc<String>>().cloned();

    let bearer = observable_auth::extract_bearer_token(req.headers()).map_err(StatusCode::from)?;
    let session_cookie = observable_auth::extract_session_cookie(req.headers());
    let tenant_id_res = observable_auth::extract_tenant_id_header(req.headers());
    let tenant_id_hdr = tenant_id_res.as_ref().ok().cloned();
    // --- End of synchronous extraction ---

    // Path 1: API key — bearer token + X-Tenant-ID header.
    if let (Some(token), Some(tenant_id)) = (bearer.as_ref(), tenant_id_hdr) {
        match verify_credentials(token.clone(), tenant_id, &db).await {
            Ok(ctx) => {
                req.extensions_mut().insert(ctx);
                return Ok(next.run(req).await);
            }
            // key_not_found → fall through to session path
            Err(StatusCode::UNAUTHORIZED) => {}
            Err(e) => return Err(e),
        }
    } else if let Err(observable_auth::AuthError::BadRequest) = tenant_id_res {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Path 2: OIDC session cookie (UI after login).
    let session_token = session_cookie.or(bearer).ok_or_else(|| {
        tracing::warn!(reason = "no_credentials", "auth rejected");
        StatusCode::UNAUTHORIZED
    })?;

    let auth_url = auth_service_url.ok_or_else(|| {
        tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let ctx = validate_session(&auth_url, &session_token).await?;

    // If X-Tenant-ID is provided and matches the session tenant, proceed as usual.
    // If it differs, check whether the user actually has a role on the requested
    // tenant (multi-tenant users can switch without re-login).
    if let Some(requested_tenant_id) = tenant_id_hdr
        && requested_tenant_id != ctx.tenant_id
    {
        let user_id = ctx.user_id.ok_or_else(|| {
            tracing::error!("session context missing user_id for cross-tenant check");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let has_access = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(user_id)
        .bind(requested_tenant_id)
        .fetch_one(&db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "db error checking cross-tenant access");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if has_access == 0 {
            tracing::warn!(
                %user_id,
                session_tenant = %ctx.tenant_id,
                requested_tenant = %requested_tenant_id,
                "user does not have access to requested tenant"
            );
            return Err(StatusCode::FORBIDDEN);
        }

        // User has access — override the tenant context to the requested tenant.
        let ctx = TenantContext {
            tenant_id: requested_tenant_id,
            user_id: Some(user_id),
            role: ctx.role,
        };
        req.extensions_mut().insert(ctx);
        return Ok(next.run(req).await);
    }

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Call auth-service to validate a session JWT and return a TenantContext.
async fn validate_session(auth_url: &str, token: &str) -> Result<TenantContext, StatusCode> {
    let session = observable_auth::verify_session(&reqwest::Client::new(), auth_url, token)
        .await
        .map_err(StatusCode::from)?;

    Ok(TenantContext {
        tenant_id: session.tenant_id,
        user_id: Some(session.user_id),
        role: session.role,
    })
}

/// Verify the bearer token against the `api_keys` table and check tenant
/// ownership. Returns a `TenantContext` on success.
async fn verify_credentials(
    token: String,
    tenant_id: Uuid,
    db: &PgPool,
) -> Result<TenantContext, StatusCode> {
    // Compute SHA-256 hex of the raw token — never log the raw value.
    let key_hash = {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    };

    // Look up the api_key; reject if not found or revoked.
    let row: ApiKeyRow =
        sqlx::query_as("SELECT tenant_id, role, revoked_at FROM api_keys WHERE key_hash = $1")
            .bind(&key_hash)
            .fetch_optional(db)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "database error during auth");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or_else(|| {
                tracing::warn!(reason = "key_not_found", "auth rejected");
                StatusCode::UNAUTHORIZED
            })?;

    if row.revoked_at.is_some() {
        tracing::warn!(reason = "key_revoked", "auth rejected");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Verify the key belongs to the requested tenant.
    if row.tenant_id != tenant_id {
        tracing::warn!(reason = "tenant_mismatch", "auth rejected");
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(TenantContext {
        tenant_id,
        user_id: None,
        role: row.role,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn sha256_hex_of_dev_key_matches_seed() {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"dev-api-key-0000");
        let hex = format!("{:x}", h.finalize());
        assert_eq!(
            hex,
            "e18f3d8fb3eb31a042e4a55877e0276960294d0980b8076efaac30dabdbbf67b"
        );
    }
}
