// HTTP integration tests for alert-evaluator's /internal/* service-to-service
// routes (Phase 6's internal-endpoint split by owner,
// docs/component-decomposition.md) against a real Postgres instance via
// Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot. These cover the alerting half of the joins that
// used to live in admin-service's combined internal endpoints; the
// deployment_markers half stays in
// services/admin-service/tests/internal_integration.rs.

use alert_evaluator::{AppState, internal, middleware::auth::InternalServiceToken, observability};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::get,
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

const INTERNAL_TOKEN: &str = "test-internal-token";

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

fn build_app(db: PgPool) -> Router {
    let state = AppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
    };
    Router::new()
        .route(
            "/internal/alerting-enrichment",
            get(internal::handle_alerting_enrichment),
        )
        .route(
            "/internal/alerting-correlation",
            get(internal::handle_alerting_correlation),
        )
        .layer(axum_middleware::from_fn(
            alert_evaluator::middleware::auth::require_internal_service,
        ))
        .layer(axum::Extension(InternalServiceToken(
            INTERNAL_TOKEN.to_string(),
        )))
        .with_state(state)
}

fn authed_request(uri: String) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-internal-token", INTERNAL_TOKEN)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn internal_routes_reject_missing_token() {
    let (db, _container) = start_postgres().await;
    let app = build_app(db);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/internal/alerting-enrichment?tenant_id=00000000-0000-0000-0000-000000000001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn alerting_enrichment_reports_slo_breach_alert_count() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;

    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'checkout', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Checkout SLO') \
         RETURNING slo_id",
    )
    .bind(tenant_id)
    .fetch_one(&db)
    .await
    .expect("slo inserted");

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, 'Checkout SLO burn', 'slo_burn_rate', 'critical', $2) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(serde_json::json!({ "slo_id": slo_id.to_string() }))
    .fetch_one(&db)
    .await
    .expect("alert rule inserted");

    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'active', 5.0)",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .execute(&db)
    .await
    .expect("alert firing inserted");

    let app = build_app(db);
    let resp = app
        .oneshot(authed_request(format!(
            "/internal/alerting-enrichment?tenant_id={tenant_id}"
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let items = body["items"].as_array().expect("items array");
    let checkout = items
        .iter()
        .find(|i| i["service_name"] == "checkout")
        .expect("checkout item present");
    assert_eq!(checkout["active_alert_count"], 1);
    assert_eq!(checkout["slo_breaching"], true);
}

#[tokio::test]
async fn alerting_correlation_filters_service_environment_and_interval() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;
    let service_name = "checkout";
    let from = Utc::now() - Duration::hours(6);
    let to = Utc::now();

    let checkout_prod_slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Checkout prod SLO') \
         RETURNING slo_id",
    )
    .bind(tenant_id)
    .bind(service_name)
    .fetch_one(&db)
    .await
    .expect("slo inserted");

    let checkout_prod_rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Checkout prod SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(serde_json::json!({
        "slo_id": checkout_prod_slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&db)
    .await
    .expect("slo rule inserted");

    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value, occurred_at) \
         VALUES ($1, $2, 'active', 0.42, NOW())",
    )
    .bind(checkout_prod_rule_id)
    .bind(tenant_id)
    .execute(&db)
    .await
    .expect("slo firing inserted");

    let checkout_staging_slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, 'staging', 'availability', 0.99, 30, 14.4, 1.0, 'Checkout staging SLO') \
         RETURNING slo_id",
    )
    .bind(tenant_id)
    .bind(service_name)
    .fetch_one(&db)
    .await
    .expect("staging slo inserted");

    let checkout_staging_rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Checkout staging SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(serde_json::json!({
        "slo_id": checkout_staging_slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&db)
    .await
    .expect("staging slo rule inserted");

    let payments_prod_slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'payments', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Payments prod SLO') \
         RETURNING slo_id",
    )
    .bind(tenant_id)
    .fetch_one(&db)
    .await
    .expect("payments slo inserted");

    let payments_prod_rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Payments prod SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant_id)
    .bind(serde_json::json!({
        "slo_id": payments_prod_slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&db)
    .await
    .expect("payments slo rule inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at, resolved_at) \
         VALUES ($1, $2, 'Checkout prod resolved', 'critical', 'resolved', 'checkout-prod-resolved', $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(checkout_prod_rule_id)
    .bind(from + Duration::hours(1))
    .bind(from + Duration::hours(2))
    .execute(&db)
    .await
    .expect("resolved incident inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at) \
         VALUES ($1, $2, 'Checkout prod open', 'warning', 'triggered', 'checkout-prod-open', $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(checkout_prod_rule_id)
    .bind(from + Duration::hours(3))
    .execute(&db)
    .await
    .expect("open incident inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at, resolved_at) \
         VALUES ($1, $2, 'Checkout staging incident', 'warning', 'resolved', 'checkout-staging', $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(checkout_staging_rule_id)
    .bind(from + Duration::hours(1))
    .bind(from + Duration::hours(2))
    .execute(&db)
    .await
    .expect("staging incident inserted");

    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, triggered_at, resolved_at) \
         VALUES ($1, $2, 'Payments prod incident', 'critical', 'resolved', 'payments-prod', $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(payments_prod_rule_id)
    .bind(from + Duration::hours(1))
    .bind(from + Duration::hours(2))
    .execute(&db)
    .await
    .expect("other service incident inserted");

    let app = build_app(db);
    let uri = format!(
        "/internal/alerting-correlation?tenant_id={tenant_id}&service_name={service_name}&environment=prod&from={}&to={}",
        from.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        to.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    let resp = app.oneshot(authed_request(uri)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = response_body_json(resp.into_body()).await;
    let incidents = body["incidents"].as_array().expect("incidents array");
    assert_eq!(incidents.len(), 2, "only prod checkout incidents");
    assert!(
        incidents.iter().all(|incident| {
            incident["title"] != "Checkout staging incident"
                && incident["title"] != "Payments prod incident"
        }),
        "report must only include the target service and environment"
    );

    let slos = body["slos"].as_array().expect("slos array");
    assert_eq!(slos.len(), 1, "only the prod checkout SLO");
    assert_eq!(slos[0]["firing"], true);
}
