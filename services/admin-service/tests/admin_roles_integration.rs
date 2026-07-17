use std::sync::Arc;

use admin_service::middleware::auth::{TenantContext, require_tenant};
use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn app(db: PgPool, auth_service_url: String) -> Router {
    // We use the real middleware but mock auth-service and DB
    Router::new()
        .route(
            "/v1/admin/members",
            get(|Extension(_ctx): Extension<TenantContext>| async move { StatusCode::OK }),
        )
        .route(
            "/v1/admin/members",
            post(|Extension(_ctx): Extension<TenantContext>| async move { StatusCode::OK }),
        )
        .route(
            "/v1/tokens",
            get(|Extension(_ctx): Extension<TenantContext>| async move { StatusCode::OK }),
        )
        .route(
            "/v1/tokens",
            post(|Extension(_ctx): Extension<TenantContext>| async move { StatusCode::OK }),
        )
        // Apply the same role check as the real handlers
        .layer(middleware::from_fn(
            |Extension(ctx): Extension<TenantContext>,
             req: Request<Body>,
             next: middleware::Next| async move {
                admin_service::middleware::auth::require_admin(&ctx)?;
                Ok::<_, StatusCode>(next.run(req).await)
            },
        ))
        .layer(middleware::from_fn(require_tenant))
        .layer(Extension(db))
        .layer(Extension(Arc::new(auth_service_url)))
}

#[tokio::test]
async fn member_role_is_denied_admin_access() {
    let mock_server = MockServer::start().await;
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Mock auth-service to return a "member" role
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant_id.to_string(),
            "role": "member",
            "environment": "production"
        })))
        .mount(&mock_server)
        .await;

    // Use a lazy pool that won't actually be used because require_tenant won't reach DB for same-tenant
    let db = PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:1/observable").unwrap();
    let app = app(db, mock_server.uri());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/members")
                .header("cookie", "session=valid-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_role_is_allowed_admin_access() {
    let mock_server = MockServer::start().await;
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // Mock auth-service to return a "tenant_admin" role
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant_id.to_string(),
            "role": "tenant_admin",
            "environment": "production"
        })))
        .mount(&mock_server)
        .await;

    let db = PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:1/observable").unwrap();
    let app = app(db, mock_server.uri());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/members")
                .header("cookie", "session=valid-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
