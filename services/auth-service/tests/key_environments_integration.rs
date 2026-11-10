// GET /internal/key-environments: the environments of a tenant's active API keys, read with the
// scoped observable_auth login so observable-control never needs auth.api_keys.

use std::sync::Arc;

use auth_service::authz::{AuthzState, router};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

const INTERNAL_TOKEN: &str = "integration-internal-token";

fn app(db: PgPool) -> Router {
    router(AuthzState {
        db,
        internal_token: Arc::new(INTERNAL_TOKEN.into()),
    })
}

async fn list(app: &Router, tenant: Uuid, internal: bool) -> (StatusCode, Value) {
    let mut request = Request::get(format!("/internal/key-environments?tenant_id={tenant}"));
    if internal {
        request = request.header("x-internal-token", INTERNAL_TOKEN);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn key_environments_are_distinct_active_and_tenant_scoped() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (tenant, other) = (Uuid::new_v4(), Uuid::new_v4());
    for (id, env, revoked) in [
        (tenant, "staging", false),
        (tenant, "prod", false),
        (tenant, "prod", false),
        (tenant, "", false),
        (tenant, "retired", true),
        (other, "qa", false),
    ] {
        sqlx::query(
            "INSERT INTO auth.api_keys (tenant_id, key_hash, name, environment, role, revoked_at) \
             VALUES ($1, $2, 'k', $3, 'member', CASE WHEN $4 THEN now() END)",
        )
        .bind(id)
        .bind(Uuid::new_v4().simple().to_string())
        .bind(env)
        .bind(revoked)
        .execute(&db)
        .await
        .unwrap();
    }

    let options = db
        .connect_options()
        .as_ref()
        .clone()
        .username("observable_auth")
        .password("observable_auth");
    let scoped = PgPoolOptions::new().connect_with(options).await.unwrap();
    let app = app(scoped.clone());

    assert_eq!(list(&app, tenant, false).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        list(&app, tenant, true).await,
        (StatusCode::OK, json!({"environments": ["prod", "staging"]}))
    );
    assert_eq!(
        list(&app, Uuid::new_v4(), true).await,
        (StatusCode::OK, json!({"environments": []}))
    );

    scoped.close().await;
    assert_eq!(
        list(&app, tenant, true).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
