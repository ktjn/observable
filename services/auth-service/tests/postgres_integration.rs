use auth_service::lookup_api_key;
use sqlx::{PgPool, Row};
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn apply_migrations(pool: &PgPool) {
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
            .expect("migration applied");
    }
}

async fn start_pool() -> (
    PgPool,
    testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

#[tokio::test]
async fn postgres_container_applies_migrations_and_validates_seed_key() {
    let (pool, _container) = start_pool().await;

    // dev-api-key-0000 / tenant 00000000-0000-0000-0000-000000000002 (dev-tenant) / role member
    // Migration 017 moved dev keys from the observable tenant (...001) to dev-tenant (...002).
    let (tenant_id, role, _environment): (Uuid, String, String) =
        lookup_api_key(&pool, "dev-api-key-0000")
            .await
            .expect("seed key must validate");

    assert_eq!(
        tenant_id.to_string(),
        "00000000-0000-0000-0000-000000000002"
    );
    assert_eq!(role, "member");
}

#[tokio::test]
async fn lookup_api_key_returns_viewer_role_for_viewer_seed_key() {
    let (pool, _container) = start_pool().await;

    // dev-viewer-key-0000 / tenant 00000000-0000-0000-0000-000000000002 (dev-tenant) / role viewer
    // Migration 017 moved dev keys from the observable tenant (...001) to dev-tenant (...002).
    let (tenant_id, role, _environment): (Uuid, String, String) =
        lookup_api_key(&pool, "dev-viewer-key-0000")
            .await
            .expect("viewer seed key must validate");

    assert_eq!(
        tenant_id.to_string(),
        "00000000-0000-0000-0000-000000000002"
    );
    assert_eq!(role, "viewer");
}

#[tokio::test]
async fn lookup_api_key_rejects_unknown_key() {
    let (pool, _container) = start_pool().await;

    let result = lookup_api_key(&pool, "key-that-does-not-exist-in-db").await;

    assert!(result.is_err(), "unknown key must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not found"),
        "error must mention 'not found', got: {msg}"
    );
}

#[tokio::test]
async fn lookup_api_key_rejects_revoked_key() {
    let (pool, _container) = start_pool().await;

    // SHA-256("revoked-test-key") precomputed for the seed INSERT below.
    // sha256("revoked-test-key") = 8e4be2e7b61e64d642b8124a44143a9d64b8d7a2efcd4c51e0bdeee54a6c8a14
    let key = "revoked-test-key";
    let hash = auth_service::validate::sha256_hex(key);
    sqlx::query(
        "INSERT INTO api_keys (tenant_id, key_hash, name, role, revoked_at) \
         VALUES ('00000000-0000-0000-0000-000000000001', $1, 'revoked-key', 'member', now())",
    )
    .bind(&hash)
    .execute(&pool)
    .await
    .expect("revoked key inserted");

    let result = lookup_api_key(&pool, key).await;

    assert!(result.is_err(), "revoked key must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("revoked"),
        "error must mention 'revoked', got: {msg}"
    );
}

#[tokio::test]
async fn create_session_expires_in_7_days() {
    let (pool, _container) = start_pool().await;

    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, 'sub-session', 'session@example.com')")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("user inserted");

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Session Tenant')")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("tenant inserted");

    let session_id = auth_service::oidc::create_session(&pool, user_id, tenant_id, "prod")
        .await
        .expect("create_session");

    let row = sqlx::query("SELECT expires_at FROM user_sessions WHERE id = $1")
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .expect("session row");

    let expires_at: chrono::DateTime<chrono::Utc> = row.try_get("expires_at").unwrap();
    let now = chrono::Utc::now();
    let diff = expires_at - now;
    assert!(
        diff.num_days() >= 6 && diff.num_days() <= 8,
        "session must expire ~7 days from now, got {} days",
        diff.num_days()
    );
}

#[tokio::test]
async fn upsert_user_creates_and_deduplicates() {
    let (pool, _container) = start_pool().await;

    let tenant_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

    // First upsert — creates
    let user_id_1 = auth_service::oidc::upsert_user(
        &pool,
        "zitadel|user-1",
        "alice@example.com",
        Some("Alice"),
    )
    .await
    .expect("upsert 1");

    // Second upsert — updates, returns same ID
    let user_id_2 = auth_service::oidc::upsert_user(
        &pool,
        "zitadel|user-1",
        "alice-updated@example.com",
        Some("Alice Updated"),
    )
    .await
    .expect("upsert 2");

    assert_eq!(user_id_1, user_id_2, "same subject must return same UUID");

    // Assign role
    auth_service::oidc::upsert_user_tenant_role(&pool, user_id_1, tenant_id, "member")
        .await
        .expect("role assignment");

    // Verify role is stored
    let role: String = sqlx::query_scalar(
        "SELECT role FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(user_id_1)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("role row");

    assert_eq!(role, "member");
}
