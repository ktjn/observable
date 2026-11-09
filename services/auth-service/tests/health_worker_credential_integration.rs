use std::sync::Arc;

use auth_service::{
    authz::{AuthzState, router},
    lookup_api_key,
    validate::sha256_hex,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

const INTERNAL_TOKEN: &str = "integration-internal-token";
const PATH: &str = "/internal/health-worker/validate";

fn worker_router(db: PgPool) -> Router {
    router(AuthzState {
        db,
        internal_token: Arc::new(INTERNAL_TOKEN.into()),
    })
}

async fn validate(app: &Router, tenant: Uuid, token: &str, internal: bool) -> StatusCode {
    let mut request = Request::post(PATH).header("content-type", "application/json");
    if internal {
        request = request.header("x-internal-token", INTERNAL_TOKEN);
    }
    app.clone()
        .oneshot(
            request
                .body(Body::from(
                    json!({"tenant_id": tenant, "token": token}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn fixture_token() -> String {
    format!("ohw_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[tokio::test]
async fn health_worker_validation_requires_internal_authentication() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/test")
        .unwrap();
    pool.close().await;
    let app = worker_router(pool);
    assert_eq!(
        validate(&app, Uuid::new_v4(), &fixture_token(), false).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        validate(&app, Uuid::new_v4(), &fixture_token(), true).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn worker_credential_is_tenant_bound_active_and_separate_from_ingestion() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenant = Uuid::new_v4();
    let other = Uuid::new_v4();
    for id in [tenant, other] {
        sqlx::query("INSERT INTO control.tenants (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(format!("worker-{id}"))
            .execute(&db)
            .await
            .unwrap();
    }
    let active = fixture_token();
    let expired = fixture_token();
    let revoked = fixture_token();
    for (token, lifetime_seconds, revoked) in [
        (&active, 3600_i32, false),
        (&expired, -1, false),
        (&revoked, 3600, true),
    ] {
        sqlx::query("INSERT INTO auth.health_worker_credentials (token_hash_sha256, tenant_id, expires_at, revoked_at) VALUES ($1, $2, now() + make_interval(secs => $3), CASE WHEN $4 THEN now() END)")
            .bind(sha256_hex(token)).bind(tenant).bind(f64::from(lifetime_seconds)).bind(revoked).execute(&db).await.unwrap();
    }
    let ingest = Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO auth.api_keys (tenant_id, key_hash, name, environment, role) VALUES ($1, $2, 'test-ingest', 'prod', 'member')")
        .bind(tenant).bind(sha256_hex(&ingest)).execute(&db).await.unwrap();
    assert!(lookup_api_key(&db, &ingest).await.is_ok());
    assert!(lookup_api_key(&db, &active).await.is_err());

    let auth_options = db
        .connect_options()
        .as_ref()
        .clone()
        .username("observable_auth")
        .password("observable_auth");
    let scoped = PgPoolOptions::new()
        .connect_with(auth_options)
        .await
        .unwrap();
    let app = worker_router(scoped.clone());
    assert_eq!(
        validate(&app, tenant, &active, true).await,
        StatusCode::NO_CONTENT
    );
    for (id, token) in [
        (other, &active),
        (tenant, &expired),
        (tenant, &revoked),
        (tenant, &ingest),
        (tenant, &fixture_token()),
    ] {
        assert_eq!(
            validate(&app, id, token, true).await,
            StatusCode::UNAUTHORIZED
        );
    }
    scoped.close().await;
    assert_eq!(
        validate(&app, tenant, &active, true).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
