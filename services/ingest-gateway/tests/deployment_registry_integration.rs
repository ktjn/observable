use ingest_gateway::deployment_registry::DeploymentRegistry;
use sqlx::PgPool;
use std::path::Path;
use std::sync::Arc;
use testcontainers::{runners::AsyncRunner, ImageExt};
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
async fn lookup_resolves_active_deployment_by_service_and_version() {
    let (pool, _container) = start_pool().await;
    let db = Arc::new(pool);
    let registry = DeploymentRegistry::new(db.clone());

    let tenant_id = Uuid::new_v4();
    let deployment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'api', 'prod', 'v2.0.0', 'in_progress', now()) \
         RETURNING deployment_id",
    )
    .bind(tenant_id)
    .fetch_one(db.as_ref())
    .await
    .expect("insert deployment marker");

    let result = registry.lookup(tenant_id, "api", "prod", "v2.0.0").await;
    assert_eq!(result, deployment_id.to_string());
}

#[tokio::test]
async fn lookup_matches_success_status() {
    let (pool, _container) = start_pool().await;
    let db = Arc::new(pool);
    let registry = DeploymentRegistry::new(db.clone());

    let tenant_id = Uuid::new_v4();
    let deployment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'worker', 'staging', 'v1.5.0', 'success', now()) \
         RETURNING deployment_id",
    )
    .bind(tenant_id)
    .fetch_one(db.as_ref())
    .await
    .expect("insert deployment marker");

    let result = registry
        .lookup(tenant_id, "worker", "staging", "v1.5.0")
        .await;
    assert_eq!(result, deployment_id.to_string());
}

#[tokio::test]
async fn lookup_ignores_failed_and_rolled_back_deployments() {
    let (pool, _container) = start_pool().await;
    let db = Arc::new(pool);
    let registry = DeploymentRegistry::new(db.clone());

    let tenant_id = Uuid::new_v4();
    for status in ["failed", "rolled_back"] {
        sqlx::query(
            "INSERT INTO deployment_markers \
             (tenant_id, service_name, environment, service_version, status, started_at) \
             VALUES ($1, 'svc', 'prod', 'v3.0.0', $2, now())",
        )
        .bind(tenant_id)
        .bind(status)
        .execute(db.as_ref())
        .await
        .expect("insert");
    }

    let result = registry.lookup(tenant_id, "svc", "prod", "v3.0.0").await;
    assert_eq!(
        result, "",
        "failed/rolled_back deployments must not be stamped"
    );
}

#[tokio::test]
async fn lookup_empty_version_matches_latest_active() {
    let (pool, _container) = start_pool().await;
    let db = Arc::new(pool);
    let registry = DeploymentRegistry::new(db.clone());

    let tenant_id = Uuid::new_v4();
    let deployment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'frontend', 'staging', 'v4.1.0', 'in_progress', now()) \
         RETURNING deployment_id",
    )
    .bind(tenant_id)
    .fetch_one(db.as_ref())
    .await
    .expect("insert deployment marker");

    let result = registry.lookup(tenant_id, "frontend", "staging", "").await;
    assert_eq!(result, deployment_id.to_string());
}

#[tokio::test]
async fn lookup_is_tenant_scoped() {
    let (pool, _container) = start_pool().await;
    let db = Arc::new(pool);
    let registry = DeploymentRegistry::new(db.clone());

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'svc', 'prod', 'v1.0.0', 'in_progress', now())",
    )
    .bind(tenant_a)
    .execute(db.as_ref())
    .await
    .expect("insert for tenant_a");

    let result = registry.lookup(tenant_b, "svc", "prod", "v1.0.0").await;
    assert_eq!(result, "", "lookup must not cross tenant boundaries");
}

#[tokio::test]
async fn lookup_returns_most_recent_when_multiple_match() {
    let (pool, _container) = start_pool().await;
    let db = Arc::new(pool);
    let registry = DeploymentRegistry::new(db.clone());

    let tenant_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'api', 'prod', 'v1.0.0', 'success', now() - interval '1 hour')",
    )
    .bind(tenant_id)
    .execute(db.as_ref())
    .await
    .expect("insert old");

    let newest_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_markers \
         (tenant_id, service_name, environment, service_version, status, started_at) \
         VALUES ($1, 'api', 'prod', 'v1.0.0', 'in_progress', now()) \
         RETURNING deployment_id",
    )
    .bind(tenant_id)
    .fetch_one(db.as_ref())
    .await
    .expect("insert new");

    let result = registry.lookup(tenant_id, "api", "prod", "v1.0.0").await;
    assert_eq!(
        result,
        newest_id.to_string(),
        "must return most recent deployment"
    );
}
