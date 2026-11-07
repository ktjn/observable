// Environment classes (phase 1 S4, OBS-01-03) against a real PostgreSQL database: tenant
// routes with an injected TenantContext and the internal route behind the internal token.

use std::sync::Arc;

use admin_service::{
    AdminServiceAppState, environments,
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

fn tenant_app(db: PgPool, ctx: TenantContext) -> Router {
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
        .with_state(state(db))
}

fn internal_app(db: PgPool) -> Router {
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

async fn api_key_env(db: &PgPool, tenant: Uuid, env: &str) {
    sqlx::query(
        "INSERT INTO api_keys (tenant_id, key_hash, name, environment, role) \
         VALUES ($1, $2, $3, $4, 'member')",
    )
    .bind(tenant)
    .bind(format!("hash-{}", Uuid::new_v4()))
    .bind(format!("key-{env}"))
    .bind(env)
    .execute(db)
    .await
    .unwrap();
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
    api_key_env(&db, tenant, "prod").await;
    api_key_env(&db, tenant, "staging").await;
    api_key_env(&db, tenant, "sandbox").await;
    let admin = tenant_app(db.clone(), ctx(tenant, "tenant_admin"));

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

    let viewer = tenant_app(db.clone(), ctx(tenant, "viewer"));
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
    let member = tenant_app(db.clone(), ctx(tenant, "member"));
    assert_eq!(
        send(&member, put_class(tenant, "prod", "production"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let admin = tenant_app(db.clone(), ctx(tenant, "tenant_admin"));
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
    tenant_app(db.clone(), ctx(tenant, "tenant_admin"))
        .oneshot(put_class(tenant, "prod", "production"))
        .await
        .unwrap();
    tenant_app(db.clone(), ctx(other, "tenant_admin"))
        .oneshot(put_class(other, "qa", "non_production"))
        .await
        .unwrap();
    let app = internal_app(db);

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
