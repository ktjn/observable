use admin_service::{
    AdminServiceAppState, admin_members, middleware::auth::TenantContext, observability,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use clickhouse::Client as ChClient;
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{path::Path, sync::Arc};
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

// ── Dev credentials (must match seed data in migrations) ────────────────────
// Migration 017 moves dev-key to the dev-tenant at ...0002.
// Tenant ...0001 is the 'observable' self-ingestion tenant.

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

// ── Container helpers ────────────────────────────────────────────────────────

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
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .expect("pg migration applied");
    }
}

// ── Response helpers ─────────────────────────────────────────────────────────

async fn response_body_json(body: axum::body::Body) -> Value {
    let bytes = body.collect().await.expect("body collected").to_bytes();
    serde_json::from_slice(&bytes).expect("valid JSON")
}

// ── App builder ───────────────────────────────────────────────────────────────

fn build_admin_members_app(db: PgPool) -> (Router, Uuid, Uuid) {
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = AdminServiceAppState {
        ch,
        db: db.clone(),
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
    };
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let caller_id = Uuid::new_v4();
    let app = Router::new()
        .route("/v1/admin/members", get(admin_members::handle_list_members))
        .route("/v1/admin/members", post(admin_members::handle_add_member))
        .route(
            "/v1/admin/members/{user_id}/role",
            axum::routing::put(admin_members::handle_update_role),
        )
        .route(
            "/v1/admin/members/{user_id}",
            axum::routing::delete(admin_members::handle_remove_member),
        )
        .route(
            "/v1/admin/members/{user_id}/revoke-sessions",
            post(admin_members::handle_revoke_sessions),
        )
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(caller_id),
            role: "tenant_admin".into(),
        }))
        .layer(axum::Extension(db))
        .with_state(state);
    (app, tenant_id, caller_id)
}

// ── Seed helpers ──────────────────────────────────────────────────────────────

async fn seed_user(db: &PgPool, email: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    seed_user_with_id(db, user_id, email).await;
    user_id
}

async fn seed_user_with_id(db: &PgPool, user_id: Uuid, email: &str) {
    sqlx::query("INSERT INTO users (id, idp_subject, email, name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind(format!("idp|{email}"))
        .bind(email)
        .bind(email.split('@').next().unwrap_or("user"))
        .execute(db)
        .await
        .expect("user inserted");
}

async fn seed_member(db: &PgPool, user_id: Uuid, tenant_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(tenant_id)
        .bind(role)
        .execute(db)
        .await
        .expect("member inserted");
}

#[tokio::test]
async fn list_members_returns_tenant_members() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());

    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/members")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_body_json(resp.into_body()).await;
    let members = body["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|m| m["email"] == "bob@example.com"));
    assert!(members.iter().any(|m| m["role"] == "member"));
}

#[tokio::test]
async fn add_member_by_email_succeeds_for_known_user() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    // A user exists in the `users` table but is NOT yet a member of this tenant.
    seed_user(&pg, "newuser@example.com").await;

    let body = serde_json::json!({ "email": "newuser@example.com", "role": "member" });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/members")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = response_body_json(resp.into_body()).await;
    assert_eq!(body["email"], "newuser@example.com");
    assert_eq!(body["role"], "member");
}

#[tokio::test]
async fn add_member_returns_404_for_unknown_email() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    let body = serde_json::json!({ "email": "nobody@example.com", "role": "member" });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/admin/members")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_role_changes_member_role() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    let body = serde_json::json!({ "role": "viewer" });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/members/{bob_id}/role"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let role: String = sqlx::query_scalar(
        "SELECT role FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(bob_id)
    .bind(tenant_id)
    .fetch_one(&pg)
    .await
    .unwrap();
    assert_eq!(role, "viewer");
}

#[tokio::test]
async fn update_role_returns_403_for_self() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;

    let body = serde_json::json!({ "role": "member" });
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/v1/admin/members/{caller_id}/role"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn remove_member_deletes_row_and_revokes_sessions() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    // Seed a session for bob.
    sqlx::query(
        "INSERT INTO user_sessions (user_id, tenant_id, environment, issued_at, expires_at) \
         VALUES ($1, $2, 'prod', now(), now() + interval '1 hour')",
    )
    .bind(bob_id)
    .bind(tenant_id)
    .execute(&pg)
    .await
    .unwrap();

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/admin/members/{bob_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Membership row removed.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(bob_id)
    .bind(tenant_id)
    .fetch_one(&pg)
    .await
    .unwrap();
    assert_eq!(count, 0);

    // Session revoked.
    let revoked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(bob_id)
    .fetch_one(&pg)
    .await
    .unwrap();
    assert_eq!(revoked, 1);
}

#[tokio::test]
async fn remove_last_admin_returns_403() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    // Make bob the only admin, demote caller to member.
    sqlx::query("UPDATE user_tenant_roles SET role = 'tenant_admin' WHERE user_id = $1")
        .bind(bob_id)
        .execute(&pg)
        .await
        .unwrap();
    sqlx::query("UPDATE user_tenant_roles SET role = 'member' WHERE user_id = $1")
        .bind(caller_id)
        .execute(&pg)
        .await
        .unwrap();

    // Try to remove bob (the only admin). Should return 403.
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/v1/admin/members/{bob_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn revoke_sessions_marks_all_active_sessions_revoked() {
    let (pg, _pg_container) = start_postgres().await;
    let (app, tenant_id, caller_id) = build_admin_members_app(pg.clone());
    seed_user_with_id(&pg, caller_id, "admin@example.com").await;
    seed_member(&pg, caller_id, tenant_id, "tenant_admin").await;
    let bob_id = seed_user(&pg, "bob@example.com").await;
    seed_member(&pg, bob_id, tenant_id, "member").await;

    // Two active sessions for bob.
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO user_sessions (user_id, tenant_id, environment, issued_at, expires_at) \
             VALUES ($1, $2, 'prod', now(), now() + interval '1 hour')",
        )
        .bind(bob_id)
        .bind(tenant_id)
        .execute(&pg)
        .await
        .unwrap();
    }

    let req = Request::builder()
        .method("POST")
        .uri(format!("/v1/admin/members/{bob_id}/revoke-sessions"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let revoked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(bob_id)
    .fetch_one(&pg)
    .await
    .unwrap();
    assert_eq!(revoked, 2);
}

#[tokio::test]
async fn admin_members_returns_403_for_non_admin() {
    let (pg, _pg_container) = start_postgres().await;
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID).unwrap();
    let caller_id = Uuid::new_v4();
    let ch = ChClient::default().with_url("http://127.0.0.1:19999");
    let state = AdminServiceAppState {
        ch,
        db: pg.clone(),
        auth_service_url: "http://auth-service:4319".into(),
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
    };
    let app = Router::new()
        .route("/v1/admin/members", get(admin_members::handle_list_members))
        .layer(axum::Extension(TenantContext {
            tenant_id,
            user_id: Some(caller_id),
            role: "member".into(), // non-admin
        }))
        .layer(axum::Extension(pg))
        .with_state(state);

    let req = Request::builder()
        .method("GET")
        .uri("/v1/admin/members")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
