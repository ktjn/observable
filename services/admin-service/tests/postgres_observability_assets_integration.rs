//! Control's observability assets for the catalog (phase 1 S6) against a real PostgreSQL
//! database: dashboards attributed to one service, behind the internal token.

use std::sync::Arc;

use admin_service::{
    AdminServiceAppState,
    dashboards::{CreateDashboardRequest, DashboardPanelRequest, create_dashboard},
    middleware::auth::InternalServiceToken,
    observability, observability_assets,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const INTERNAL_TOKEN: &str = "test-internal-token";

fn internal_app(db: PgPool) -> Router {
    Router::new()
        .route(
            "/internal/observability-assets",
            get(observability_assets::handle_internal_observability_assets),
        )
        .layer(axum_middleware::from_fn(
            admin_service::middleware::auth::require_internal_service,
        ))
        .layer(axum::Extension(InternalServiceToken(
            INTERNAL_TOKEN.to_string(),
        )))
        .with_state(AdminServiceAppState {
            db,
            ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
            auth_service_url: "http://auth-service:4319".into(),
            http_client: reqwest::Client::new(),
            metrics: Arc::new(observability::AdminServiceMetrics::new()),
            producer: None,
        })
}

async fn tenant(db: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(id)
        .bind(format!("tenant-{id}"))
        .execute(db)
        .await
        .unwrap();
    id
}

async fn user(db: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (idp_subject, email) VALUES ($1, 'u@example.test') RETURNING id",
    )
    .bind(Uuid::new_v4().to_string())
    .fetch_one(db)
    .await
    .unwrap()
}

fn panel(service: Option<&str>, filters: Value) -> DashboardPanelRequest {
    match service {
        Some(s) => DashboardPanelRequest {
            title: "panel".into(),
            query_kind: Some("logs".into()),
            service: Some(s.into()),
            preset: Some("1h".into()),
            filters,
            ..Default::default()
        },
        None => DashboardPanelRequest {
            title: "notes".into(),
            panel_kind: Some("text".into()),
            content: Some("notes".into()),
            ..Default::default()
        },
    }
}

async fn dashboard(
    db: &PgPool,
    tenant: Uuid,
    name: &str,
    panels: Vec<DashboardPanelRequest>,
    owner: Option<Uuid>,
) -> Uuid {
    create_dashboard(
        db,
        tenant,
        &CreateDashboardRequest {
            name: name.into(),
            panels,
        },
        owner,
    )
    .await
    .unwrap()
    .dashboard_id
}

async fn fetch(app: &Router, tenant: Uuid, token: Option<&str>) -> (StatusCode, Value) {
    let mut req =
        Request::builder().uri(format!("/internal/observability-assets?tenant_id={tenant}"));
    if let Some(t) = token {
        req = req.header("x-internal-token", t);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn dashboards_attribute_to_a_single_panel_service_with_their_owner() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (a, b) = (tenant(&db).await, tenant(&db).await);
    let owner = user(&db).await;
    let single = dashboard(
        &db,
        a,
        "Checkout",
        vec![
            panel(Some("checkout"), json!({"environment": "prod"})),
            panel(Some("checkout"), json!({"environment": "prod"})),
            panel(None, json!({})),
        ],
        Some(owner),
    )
    .await;
    dashboard(
        &db,
        a,
        "Mixed",
        vec![
            panel(Some("checkout"), json!({})),
            panel(Some("ledger"), json!({})),
        ],
        None,
    )
    .await;
    let no_env = dashboard(
        &db,
        a,
        "Ledger",
        vec![panel(Some("ledger"), json!({}))],
        None,
    )
    .await;
    dashboard(&db, a, "Notes", vec![panel(None, json!({}))], None).await;
    dashboard(
        &db,
        b,
        "Foreign",
        vec![panel(Some("checkout"), json!({}))],
        None,
    )
    .await;

    let app = internal_app(db);
    let (status, body) = fetch(&app, a, Some(INTERNAL_TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let mut assets = body["assets"].as_array().unwrap().clone();
    assets.sort_by_key(|a| a["name"].as_str().unwrap().to_owned());
    assert_eq!(
        assets,
        vec![
            json!({"kind": "dashboard", "external_id": single.to_string(), "service_name": "checkout",
                   "environment": "prod", "name": "Checkout",
                   "owner": {"principal_kind": "user", "principal_id": owner}}),
            json!({"kind": "dashboard", "external_id": no_env.to_string(), "service_name": "ledger",
                   "environment": "", "name": "Ledger"}),
        ]
    );
}

#[tokio::test]
async fn observability_assets_require_the_internal_token() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let a = tenant(&db).await;
    let app = internal_app(db);
    assert_eq!(fetch(&app, a, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        fetch(&app, a, Some("wrong")).await.0,
        StatusCode::UNAUTHORIZED
    );
}
