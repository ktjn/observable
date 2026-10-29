use std::sync::Arc;

use admin_service::middleware::auth::{TenantContext, require_tenant};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn app(auth_service_url: String) -> Router {
    Router::new()
        .route(
            "/",
            get(|Extension(ctx): Extension<TenantContext>| async move {
                format!("{}:{}", ctx.tenant_id, ctx.role)
            }),
        )
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(Arc::new(auth_service_url)))
}

#[tokio::test]
async fn session_cannot_switch_to_unrelated_tenant() {
    let mock_server = MockServer::start().await;
    let requested_tenant_id = Uuid::new_v4();

    // When requested tenant is unauthorized, auth-service returns 403 Forbidden
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock_server)
        .await;

    let response = app(mock_server.uri())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", "session=valid-session")
                .header("x-tenant-id", requested_tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn session_switch_rebinds_role_from_requested_tenant() {
    let mock_server = MockServer::start().await;
    let user_id = Uuid::new_v4();
    let requested_tenant_id = Uuid::new_v4();

    // When user has access to requested tenant, auth-service returns 200 with that tenant and role
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": requested_tenant_id.to_string(),
            "role": "member",
            "environment": "production"
        })))
        .mount(&mock_server)
        .await;

    let response = app(mock_server.uri())
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", "session=valid-session")
                .header("x-tenant-id", requested_tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        body.as_ref(),
        format!("{requested_tenant_id}:member").as_bytes()
    );
}

#[tokio::test]
async fn session_switch_enforces_target_role() {
    let mock_server = MockServer::start().await;
    let user_id = Uuid::new_v4();
    let requested_tenant_id = Uuid::new_v4();

    // User is only a member in the requested tenant
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": requested_tenant_id.to_string(),
            "role": "member",
            "environment": "production"
        })))
        .mount(&mock_server)
        .await;

    // A mock handler that requires admin role.
    let app = Router::new()
        .route(
            "/",
            get(|Extension(ctx): Extension<TenantContext>| async move {
                admin_service::middleware::auth::require_admin(&ctx)?;
                Ok::<_, StatusCode>(ctx.role)
            }),
        )
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(Arc::new(mock_server.uri())));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("cookie", "session=valid-session")
                .header("x-tenant-id", requested_tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
