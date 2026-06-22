use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use ingest_gateway::readyz::{IngestGatewayProbeState, readyz};
use std::sync::Arc;
use tower::ServiceExt;

fn test_probe_app(pg_url: &str) -> Router {
    let probe_state = IngestGatewayProbeState {
        db: Arc::new(sqlx::PgPool::connect_lazy(pg_url).expect("lazy pool")),
    };
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(readyz))
        .with_state(probe_state)
}

#[tokio::test]
async fn ingest_gateway_readyz_returns_503_when_postgres_unavailable() {
    let app = test_probe_app("postgres://localhost:1/nonexistent");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
#[ignore]
async fn ingest_gateway_readyz_returns_200_when_postgres_reachable() {
    use std::path::Path;
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::postgres::Postgres;

    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let pg_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = sqlx::PgPool::connect(&pg_url).await.expect("connect");
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");
    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("dir exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .expect("migration applied");
    }

    let probe_state = IngestGatewayProbeState { db: Arc::new(pool) };
    let app = Router::new()
        .route("/readyz", get(readyz))
        .with_state(probe_state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::OK);
}
