use query_api::dashboards::{
    create_dashboard, list_dashboards, CreateDashboardRequest, DashboardPanelRequest,
};
use sqlx::PgPool;
use std::path::Path;
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
        .with_tag("16")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(tenant_id)
        .bind(format!("tenant-{tenant_id}"))
        .execute(pool)
        .await
        .expect("tenant inserted");
}

#[tokio::test]
async fn create_dashboard_preserves_promoted_panel_filters() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;

    let created = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Promoted log query".into(),
            panels: vec![DashboardPanelRequest {
                title: "Logs for checkout".into(),
                query_kind: "logs".into(),
                service: Some("checkout".into()),
                preset: Some("1h".into()),
                filters: serde_json::json!({"facets":["service_name","severity_number"]}),
            }],
        },
    )
    .await
    .unwrap();

    assert_eq!(created.name, "Promoted log query");
    assert_eq!(created.panels.len(), 1);
    assert_eq!(created.panels[0].query_kind, "logs");
    assert_eq!(created.panels[0].service.as_deref(), Some("checkout"));
    assert_eq!(created.panels[0].preset.as_deref(), Some("1h"));

    let dashboards = list_dashboards(&pool, tenant).await.unwrap();
    assert_eq!(dashboards.len(), 1);
    assert_eq!(dashboards[0].dashboard_id, created.dashboard_id);
    assert_eq!(dashboards[0].panels[0].filters["facets"][0], "service_name");
}

#[tokio::test]
async fn list_dashboards_does_not_return_other_tenant_dashboards() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    insert_tenant(&pool, tenant_a).await;
    insert_tenant(&pool, tenant_b).await;

    create_dashboard(
        &pool,
        tenant_a,
        &CreateDashboardRequest {
            name: "Tenant A dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "Trace search".into(),
                query_kind: "traces".into(),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
            }],
        },
    )
    .await
    .unwrap();

    let tenant_b_dashboards = list_dashboards(&pool, tenant_b).await.unwrap();
    assert!(
        tenant_b_dashboards.is_empty(),
        "tenant B must not see tenant A dashboards"
    );
}
