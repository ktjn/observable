// HTTP integration test for the saved-views routes against a real Postgres
// instance via Testcontainers, exercising the full handler path via
// tower::ServiceExt::oneshot.

use admin_service::{
    AdminServiceAppState, middleware::auth::TenantContext, observability, saved_views,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{delete, get},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
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

// saved_views.owner_user_id and saved_view_grants.user_id both carry FK
// references to users(id), so grant-touching tests need a real row there.
async fn insert_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("sub-{user_id}"))
        .bind(format!("{user_id}@test.com"))
        .execute(pool)
        .await
        .expect("user inserted");
}

fn build_app(db: PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) -> Router {
    let state = AdminServiceAppState {
        db: db.clone(),
        ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
        auth_service_url: "http://auth-service:4319".into(),
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
        producer: None,
    };
    Router::new()
        .route(
            "/v1/saved-views",
            get(saved_views::handle_list_saved_views).post(saved_views::handle_create_saved_view),
        )
        .route(
            "/v1/saved-views/{id}",
            get(saved_views::handle_get_saved_view)
                .put(saved_views::handle_update_saved_view)
                .delete(saved_views::handle_delete_saved_view),
        )
        .route(
            "/v1/saved-views/{id}/grants",
            get(saved_views::handle_list_saved_view_grants)
                .post(saved_views::handle_add_saved_view_grant),
        )
        .route(
            "/v1/saved-views/{id}/grants/{user_id}",
            delete(saved_views::handle_revoke_saved_view_grant),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(user_id),
            role: role.into(),
        }))
        .with_state(state)
}

#[tokio::test]
async fn create_then_list_then_get_round_trips_config() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;
    insert_user(&db, user_id).await;
    let app = build_app(db, tenant_id, user_id, "member");

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/saved-views")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "Errors in checkout",
                        "signal_kind": "logs",
                        "config": {"severity_filter": "error", "visible_columns": ["level", "service"]}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = response_body_json(create_resp.into_body()).await;
    let saved_view_id = created["saved_view_id"].as_str().unwrap().to_string();

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/saved-views?signal_kind=logs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = response_body_json(list_resp.into_body()).await;
    let items = list_body["items"].as_array().expect("items array");
    assert!(items.iter().any(|i| i["saved_view_id"] == saved_view_id));

    let get_resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/saved-views/{saved_view_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body = response_body_json(get_resp.into_body()).await;
    assert_eq!(get_body["name"], "Errors in checkout");
    assert_eq!(get_body["config"]["severity_filter"], "error");
}

#[tokio::test]
async fn private_view_hidden_from_non_owner_then_visible_after_grant() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let other_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;
    insert_user(&db, owner_id).await;
    insert_user(&db, other_id).await;

    let owner_app = build_app(db.clone(), tenant_id, owner_id, "member");
    let create_resp = owner_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/saved-views")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"name": "Private view", "signal_kind": "logs", "config": {}})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = response_body_json(create_resp.into_body()).await;
    let saved_view_id = created["saved_view_id"].as_str().unwrap().to_string();
    // Newly-created views default to private visibility.
    assert_eq!(created["visibility"], "private");

    let other_app = build_app(db.clone(), tenant_id, other_id, "member");
    let forbidden_resp = other_app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/saved-views/{saved_view_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden_resp.status(), StatusCode::FORBIDDEN);

    // Grant the other user viewer access via the owner's session.
    sqlx::query(
        "INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, 'member')",
    )
    .bind(other_id)
    .bind(tenant_id)
    .execute(&db)
    .await
    .unwrap();
    let owner_app = build_app(db.clone(), tenant_id, owner_id, "member");
    let grant_resp = owner_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/saved-views/{saved_view_id}/grants"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"user_id": other_id, "relation": "viewer"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(grant_resp.status(), StatusCode::NO_CONTENT);

    let other_app = build_app(db, tenant_id, other_id, "member");
    let now_visible_resp = other_app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/saved-views/{saved_view_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(now_visible_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn update_and_delete_require_write_and_delete_grants() {
    let (db, _container) = start_postgres().await;
    let tenant_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    insert_tenant(&db, tenant_id).await;
    insert_user(&db, owner_id).await;

    let app = build_app(db.clone(), tenant_id, owner_id, "member");
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/saved-views")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"name": "View", "signal_kind": "logs", "config": {}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let created = response_body_json(create_resp.into_body()).await;
    let saved_view_id = created["saved_view_id"].as_str().unwrap().to_string();

    let update_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/saved-views/{saved_view_id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"name": "Renamed", "config": {}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);
    let updated = response_body_json(update_resp.into_body()).await;
    assert_eq!(updated["name"], "Renamed");

    let delete_resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/saved-views/{saved_view_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);
}
