use alert_evaluator::{AppState, readyz};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

fn test_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(readyz::readyz))
        .with_state(state)
}

fn disconnected_state(ch_url: &str) -> AppState {
    AppState {
        db: Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").expect("lazy pool")),
        ch: clickhouse::Client::default().with_url(ch_url),
    }
}

#[tokio::test]
async fn alert_evaluator_readyz_returns_503_when_clickhouse_unavailable() {
    let app = test_app(disconnected_state("http://127.0.0.1:1"));

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
async fn alert_evaluator_readyz_returns_200_when_dependencies_reachable() {
    use std::path::Path;
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::{clickhouse::ClickHouse, postgres::Postgres};

    let pg = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres started");
    let pg_port = pg.get_host_port_ipv4(5432).await.unwrap();
    let pg_url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");

    let ch = ClickHouse::default()
        .with_tag("25.3")
        .start()
        .await
        .expect("clickhouse started");
    let ch_port = ch.get_host_port_ipv4(8123).await.unwrap();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&pg_url)
        .await
        .expect("connect pg");

    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");
    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations dir exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(&sql)
            .execute(&pool)
            .await
            .expect("migration applied");
    }

    let state = AppState {
        db: Arc::new(pool),
        ch: clickhouse::Client::default().with_url(format!("http://127.0.0.1:{ch_port}")),
    };

    let response = test_app(state)
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
