use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
    pub role: String,
}

/// Middleware that accepts either:
///   1. `Authorization: Bearer <api-key>` — forwarded to
///      auth-service `POST /internal/validate` (SDK / CLI path).
///   2. `Cookie: session=<jwt>` (or `Authorization: Bearer <session-jwt>`) —
///      forwarded to auth-service `POST /internal/validate-session` (browser / UI path).
///
/// Both paths route through auth-service so that every credential check
/// gains an entry in auth-service's `credential_audit_log` table.
///
/// Extensions required in the tower stack (via `.layer(axum::Extension(...))`):
///   - `Arc<String>`     — the auth-service base URL (for API-key + session validation)
pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    // --- Synchronous extraction (no await) ---
    let auth_service_url = req.extensions().get::<Arc<String>>().cloned();
    let http_client = req
        .extensions()
        .get::<reqwest::Client>()
        .cloned()
        .unwrap_or_default();

    let bearer = observable_auth::extract_bearer_token(req.headers()).map_err(StatusCode::from)?;
    let session_cookie = observable_auth::extract_session_cookie(req.headers());
    // --- End of synchronous extraction ---

    // Path 1: API key — bearer token (when no session cookie is present).
    if let (Some(token), None) = (bearer.as_ref(), session_cookie.as_ref()) {
        let auth_url = auth_service_url.as_deref().ok_or_else(|| {
            tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        match verify_api_key_only(&http_client, token, auth_url).await {
            Ok(ctx) => {
                req.extensions_mut().insert(ctx);
                return Ok(next.run(req).await);
            }
            // key_not_found → fall through to session path (bearer may be a session JWT)
            Err(StatusCode::UNAUTHORIZED) => {}
            Err(e) => return Err(e),
        }
    }

    // Path 2: OIDC session cookie (or bearer session JWT).
    let session_token = session_cookie.or(bearer).ok_or_else(|| {
        tracing::warn!(reason = "no_credentials", "auth rejected");
        StatusCode::UNAUTHORIZED
    })?;

    let auth_url = auth_service_url.ok_or_else(|| {
        tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let ctx = validate_session(&http_client, &auth_url, &session_token).await?;
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

async fn validate_session(
    client: &reqwest::Client,
    auth_url: &str,
    token: &str,
) -> Result<TenantContext, StatusCode> {
    let session = observable_auth::verify_session(client, auth_url, token)
        .await
        .map_err(StatusCode::from)?;

    Ok(TenantContext {
        tenant_id: session.tenant_id,
        user_id: Some(session.user_id),
        role: session.role,
    })
}

async fn verify_api_key_only(
    client: &reqwest::Client,
    token: &str,
    auth_url: &str,
) -> Result<TenantContext, StatusCode> {
    let ctx = observable_auth::verify_api_key(client, auth_url, token)
        .await
        .map_err(StatusCode::from)?;

    Ok(TenantContext {
        tenant_id: ctx.tenant_id,
        user_id: None,
        role: ctx.role,
    })
}
