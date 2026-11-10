// Session-scoped behavior of the bootstrap tenant endpoints: GET /v1/tenants
// filters to the caller's tenants when a session cookie/bearer token is
// present, and GET /v1/tenants/{id}/environments rejects a session that
// lacks access to the requested tenant. Both endpoints must also stay usable
// without any session at all (bootstrap resources, outside require_tenant).
// Memberships and key environments are auth data, read through auth-service's
// directory views; the control login never reads auth.* tables.

use admin_service::{AdminServiceAppState, auth_directory::AuthDirectory, observability, tenants};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header as has_header, method, path},
};

const INTERNAL_TOKEN: &str = "test-internal-token";

async fn start_pool() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

/// The same database through the scoped observable_control login used in deployments.
async fn control_login(db: &PgPool) -> PgPool {
    let options = db
        .connect_options()
        .as_ref()
        .clone()
        .username("observable_control")
        .password("observable_control");
    PgPoolOptions::new().connect_with(options).await.unwrap()
}

fn build_app(db: PgPool, auth_service_url: String) -> Router {
    let directory = AuthDirectory::new(&auth_service_url, INTERNAL_TOKEN, reqwest::Client::new());
    let state = AdminServiceAppState {
        db,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url,
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    };
    Router::new()
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/{id}/environments",
            get(tenants::list_tenant_environments),
        )
        .layer(axum::Extension(directory))
        .with_state(state)
}

async fn response_body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

async fn seed_tenants(db: &PgPool, tenants: &[(Uuid, &str)]) {
    for (id, name) in tenants {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(name)
            .execute(db)
            .await
            .unwrap();
    }
}

async fn auth_with_session(
    user_id: Uuid,
    tenant: Uuid,
    memberships: &[(Uuid, &str)],
) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant.to_string(),
            "role": "admin",
            "environment": "prod"
        })))
        .mount(&server)
        .await;
    let tenants: Vec<Value> = memberships
        .iter()
        .map(|(t, role)| json!({"tenant_id": t, "role": role}))
        .collect();
    Mock::given(method("GET"))
        .and(path(format!("/internal/users/{user_id}/tenants")))
        .and(has_header("x-internal-token", INTERNAL_TOKEN))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tenants": tenants })))
        .mount(&server)
        .await;
    server
}

async fn key_environments(server: &MockServer, environments: &[&str]) {
    Mock::given(method("GET"))
        .and(path("/internal/key-environments"))
        .and(has_header("x-internal-token", INTERNAL_TOKEN))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "environments": environments })),
        )
        .mount(server)
        .await;
}

fn get_with(uri: String, h: (header::HeaderName, &str)) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(h.0, h.1)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn list_tenants_filtered_by_session_under_the_control_login() {
    let (db, _container) = start_pool().await;
    let (user_id, tenant1, tenant2, tenant3) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    seed_tenants(
        &db,
        &[
            (tenant1, "Tenant 1"),
            (tenant2, "Tenant 2"),
            (tenant3, "Tenant 3"),
        ],
    )
    .await;
    let auth = auth_with_session(
        user_id,
        tenant1,
        &[(tenant1, "tenant_admin"), (tenant2, "viewer")],
    )
    .await;

    let app = build_app(control_login(&db).await, auth.uri());
    let resp = app
        .oneshot(get_with(
            "/v1/tenants".into(),
            (header::AUTHORIZATION, "Bearer some-session"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    assert_eq!(
        json,
        json!({"tenants": [{"id": tenant1, "name": "Tenant 1"}, {"id": tenant2, "name": "Tenant 2"}]})
    );
}

#[tokio::test]
async fn list_environments_filtered_by_session_under_the_control_login() {
    let (db, _container) = start_pool().await;
    let (user_id, tenant1, tenant2) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    seed_tenants(&db, &[(tenant1, "Tenant 1"), (tenant2, "Tenant 2")]).await;
    let auth = auth_with_session(user_id, tenant1, &[(tenant1, "tenant_admin")]).await;
    key_environments(&auth, &["prod"]).await;

    let app = build_app(control_login(&db).await, auth.uri());
    let resp = app
        .clone()
        .oneshot(get_with(
            format!("/v1/tenants/{tenant1}/environments"),
            (header::COOKIE, "session=valid"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        response_body_json(resp.into_body()).await,
        json!({"environments": [{"environment": "prod"}]})
    );

    let resp = app
        .oneshot(get_with(
            format!("/v1/tenants/{tenant2}/environments"),
            (header::COOKIE, "session=valid"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn session_reads_fail_closed_when_the_auth_directory_is_unavailable() {
    let (db, _container) = start_pool().await;
    let (user_id, tenant) = (Uuid::new_v4(), Uuid::new_v4());
    seed_tenants(&db, &[(tenant, "Tenant")]).await;
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_id": user_id.to_string(),
            "tenant_id": tenant.to_string(),
            "role": "admin",
            "environment": "prod"
        })))
        .mount(&auth)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/internal/users/{user_id}/tenants")))
        .respond_with(ResponseTemplate::new(503))
        .mount(&auth)
        .await;

    let app = build_app(db, auth.uri());
    for uri in [
        "/v1/tenants".to_owned(),
        format!("/v1/tenants/{tenant}/environments"),
    ] {
        let resp = app
            .clone()
            .oneshot(get_with(uri, (header::COOKIE, "session=valid")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

#[tokio::test]
async fn bootstrap_endpoints_public_without_session() {
    let (db, _container) = start_pool().await;
    let (t1, t2) = (Uuid::new_v4(), Uuid::new_v4());
    seed_tenants(&db, &[(t1, "T1"), (t2, "T2")]).await;
    let auth = MockServer::start().await;
    key_environments(&auth, &[]).await;

    let app = build_app(control_login(&db).await, auth.uri());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/tenants")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = response_body_json(resp.into_body()).await;
    assert!(json["tenants"].as_array().unwrap().len() >= 2);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/tenants/{t2}/environments"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
