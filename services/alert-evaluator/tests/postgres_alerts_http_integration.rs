// HTTP integration test for GET /v1/alerts/rules and GET /v1/alerts/rules/{id}
// against a real Postgres instance via Testcontainers, exercising the full
// handler path via tower::ServiceExt::oneshot. Moved here from admin-service
// when alert-rule ownership moved to the alerting component (Phase 6,
// docs/component-decomposition.md).

use alert_evaluator::{AppState, alerts, middleware::auth::TenantContext, observability};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = observable_config::with_search_path(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ));
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("postgres pool connected");
    apply_pg_migrations(&pool).await;
    (pool, container)
}

async fn apply_pg_migrations(pool: &PgPool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .expect("pg migration applied");
    }
}

async fn response_body_json(body: Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("tenant-{tenant_id}"))
        .execute(pool)
        .await
        .expect("tenant inserted");
}

fn build_app(db: PgPool, tenant_id: Uuid) -> Router {
    let state = AppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
    };
    Router::new()
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn list_alert_rules_http_returns_lifecycle_state() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let rule_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'HTTP lifecycle rule', 'threshold', 'warning', $3)",
    )
    .bind(rule_id)
    .bind(tenant)
    .bind(serde_json::json!({
        "metric_name": "http_lifecycle_metric",
        "operator": "gt",
        "threshold": 0.05,
    }))
    .execute(&db)
    .await
    .expect("alert rule inserted");
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'pending', 0.10)",
    )
    .bind(rule_id)
    .bind(tenant)
    .execute(&db)
    .await
    .expect("alert firing inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/alerts/rules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["rule_id"] == rule_id.to_string())
        .expect("inserted rule appears in HTTP response");
    assert_eq!(item["state"], "pending");
    assert_eq!(item["firing"], false);
}

#[tokio::test]
async fn get_alert_rule_returns_detail_with_firings() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'High Error Rate', 'threshold', 'critical', \
                 '{\"metric_name\":\"error_rate\",\"operator\":\"gt\",\"threshold\":0.05}', \
                 '{}', false) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .expect("rule inserted");

    for state in ["active", "resolved"] {
        sqlx::query(
            "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(rule_id)
        .bind(tenant)
        .bind(state)
        .bind(0.08_f64)
        .execute(&db)
        .await
        .expect("firing inserted");
    }

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/alerts/rules/{rule_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["name"], "High Error Rate");
    assert_eq!(body["severity"], "critical");
    assert_eq!(body["alert_type"], "threshold");
    let firings = body["firings"].as_array().unwrap();
    assert_eq!(firings.len(), 2);
}

#[tokio::test]
async fn get_alert_rule_returns_404_for_wrong_tenant() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    insert_tenant(&db, other_tenant).await;

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Other Tenant Rule', 'threshold', 'warning', \
                 '{\"metric_name\":\"m\",\"operator\":\"gt\",\"threshold\":1.0}', \
                 '{}', false) \
         RETURNING rule_id",
    )
    .bind(other_tenant) // different tenant — NOT the requesting tenant
    .fetch_one(&db)
    .await
    .expect("rule inserted");

    // Request is authenticated as `tenant`, not `other_tenant`.
    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/alerts/rules/{rule_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
