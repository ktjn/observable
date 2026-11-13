// Environment classes (phase 1 S4, OBS-01-03) against a real PostgreSQL database: tenant
// routes with an injected TenantContext and the internal route behind the internal token.

use std::sync::Arc;

use admin_service::{
    AdminServiceAppState,
    auth_directory::AuthDirectory,
    environments,
    middleware::auth::{InternalServiceToken, TenantContext},
    observability,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::{get, put},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INTERNAL_TOKEN: &str = "test-internal-token";

fn state(db: PgPool) -> AdminServiceAppState {
    AdminServiceAppState {
        db,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    }
}

/// auth-service owning `auth.api_keys`: the control role never reads that table.
struct Auth {
    server: MockServer,
}

impl Auth {
    async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    async fn key_environments(&self, tenant: Uuid, environments: &[&str]) {
        Mock::given(method("GET"))
            .and(path("/internal/key-environments"))
            .and(query_param("tenant_id", tenant.to_string()))
            .and(header("x-internal-token", INTERNAL_TOKEN))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "environments": environments })),
            )
            .mount(&self.server)
            .await;
    }

    fn keys(&self) -> AuthDirectory {
        AuthDirectory::new(&self.server.uri(), INTERNAL_TOKEN, reqwest::Client::new())
    }
}

fn tenant_app(db: PgPool, keys: AuthDirectory, ctx: TenantContext) -> Router {
    Router::new()
        .route(
            "/v1/tenants/{id}/environment-classes",
            get(environments::handle_list_environment_classes),
        )
        .route(
            "/v1/tenants/{id}/environment-classes/{environment}",
            put(environments::handle_set_environment_class)
                .delete(environments::handle_clear_environment_class),
        )
        .layer(axum::Extension(ctx))
        .layer(axum::Extension(keys))
        .with_state(state(db))
}

fn internal_app(db: PgPool, keys: AuthDirectory) -> Router {
    Router::new()
        .route(
            "/internal/environment-classes",
            get(environments::handle_internal_environment_classes),
        )
        .layer(axum_middleware::from_fn(
            admin_service::middleware::auth::require_internal_service,
        ))
        .layer(axum::Extension(InternalServiceToken(
            INTERNAL_TOKEN.to_string(),
        )))
        .layer(axum::Extension(keys))
        .with_state(state(db))
}

fn ctx(tenant_id: Uuid, role: &str) -> TenantContext {
    TenantContext {
        tenant_id,
        user_id: Some(Uuid::new_v4()),
        role: role.into(),
    }
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn put_class(tenant: Uuid, env: &str, class: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!("/v1/tenants/{tenant}/environment-classes/{env}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"class": class}).to_string()))
        .unwrap()
}

fn get_uri(uri: String) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn classes(v: &Value) -> Vec<(String, String)> {
    v["environments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["environment"].as_str().unwrap().to_owned(),
                e["class"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn each_environment_has_exactly_one_class_and_unclassified_is_explicit() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant = Uuid::new_v4();
    let auth = Auth::start().await;
    auth.key_environments(tenant, &["prod", "sandbox", "staging"])
        .await;
    let admin = tenant_app(db.clone(), auth.keys(), ctx(tenant, "tenant_admin"));

    assert_eq!(
        send(&admin, put_class(tenant, "prod", "production"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        send(&admin, put_class(tenant, "staging", "non_production"))
            .await
            .0,
        StatusCode::OK
    );

    let viewer = tenant_app(db.clone(), auth.keys(), ctx(tenant, "viewer"));
    let (status, body) = send(
        &viewer,
        get_uri(format!("/v1/tenants/{tenant}/environment-classes")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        classes(&body),
        vec![
            ("prod".into(), "production".into()),
            ("sandbox".into(), "unclassified".into()),
            ("staging".into(), "non_production".into()),
        ]
    );

    // Reclassifying replaces the class; clearing returns it to unclassified.
    assert_eq!(
        send(&admin, put_class(tenant, "staging", "production"))
            .await
            .0,
        StatusCode::OK
    );
    let del = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/tenants/{tenant}/environment-classes/staging"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&admin, del).await.0, StatusCode::NO_CONTENT);
    let (_, body) = send(
        &viewer,
        get_uri(format!("/v1/tenants/{tenant}/environment-classes")),
    )
    .await;
    assert!(classes(&body).contains(&("staging".into(), "unclassified".into())));
}

#[tokio::test]
async fn only_tenant_admins_classify_and_input_is_validated() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant = Uuid::new_v4();
    let auth = Auth::start().await;
    let member = tenant_app(db.clone(), auth.keys(), ctx(tenant, "member"));
    assert_eq!(
        send(&member, put_class(tenant, "prod", "production"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let admin = tenant_app(db.clone(), auth.keys(), ctx(tenant, "tenant_admin"));
    assert_eq!(
        send(&admin, put_class(tenant, "prod", "business_critical"))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(&admin, put_class(tenant, "prod%20env", "production"))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Another tenant's path is refused.
    assert_eq!(
        send(&admin, put_class(Uuid::new_v4(), "prod", "production"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, _) = send(
        &admin,
        get_uri(format!(
            "/v1/tenants/{}/environment-classes",
            Uuid::new_v4()
        )),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn internal_classes_require_the_token_and_stay_tenant_scoped() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (tenant, other) = (Uuid::new_v4(), Uuid::new_v4());
    let auth = Auth::start().await;
    auth.key_environments(tenant, &[]).await;
    tenant_app(db.clone(), auth.keys(), ctx(tenant, "tenant_admin"))
        .oneshot(put_class(tenant, "prod", "production"))
        .await
        .unwrap();
    tenant_app(db.clone(), auth.keys(), ctx(other, "tenant_admin"))
        .oneshot(put_class(other, "qa", "non_production"))
        .await
        .unwrap();
    let app = internal_app(db, auth.keys());

    let (status, _) = send(
        &app,
        get_uri(format!("/internal/environment-classes?tenant_id={tenant}")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let req = Request::builder()
        .uri(format!("/internal/environment-classes?tenant_id={tenant}"))
        .header("x-internal-token", INTERNAL_TOKEN)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(classes(&body), vec![("prod".into(), "production".into())]);
}

#[tokio::test]
async fn listing_fails_closed_when_auth_cannot_list_key_environments() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant = Uuid::new_v4();
    let auth = Auth::start().await;
    Mock::given(method("GET"))
        .and(path("/internal/key-environments"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&auth.server)
        .await;
    let admin = tenant_app(db.clone(), auth.keys(), ctx(tenant, "tenant_admin"));
    assert_eq!(
        send(&admin, put_class(tenant, "prod", "production"))
            .await
            .0,
        StatusCode::OK
    );
    let (status, _) = send(
        &admin,
        get_uri(format!("/v1/tenants/{tenant}/environment-classes")),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let unreachable =
        AuthDirectory::new("http://127.0.0.1:1", INTERNAL_TOKEN, reqwest::Client::new());
    let req = Request::builder()
        .uri(format!("/internal/environment-classes?tenant_id={tenant}"))
        .header("x-internal-token", INTERNAL_TOKEN)
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&internal_app(db, unreachable), req).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn scoped_control_login_lists_classes_without_reading_api_keys() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant = Uuid::new_v4();
    let auth = Auth::start().await;
    auth.key_environments(tenant, &["prod", "sandbox"]).await;
    let options = db
        .connect_options()
        .as_ref()
        .clone()
        .username("observable_control")
        .password("observable_control");
    let scoped = sqlx::postgres::PgPoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    let admin = tenant_app(scoped.clone(), auth.keys(), ctx(tenant, "tenant_admin"));
    assert_eq!(
        send(&admin, put_class(tenant, "prod", "production"))
            .await
            .0,
        StatusCode::OK
    );
    let req = Request::builder()
        .uri(format!("/internal/environment-classes?tenant_id={tenant}"))
        .header("x-internal-token", INTERNAL_TOKEN)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&internal_app(scoped, auth.keys()), req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        classes(&body),
        vec![
            ("prod".into(), "production".into()),
            ("sandbox".into(), "unclassified".into()),
        ]
    );
}
