use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub role: String,
}

/// Row returned from the api_keys lookup.
#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    tenant_id: Uuid,
    role: String,
    revoked_at: Option<DateTime<Utc>>,
}

/// Middleware that verifies `Authorization: Bearer <token>` + `X-Tenant-ID`
/// against the `api_keys` table and inserts `TenantContext` into request
/// extensions.
///
/// The `PgPool` is read from request extensions. Callers must place a
/// `.layer(axum::Extension(db))` BELOW this middleware in the tower stack.
///
/// # !Send note
/// `axum::body::Body` is `!Sync`, which makes `&Request<Body>` `!Send`.
/// All header extraction MUST happen synchronously before the first `.await`
/// so that no reference to `req` is held across an await point.
pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    // --- Synchronous extraction (no await) ---
    let db = req
        .extensions()
        .get::<PgPool>()
        .cloned()
        .ok_or_else(|| {
            tracing::error!(
                "PgPool not found in request extensions — misconfigured middleware stack"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let token = extract_bearer_token(req.headers())?;
    let tenant_id = extract_tenant_id(req.headers())?;
    // --- End of synchronous extraction ---

    // Now we can safely await: no reference to `req` remains.
    let ctx = verify_credentials(token, tenant_id, &db).await?;
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Extract the raw bearer token value from the Authorization header.
fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<String, StatusCode> {
    headers
        .get("Authorization")
        .ok_or_else(|| {
            tracing::warn!(reason = "missing_authorization_header", "auth rejected");
            StatusCode::UNAUTHORIZED
        })?
        .to_str()
        .ok()
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_owned())
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
    let row: ApiKeyRow = sqlx::query_as(
        "SELECT tenant_id, role, revoked_at FROM api_keys WHERE key_hash = $1",
    )
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
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
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
        assert_eq!(result, "dev-api-key-0000");
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
