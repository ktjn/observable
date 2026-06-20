//! Cross-service integration test: an API-key-authenticated query-api
//! request must produce a `credential_audit_log` row in auth-service's
//! audit trail (the gap closed by routing query-api's API-key path through
//! auth-service's `/internal/validate` instead of querying Postgres directly).
//!
//! This stands up a real, in-process auth-service router (mirroring the
//! `/internal/validate` route wired in `auth-service/src/main.rs`, since
//! `main.rs` isn't part of the `auth_service` lib crate) bound to a real
//! TCP port, backed by the same Postgres instance query-api uses. This is
//! the lightest-weight way to exercise the real audit-write path without
//! introducing new test infrastructure (no extra containers needed beyond
//! the Postgres one this test suite already uses).

use auth_service::{audit, lookup_api_key, validate};
use axum::{
    Json, Router,
    extract::State,
    http::{Request, StatusCode, header},
    middleware as axum_middleware,
    routing::{get, post},
};
use clickhouse::Client as ChClient;
use http_body_util::BodyExt as _;
use query_api::{middleware::auth::require_tenant, planner::QueryPlanner, traces};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use tracing::Instrument as _;
use uuid::Uuid;

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";
const DEV_API_KEY: &str = "dev-api-key-0000";

async fn start_postgres() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
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
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("pg migration applied");
    }
}

// ── Minimal real auth-service, mirroring main.rs's /internal/validate ───────
//
// main.rs is a binary entrypoint (not part of the `auth_service` lib), so its
// `validate_handler` can't be imported directly. This replicates it exactly,
// using the same `lookup_api_key` + `audit::write` calls main.rs uses, so the
// audit-log behavior under test is the real production code path.

#[derive(Deserialize)]
struct ValidateRequest {
    api_key: String,
}

#[derive(Serialize)]
struct ValidateResponse {
    tenant_id: Uuid,
    role: String,
    environment: String,
}

#[derive(Clone)]
struct AuthState {
    db: PgPool,
}

async fn validate_handler(
    State(state): State<AuthState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let hash = validate::sha256_hex(&req.api_key);
    async move {
        match lookup_api_key(&state.db, &req.api_key).await {
            Ok((tenant_id, role, environment)) => {
                audit::write(&state.db, &audit::AuditEntry::allow(hash, tenant_id)).await;
                Ok(Json(ValidateResponse {
                    tenant_id,
                    role,
                    environment,
                }))
            }
            Err(e) => {
                let reason = if e.to_string().contains("revoked") {
                    "revoked"
                } else if e.to_string().contains("not found") {
                    audit::write(&state.db, &audit::AuditEntry::deny_not_found(hash)).await;
                    return Err(StatusCode::UNAUTHORIZED);
                } else {
                    "hash_mismatch"
                };
                audit::write(
                    &state.db,
                    &audit::AuditEntry::deny(hash, Uuid::nil(), reason),
                )
                .await;
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
    .instrument(tracing::info_span!("auth.validate"))
    .await
}

/// Spin up a real auth-service `/internal/validate` router on an actual TCP
/// port (axum routers built purely with `oneshot` never make a real network
/// hop, but query-api's middleware now performs a genuine `reqwest` POST, so
/// the test double needs a bindable address, not just an in-memory `Router`).
async fn start_real_auth_service(db: PgPool) -> String {
    let app = Router::new().route("/internal/validate", post(validate_handler));
    let app = app.with_state(AuthState { db });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("auth-service test listener bound");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("auth-service test server");
    });

    format!("http://{addr}")
}

// ── query-api app builder ────────────────────────────────────────────────────

fn build_app(db: PgPool, auth_service_url: String) -> Router {
    let state = traces::AppState {
        ch: ChClient::default().with_url("http://127.0.0.1:19999"),
        db: db.clone(),
        planner: Arc::new(QueryPlanner),
        llm: None,
        auth_service_url: auth_service_url.clone(),
        metrics: Arc::new(query_api::observability::QueryApiMetrics::new()),
    };
    Router::new()
        .route("/v1/traces/histogram", get(|| async { StatusCode::OK }))
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(db))
        .layer(axum::Extension(Arc::new(auth_service_url)))
        .with_state(state)
}

async fn body_to_bytes(body: axum::body::Body) -> Vec<u8> {
    body.collect()
        .await
        .expect("body collected")
        .to_bytes()
        .to_vec()
}

#[tokio::test]
async fn api_key_auth_through_query_api_writes_audit_log_row() {
    let (db, _pg) = start_postgres().await;
    let auth_service_url = start_real_auth_service(db.clone()).await;
    let app = build_app(db.clone(), auth_service_url);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram")
        .header(header::AUTHORIZATION, format!("Bearer {DEV_API_KEY}"))
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let _ = body_to_bytes(resp.into_body()).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "expected auth to pass and reach the handler"
    );

    let hash = auth_service::validate::sha256_hex(DEV_API_KEY);
    let row: (String, String, String, Option<Uuid>) = sqlx::query_as(
        "SELECT action, outcome, credential_hash, tenant_id FROM credential_audit_log \
         WHERE credential_hash = $1 ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(&hash)
    .fetch_one(&db)
    .await
    .expect("credential_audit_log row written for the API-key request");

    let (action, outcome, credential_hash, tenant_id) = row;
    assert_eq!(action, "credential_validate");
    assert_eq!(outcome, "allow");
    assert_eq!(credential_hash, hash);
    assert_eq!(tenant_id, Some(Uuid::parse_str(DEV_TENANT_ID).unwrap()));
}

#[tokio::test]
async fn api_key_auth_failure_through_query_api_writes_deny_audit_log_row() {
    let (db, _pg) = start_postgres().await;
    let auth_service_url = start_real_auth_service(db.clone()).await;
    let app = build_app(db.clone(), auth_service_url);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/traces/histogram")
        .header(header::AUTHORIZATION, "Bearer not-a-real-key")
        .header("X-Tenant-ID", DEV_TENANT_ID)
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let hash = auth_service::validate::sha256_hex("not-a-real-key");
    let row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT action, outcome, denial_reason FROM credential_audit_log \
         WHERE credential_hash = $1 ORDER BY occurred_at DESC LIMIT 1",
    )
    .bind(&hash)
    .fetch_one(&db)
    .await
    .expect("credential_audit_log deny row written for the invalid API-key request");

    let (action, outcome, denial_reason) = row;
    assert_eq!(action, "credential_validate");
    assert_eq!(outcome, "deny");
    assert_eq!(denial_reason, Some("not_found".to_string()));
}
