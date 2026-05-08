// HTTP integration tests for the Simple IR Shorthand handler paths (ADR-029).
//
// Coverage intent:
//   1. '/' prefix → shorthand bypass → handler returns IR without touching LLM or DB
//   2. '/' prefix + base_ir → shorthand merged with page context, returned as IR
//   3. No LLM configured in DB → graceful shorthand fallback → returns IR
//
// Tests 1 and 2 use lazy/fake Postgres + fake ClickHouse — neither is reached before
// the early-return in interpret mode. Test 3 needs a real Postgres container so the
// DB config lookup (fetch_db_key, fetch_db_value) can return "not configured".

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    response::Response,
    routing::post,
    Router,
};
use clickhouse::Client as ChClient;
use http_body_util::BodyExt;
use query_api::{llm_adapter, middleware::auth::TenantContext, planner::QueryPlanner, traces};
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::{path::Path, sync::Arc};
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Test-only middleware: reads X-Tenant-ID and injects TenantContext directly,
/// bypassing API-key auth. These tests cover NLQ shorthand logic, not auth.
async fn inject_tenant_ctx(mut req: Request<Body>, next: axum_middleware::Next) -> Response {
    let tenant_id: Uuid = req
        .headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();
    req.extensions_mut().insert(TenantContext {
        tenant_id,
        role: "member".into(),
    });
    next.run(req).await
}

fn build_nlq_app(ch: ChClient, db: PgPool) -> Router {
    let state = traces::AppState {
        ch,
        db,
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: "http://auth-service:4319".into(),
    };
    Router::new()
        .route("/v1/nlq", post(llm_adapter::handle_nlq_query))
        .layer(axum_middleware::from_fn(inject_tenant_ctx))
        .with_state(state)
}

fn fake_ch() -> ChClient {
    ChClient::default().with_url("http://127.0.0.1:19999")
}

fn fake_db() -> PgPool {
    PgPool::connect_lazy("postgres://x:x@127.0.0.1:5432/x").expect("valid url")
}

fn nlq_post(tenant_id: Uuid, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/nlq")
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_json(body: axum::body::Body) -> Value {
    let bytes = body.collect().await.expect("body").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let host = container.get_host().await.expect("host");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_pg_migrations(&pool).await;
    (pool, container)
}

async fn apply_pg_migrations(pool: &PgPool) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("migration applied");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// '/' prefix bypasses the LLM entirely. Interpret mode returns the parsed IR
/// without hitting ClickHouse or Postgres — neither is needed in this path.
#[tokio::test]
async fn nlq_slash_prefix_interpret_returns_shorthand_ir() {
    let app = build_nlq_app(fake_ch(), fake_db());
    let tenant = Uuid::new_v4();

    let resp = app
        .oneshot(nlq_post(
            tenant,
            r#"{"question":"/service:checkout error","mode":"interpret"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(
        json["type"], "ir",
        "slash prefix must return ir discriminant"
    );
    let filters = json["ir"]["filters"].as_array().unwrap();
    assert!(
        filters
            .iter()
            .any(|f| f["field"] == "service" && f["value"] == "checkout"),
        "service:checkout must appear as a filter"
    );
    assert_eq!(
        json["ir"]["query"], "error",
        "unquoted freetext must become query field"
    );
}

/// '/' prefix merges shorthand tokens on top of the page's base_ir. Base filters
/// that are not overridden are preserved; shorthand filters replace matching fields.
#[tokio::test]
async fn nlq_slash_prefix_with_base_ir_merges_shorthand_filters() {
    let app = build_nlq_app(fake_ch(), fake_db());
    let tenant = Uuid::new_v4();

    let body = r#"{
        "question": "/env:prod",
        "base_ir": {
            "operation": "timeseries",
            "signals": ["logs"],
            "filters": [{"field": "service", "op": "=", "value": "checkout"}],
            "group_by": [],
            "time_range": {"from": "now-1h", "to": "now"}
        },
        "mode": "interpret"
    }"#;

    let resp = app.oneshot(nlq_post(tenant, body)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["type"], "ir");
    let ir = &json["ir"];
    let filters = ir["filters"].as_array().unwrap();
    assert!(
        filters
            .iter()
            .any(|f| f["field"] == "service" && f["value"] == "checkout"),
        "base_ir service filter must be preserved"
    );
    assert!(
        filters
            .iter()
            .any(|f| f["field"] == "env" && f["value"] == "prod"),
        "shorthand env filter must be added"
    );
    assert_eq!(
        ir["time_range"]["from"], "now-1h",
        "base_ir time_range must be preserved"
    );
}

/// When no LLM is configured (neither env key nor DB config), the handler falls
/// back to the shorthand parser rather than returning 503. A real Postgres
/// container is required so the DB config lookup can legitimately find nothing.
#[tokio::test]
async fn nlq_no_llm_configured_falls_back_to_shorthand_interpret() {
    let (db, _container) = start_postgres().await;
    let app = build_nlq_app(fake_ch(), db);
    let tenant = Uuid::new_v4();

    // Plain text query (no '/' prefix) with empty DB config — shorthand fallback.
    let resp = app
        .oneshot(nlq_post(
            tenant,
            r#"{"question":"service:checkout","mode":"interpret"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "unconfigured LLM must not return 503 — shorthand fallback must engage"
    );
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["type"], "ir");
    let filters = json["ir"]["filters"].as_array().unwrap();
    assert!(
        filters
            .iter()
            .any(|f| f["field"] == "service" && f["value"] == "checkout"),
        "shorthand filter must be parsed from plain-text input"
    );
}
