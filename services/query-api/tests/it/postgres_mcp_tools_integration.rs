use query_api::mcp_tools::{
    ResolveLabelResult, get_metric_schema, list_signal_fields, resolve_label_to_column,
};
use sqlx::PgPool;
use uuid::Uuid;

// ── helpers ───────────────────────────────────────────────────────────────────
//
// `semantic_annotations` writes now live in admin-service (schemas.rs); this test only
// needs to seed rows directly via SQL, not exercise the write path itself.

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

#[derive(Default)]
struct AnnotationSeed {
    display_name: Option<&'static str>,
    metric_type: Option<&'static str>,
    timestamp_column: Option<&'static str>,
    unit: Option<&'static str>,
    recommended_downsampling: Option<&'static str>,
    interpretation_rule: Option<&'static str>,
    not_for_billing: Option<bool>,
}

// Insert a semantic_annotations row directly (the write handlers now live in admin-service).
async fn insert_annotation(
    pool: &PgPool,
    tenant_id: Uuid,
    signal_type: &str,
    field_name: &str,
    seed: AnnotationSeed,
) {
    sqlx::query(
        "INSERT INTO semantic_annotations \
         (tenant_id, signal_type, field_name, display_name, metric_type, \
          timestamp_column, unit, recommended_downsampling, interpretation_rule, not_for_billing) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(tenant_id)
    .bind(signal_type)
    .bind(field_name)
    .bind(seed.display_name)
    .bind(seed.metric_type)
    .bind(seed.timestamp_column)
    .bind(seed.unit)
    .bind(seed.recommended_downsampling)
    .bind(seed.interpretation_rule)
    .bind(seed.not_for_billing.unwrap_or(false))
    .execute(pool)
    .await
    .expect("inserted semantic_annotation");
}

// ── get_metric_schema ─────────────────────────────────────────────────────────

#[tokio::test]
async fn get_metric_schema_returns_none_for_unknown_metric() {
    let pool = test_support::postgres::shared_pool().await;
    let result = get_metric_schema(&pool, TENANT_A, "nonexistent_metric")
        .await
        .unwrap();
    assert!(result.is_none(), "unknown metric must return None");
}

#[tokio::test]
async fn get_metric_schema_returns_structural_data_without_annotation() {
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "error_rate", "float64").await;

    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "error_rate",
        AnnotationSeed {
            display_name: Some("Error Rate"),
            metric_type: Some("gauge"),
            timestamp_column: Some("ts"),
            unit: Some("req/s"),
            recommended_downsampling: Some("1m"),
            interpretation_rule: Some("higher_is_worse"),
            not_for_billing: Some(true),
        },
    )
    .await;

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
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "tenant_metric", "float64").await;

    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "tenant_metric",
        AnnotationSeed {
            display_name: Some("Tenant A Metric"),
            metric_type: Some("counter"),
            timestamp_column: Some("ts"),
            ..Default::default()
        },
    )
    .await;

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
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
    // "profiles" signal type has no seeded schema_entries
    let fields = list_signal_fields(&pool, TENANT_A, "profiles")
        .await
        .unwrap();
    assert!(fields.is_empty(), "no entries for profiles → empty list");
}

#[tokio::test]
async fn list_signal_fields_returns_seeded_metrics_fields() {
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "heap_used", "float64").await;

    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "heap_used",
        AnnotationSeed {
            display_name: Some("Heap Used"),
            unit: Some("bytes"),
            ..Default::default()
        },
    )
    .await;

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
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "disk_writes", "float64").await;

    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "disk_writes",
        AnnotationSeed {
            display_name: Some("Disk Writes"),
            ..Default::default()
        },
    )
    .await;

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
    let pool = test_support::postgres::shared_pool().await;
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
    let pool = test_support::postgres::shared_pool().await;
    let result =
        resolve_label_to_column(&pool, TENANT_A, "metrics", "completely_unknown_label_xyz")
            .await
            .unwrap();
    assert_eq!(result, ResolveLabelResult::NotFound);
}

#[tokio::test]
async fn resolve_label_display_name_case_insensitive_match() {
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "net_rx_bytes", "float64").await;

    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "net_rx_bytes",
        AnnotationSeed {
            display_name: Some("Network Receive Bytes"),
            ..Default::default()
        },
    )
    .await;

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
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "auth_failures", "float64").await;

    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "auth_failures",
        AnnotationSeed {
            display_name: Some("Auth Failures"),
            ..Default::default()
        },
    )
    .await;

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
    let pool = test_support::postgres::shared_pool().await;
    insert_schema_entry(&pool, "metrics", "field_alpha", "float64").await;
    insert_schema_entry(&pool, "metrics", "field_beta", "float64").await;

    // Both fields get the same display_name for the same tenant (ambiguous)
    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "field_alpha",
        AnnotationSeed {
            display_name: Some("Shared Display Name"),
            ..Default::default()
        },
    )
    .await;
    insert_annotation(
        &pool,
        TENANT_A,
        "metrics",
        "field_beta",
        AnnotationSeed {
            display_name: Some("Shared Display Name"),
            ..Default::default()
        },
    )
    .await;

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
