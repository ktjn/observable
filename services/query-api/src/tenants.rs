use crate::traces::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct TenantListResponse {
    pub tenants: Vec<TenantRecord>,
}

#[derive(Serialize)]
pub struct EnvironmentRecord {
    pub environment: String,
}

#[derive(Serialize)]
pub struct EnvironmentListResponse {
    pub environments: Vec<EnvironmentRecord>,
}

#[derive(Deserialize)]
struct ValidateSessionResponse {
    user_id: String,
    #[allow(dead_code)]
    tenant_id: String,
}

/// GET /v1/tenants
/// Without a session cookie: returns all tenants (backwards-compatible for API-key callers).
/// With a session cookie: filters to only the tenants the authenticated user belongs to.
pub async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TenantListResponse>, StatusCode> {
    let session_token = extract_session_cookie(&headers).or_else(|| extract_bearer_token(&headers));

    if let Some(session_token) = session_token {
        let user_id = validate_session_with_auth_service(&state.auth_service_url, &session_token)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let rows = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT t.id, t.name
            FROM tenants t
            JOIN user_tenant_roles utr ON utr.tenant_id = t.id
            WHERE utr.user_id = $1
            ORDER BY t.name ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list user tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        return Ok(Json(TenantListResponse {
            tenants: rows
                .into_iter()
                .map(|(id, name)| TenantRecord { id, name })
                .collect(),
        }));
    }

    // No session cookie — legacy path: return all tenants (API key callers).
    let rows = sqlx::query!(r#"SELECT id, name FROM tenants ORDER BY name ASC"#)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TenantListResponse {
        tenants: rows
            .into_iter()
            .map(|r| TenantRecord {
                id: r.id,
                name: r.name,
            })
            .collect(),
    }))
}

/// GET /v1/tenants/:id/environments
pub async fn list_tenant_environments(
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<EnvironmentListResponse>, StatusCode> {
    // If a session is present, verify the user has access to the requested tenant.
    let session_token = extract_session_cookie(&headers).or_else(|| extract_bearer_token(&headers));
    if let Some(session_token) = session_token {
        let user_id = validate_session_with_auth_service(&state.auth_service_url, &session_token)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let has_access = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if has_access == 0 {
            tracing::warn!(%user_id, %tenant_id, "User attempted to access unauthorized tenant environments");
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let rows = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT environment
        FROM api_keys
        WHERE tenant_id = $1
          AND revoked_at IS NULL
          AND environment != ''
        ORDER BY environment ASC
        "#,
        tenant_id,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to list tenant environments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(EnvironmentListResponse {
        environments: rows
            .into_iter()
            .map(|e| EnvironmentRecord { environment: e })
            .collect(),
    }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').map(str::trim).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k.trim() == "session" {
            Some(v.trim().to_owned())
        } else {
            None
        }
    })
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_owned())
}

async fn validate_session_with_auth_service(
    auth_service_url: &str,
    session_token: &str,
) -> anyhow::Result<Uuid> {
    let resp = reqwest::Client::new()
        .post(format!("{auth_service_url}/internal/validate-session"))
        .json(&serde_json::json!({ "session_token": session_token }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("session validation failed: {}", resp.status());
    }

    let body: ValidateSessionResponse = resp.json().await?;
    Ok(Uuid::parse_str(&body.user_id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, header};

    #[test]
    fn extract_session_cookie_returns_none_when_absent() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert!(extract_session_cookie(req.headers()).is_none());
    }

    #[test]
    fn extract_session_cookie_returns_value() {
        let req = Request::builder()
            .header(header::COOKIE, "session=tok123; other=x")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            extract_session_cookie(req.headers()),
            Some("tok123".to_string())
        );
    }
}
