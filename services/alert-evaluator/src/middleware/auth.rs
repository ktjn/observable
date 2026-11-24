use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub user_id: Option<Uuid>,
    pub role: String,
}

/// The validated API key behind a request, set by `require_tenant` for API-key callers only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiKeyIdentity(pub Uuid);

/// Shared secret for `require_internal_service`, distinct from the
/// `Arc<String>` extension that already carries `auth_service_url` (axum
/// extensions are keyed by type, so a second bare `Arc<String>` would
/// collide with it).
#[derive(Clone)]
pub struct InternalServiceToken(pub String);

/// Middleware that accepts either:
///   1. `Authorization: Bearer <api-key>` — forwarded to
///      auth-service `POST /internal/validate`, resolving tenant from the key.
///   2. `Cookie: session=<jwt>` (or `Authorization: Bearer <session-jwt>`) —
///      forwarded to auth-service `POST /internal/validate-session` (browser / UI path).
///
/// Both paths route through auth-service so that every credential check
/// gains an entry in auth-service's `credential_audit_log` table.
///
/// Extensions required in the tower stack (via `.layer(axum::Extension(...))`):
///   - `Arc<String>`     — the auth-service base URL (for API-key + session validation)
pub fn require_admin(ctx: &TenantContext) -> Result<(), StatusCode> {
    if ctx.role != "tenant_admin" {
        Err(StatusCode::FORBIDDEN)
    } else {
        Ok(())
    }
}

/// Gate for `/internal/*` service-to-service routes (e.g. query-api's
/// cross-owner correlation reads, Phase 4's "remove cross-owner SQL"
/// follow-on, docs/component-decomposition.md). Checks `X-Internal-Token`
/// against a shared secret rather than going through auth-service -- there is
/// no end-user credential on these calls, only a trusted caller within the
/// cluster. Callers pass `tenant_id` explicitly as a request parameter.
///
/// Requires an `InternalServiceToken` extension in the tower stack (via
/// `.layer(axum::Extension(...))`).
pub async fn require_internal_service(req: Request, next: Next) -> Result<Response, StatusCode> {
    let expected = req
        .extensions()
        .get::<InternalServiceToken>()
        .cloned()
        .ok_or_else(|| {
            tracing::error!(
                "InternalServiceToken extension missing — misconfigured middleware stack"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    observable_auth::verify_internal_token(req.headers(), &expected.0).map_err(StatusCode::from)?;

    Ok(next.run(req).await)
}

pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let auth_service_url = req.extensions().get::<Arc<String>>().cloned();
    let http_client = req
        .extensions()
        .get::<reqwest::Client>()
        .cloned()
        .unwrap_or_default();

    let bearer = observable_auth::extract_bearer_token(req.headers()).map_err(StatusCode::from)?;
    let session_cookie = observable_auth::extract_session_cookie(req.headers());

    if let (Some(token), None) = (bearer.as_ref(), session_cookie.as_ref()) {
        // API key present, no session cookie. Resolve tenant directly from the key.
        let auth_url = auth_service_url.as_deref().ok_or_else(|| {
            tracing::error!("auth_service_url extension missing — misconfigured middleware stack");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        match verify_api_key_only(&http_client, token.clone(), auth_url).await {
            Ok((ctx, key_id)) => {
                req.extensions_mut().insert(ctx);
                if let Some(id) = key_id {
                    req.extensions_mut().insert(ApiKeyIdentity(id));
                }
                return Ok(next.run(req).await);
            }
            Err(StatusCode::UNAUTHORIZED) => {}
            Err(e) => return Err(e),
        }
    }

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

/// Resolve tenant directly from the API key.
async fn verify_api_key_only(
    client: &reqwest::Client,
    token: String,
    auth_url: &str,
) -> Result<(TenantContext, Option<Uuid>), StatusCode> {
    let ctx = observable_auth::verify_api_key(client, auth_url, &token)
        .await
        .map_err(StatusCode::from)?;

    Ok((
        TenantContext {
            tenant_id: ctx.tenant_id,
            user_id: None,
            role: ctx.role,
        },
        ctx.key_id,
    ))
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

    fn app(auth_service_url: String) -> Router {
        Router::new()
            .route(
                "/",
                get(|Extension(ctx): Extension<TenantContext>| async move {
                    ctx.tenant_id.to_string()
                }),
            )
            .layer(middleware::from_fn(require_tenant))
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
    async fn api_key_alone_without_tenant_header_resolves_tenant_from_key() {
        let mock_server = MockServer::start().await;
        let tenant_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tenant_id": tenant_id,
                "role": "member",
                "environment": "production"
            })))
            .mount(&mock_server)
            .await;

        let response = app(mock_server.uri())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer valid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_text(response).await, tenant_id.to_string());
    }

    #[tokio::test]
    async fn api_key_request_carries_the_validated_key_id() {
        let mock_server = MockServer::start().await;
        let key_id = Uuid::new_v4();
        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tenant_id": Uuid::new_v4(),
                "role": "member",
                "environment": "production",
                "key_id": key_id
            })))
            .mount(&mock_server)
            .await;
        let app = Router::new()
            .route(
                "/",
                get(|key: Option<Extension<ApiKeyIdentity>>| async move {
                    key.map(|Extension(ApiKeyIdentity(k))| k.to_string())
                        .unwrap_or_default()
                }),
            )
            .layer(middleware::from_fn(require_tenant))
            .layer(Extension(Arc::new(mock_server.uri())));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer valid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_text(response).await, key_id.to_string());
    }

    #[tokio::test]
    async fn invalid_api_key_alone_is_rejected() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let response = app(mock_server.uri())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer bad-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
        let response = app("http://127.0.0.1:1".to_string())
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    fn internal_app(expected_token: &str) -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(require_internal_service))
            .layer(Extension(InternalServiceToken(expected_token.to_string())))
    }

    #[tokio::test]
    async fn internal_service_with_matching_token_is_accepted() {
        let response = internal_app("shared-secret")
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("x-internal-token", "shared-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn internal_service_with_wrong_token_is_rejected() {
        let response = internal_app("shared-secret")
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("x-internal-token", "wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn internal_service_with_no_token_is_rejected() {
        let response = internal_app("shared-secret")
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
