use auth_service::{
    observability,
    oidc::{self, OidcConfig, OidcState},
    session::sign_session_jwt,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt as _;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SESSION_SECRET: &str = "dev-session-secret-change-in-prod!!";

fn test_state(db: PgPool) -> OidcState {
    OidcState {
        db,
        config: OidcConfig {
            issuer: "http://localhost:8082".into(),
            api_base: "http://localhost:8082".into(),
            client_id: "dev-client-id".into(),
            redirect_uri: "http://localhost:5173/auth/callback".into(),
            session_secret: SESSION_SECRET.into(),
            dev_mode: true,
        },
        http_client: reqwest::Client::new(),
        metrics: std::sync::Arc::new(observability::AuthServiceMetrics::new()),
    }
}

fn test_app(state: OidcState) -> Router {
    Router::new()
        .route(
            "/internal/validate-session",
            axum::routing::post(oidc::validate_session_handler),
        )
        .with_state(state)
}

async fn start_pool() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

async fn seed_user_and_tenant(pool: &PgPool, user_id: Uuid, tenant_id: Uuid) {
    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, 'sub-validate-session', 'validate-session@example.com')")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("user inserted");

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Validate Session Tenant')")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("tenant inserted");
}

#[tokio::test]
async fn validate_session_rejects_session_with_no_active_membership() {
    let (pool, _container) = start_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    seed_user_and_tenant(&pool, user_id, tenant_id).await;

    // Session row is valid (not expired/revoked), but there is deliberately no
    // corresponding user_tenant_roles row: the user's membership was revoked
    // after the session JWT was issued.
    let session_id = oidc::create_session(&pool, user_id, tenant_id, "production")
        .await
        .expect("create_session");

    // The JWT still carries the stale role from login time.
    let token = sign_session_jwt(
        SESSION_SECRET,
        user_id,
        tenant_id,
        "tenant_admin",
        "production",
        session_id,
    )
    .expect("sign session jwt");

    let app = test_app(test_state(pool));
    let req = Request::builder()
        .method("POST")
        .uri("/internal/validate-session")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"session_token": token}).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "session with no active membership row must fail closed, not fall back to the stale JWT role"
    );
}

#[tokio::test]
async fn validate_session_accepts_session_with_active_membership() {
    let (pool, _container) = start_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    seed_user_and_tenant(&pool, user_id, tenant_id).await;

    oidc::upsert_user_tenant_role(&pool, user_id, tenant_id, "viewer")
        .await
        .expect("role assignment");

    let session_id = oidc::create_session(&pool, user_id, tenant_id, "production")
        .await
        .expect("create_session");

    // JWT role is stale ("tenant_admin"); the DB role ("viewer") must win.
    let token = sign_session_jwt(
        SESSION_SECRET,
        user_id,
        tenant_id,
        "tenant_admin",
        "production",
        session_id,
    )
    .expect("sign session jwt");

    let app = test_app(test_state(pool));
    let req = Request::builder()
        .method("POST")
        .uri("/internal/validate-session")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"session_token": token}).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["role"], "viewer");
    assert_eq!(body["user_id"], user_id.to_string());
    assert_eq!(body["tenant_id"], tenant_id.to_string());
}
