use query_api::schemas::{
    delete_annotation, get_annotation, list_schema_attributes, patch_annotation, upsert_annotation,
    PatchAnnotationRequest, UpsertAnnotationRequest,
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

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
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

// ── schema_entries: structural catalog ───────────────────────────────────────

#[tokio::test]
async fn list_attributes_returns_seeded_metrics_entry() {
    let (pool, _container) = start_pool().await;

    let entries = list_schema_attributes(&pool, "metrics").await.unwrap();

    assert!(
        entries
            .iter()
            .any(|e| e.field_name == "request_duration_ms"),
        "seeded 'request_duration_ms' must appear in metrics attributes"
    );
    let entry = entries
        .iter()
        .find(|e| e.field_name == "request_duration_ms")
        .unwrap();
    assert_eq!(entry.field_type, "float64");
    assert_eq!(entry.otel_spec_version.as_deref(), Some("1.26.0"));
}

#[tokio::test]
async fn list_attributes_for_unknown_signal_type_returns_empty() {
    let (pool, _container) = start_pool().await;

    // 'traces' has no seeded entries in migration 011
    let entries = list_schema_attributes(&pool, "traces").await.unwrap();
    assert!(
        entries.is_empty(),
        "no schema entries seeded for traces signal type"
    );
}

// ── semantic_annotations: tenant-scoped CRUD ─────────────────────────────────

#[tokio::test]
async fn seeded_annotation_is_readable_for_dev_tenant() {
    let (pool, _container) = start_pool().await;
    let dev_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let ann = get_annotation(&pool, dev_tenant, "metrics", "request_duration_ms")
        .await
        .unwrap();

    assert!(ann.is_some(), "seeded annotation must exist for dev tenant");
    let ann = ann.unwrap();
    assert_eq!(ann.metric_type.as_deref(), Some("gauge"));
    assert_eq!(ann.unit.as_deref(), Some("ms"));
    assert_eq!(ann.recommended_downsampling.as_deref(), Some("1m"));
    assert_eq!(ann.interpretation_rule.as_deref(), Some("higher_is_worse"));
    assert!(ann.not_for_billing);
    assert!((ann.effective_sample_rate.unwrap() - 1.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn seeded_annotation_is_not_visible_to_other_tenant() {
    let (pool, _container) = start_pool().await;
    let other_tenant = Uuid::new_v4();

    let ann = get_annotation(&pool, other_tenant, "metrics", "request_duration_ms")
        .await
        .unwrap();

    assert!(
        ann.is_none(),
        "seeded annotation must not be visible to a different tenant"
    );
}

#[tokio::test]
async fn upsert_creates_new_annotation() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let req = UpsertAnnotationRequest {
        display_name: Some("Error Rate".into()),
        metric_type: Some("gauge".into()),
        unit: Some("1".into()),
        interpretation_rule: Some("higher_is_worse".into()),
        effective_sample_rate: Some(0.1),
        not_for_billing: Some(true),
        known_derivations: Some(vec!["error_pct".into()]),
        recommended_downsampling: Some("5m".into()),
        timestamp_column: Some("timestamp_unix_nano".into()),
        business_description: Some("Fraction of requests returning 5xx.".into()),
        owner_team: Some("sre".into()),
    };
    let created = upsert_annotation(&pool, tenant, "metrics", "error_rate", &req)
        .await
        .unwrap();

    assert_eq!(created.signal_type, "metrics");
    assert_eq!(created.field_name, "error_rate");
    assert_eq!(created.metric_type.as_deref(), Some("gauge"));
    assert_eq!(created.unit.as_deref(), Some("1"));
    assert!(created.not_for_billing);
    assert_eq!(created.known_derivations, vec!["error_pct".to_string()]);
    assert_eq!(created.owner_team.as_deref(), Some("sre"));
}

#[tokio::test]
async fn upsert_replaces_existing_annotation() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let initial = UpsertAnnotationRequest {
        display_name: Some("Old Name".into()),
        metric_type: Some("counter".into()),
        unit: Some("req".into()),
        ..Default::default()
    };
    upsert_annotation(&pool, tenant, "metrics", "http_requests_total", &initial)
        .await
        .unwrap();

    let update = UpsertAnnotationRequest {
        display_name: Some("HTTP Requests Total".into()),
        metric_type: Some("counter".into()),
        unit: Some("req".into()),
        recommended_downsampling: Some("1m".into()),
        ..Default::default()
    };
    let updated = upsert_annotation(&pool, tenant, "metrics", "http_requests_total", &update)
        .await
        .unwrap();

    assert_eq!(updated.display_name.as_deref(), Some("HTTP Requests Total"));
    assert_eq!(updated.recommended_downsampling.as_deref(), Some("1m"));
    // PUT clears fields not in the request
    assert!(updated.owner_team.is_none());
}

#[tokio::test]
async fn patch_updates_only_provided_fields() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let initial = UpsertAnnotationRequest {
        display_name: Some("CPU Usage".into()),
        metric_type: Some("gauge".into()),
        unit: Some("percent".into()),
        owner_team: Some("infra".into()),
        ..Default::default()
    };
    upsert_annotation(&pool, tenant, "metrics", "cpu_usage", &initial)
        .await
        .unwrap();

    let patch_req = PatchAnnotationRequest {
        recommended_downsampling: Some("5m".into()),
        ..Default::default()
    };
    let patched = patch_annotation(&pool, tenant, "metrics", "cpu_usage", &patch_req)
        .await
        .unwrap()
        .expect("annotation must exist");

    // Patched field is updated
    assert_eq!(patched.recommended_downsampling.as_deref(), Some("5m"));
    // Unpatched fields are unchanged
    assert_eq!(patched.display_name.as_deref(), Some("CPU Usage"));
    assert_eq!(patched.metric_type.as_deref(), Some("gauge"));
    assert_eq!(patched.unit.as_deref(), Some("percent"));
    assert_eq!(patched.owner_team.as_deref(), Some("infra"));
}

#[tokio::test]
async fn patch_returns_none_for_nonexistent_annotation() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let result = patch_annotation(
        &pool,
        tenant,
        "metrics",
        "nonexistent_field",
        &PatchAnnotationRequest::default(),
    )
    .await
    .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn delete_removes_annotation() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    upsert_annotation(
        &pool,
        tenant,
        "logs",
        "log_level",
        &UpsertAnnotationRequest {
            display_name: Some("Log Level".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let deleted = delete_annotation(&pool, tenant, "logs", "log_level")
        .await
        .unwrap();
    assert!(deleted, "delete must return true when annotation exists");

    let after = get_annotation(&pool, tenant, "logs", "log_level")
        .await
        .unwrap();
    assert!(after.is_none(), "annotation must not exist after deletion");
}

#[tokio::test]
async fn delete_returns_false_for_nonexistent_annotation() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();

    let deleted = delete_annotation(&pool, tenant, "metrics", "ghost_field")
        .await
        .unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn annotations_are_isolated_between_tenants() {
    let (pool, _container) = start_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    upsert_annotation(
        &pool,
        tenant_a,
        "metrics",
        "shared_metric",
        &UpsertAnnotationRequest {
            display_name: Some("Tenant A view".into()),
            metric_type: Some("counter".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Tenant B creates their own annotation for the same field
    upsert_annotation(
        &pool,
        tenant_b,
        "metrics",
        "shared_metric",
        &UpsertAnnotationRequest {
            display_name: Some("Tenant B view".into()),
            metric_type: Some("gauge".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let ann_a = get_annotation(&pool, tenant_a, "metrics", "shared_metric")
        .await
        .unwrap()
        .unwrap();
    let ann_b = get_annotation(&pool, tenant_b, "metrics", "shared_metric")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(ann_a.display_name.as_deref(), Some("Tenant A view"));
    assert_eq!(ann_a.metric_type.as_deref(), Some("counter"));
    assert_eq!(ann_b.display_name.as_deref(), Some("Tenant B view"));
    assert_eq!(ann_b.metric_type.as_deref(), Some("gauge"));

    // Deleting tenant A's annotation does not affect tenant B
    delete_annotation(&pool, tenant_a, "metrics", "shared_metric")
        .await
        .unwrap();
    let ann_b_after = get_annotation(&pool, tenant_b, "metrics", "shared_metric")
        .await
        .unwrap();
    assert!(
        ann_b_after.is_some(),
        "tenant B annotation must survive tenant A deletion"
    );
}
