use query_api::dashboards::{
    create_dashboard, export_dashboard, get_dashboard, import_dashboard, list_dashboards,
    update_dashboard, CreateDashboardRequest, DashboardExport, DashboardExportPanel,
    DashboardPanelRequest, UpdateDashboardRequest,
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
                query_kind: Some("logs".into()),
                service: Some("checkout".into()),
                preset: Some("1h".into()),
                filters: serde_json::json!({"facets":["service_name","severity_number"]}),
                ..Default::default()
            }],
        },
    )
    .await
    .unwrap();

    assert_eq!(created.name, "Promoted log query");
    assert_eq!(created.panels.len(), 1);
    assert_eq!(created.panels[0].query_kind.as_deref(), Some("logs"));
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
                query_kind: Some("traces".into()),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                ..Default::default()
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

#[tokio::test]
async fn export_dashboard_round_trips_panels() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;

    let created = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Export test".into(),
            panels: vec![DashboardPanelRequest {
                title: "Error logs".into(),
                query_kind: Some("logs".into()),
                service: Some("checkout".into()),
                preset: Some("1h".into()),
                filters: serde_json::json!({"facets":["severity_number"]}),
                ..Default::default()
            }],
        },
    )
    .await
    .unwrap();

    let export = export_dashboard(&pool, tenant, created.dashboard_id)
        .await
        .unwrap()
        .expect("export must be found");

    assert_eq!(export.schema_version, "2");
    assert_eq!(export.name, "Export test");
    assert_eq!(export.panels.len(), 1);
    assert_eq!(export.panels[0].title, "Error logs");
    assert_eq!(export.panels[0].query_kind.as_deref(), Some("logs"));
    assert_eq!(export.panels[0].service.as_deref(), Some("checkout"));
    assert_eq!(export.panels[0].preset.as_deref(), Some("1h"));
    assert_eq!(export.panels[0].filters["facets"][0], "severity_number");
}

#[tokio::test]
async fn import_creates_new_dashboard_from_export() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;

    let export = DashboardExport {
        schema_version: "1".into(),
        name: "Imported dashboard".into(),
        panels: vec![DashboardExportPanel {
            title: "Trace search".into(),
            panel_kind: None,
            query_kind: Some("traces".into()),
            service: None,
            preset: Some("3h".into()),
            filters: serde_json::json!({}),
            query_text: None,
            content: None,
            layout: None,
            time_range: None,
        }],
    };

    let imported = import_dashboard(&pool, tenant, &export).await.unwrap();

    assert_eq!(imported.name, "Imported dashboard");
    assert_eq!(imported.panels.len(), 1);
    assert_eq!(imported.panels[0].title, "Trace search");
    assert_eq!(imported.panels[0].query_kind.as_deref(), Some("traces"));
    assert_eq!(imported.panels[0].preset.as_deref(), Some("3h"));

    let all = list_dashboards(&pool, tenant).await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn export_returns_none_for_wrong_tenant() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    insert_tenant(&pool, tenant_a).await;
    insert_tenant(&pool, tenant_b).await;

    let created = create_dashboard(
        &pool,
        tenant_a,
        &CreateDashboardRequest {
            name: "Tenant A dashboard".into(),
            panels: vec![DashboardPanelRequest {
                title: "Logs".into(),
                query_kind: Some("logs".into()),
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                ..Default::default()
            }],
        },
    )
    .await
    .unwrap();

    let result = export_dashboard(&pool, tenant_b, created.dashboard_id)
        .await
        .unwrap();

    assert!(
        result.is_none(),
        "tenant B must not export tenant A dashboard"
    );
}

#[tokio::test]
async fn dashboard_v2_persists_query_layout_time_override_and_text_panels() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;

    let created = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Runtime dashboard".into(),
            panels: vec![
                DashboardPanelRequest {
                    panel_id: None,
                    title: "Latency".into(),
                    panel_kind: Some("query".into()),
                    query_kind: Some("metrics".into()),
                    service: Some("checkout".into()),
                    preset: None,
                    filters: serde_json::json!({}),
                    query_text: Some("p95 latency for checkout".into()),
                    content: None,
                    layout: Some(serde_json::json!({"x":0,"y":0,"w":6,"h":4})),
                    time_range: Some(serde_json::json!({"mode":"preset","preset":"3h"})),
                },
                DashboardPanelRequest {
                    panel_id: None,
                    title: "Context".into(),
                    panel_kind: Some("text".into()),
                    query_kind: None,
                    service: None,
                    preset: None,
                    filters: serde_json::json!({}),
                    query_text: None,
                    content: Some("Watch deploy windows before paging.".into()),
                    layout: Some(serde_json::json!({"x":6,"y":0,"w":6,"h":2})),
                    time_range: None,
                },
            ],
        },
    )
    .await
    .unwrap();

    let fetched = get_dashboard(&pool, tenant, created.dashboard_id)
        .await
        .unwrap()
        .expect("dashboard found");

    assert_eq!(fetched.panels.len(), 2);
    assert_eq!(fetched.panels[0].panel_kind, "query");
    assert_eq!(fetched.panels[0].query_kind.as_deref(), Some("metrics"));
    assert_eq!(
        fetched.panels[0].query_text.as_deref(),
        Some("p95 latency for checkout")
    );
    assert_eq!(fetched.panels[0].layout["w"], 6);
    assert_eq!(fetched.panels[0].time_range["mode"], "preset");
    assert_eq!(fetched.panels[1].panel_kind, "text");
    assert!(fetched.panels[1].query_kind.is_none());
    assert_eq!(
        fetched.panels[1].content.as_deref(),
        Some("Watch deploy windows before paging.")
    );
}

#[tokio::test]
async fn update_dashboard_replaces_panel_layout_and_content() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;

    let created = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Original".into(),
            panels: vec![DashboardPanelRequest {
                panel_id: None,
                title: "Notes".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                query_text: None,
                content: Some("Old text".into()),
                layout: Some(serde_json::json!({"x":0,"y":0,"w":4,"h":2})),
                time_range: None,
            }],
        },
    )
    .await
    .unwrap();

    let updated = update_dashboard(
        &pool,
        tenant,
        created.dashboard_id,
        &UpdateDashboardRequest {
            name: "Updated".into(),
            panels: vec![DashboardPanelRequest {
                panel_id: None,
                title: "Notes".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                query_text: None,
                content: Some("New text".into()),
                layout: Some(serde_json::json!({"x":0,"y":0,"w":8,"h":3})),
                time_range: None,
            }],
        },
    )
    .await
    .unwrap()
    .expect("dashboard updated");

    assert_eq!(updated.name, "Updated");
    assert_eq!(updated.panels[0].content.as_deref(), Some("New text"));
    assert_eq!(updated.panels[0].layout["w"], 8);
    assert_eq!(updated.panels[0].layout["h"], 3);
}

#[tokio::test]
async fn v2_export_round_trips_text_and_query_panels() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;

    let export = DashboardExport {
        schema_version: "2".into(),
        name: "Imported v2".into(),
        panels: vec![
            DashboardExportPanel {
                title: "Errors".into(),
                panel_kind: Some("query".into()),
                query_kind: Some("logs".into()),
                service: Some("checkout".into()),
                preset: None,
                filters: serde_json::json!({}),
                query_text: Some("errors in checkout".into()),
                content: None,
                layout: Some(serde_json::json!({"x":0,"y":0,"w":6,"h":4})),
                time_range: Some(serde_json::json!({"mode":"global"})),
            },
            DashboardExportPanel {
                title: "Runbook".into(),
                panel_kind: Some("text".into()),
                query_kind: None,
                service: None,
                preset: None,
                filters: serde_json::json!({}),
                query_text: None,
                content: Some("Escalate only after 10 minutes.".into()),
                layout: Some(serde_json::json!({"x":6,"y":0,"w":6,"h":2})),
                time_range: None,
            },
        ],
    };

    let imported = import_dashboard(&pool, tenant, &export).await.unwrap();
    let exported = export_dashboard(&pool, tenant, imported.dashboard_id)
        .await
        .unwrap()
        .expect("export found");

    assert_eq!(exported.schema_version, "2");
    assert_eq!(exported.panels[0].panel_kind.as_deref(), Some("query"));
    assert_eq!(
        exported.panels[0].query_text.as_deref(),
        Some("errors in checkout")
    );
    assert_eq!(exported.panels[1].panel_kind.as_deref(), Some("text"));
    assert_eq!(
        exported.panels[1].content.as_deref(),
        Some("Escalate only after 10 minutes.")
    );
}
