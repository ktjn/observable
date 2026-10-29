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
///   1. `Authorization: Bearer <api-key>` + `X-Tenant-ID` — forwarded to
///      auth-service `POST /internal/validate` (existing SDK / CLI path).
///   2. `Cookie: session=<jwt>` — forwarded to auth-service
///      `POST /internal/validate-session` (browser / UI path after OIDC login).
///
/// Both paths now route through auth-service so that every credential check
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
    let tenant_id_res = observable_auth::extract_tenant_id_header(req.headers());
    let tenant_id_hdr = tenant_id_res.as_ref().ok().cloned();
    // --- End of synchronous extraction ---

    // Path 1: API key — bearer token + X-Tenant-ID header.
    if let (Some(token), Some(tenant_id)) = (bearer.as_ref(), tenant_id_hdr) {
        let auth_url = auth_service_url.as_deref().ok_or_else(|| {
            tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        match verify_credentials(&http_client, token.clone(), tenant_id, auth_url).await {
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

    let ctx = validate_session(&http_client, &auth_url, &session_token, tenant_id_hdr).await?;
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

async fn validate_session(
    client: &reqwest::Client,
    auth_url: &str,
    token: &str,
    requested_tenant_id: Option<Uuid>,
) -> Result<TenantContext, StatusCode> {
    let session =
        observable_auth::verify_session_with_tenant(client, auth_url, token, requested_tenant_id)
            .await
            .map_err(StatusCode::from)?;

    Ok(TenantContext {
        tenant_id: session.tenant_id,
        user_id: Some(session.user_id),
        role: session.role,
    })
}

async fn verify_credentials(
    client: &reqwest::Client,
    token: String,
    tenant_id: Uuid,
    auth_url: &str,
) -> Result<TenantContext, StatusCode> {
    let ctx = observable_auth::verify_api_key(client, auth_url, &token)
        .await
        .map_err(StatusCode::from)?;

    // Verify the key belongs to the requested tenant.
    if ctx.tenant_id != tenant_id {
        tracing::warn!(reason = "tenant_mismatch", "auth rejected");
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(TenantContext {
        tenant_id,
        user_id: None,
        role: ctx.role,
    })
}
