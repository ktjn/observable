// HTTP integration test for GET /v1/incidents and GET /v1/incidents/{id}
// against a real Postgres instance via Testcontainers, exercising the full
// handler path via tower::ServiceExt::oneshot.

use alert_evaluator::{AppState, incidents, middleware::auth::TenantContext, observability};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
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
        .route("/v1/incidents", get(incidents::handle_list_incidents))
        .route(
            "/v1/incidents/{incident_id}",
            get(incidents::handle_get_incident),
        )
        .route(
            "/v1/incidents/{incident_id}/acknowledge",
            post(incidents::handle_acknowledge_incident),
        )
        .route(
            "/v1/incidents/{incident_id}/status",
            post(incidents::handle_update_incident_status),
        )
        .route(
            "/v1/incidents/{incident_id}/resolve",
            post(incidents::handle_resolve_incident),
        )
        .route(
            "/v1/incidents/{incident_id}/comments",
            post(incidents::handle_add_incident_comment),
        )
        .route(
            "/v1/incidents/{incident_id}/link-deployment",
            post(incidents::handle_link_incident_deployment),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: None,
            role: "tenant_admin".into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn list_incidents_returns_tenant_scoped_incidents() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    sqlx::query(
        "INSERT INTO incidents (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'HTTP test incident', 'critical', 'triggered', 'dedup-1')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant)
    .execute(&db)
    .await
    .expect("incident inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/incidents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    let items = body["items"].as_array().unwrap();
    assert!(
        items.iter().any(|i| i["title"] == "HTTP test incident"),
        "incident must appear in list"
    );
}

#[tokio::test]
async fn get_incident_returns_detail_with_timeline() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let incident_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO incidents (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Detail incident', 'warning', 'resolved', 'dedup-2')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&db)
    .await
    .expect("incident inserted");

    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'triggered', 'system', 'Alert fired')",
    )
    .bind(incident_id)
    .execute(&db)
    .await
    .expect("event inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/incidents/{incident_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["title"], "Detail incident");
    assert_eq!(body["status"], "resolved");
    let timeline = body["timeline"].as_array().unwrap();
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0]["event_type"], "triggered");
    assert_eq!(timeline[0]["actor"], "system");
}

#[tokio::test]
async fn get_incident_returns_404_for_unknown_id() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;
    let unknown_id = Uuid::new_v4();

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/incidents/{unknown_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_incident_detail_includes_rule_name() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'CPU High', 'threshold', 'critical', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":90}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .expect("rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'CPU spike', 'critical', 'triggered', 'dedup-rule-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&db)
    .await
    .expect("incident inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/incidents/{incident_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(body["rule_name"], "CPU High");
}

#[tokio::test]
async fn get_incident_detail_rule_name_null_when_no_rule() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Manual incident', 'warning', 'triggered', 'dedup-norule')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&db)
    .await
    .expect("incident inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/incidents/{incident_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert!(body["rule_name"].is_null());
}

#[tokio::test]
async fn get_incident_detail_includes_impacted_service_for_slo_rule() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    // Seed an SLO definition for "payments" service
    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, 'payments', 'prod', 'availability', 0.99, 30, 14.4, 1.0, 'Payments SLO') \
         RETURNING slo_id",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .expect("slo inserted");

    // Seed an slo_burn_rate alert rule referencing the SLO
    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'Payments SLO burn', 'slo_burn_rate', 'critical', $2, '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .bind(serde_json::json!({
        "slo_id": slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    }))
    .fetch_one(&db)
    .await
    .expect("slo_burn_rate rule inserted");

    // Seed an incident linked to that rule
    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'Payments SLO burn', 'critical', 'triggered', 'slo-dedup-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&db)
    .await
    .expect("slo incident inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/incidents/{incident_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert_eq!(
        body["impacted_service"], "payments",
        "slo_burn_rate incident must carry impacted_service from slo_definitions"
    );
}

#[tokio::test]
async fn get_incident_detail_impacted_service_null_for_threshold_rule() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let rule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, notification_channels, auto_trigger_incident) \
         VALUES ($1, 'High CPU', 'threshold', 'warning', \
                 '{\"metric_name\":\"cpu\",\"operator\":\"gt\",\"threshold\":80}', \
                 '{}', true) \
         RETURNING rule_id",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .expect("threshold rule inserted");

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents \
         (incident_id, tenant_id, title, severity, status, dedup_key, triggered_by_rule_id) \
         VALUES ($1, $2, 'High CPU', 'warning', 'triggered', 'threshold-dedup-1', $3)",
    )
    .bind(incident_id)
    .bind(tenant)
    .bind(rule_id)
    .execute(&db)
    .await
    .expect("threshold incident inserted");

    let app = build_app(db, tenant);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/incidents/{incident_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_body_json(response.into_body()).await;
    assert!(
        body["impacted_service"].is_null(),
        "threshold incident must have null impacted_service"
    );
}

#[tokio::test]
async fn incident_lifecycle_transitions_and_responder_actions() {
    let (db, _container) = start_postgres().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&db, tenant).await;

    let incident_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO incidents (incident_id, tenant_id, title, severity, status, dedup_key) \
         VALUES ($1, $2, 'Payment latency spike', 'critical', 'triggered', 'dedup-lifecycle-1')",
    )
    .bind(incident_id)
    .bind(tenant)
    .execute(&db)
    .await
    .expect("incident inserted");

    let app = build_app(db.clone(), tenant);

    // 1. Acknowledge incident
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/acknowledge"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"actor":"alice","message":"Looking into it"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = response_body_json(res.into_body()).await;
    assert_eq!(body["status"], "acknowledged");
    assert_eq!(body["acknowledged_by"], "alice");
    assert!(body["acknowledged_at"].is_string());
    let timeline = body["timeline"].as_array().unwrap();
    assert!(
        timeline
            .iter()
            .any(|e| e["event_type"] == "acknowledged" && e["actor"] == "alice")
    );

    // 2. Add a comment
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/comments"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"actor":"bob","message":"Identified DB connection pool saturation"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = response_body_json(res.into_body()).await;
    let timeline = body["timeline"].as_array().unwrap();
    assert!(timeline.iter().any(|e| e["event_type"] == "comment"
        && e["actor"] == "bob"
        && e["message"] == "Identified DB connection pool saturation"));

    // 3. Link deployment
    let dep_id = Uuid::new_v4();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/link-deployment"))
                .header("Content-Type", "application/json")
                .body(Body::from(format!(
                    r#"{{"deployment_id":"{dep_id}","actor":"alice","message":"Correlated with rollout"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = response_body_json(res.into_body()).await;
    assert_eq!(body["related_deployment_id"], dep_id.to_string());
    let timeline = body["timeline"].as_array().unwrap();
    assert!(timeline.iter().any(
        |e| e["event_type"] == "deployment_linked" && e["message"] == "Correlated with rollout"
    ));

    // 4. Update status to investigating
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/status"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"investigating","actor":"bob"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = response_body_json(res.into_body()).await;
    assert_eq!(body["status"], "investigating");

    // 5. Resolve incident
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/resolve"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"actor":"alice","message":"Pool size bumped, recovered"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = response_body_json(res.into_body()).await;
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["resolved_by"], "alice");
    assert!(body["resolved_at"].is_string());

    // 6. Cross-tenant isolation verification
    let other_tenant = Uuid::new_v4();
    insert_tenant(&db, other_tenant).await;
    let other_app = build_app(db, other_tenant);
    let res = other_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/incidents/{incident_id}/acknowledge"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"actor":"mallory"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
