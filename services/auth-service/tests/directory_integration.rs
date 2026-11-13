// Directory views for observable-control, read with the scoped observable_auth login so control
// never needs auth.* tables: GET /internal/key-environments and GET /internal/users/{id}/tenants.

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

#[tokio::test]
async fn user_tenants_lists_only_that_users_memberships() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (user, other_user) = (Uuid::new_v4(), Uuid::new_v4());
    let (t1, t2, t3) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (id, sub) in [(user, "dir-a"), (other_user, "dir-b")] {
        sqlx::query("INSERT INTO auth.users (id, idp_subject, email) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(sub)
            .bind(format!("{sub}@example.com"))
            .execute(&db)
            .await
            .unwrap();
    }
    for (u, t, role) in [
        (user, t1, "tenant_admin"),
        (user, t2, "viewer"),
        (other_user, t3, "member"),
    ] {
        sqlx::query(
            "INSERT INTO auth.user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, $3)",
        )
        .bind(u)
        .bind(t)
        .bind(role)
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
    let get = |id: Uuid, internal: bool| {
        let mut r = Request::get(format!("/internal/users/{id}/tenants"));
        if internal {
            r = r.header("x-internal-token", INTERNAL_TOKEN);
        }
        r.body(Body::empty()).unwrap()
    };
    let call = |req: Request<Body>| {
        let app = app.clone();
        async move {
            let resp = app.oneshot(req).await.unwrap();
            let status = resp.status();
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };

    assert_eq!(call(get(user, false)).await.0, StatusCode::UNAUTHORIZED);
    let (status, body) = call(get(user, true)).await;
    assert_eq!(status, StatusCode::OK);
    let mut got: Vec<(String, String)> = body["tenants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["tenant_id"].as_str().unwrap().to_owned(),
                m["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    got.sort();
    let mut want = vec![
        (t1.to_string(), "tenant_admin".to_owned()),
        (t2.to_string(), "viewer".to_owned()),
    ];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(
        call(get(Uuid::new_v4(), true)).await,
        (StatusCode::OK, json!({"tenants": []}))
    );
    scoped.close().await;
    assert_eq!(
        call(get(user, true)).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
