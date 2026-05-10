use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    #[allow(dead_code)]
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

    let bearer = extract_bearer_token(req.headers())?;
    let session_cookie = extract_session_cookie(req.headers());
    let tenant_id_res = extract_tenant_id(req.headers());
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
    } else if let Err(StatusCode::BAD_REQUEST) = tenant_id_res {
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

    // If X-Tenant-ID is also provided, it MUST match the session's tenant.
    if let Some(requested_tenant_id) = tenant_id_hdr {
        if requested_tenant_id != ctx.tenant_id {
            tracing::warn!(
                session_tenant = %ctx.tenant_id,
                requested_tenant = %requested_tenant_id,
                "tenant mismatch between session and X-Tenant-ID header"
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Call auth-service to validate a session JWT and return a TenantContext.
async fn validate_session(auth_url: &str, token: &str) -> Result<TenantContext, StatusCode> {
    #[derive(serde::Serialize)]
    struct Req<'a> {
        session_token: &'a str,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        tenant_id: String,
        role: String,
    }

    let resp = reqwest::Client::new()
        .post(format!("{auth_url}/internal/validate-session"))
        .json(&Req {
            session_token: token,
        })
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "auth-service unreachable");
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), reason = "session_invalid", "auth rejected");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let body: Resp = resp
        .json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tenant_id = body
        .tenant_id
        .parse::<Uuid>()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(TenantContext {
        tenant_id,
        role: body.role,
    })
}

/// Extract the `session` cookie value from the Cookie header.
fn extract_session_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    cookie.split(';').map(str::trim).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k.trim() == "session" {
            Some(v.trim().to_owned())
        } else {
            None
        }
    })
}

/// Extract the raw bearer token value from the Authorization header.
fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<Option<String>, StatusCode> {
    let Some(value) = headers.get("Authorization") else {
        return Ok(None);
    };

    value
        .to_str()
        .ok()
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| Some(s.to_owned()))
        .ok_or_else(|| {
            tracing::warn!(reason = "malformed_authorization_header", "auth rejected");
            StatusCode::UNAUTHORIZED
        })
}

/// Extract and parse the X-Tenant-ID header value as a UUID.
fn extract_tenant_id(headers: &axum::http::HeaderMap) -> Result<Uuid, StatusCode> {
    headers
        .get("X-Tenant-ID")
        .ok_or_else(|| {
            tracing::warn!(reason = "missing_tenant_id_header", "auth rejected");
            StatusCode::UNAUTHORIZED
        })?
        .to_str()
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            tracing::warn!(reason = "malformed_tenant_id_header", "auth rejected");
            StatusCode::BAD_REQUEST
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
        role: row.role,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::HeaderMap};

    fn headers_from(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    fn req_with_headers(headers: &[(&str, &str)]) -> Request {
        let mut builder = Request::builder();
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn missing_authorization_header_is_rejected() {
        let headers = headers_from(&[("X-Tenant-ID", "00000000-0000-0000-0000-000000000001")]);
        let result = extract_bearer_token(&headers);
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn missing_tenant_id_is_rejected() {
        let headers = headers_from(&[("Authorization", "Bearer some-token")]);
        let result = extract_tenant_id(&headers);
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn malformed_tenant_id_is_bad_request() {
        let headers = headers_from(&[
            ("Authorization", "Bearer some-token"),
            ("X-Tenant-ID", "not-a-uuid"),
        ]);
        let result = extract_tenant_id(&headers);
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn bearer_prefix_is_stripped_correctly() {
        let headers = headers_from(&[("Authorization", "Bearer dev-api-key-0000")]);
        let result = extract_bearer_token(&headers).unwrap();
        assert_eq!(result, Some("dev-api-key-0000".into()));
    }

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

    #[test]
    fn missing_tenant_header_is_rejected_via_req() {
        let req = req_with_headers(&[("Authorization", "Bearer some-token")]);
        let err = extract_tenant_id(req.headers()).unwrap_err();
        assert_eq!(err, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn invalid_tenant_header_is_bad_request_via_req() {
        let req = req_with_headers(&[
            ("Authorization", "Bearer some-token"),
            ("X-Tenant-ID", "not-a-uuid"),
        ]);
        let err = extract_tenant_id(req.headers()).unwrap_err();
        assert_eq!(err, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn valid_tenant_header_parses_via_req() {
        let tenant_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let req = req_with_headers(&[
            ("Authorization", "Bearer dev-api-key-0000"),
            ("X-Tenant-ID", "00000000-0000-0000-0000-000000000001"),
        ]);
        let result = extract_tenant_id(req.headers()).unwrap();
        assert_eq!(result, tenant_id);
    }
}
