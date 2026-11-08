//! Accountable owners for alert rules and objectives (phase 1 S6, OBS-34-01) against a real
//! PostgreSQL database, with an injected TenantContext.

use alert_evaluator::{
    AppState, alerts, catalog_sync, middleware::auth::TenantContext, observability, slos,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post, put},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn app(db: PgPool, tenant_id: Uuid, user_id: Option<Uuid>, role: &str) -> Router {
    let state = AppState {
        db,
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
    };
    Router::new()
        .route("/v1/admin/alerts/rules", post(alerts::handle_create_rule))
        .route(
            "/v1/admin/alerts/rules/{rule_id}/owner",
            put(alerts::handle_update_rule_owner),
        )
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .route(
            "/v1/slos",
            get(slos::handle_list_slos).post(slos::handle_create_slo),
        )
        .route(
            "/v1/slos/{slo_id}/owner",
            put(slos::handle_update_slo_owner),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id,
            role: role.into(),
        }))
        .with_state(state)
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(uri);
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn rule(name: &str, owner: Option<Value>) -> Value {
    let mut body = json!({
        "name": name, "metric_name": "errors", "operator": "gt", "threshold": 1.0,
        "service_name": "payments-api",
    });
    if let Some(o) = owner {
        body["owner"] = o;
    }
    body
}

async fn create_rule(app: &Router, body: Value) -> (StatusCode, Option<Uuid>) {
    let (status, json) = call(app, "POST", "/v1/admin/alerts/rules", Some(body)).await;
    (status, json["rule_id"].as_str().map(|s| s.parse().unwrap()))
}

async fn rule_owner(app: &Router, id: Uuid) -> (Value, Value) {
    let (status, json) = call(app, "GET", &format!("/v1/alerts/rules/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    (json["owner_kind"].clone(), json["owner_id"].clone())
}

#[tokio::test]
async fn a_rule_is_owned_by_its_creator_or_an_explicit_principal() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (tenant, user, team) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let session = app(db.clone(), tenant, Some(user), "tenant_admin");
    let api_key = app(db.clone(), tenant, None, "tenant_admin");

    let (status, by_user) = create_rule(&session, rule("By user", None)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        rule_owner(&session, by_user.unwrap()).await,
        (json!("user"), json!(user))
    );

    let team_owner = json!({"principal_kind": "team", "principal_id": team});
    let (_, by_team) = create_rule(&api_key, rule("For team", Some(team_owner))).await;
    assert_eq!(
        rule_owner(&session, by_team.unwrap()).await,
        (json!("team"), json!(team))
    );

    // An API key without an owner creates an ownerless rule, which the catalog reports.
    let (_, ownerless) = create_rule(&api_key, rule("Ownerless", None)).await;
    assert_eq!(
        rule_owner(&session, ownerless.unwrap()).await,
        (Value::Null, Value::Null)
    );
    let bad = json!({"principal_kind": "group", "principal_id": team});
    assert_eq!(
        create_rule(&session, rule("Bad", Some(bad))).await.0,
        StatusCode::BAD_REQUEST
    );

    // The list shows the owner too.
    let (_, list) = call(&session, "GET", "/v1/alerts/rules", None).await;
    let listed = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "By user")
        .unwrap()
        .clone();
    assert_eq!(listed["owner_id"], json!(user));
}

#[tokio::test]
async fn only_tenant_admins_reassign_a_rule_owner_within_their_tenant() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (tenant, user, next) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let admin = app(db.clone(), tenant, Some(user), "tenant_admin");
    let (_, id) = create_rule(&admin, rule("Errors", None)).await;
    let id = id.unwrap();
    let uri = format!("/v1/admin/alerts/rules/{id}/owner");
    let body = json!({"owner": {"principal_kind": "user", "principal_id": next}});

    let member = app(db.clone(), tenant, Some(next), "member");
    assert_eq!(
        call(&member, "PUT", &uri, Some(body.clone())).await.0,
        StatusCode::FORBIDDEN
    );
    let other = app(db.clone(), Uuid::new_v4(), Some(user), "tenant_admin");
    assert_eq!(
        call(&other, "PUT", &uri, Some(body.clone())).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&admin, "PUT", &uri, Some(body)).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(rule_owner(&admin, id).await, (json!("user"), json!(next)));
    assert_eq!(
        call(&admin, "PUT", &uri, Some(json!({"owner": null})))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(rule_owner(&admin, id).await, (Value::Null, Value::Null));
}

#[tokio::test]
async fn an_objective_and_its_burn_rate_rule_share_an_owner_that_reaches_the_catalog() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (tenant, user, team) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let admin = app(db.clone(), tenant, Some(user), "tenant_admin");
    let (status, slo) = call(
        &admin,
        "POST",
        "/v1/slos",
        Some(json!({
            "service_name": "payments-api", "environment": "prod", "target": 0.999,
            "window_days": 30, "burn_rate_fast_threshold": 14.4, "burn_rate_slow_threshold": 1.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (&slo["owner_kind"], &slo["owner_id"]),
        (&json!("user"), &json!(user))
    );
    let slo_id = slo["slo_id"].as_str().unwrap().to_owned();

    let synced = catalog_sync::collect(&db).await.unwrap();
    let owner = |kind: &str| -> Vec<Value> {
        synced[&tenant][kind]
            .iter()
            .map(|a| serde_json::to_value(a).unwrap()["owner"].clone())
            .collect()
    };
    let user_owner = json!({"principal_kind": "user", "principal_id": user});
    assert_eq!(owner("objective"), vec![user_owner.clone()]);
    // The burn-rate rule is attributed to the objective's service, with the same owner.
    assert_eq!(owner("alert_rule"), vec![user_owner]);

    let (status, _) = call(
        &admin,
        "PUT",
        &format!("/v1/slos/{slo_id}/owner"),
        Some(json!({"owner": {"principal_kind": "team", "principal_id": team}})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let synced = catalog_sync::collect(&db).await.unwrap();
    for kind in ["objective", "alert_rule"] {
        let a = serde_json::to_value(&synced[&tenant][kind][0]).unwrap();
        assert_eq!(a["owner"]["principal_id"], json!(team), "{kind}");
    }
    let member = app(db.clone(), tenant, Some(user), "member");
    assert_eq!(
        call(
            &member,
            "PUT",
            &format!("/v1/slos/{slo_id}/owner"),
            Some(json!({"owner": null}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
