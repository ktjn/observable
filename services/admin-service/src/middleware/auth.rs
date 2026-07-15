use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
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
///   - `PgPool`          — for the cross-tenant-switch lookup on the session path
///   - `Arc<String>`     — the auth-service base URL (for API-key + session validation)
pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let db = req.extensions().get::<PgPool>().cloned().ok_or_else(|| {
        tracing::error!("PgPool not found in request extensions — misconfigured middleware stack");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let auth_service_url = req.extensions().get::<Arc<String>>().cloned();

    let bearer = observable_auth::extract_bearer_token(req.headers()).map_err(StatusCode::from)?;
    let session_cookie = observable_auth::extract_session_cookie(req.headers());
    let tenant_id_res = observable_auth::extract_tenant_id_header(req.headers());
    let tenant_id_hdr = tenant_id_res.as_ref().ok().cloned();

    if let (Some(token), Some(tenant_id)) = (bearer.as_ref(), tenant_id_hdr) {
        let auth_url = auth_service_url.as_deref().ok_or_else(|| {
            tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        match verify_credentials(token.clone(), tenant_id, auth_url).await {
            Ok(ctx) => {
                req.extensions_mut().insert(ctx);
                return Ok(next.run(req).await);
            }
            Err(StatusCode::UNAUTHORIZED) => {}
            Err(e) => return Err(e),
        }
    } else if let Err(observable_auth::AuthError::BadRequest) = tenant_id_res {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session_token = session_cookie.or(bearer).ok_or_else(|| {
        tracing::warn!(reason = "no_credentials", "auth rejected");
        StatusCode::UNAUTHORIZED
    })?;

    let auth_url = auth_service_url.ok_or_else(|| {
        tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let ctx = validate_session(&auth_url, &session_token).await?;

    if let Some(requested_tenant_id) = tenant_id_hdr
        && requested_tenant_id != ctx.tenant_id
    {
        let user_id = ctx.user_id.ok_or_else(|| {
            tracing::error!("session context missing user_id for cross-tenant check");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let requested_role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(user_id)
        .bind(requested_tenant_id)
        .fetch_optional(&db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "db error checking cross-tenant access");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let Some(role) = requested_role else {
            tracing::warn!(
                %user_id,
                session_tenant = %ctx.tenant_id,
                requested_tenant = %requested_tenant_id,
                "user does not have access to requested tenant"
            );
            return Err(StatusCode::FORBIDDEN);
        };

        let ctx = TenantContext {
            tenant_id: requested_tenant_id,
            user_id: Some(user_id),
            role,
        };
        req.extensions_mut().insert(ctx);
        return Ok(next.run(req).await);
    }

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

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

async fn verify_credentials(
    token: String,
    tenant_id: Uuid,
    auth_url: &str,
) -> Result<TenantContext, StatusCode> {
    let ctx = observable_auth::verify_api_key(&reqwest::Client::new(), auth_url, &token)
        .await
        .map_err(StatusCode::from)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Extension, Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    fn test_pool() -> PgPool {
        PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:1/observable").unwrap()
    }

    fn app(auth_service_url: String) -> Router {
        Router::new()
            .route(
                "/",
                get(|Extension(ctx): Extension<TenantContext>| async move {
                    ctx.tenant_id.to_string()
                }),
            )
            .layer(middleware::from_fn(require_tenant))
            .layer(Extension(test_pool()))
            .layer(Extension(Arc::new(auth_service_url)))
    }

    async fn body_text(response: Response) -> String {
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn missing_credentials_are_rejected() {
        let response = app("http://127.0.0.1:1".to_string())
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn malformed_tenant_header_is_bad_request() {
        let response = app("http://127.0.0.1:1".to_string())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer token")
                    .header("x-tenant-id", "not-a-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn api_key_for_requested_tenant_is_accepted() {
        let mock_server = MockServer::start().await;
        let tenant_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tenant_id": tenant_id,
                "role": "admin",
                "environment": "production"
            })))
            .mount(&mock_server)
            .await;

        let response = app(mock_server.uri())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer valid-key")
                    .header("x-tenant-id", tenant_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_text(response).await, tenant_id.to_string());
    }

    #[tokio::test]
    async fn api_key_for_other_tenant_is_forbidden() {
        let mock_server = MockServer::start().await;
        let key_tenant_id = Uuid::new_v4();
        let requested_tenant_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tenant_id": key_tenant_id,
                "role": "admin",
                "environment": "production"
            })))
            .mount(&mock_server)
            .await;

        let response = app(mock_server.uri())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer valid-key")
                    .header("x-tenant-id", requested_tenant_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn session_cookie_is_accepted() {
        let mock_server = MockServer::start().await;
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/internal/validate-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user_id": user_id.to_string(),
                "tenant_id": tenant_id.to_string(),
                "role": "admin"
            })))
            .mount(&mock_server)
            .await;

        let response = app(mock_server.uri())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("cookie", "session=valid-session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_text(response).await, tenant_id.to_string());
    }

    #[tokio::test]
    async fn auth_service_outage_fails_closed() {
        let tenant_id = Uuid::new_v4();
        let response = app("http://127.0.0.1:1".to_string())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer key")
                    .header("x-tenant-id", tenant_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
