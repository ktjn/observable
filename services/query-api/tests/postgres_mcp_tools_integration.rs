use query_api::mcp_tools::{
    get_metric_schema, list_signal_fields, resolve_label_to_column, ResolveLabelResult,
};
use query_api::schemas::{upsert_annotation, UpsertAnnotationRequest};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

// ── helpers ───────────────────────────────────────────────────────────────────

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

const TENANT_A: Uuid = Uuid::from_u128(0xAAAA_0000_0000_0000_0000_0000_0000_0001);
const TENANT_B: Uuid = Uuid::from_u128(0xBBBB_0000_0000_0000_0000_0000_0000_0002);

// Insert a schema_entry row directly (the seeded migration only covers request_duration_ms)
async fn insert_schema_entry(pool: &PgPool, signal_type: &str, field_name: &str, field_type: &str) {
    sqlx::query(
        "INSERT INTO schema_entries (signal_type, field_name, field_type) \
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(signal_type)
    .bind(field_name)
    .bind(field_type)
    .execute(pool)
    .await
    .expect("inserted schema_entry");
}

// ── get_metric_schema ─────────────────────────────────────────────────────────

#[tokio::test]
async fn get_metric_schema_returns_none_for_unknown_metric() {
    let (pool, _c) = start_pool().await;
    let result = get_metric_schema(&pool, TENANT_A, "nonexistent_metric")
        .await
        .unwrap();
    assert!(result.is_none(), "unknown metric must return None");
}

#[tokio::test]
async fn get_metric_schema_returns_structural_data_without_annotation() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "cpu_usage", "float64").await;

    let result = get_metric_schema(&pool, TENANT_A, "cpu_usage")
        .await
        .unwrap();
    let schema = result.expect("cpu_usage must be found");

    assert_eq!(schema.field_name, "cpu_usage");
    assert_eq!(schema.field_type, "float64");
    // no annotation for this tenant
    assert!(schema.metric_type.is_none());
    assert!(schema.not_for_billing.is_none());
    assert!(!schema.schema_complete, "no annotation → incomplete");
}

#[tokio::test]
async fn get_metric_schema_includes_tenant_annotation_overlay() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "error_rate", "float64").await;

    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "error_rate",
        &UpsertAnnotationRequest {
            display_name: Some("Error Rate".into()),
            metric_type: Some("gauge".into()),
            timestamp_column: Some("ts".into()),
            unit: Some("req/s".into()),
            recommended_downsampling: Some("1m".into()),
            interpretation_rule: Some("higher_is_worse".into()),
            not_for_billing: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let schema = get_metric_schema(&pool, TENANT_A, "error_rate")
        .await
        .unwrap()
        .expect("error_rate must be found");

    assert_eq!(schema.metric_type.as_deref(), Some("gauge"));
    assert_eq!(schema.timestamp_column.as_deref(), Some("ts"));
    assert_eq!(schema.unit.as_deref(), Some("req/s"));
    assert_eq!(
        schema.interpretation_rule.as_deref(),
        Some("higher_is_worse")
    );
    assert_eq!(schema.not_for_billing, Some(true));
    assert!(
        schema.schema_complete,
        "metric_type + timestamp_column present → complete"
    );
}

#[tokio::test]
async fn get_metric_schema_tenant_scoped_annotation_not_visible_to_other_tenant() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "tenant_metric", "float64").await;

    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "tenant_metric",
        &UpsertAnnotationRequest {
            display_name: Some("Tenant A Metric".into()),
            metric_type: Some("counter".into()),
            timestamp_column: Some("ts".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Tenant B queries the same metric → sees structural data but not A's annotation
    let schema = get_metric_schema(&pool, TENANT_B, "tenant_metric")
        .await
        .unwrap()
        .expect("structural entry must be found");

    assert!(
        schema.display_name.is_none(),
        "Tenant B must not see Tenant A's annotation"
    );
    assert!(schema.metric_type.is_none());
    assert!(!schema.schema_complete);
}

#[tokio::test]
async fn get_metric_schema_uses_seeded_data() {
    let (pool, _c) = start_pool().await;
    // The migration seeds tenant 00000000-0000-0000-0000-000000000001 with request_duration_ms
    let seed_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let schema = get_metric_schema(&pool, seed_tenant, "request_duration_ms")
        .await
        .unwrap()
        .expect("seeded metric must be found");

    assert_eq!(schema.field_name, "request_duration_ms");
    assert_eq!(schema.metric_type.as_deref(), Some("gauge"));
    assert_eq!(schema.unit.as_deref(), Some("ms"));
    assert!(
        schema.schema_complete,
        "seeded data has metric_type + timestamp_column"
    );
}

// ── list_signal_fields ────────────────────────────────────────────────────────

#[tokio::test]
async fn list_signal_fields_returns_empty_for_unknown_signal_type() {
    let (pool, _c) = start_pool().await;
    // "profiles" signal type has no seeded schema_entries
    let fields = list_signal_fields(&pool, TENANT_A, "profiles")
        .await
        .unwrap();
    assert!(fields.is_empty(), "no entries for profiles → empty list");
}

#[tokio::test]
async fn list_signal_fields_returns_seeded_metrics_fields() {
    let (pool, _c) = start_pool().await;
    let fields = list_signal_fields(&pool, TENANT_A, "metrics")
        .await
        .unwrap();
    assert!(
        fields.iter().any(|f| f.field_name == "request_duration_ms"),
        "seeded request_duration_ms must appear"
    );
}

#[tokio::test]
async fn list_signal_fields_merges_tenant_annotation() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "heap_used", "float64").await;

    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "heap_used",
        &UpsertAnnotationRequest {
            display_name: Some("Heap Used".into()),
            unit: Some("bytes".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let fields = list_signal_fields(&pool, TENANT_A, "metrics")
        .await
        .unwrap();
    let field = fields
        .iter()
        .find(|f| f.field_name == "heap_used")
        .expect("heap_used must be in list");

    assert_eq!(field.display_name.as_deref(), Some("Heap Used"));
    assert_eq!(field.unit.as_deref(), Some("bytes"));
}

#[tokio::test]
async fn list_signal_fields_annotation_absent_for_other_tenant() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "disk_writes", "float64").await;

    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "disk_writes",
        &UpsertAnnotationRequest {
            display_name: Some("Disk Writes".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Tenant B sees the structural field but not A's display_name
    let fields = list_signal_fields(&pool, TENANT_B, "metrics")
        .await
        .unwrap();
    let field = fields
        .iter()
        .find(|f| f.field_name == "disk_writes")
        .expect("disk_writes must be in list (structural)");

    assert!(
        field.display_name.is_none(),
        "Tenant B must not see Tenant A's display_name"
    );
}

// ── resolve_label_to_column ───────────────────────────────────────────────────

#[tokio::test]
async fn resolve_label_exact_field_name_match() {
    let (pool, _c) = start_pool().await;
    // "request_duration_ms" is in schema_entries (seeded)
    let result = resolve_label_to_column(&pool, TENANT_A, "metrics", "request_duration_ms")
        .await
        .unwrap();
    assert_eq!(
        result,
        ResolveLabelResult::Found("request_duration_ms".into()),
        "exact field_name match must resolve"
    );
}

#[tokio::test]
async fn resolve_label_not_found_for_unknown_label() {
    let (pool, _c) = start_pool().await;
    let result =
        resolve_label_to_column(&pool, TENANT_A, "metrics", "completely_unknown_label_xyz")
            .await
            .unwrap();
    assert_eq!(result, ResolveLabelResult::NotFound);
}

#[tokio::test]
async fn resolve_label_display_name_case_insensitive_match() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "net_rx_bytes", "float64").await;

    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "net_rx_bytes",
        &UpsertAnnotationRequest {
            display_name: Some("Network Receive Bytes".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Query with different case and extra whitespace
    let result = resolve_label_to_column(&pool, TENANT_A, "metrics", "  network receive bytes  ")
        .await
        .unwrap();
    assert_eq!(
        result,
        ResolveLabelResult::Found("net_rx_bytes".into()),
        "case-insensitive trimmed display_name match must resolve"
    );
}

#[tokio::test]
async fn resolve_label_display_name_not_visible_to_other_tenant() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "auth_failures", "float64").await;

    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "auth_failures",
        &UpsertAnnotationRequest {
            display_name: Some("Auth Failures".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Tenant B uses the display_name — must not resolve (cross-tenant isolation)
    let result = resolve_label_to_column(&pool, TENANT_B, "metrics", "Auth Failures")
        .await
        .unwrap();
    assert_eq!(
        result,
        ResolveLabelResult::NotFound,
        "Tenant B must not see Tenant A's display_name annotations"
    );
}

#[tokio::test]
async fn resolve_label_ambiguous_when_multiple_fields_share_display_name() {
    let (pool, _c) = start_pool().await;
    insert_schema_entry(&pool, "metrics", "field_alpha", "float64").await;
    insert_schema_entry(&pool, "metrics", "field_beta", "float64").await;

    // Both fields get the same display_name for the same tenant (ambiguous)
    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "field_alpha",
        &UpsertAnnotationRequest {
            display_name: Some("Shared Display Name".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    upsert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "field_beta",
        &UpsertAnnotationRequest {
            display_name: Some("Shared Display Name".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let result = resolve_label_to_column(&pool, TENANT_A, "metrics", "Shared Display Name")
        .await
        .unwrap();
    match result {
        ResolveLabelResult::Ambiguous(candidates) => {
            assert!(candidates.contains(&"field_alpha".to_string()));
            assert!(candidates.contains(&"field_beta".to_string()));
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}
