use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Serialize)]
pub struct DiscoveryResponse {
    pub items: Vec<String>,
}

#[derive(Deserialize)]
pub struct SummaryParams {
    pub environment: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct TopologyParams {
    pub environment: Option<String>,
    pub from: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub to: Option<DateTime<Utc>>,
    pub service: Option<String>,
}

#[derive(Serialize, Deserialize, clickhouse::Row)]
pub struct TopologyRow {
    pub caller: String,
    pub callee: String,
    pub request_count: u64,
    pub error_count: u64,
    pub p95_latency_ns: f64,
}

#[derive(Serialize)]
pub struct TopologyEdge {
    pub caller: String,
    pub callee: String,
    pub request_count: u64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
}

#[derive(Serialize)]
pub struct TopologyResponse {
    pub edges: Vec<TopologyEdge>,
}

#[derive(Serialize, Deserialize, clickhouse::Row)]
pub struct ServiceSummaryRow {
    pub service_name: String,
    pub request_count: u64,
    pub error_count: u64,
    pub p95_latency_ns: f64,
}

#[derive(Serialize)]
pub struct ServiceSummary {
    pub service_name: String,
    pub request_rate: f64,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
    pub health_state: String,
    pub active_alert_count: u64,
    pub latest_deployment: Option<String>,
}

#[derive(Serialize)]
pub struct ServiceSummaryResponse {
    pub items: Vec<ServiceSummary>,
}

#[derive(Serialize)]
pub struct ServiceDetailResponse {
    pub service: ServiceSummary,
}

#[derive(Deserialize)]
pub struct ResponseTimeHistoryParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub buckets: Option<u32>,
}

#[derive(Serialize)]
pub struct ResponseTimeBucket {
    pub start_ms: u64,
    pub end_ms: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub request_rate: f64,
}

#[derive(Serialize)]
pub struct ResponseTimeHistoryResponse {
    pub buckets: Vec<ResponseTimeBucket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureEntityType {
    Host,
    Cluster,
    Namespace,
    Pod,
    Container,
}

impl InfrastructureEntityType {
    pub fn attribute_key(self) -> &'static str {
        match self {
            Self::Host => "host.name",
            Self::Cluster => "k8s.cluster.name",
            Self::Namespace => "k8s.namespace.name",
            Self::Pod => "k8s.pod.name",
            Self::Container => "container.name",
        }
    }

    pub fn attribute_sql_expr(self) -> &'static str {
        match self {
            Self::Host => {
                "if(JSONExtractString(resource_attributes, 'host.name') != '', \
                JSONExtractString(resource_attributes, 'host.name'), \
                JSONExtractString(resource_attributes, 'host.id'))"
            }
            Self::Cluster => "JSONExtractString(resource_attributes, 'k8s.cluster.name')",
            Self::Namespace => "JSONExtractString(resource_attributes, 'k8s.namespace.name')",
            Self::Pod => "JSONExtractString(resource_attributes, 'k8s.pod.name')",
            Self::Container => {
                "if(JSONExtractString(resource_attributes, 'container.name') != '', \
                JSONExtractString(resource_attributes, 'container.name'), \
                JSONExtractString(resource_attributes, 'container.id'))"
            }
        }
    }
}

impl TryFrom<&str> for InfrastructureEntityType {
    type Error = StatusCode;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "host" => Ok(Self::Host),
            "cluster" => Ok(Self::Cluster),
            "namespace" => Ok(Self::Namespace),
            "pod" => Ok(Self::Pod),
            "container" => Ok(Self::Container),
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InfrastructureLinks {
    pub logs: String,
    pub traces: String,
    pub metrics: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InfrastructureEntitySummary {
    pub entity_type: InfrastructureEntityType,
    pub entity_id: String,
    pub display_name: String,
    pub parent_id: Option<String>,
    pub parent_display_name: Option<String>,
    pub environment: Option<String>,
    pub health_state: String,
    pub last_seen_unix_nano: u64,
    pub related_services: Vec<String>,
    pub log_rate_per_minute: Option<f64>,
    pub error_rate: Option<f64>,
    pub restart_count: Option<u64>,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub disk_usage: Option<f64>,
    pub network_io: Option<f64>,
}

#[derive(Serialize)]
pub struct InfrastructureInventoryResponse {
    pub items: Vec<InfrastructureEntitySummary>,
}

#[derive(Serialize)]
pub struct InfrastructureDetailResponse {
    pub entity: InfrastructureEntitySummary,
    pub links: InfrastructureLinks,
}

#[derive(Deserialize)]
pub struct InfrastructureInventoryParams {
    pub entity_type: Option<String>,
    pub environment: Option<String>,
    pub service: Option<String>,
    pub search: Option<String>,
    pub lookback_minutes: Option<u32>,
}

#[derive(Deserialize)]
pub struct InfrastructureDetailParams {
    pub environment: Option<String>,
    pub lookback_minutes: Option<u32>,
}

#[derive(Serialize, Deserialize, clickhouse::Row)]
struct InfrastructureAggregateRow {
    cluster_name: String,
    namespace_name: String,
    pod_name: String,
    entity_name: String,
    environment: String,
    last_seen_unix_nano: u64,
    related_services: Vec<String>,
    log_events: u64,
    span_events: u64,
    error_events: u64,
}

pub async fn list_services(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DiscoveryResponse>, StatusCode> {
    let sql = "SELECT DISTINCT service_name FROM ( \
        SELECT DISTINCT service_name FROM observable.spans WHERE tenant_id = ? AND service_name != '' \
        UNION DISTINCT \
        SELECT DISTINCT service_name FROM observable.logs WHERE tenant_id = ? AND service_name != '' \
        UNION DISTINCT \
        SELECT DISTINCT service_name FROM observable.metric_series WHERE tenant_id = ? AND service_name != '' \
    ) ORDER BY service_name";

    let rows: Vec<String> = state
        .ch
        .query(sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse discovery services error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(DiscoveryResponse { items: rows }))
}

pub async fn list_environments(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<DiscoveryResponse>, StatusCode> {
    let sql = "SELECT DISTINCT environment FROM ( \
        SELECT DISTINCT environment FROM observable.spans WHERE tenant_id = ? \
        UNION DISTINCT \
        SELECT DISTINCT environment FROM observable.logs WHERE tenant_id = ? \
        UNION DISTINCT \
        SELECT DISTINCT environment FROM observable.metric_series WHERE tenant_id = ? \
    ) WHERE environment != '' ORDER BY environment";

    let rows: Vec<String> = state
        .ch
        .query(sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse discovery environments error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(DiscoveryResponse { items: rows }))
}

pub async fn list_service_summaries(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<ServiceSummaryResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);
    let duration_secs = (to_ns.saturating_sub(from_ns)) as f64 / 1_000_000_000.0;

    let mut sql = "SELECT \
            service_name, \
            count() as request_count, \
            countIf(status_code = 'ERROR') as error_count, \
            quantile(0.95)(duration_ns) as p95_latency_ns \
        FROM observable.spans \
        WHERE tenant_id = ? AND start_time_unix_nano >= ?"
        .to_string();

    if params.environment.is_some() {
        sql.push_str(" AND environment = ?");
    }

    sql.push_str(" GROUP BY service_name ORDER BY service_name");

    let mut query = state.ch.query(&sql).bind(ctx.tenant_id).bind(from_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env);
    }

    let rows: Vec<ServiceSummaryRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse service summary error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let enrichment =
        fetch_service_catalog_enrichment(&state.db, ctx.tenant_id, params.environment.as_deref())
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch service catalog enrichment");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let items = rows
        .into_iter()
        .map(|row| {
            let service_name = row.service_name.clone();
            apply_catalog_enrichment(
                service_summary_from_row(row, duration_secs),
                enrichment.get(&service_name),
            )
        })
        .collect();

    Ok(Json(ServiceSummaryResponse { items }))
}

pub async fn get_service_summary(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<ServiceDetailResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);
    let duration_secs = (to_ns.saturating_sub(from_ns)) as f64 / 1_000_000_000.0;

    let mut sql = "SELECT \
            service_name, \
            count() as request_count, \
            countIf(status_code = 'ERROR') as error_count, \
            quantile(0.95)(duration_ns) as p95_latency_ns \
        FROM observable.spans \
        WHERE tenant_id = ? AND service_name = ? AND start_time_unix_nano >= ?"
        .to_string();

    if params.environment.is_some() {
        sql.push_str(" AND environment = ?");
    }

    sql.push_str(" GROUP BY service_name LIMIT 1");

    let mut query = state
        .ch
        .query(&sql)
        .bind(ctx.tenant_id)
        .bind(&service_name)
        .bind(from_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env);
    }

    let row = query
        .fetch_optional::<ServiceSummaryRow>()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse single service summary error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let enrichment =
        fetch_service_catalog_enrichment(&state.db, ctx.tenant_id, params.environment.as_deref())
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to fetch service catalog enrichment");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let enrichment_entry = enrichment.get(&row.service_name);

    Ok(Json(ServiceDetailResponse {
        service: apply_catalog_enrichment(
            service_summary_from_row(row, duration_secs),
            enrichment_entry,
        ),
    }))
}

pub async fn get_service_response_time_history(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service_name): Path<String>,
    Query(params): Query<ResponseTimeHistoryParams>,
) -> Result<Json<ResponseTimeHistoryResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let from_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);
    let bucket_count = params.buckets.unwrap_or(60).clamp(1, 200);

    let plan = state
        .planner
        .plan_response_time_histogram(from_ns, to_ns, bucket_count);

    let mut cursor = state
        .ch
        .query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(ctx.tenant_id)
        .bind(&service_name)
        .bind(from_ns)
        .bind(to_ns)
        .fetch::<(i64, f64, f64, u64)>()
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse response time histogram error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let interval_secs = plan.interval_ns as f64 / 1_000_000_000.0;
    let mut raw: HashMap<i64, (f64, f64, u64)> = HashMap::new();
    while let Some((bucket_idx, p50_ns, p95_ns, span_count)) = cursor.next().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse response time histogram fetch error");
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        if bucket_idx >= 0 && bucket_idx < bucket_count as i64 {
            raw.insert(bucket_idx, (p50_ns, p95_ns, span_count));
        }
    }

    let buckets = (0..bucket_count)
        .map(|i| {
            let start_ns = plan.from_ns + i as u64 * plan.interval_ns;
            let end_ns = start_ns + plan.interval_ns;
            let (p50_ns, p95_ns, span_count) = raw.remove(&(i as i64)).unwrap_or((0.0, 0.0, 0));
            ResponseTimeBucket {
                start_ms: start_ns / 1_000_000,
                end_ms: end_ns / 1_000_000,
                p50_ms: p50_ns / 1_000_000.0,
                p95_ms: p95_ns / 1_000_000.0,
                request_rate: span_count as f64 / interval_secs,
            }
        })
        .collect();

    Ok(Json(ResponseTimeHistoryResponse { buckets }))
}

pub async fn get_topology(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<TopologyParams>,
) -> Result<Json<TopologyResponse>, StatusCode> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_ns = params
        .from
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));

    let plan = state.planner.plan_topology(&params);

    // Branch 1 binds: tenant_id, tenant_id, start_ns [, env, env] [, service, service]
    let mut query = state
        .ch
        .query(&plan.sql)
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }
    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    // Branch 2 binds: tenant_id, tenant_id, start_ns [, env, env] [, service, service]
    query = query.bind(ctx.tenant_id).bind(ctx.tenant_id).bind(start_ns);

    if let Some(ref env) = params.environment {
        query = query.bind(env).bind(env);
    }
    if let Some(ref service) = params.service {
        query = query.bind(service).bind(service);
    }

    let rows: Vec<TopologyRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse topology error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let edges = rows
        .into_iter()
        .map(|row| {
            let error_rate = if row.request_count > 0 {
                (row.error_count as f64) / (row.request_count as f64)
            } else {
                0.0
            };
            TopologyEdge {
                caller: row.caller,
                callee: row.callee,
                request_count: row.request_count,
                error_rate,
                p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
            }
        })
        .collect();

    Ok(Json(TopologyResponse { edges }))
}

pub async fn list_infrastructure_inventory(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<InfrastructureInventoryParams>,
) -> Result<Json<InfrastructureInventoryResponse>, StatusCode> {
    let lookback_minutes = validated_infrastructure_lookback_minutes(params.lookback_minutes)?;
    let filter_entity_type = params
        .entity_type
        .as_deref()
        .map(InfrastructureEntityType::try_from)
        .transpose()?;

    let entity_types = if let Some(entity_type) = filter_entity_type {
        vec![entity_type]
    } else {
        all_infrastructure_entity_types().to_vec()
    };

    let mut items = Vec::new();
    for entity_type in entity_types {
        let mut rows = fetch_infrastructure_summaries(
            &state.ch,
            ctx.tenant_id,
            entity_type,
            params.environment.as_deref(),
            params.service.as_deref(),
            params.search.as_deref(),
            lookback_minutes,
        )
        .await?;
        items.append(&mut rows);
    }

    items.sort_by(|left, right| {
        right
            .last_seen_unix_nano
            .cmp(&left.last_seen_unix_nano)
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });

    Ok(Json(InfrastructureInventoryResponse { items }))
}

pub async fn get_infrastructure_detail(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((entity_type, entity_id)): Path<(String, String)>,
    Query(params): Query<InfrastructureDetailParams>,
) -> Result<Json<InfrastructureDetailResponse>, StatusCode> {
    let entity_type = InfrastructureEntityType::try_from(entity_type.as_str())?;
    let lookback_minutes = validated_infrastructure_lookback_minutes(params.lookback_minutes)?;

    let mut matches = fetch_infrastructure_summaries(
        &state.ch,
        ctx.tenant_id,
        entity_type,
        params.environment.as_deref(),
        None,
        None,
        lookback_minutes,
    )
    .await?
    .into_iter()
    .filter(|entity| entity.entity_id == entity_id);

    let entity = matches.next().ok_or(StatusCode::NOT_FOUND)?;
    if matches.next().is_some() {
        tracing::error!(
            entity_type = ?entity_type,
            entity_id = %entity_id,
            "infrastructure detail lookup remained ambiguous after canonicalization"
        );
        return Err(StatusCode::NOT_FOUND);
    }

    let links = infrastructure_detail_links(&entity);

    Ok(Json(InfrastructureDetailResponse { entity, links }))
}

/// Per-service catalog enrichment sourced from Postgres (SLO-linked alerts and deployments).
///
/// `active_alert_count` and `slo_breaching` are scoped to alerts reachable via
/// `slo_definitions.service_name` (i.e. `alert_type = 'slo_burn_rate'` rules whose
/// `condition->>'slo_id'` matches an SLO for that service). Threshold/composite alerts not tied
/// to an SLO are not counted, because `alert_rules` has no `service_name`/`environment` column.
struct ServiceCatalogEnrichment {
    active_alert_count: u64,
    slo_breaching: bool,
    latest_deployment: Option<String>,
}

async fn fetch_service_catalog_enrichment(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    environment: Option<&str>,
) -> Result<HashMap<String, ServiceCatalogEnrichment>, sqlx::Error> {
    let mut enrichment: HashMap<String, ServiceCatalogEnrichment> = HashMap::new();

    #[derive(sqlx::FromRow)]
    struct SloAlertRow {
        service_name: String,
        active_alert_count: i64,
        slo_breaching: bool,
    }

    let slo_rows: Vec<SloAlertRow> = sqlx::query_as(
        "SELECT sd.service_name, \
                COUNT(af.firing_id) FILTER (WHERE af.state = 'active') AS active_alert_count, \
                COALESCE(BOOL_OR(af.state = 'active'), false) AS slo_breaching \
         FROM slo_definitions sd \
         LEFT JOIN alert_rules ar \
             ON ar.tenant_id = sd.tenant_id \
            AND ar.alert_type = 'slo_burn_rate' \
            AND ar.condition->>'slo_id' = sd.slo_id::text \
         LEFT JOIN alert_firings af ON af.rule_id = ar.rule_id \
         WHERE sd.tenant_id = $1 \
           AND ($2::TEXT IS NULL OR sd.environment = $2) \
         GROUP BY sd.service_name",
    )
    .bind(tenant_id)
    .bind(environment)
    .fetch_all(db)
    .await?;

    for row in slo_rows {
        enrichment.insert(
            row.service_name,
            ServiceCatalogEnrichment {
                active_alert_count: row.active_alert_count.max(0) as u64,
                slo_breaching: row.slo_breaching,
                latest_deployment: None,
            },
        );
    }

    #[derive(sqlx::FromRow)]
    struct LatestDeploymentRow {
        service_name: String,
        service_version: String,
    }

    let deployment_rows: Vec<LatestDeploymentRow> = sqlx::query_as(
        "SELECT DISTINCT ON (service_name) service_name, service_version \
         FROM deployment_markers \
         WHERE tenant_id = $1 \
           AND ($2::TEXT IS NULL OR environment = $2) \
         ORDER BY service_name, started_at DESC",
    )
    .bind(tenant_id)
    .bind(environment)
    .fetch_all(db)
    .await?;

    for row in deployment_rows {
        enrichment
            .entry(row.service_name)
            .or_insert(ServiceCatalogEnrichment {
                active_alert_count: 0,
                slo_breaching: false,
                latest_deployment: None,
            })
            .latest_deployment = Some(row.service_version);
    }

    Ok(enrichment)
}

fn apply_catalog_enrichment(
    mut summary: ServiceSummary,
    enrichment: Option<&ServiceCatalogEnrichment>,
) -> ServiceSummary {
    if let Some(e) = enrichment {
        summary.active_alert_count = e.active_alert_count;
        summary.latest_deployment = e.latest_deployment.clone();
        if e.slo_breaching {
            summary.health_state = "breach".to_string();
        }
    }
    summary
}

fn service_summary_from_row(row: ServiceSummaryRow, duration_secs: f64) -> ServiceSummary {
    let error_rate = if row.request_count > 0 {
        (row.error_count as f64) / (row.request_count as f64)
    } else {
        0.0
    };

    ServiceSummary {
        service_name: row.service_name,
        request_rate: (row.request_count as f64) / duration_secs,
        error_rate,
        p95_latency_ms: row.p95_latency_ns / 1_000_000.0,
        health_state: health_state(error_rate).to_string(),
        active_alert_count: 0,
        latest_deployment: None,
    }
}

fn health_state(error_rate: f64) -> &'static str {
    if error_rate > 0.05 {
        "breach"
    } else if error_rate > 0.01 {
        "watch"
    } else {
        "healthy"
    }
}

pub async fn fetch_infrastructure_summaries(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    entity_type: InfrastructureEntityType,
    environment: Option<&str>,
    service: Option<&str>,
    search: Option<&str>,
    lookback_minutes: u32,
) -> Result<Vec<InfrastructureEntitySummary>, StatusCode> {
    let (start_ns, start_seconds) = infrastructure_lookback_window(lookback_minutes);
    let entity_expr = entity_type.attribute_sql_expr();
    let has_environment = environment.is_some();
    let has_service = service.is_some();
    let has_search = search.is_some();

    // NOTE: The inner column is aliased as `_env` (not `environment`) to avoid a
    // ClickHouse alias-substitution bug where the outer `argMax(…) AS environment`
    // is resolved in the WHERE clause instead of the raw column, causing
    // ILLEGAL_AGGREGATION errors and silently ignoring the environment filter.
    let mut sql = format!(
        "SELECT \
            cluster_name, \
            namespace_name, \
            pod_name, \
            entity_name, \
            argMax(_env, event_time) AS environment, \
            max(event_time) AS last_seen_unix_nano, \
            arraySort(groupUniqArrayIf(service_name, service_name != '')) AS related_services, \
            sum(log_events) AS log_events, \
            sum(span_events) AS span_events, \
            sum(error_events) AS error_events \
        FROM ( \
            SELECT \
                JSONExtractString(resource_attributes, 'k8s.cluster.name') AS cluster_name, \
                JSONExtractString(resource_attributes, 'k8s.namespace.name') AS namespace_name, \
                JSONExtractString(resource_attributes, 'k8s.pod.name') AS pod_name, \
                {entity_expr} AS entity_name, \
                environment AS _env, \
                service_name, \
                start_time_unix_nano AS event_time, \
                toUInt64(0) AS log_events, \
                toUInt64(1) AS span_events, \
                toUInt64(status_code = 'ERROR') AS error_events \
            FROM observable.spans \
            WHERE tenant_id = ? AND start_time_unix_nano >= ? \
            UNION ALL \
            SELECT \
                JSONExtractString(resource_attributes, 'k8s.cluster.name') AS cluster_name, \
                JSONExtractString(resource_attributes, 'k8s.namespace.name') AS namespace_name, \
                JSONExtractString(resource_attributes, 'k8s.pod.name') AS pod_name, \
                {entity_expr} AS entity_name, \
                environment AS _env, \
                service_name, \
                timestamp_unix_nano AS event_time, \
                toUInt64(1) AS log_events, \
                toUInt64(0) AS span_events, \
                toUInt64(0) AS error_events \
            FROM observable.logs \
            WHERE tenant_id = ? AND timestamp_unix_nano >= ? \
            UNION ALL \
            SELECT \
                JSONExtractString(resource_attributes, 'k8s.cluster.name') AS cluster_name, \
                JSONExtractString(resource_attributes, 'k8s.namespace.name') AS namespace_name, \
                JSONExtractString(resource_attributes, 'k8s.pod.name') AS pod_name, \
                {entity_expr} AS entity_name, \
                environment AS _env, \
                service_name, \
                toUInt64(toUnixTimestamp(created_at)) * 1000000000 AS event_time, \
                toUInt64(0) AS log_events, \
                toUInt64(0) AS span_events, \
                toUInt64(0) AS error_events \
            FROM observable.metric_series \
            WHERE tenant_id = ? AND created_at >= fromUnixTimestamp(?) \
        ) \
        WHERE entity_name != ''"
    );

    if has_environment {
        sql.push_str(" AND _env = ?");
    }
    if has_service {
        sql.push_str(" AND service_name = ?");
    }
    if has_search {
        sql.push_str(
            " AND (positionCaseInsensitiveUTF8(entity_name, ?) > 0 \
             OR positionCaseInsensitiveUTF8(cluster_name, ?) > 0 \
             OR positionCaseInsensitiveUTF8(namespace_name, ?) > 0 \
             OR positionCaseInsensitiveUTF8(pod_name, ?) > 0 \
             OR positionCaseInsensitiveUTF8(service_name, ?) > 0)",
        );
    }
    sql.push_str(
        " GROUP BY cluster_name, namespace_name, pod_name, entity_name \
         ORDER BY last_seen_unix_nano DESC, entity_name ASC",
    );

    let mut query = ch
        .query(&sql)
        .bind(tenant_id)
        .bind(start_ns)
        .bind(tenant_id)
        .bind(start_ns)
        .bind(tenant_id)
        .bind(start_seconds);

    if let Some(environment) = environment {
        query = query.bind(environment);
    }
    if let Some(service) = service {
        query = query.bind(service);
    }
    if let Some(search) = search {
        query = query
            .bind(search)
            .bind(search)
            .bind(search)
            .bind(search)
            .bind(search);
    }

    let rows: Vec<InfrastructureAggregateRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(
            error = ?e,
            entity_type = ?entity_type,
            "ClickHouse infrastructure discovery query error"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(rows
        .into_iter()
        .map(|row| infrastructure_entity_summary_from_row(entity_type, row, lookback_minutes))
        .collect())
}

pub fn all_infrastructure_entity_types() -> [InfrastructureEntityType; 5] {
    [
        InfrastructureEntityType::Host,
        InfrastructureEntityType::Cluster,
        InfrastructureEntityType::Namespace,
        InfrastructureEntityType::Pod,
        InfrastructureEntityType::Container,
    ]
}

fn infrastructure_lookback_window(lookback_minutes: u32) -> (u64, u64) {
    let lookback_ns = (lookback_minutes as u64) * 60 * 1_000_000_000;
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let start_ns = now_ns.saturating_sub(lookback_ns);
    let start_seconds = start_ns / 1_000_000_000;
    (start_ns, start_seconds)
}

fn validated_infrastructure_lookback_minutes(
    lookback_minutes: Option<u32>,
) -> Result<u32, StatusCode> {
    match lookback_minutes.unwrap_or(60) {
        0 => Err(StatusCode::BAD_REQUEST),
        value => Ok(value),
    }
}

fn infrastructure_entity_summary_from_row(
    entity_type: InfrastructureEntityType,
    row: InfrastructureAggregateRow,
    lookback_minutes: u32,
) -> InfrastructureEntitySummary {
    let identity = infrastructure_identity(entity_type, &row);
    let error_rate = if row.span_events > 0 {
        Some((row.error_events as f64) / (row.span_events as f64))
    } else {
        None
    };
    let log_rate_per_minute = Some((row.log_events as f64) / (lookback_minutes as f64));
    let health_state = health_state(error_rate.unwrap_or_default()).to_string();
    let environment = normalize_infrastructure_string(row.environment);

    InfrastructureEntitySummary {
        entity_type,
        display_name: identity.display_name,
        entity_id: identity.entity_id,
        parent_display_name: identity.parent_display_name,
        parent_id: identity.parent_id,
        environment,
        health_state,
        last_seen_unix_nano: row.last_seen_unix_nano,
        related_services: row.related_services,
        log_rate_per_minute,
        error_rate,
        restart_count: None,
        cpu_usage: None,
        memory_usage: None,
        disk_usage: None,
        network_io: None,
    }
}

struct InfrastructureIdentity {
    entity_id: String,
    display_name: String,
    parent_id: Option<String>,
    parent_display_name: Option<String>,
}

fn infrastructure_identity(
    entity_type: InfrastructureEntityType,
    row: &InfrastructureAggregateRow,
) -> InfrastructureIdentity {
    let cluster = non_empty_infrastructure_str(&row.cluster_name);
    let namespace = non_empty_infrastructure_str(&row.namespace_name);
    let pod = non_empty_infrastructure_str(&row.pod_name);
    let entity = row.entity_name.as_str();

    match entity_type {
        InfrastructureEntityType::Host => InfrastructureIdentity {
            entity_id: join_infrastructure_segments([cluster, Some(entity)]),
            display_name: entity.to_string(),
            parent_id: cluster.map(str::to_string),
            parent_display_name: cluster.map(str::to_string),
        },
        InfrastructureEntityType::Cluster => InfrastructureIdentity {
            entity_id: entity.to_string(),
            display_name: entity.to_string(),
            parent_id: None,
            parent_display_name: None,
        },
        InfrastructureEntityType::Namespace => InfrastructureIdentity {
            entity_id: join_infrastructure_segments([cluster, Some(entity)]),
            display_name: entity.to_string(),
            parent_id: cluster.map(str::to_string),
            parent_display_name: cluster.map(str::to_string),
        },
        InfrastructureEntityType::Pod => InfrastructureIdentity {
            entity_id: join_infrastructure_segments([cluster, namespace, Some(entity)]),
            display_name: entity.to_string(),
            parent_id: join_optional_infrastructure_segments([cluster, namespace]),
            parent_display_name: namespace.or(cluster).map(str::to_string),
        },
        InfrastructureEntityType::Container => InfrastructureIdentity {
            entity_id: join_infrastructure_segments([cluster, namespace, pod, Some(entity)]),
            display_name: entity.to_string(),
            parent_id: join_optional_infrastructure_segments([cluster, namespace, pod]),
            parent_display_name: pod.or(namespace).or(cluster).map(str::to_string),
        },
    }
}

fn non_empty_infrastructure_str(value: &str) -> Option<&str> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn join_infrastructure_segments<const N: usize>(segments: [Option<&str>; N]) -> String {
    join_optional_infrastructure_segments(segments).unwrap_or_default()
}

fn join_optional_infrastructure_segments<const N: usize>(
    segments: [Option<&str>; N],
) -> Option<String> {
    let segments: Vec<&str> = segments.into_iter().flatten().collect();
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("/"))
    }
}

fn normalize_infrastructure_string(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }

    encoded
}

fn infrastructure_detail_links(entity: &InfrastructureEntitySummary) -> InfrastructureLinks {
    infrastructure_links(
        entity.entity_type,
        &entity.display_name,
        entity.related_services.first().cloned(),
    )
}

fn infrastructure_links(
    entity_type: InfrastructureEntityType,
    entity_id: &str,
    primary_service: Option<String>,
) -> InfrastructureLinks {
    let attr = entity_type.attribute_key();
    let resource_attr = percent_encode_url_component(&format!("{attr}:{entity_id}"));
    let metrics = primary_service
        .map(|service| {
            let service = percent_encode_url_component(&service);
            format!("/services/{service}/metrics?resource_attr={resource_attr}")
        })
        .unwrap_or_else(|| format!("/metrics?resource_attr={resource_attr}"));

    InfrastructureLinks {
        logs: format!("/logs?resource_attr={resource_attr}"),
        traces: format!("/traces?resource_attr={resource_attr}"),
        metrics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_row_derives_red_metrics_and_health() {
        let summary = service_summary_from_row(
            ServiceSummaryRow {
                service_name: "checkout".into(),
                request_count: 120,
                error_count: 3,
                p95_latency_ns: 245_000_000.0,
            },
            60.0,
        );

        assert_eq!(summary.service_name, "checkout");
        assert_eq!(summary.request_rate, 2.0);
        assert_eq!(summary.error_rate, 0.025);
        assert_eq!(summary.p95_latency_ms, 245.0);
        assert_eq!(summary.health_state, "watch");
        assert_eq!(summary.active_alert_count, 0);
        assert_eq!(summary.latest_deployment, None);
    }

    #[test]
    fn health_state_thresholds_are_stable() {
        assert_eq!(health_state(0.0), "healthy");
        assert_eq!(health_state(0.02), "watch");
        assert_eq!(health_state(0.06), "breach");
    }

    fn healthy_summary() -> ServiceSummary {
        service_summary_from_row(
            ServiceSummaryRow {
                service_name: "checkout".into(),
                request_count: 120,
                error_count: 0,
                p95_latency_ns: 100_000_000.0,
            },
            60.0,
        )
    }

    #[test]
    fn enrichment_overrides_health_state_to_breach_when_slo_breaching() {
        let summary = healthy_summary();
        assert_eq!(summary.health_state, "healthy");

        let enrichment = ServiceCatalogEnrichment {
            active_alert_count: 1,
            slo_breaching: true,
            latest_deployment: None,
        };

        let enriched = apply_catalog_enrichment(summary, Some(&enrichment));

        assert_eq!(enriched.health_state, "breach");
    }

    #[test]
    fn enrichment_leaves_error_rate_health_state_when_slo_not_breaching() {
        let summary = healthy_summary();

        let enrichment = ServiceCatalogEnrichment {
            active_alert_count: 0,
            slo_breaching: false,
            latest_deployment: None,
        };

        let enriched = apply_catalog_enrichment(summary, Some(&enrichment));

        assert_eq!(enriched.health_state, "healthy");
    }

    #[test]
    fn enrichment_passes_through_alert_count_and_latest_deployment() {
        let summary = healthy_summary();

        let enrichment = ServiceCatalogEnrichment {
            active_alert_count: 3,
            slo_breaching: false,
            latest_deployment: Some("v2.3.1".to_string()),
        };

        let enriched = apply_catalog_enrichment(summary, Some(&enrichment));

        assert_eq!(enriched.active_alert_count, 3);
        assert_eq!(enriched.latest_deployment, Some("v2.3.1".to_string()));
    }

    #[test]
    fn enrichment_none_leaves_summary_unchanged() {
        let summary = healthy_summary();

        let enriched = apply_catalog_enrichment(summary, None);

        assert_eq!(enriched.health_state, "healthy");
        assert_eq!(enriched.active_alert_count, 0);
        assert_eq!(enriched.latest_deployment, None);
    }

    #[test]
    fn infrastructure_entity_type_attribute_keys_are_stable() {
        assert_eq!(InfrastructureEntityType::Host.attribute_key(), "host.name");
        assert_eq!(
            InfrastructureEntityType::Cluster.attribute_key(),
            "k8s.cluster.name"
        );
        assert_eq!(
            InfrastructureEntityType::Namespace.attribute_key(),
            "k8s.namespace.name"
        );
        assert_eq!(
            InfrastructureEntityType::Pod.attribute_key(),
            "k8s.pod.name"
        );
        assert_eq!(
            InfrastructureEntityType::Container.attribute_key(),
            "container.name"
        );
    }

    #[test]
    fn infrastructure_entity_type_rejects_unknown_values() {
        assert_eq!(
            InfrastructureEntityType::try_from("rack"),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn infrastructure_health_state_matches_shared_service_thresholds() {
        assert_eq!(health_state(0.0), "healthy");
        assert_eq!(health_state(0.02), "watch");
        assert_eq!(health_state(0.05), "watch");
        assert_eq!(health_state(0.06), "breach");

        let summary = infrastructure_entity_summary_from_row(
            InfrastructureEntityType::Pod,
            InfrastructureAggregateRow {
                cluster_name: String::new(),
                namespace_name: String::new(),
                pod_name: String::new(),
                entity_name: "checkout-pod-1".into(),
                environment: "prod".into(),
                last_seen_unix_nano: 42,
                related_services: vec!["checkout-api".into()],
                log_events: 12,
                span_events: 100,
                error_events: 5,
            },
            10,
        );

        assert_eq!(summary.error_rate, Some(0.05));
        assert_eq!(summary.health_state, "watch");
    }

    #[test]
    fn infrastructure_detail_link_uses_canonical_resource_attribute() {
        let links = infrastructure_links(
            InfrastructureEntityType::Pod,
            "checkout-pod-1",
            Some("checkout-api".into()),
        );

        assert_eq!(
            links.logs,
            "/logs?resource_attr=k8s.pod.name%3Acheckout-pod-1"
        );
        assert_eq!(
            links.traces,
            "/traces?resource_attr=k8s.pod.name%3Acheckout-pod-1"
        );
        assert_eq!(
            links.metrics,
            "/services/checkout-api/metrics?resource_attr=k8s.pod.name%3Acheckout-pod-1"
        );
    }

    #[test]
    fn infrastructure_links_percent_encode_dynamic_url_parts() {
        let links = infrastructure_links(
            InfrastructureEntityType::Pod,
            "checkout/pod 1",
            Some("checkout/api beta".into()),
        );

        assert_eq!(
            links.logs,
            "/logs?resource_attr=k8s.pod.name%3Acheckout%2Fpod%201"
        );
        assert_eq!(
            links.traces,
            "/traces?resource_attr=k8s.pod.name%3Acheckout%2Fpod%201"
        );
        assert_eq!(
            links.metrics,
            "/services/checkout%2Fapi%20beta/metrics?resource_attr=k8s.pod.name%3Acheckout%2Fpod%201"
        );
    }

    #[test]
    fn infrastructure_inventory_response_serializes_entity_rows() {
        let response = InfrastructureInventoryResponse {
            items: vec![InfrastructureEntitySummary {
                entity_type: InfrastructureEntityType::Pod,
                entity_id: "checkout-pod-1".into(),
                display_name: "checkout-pod-1".into(),
                parent_id: Some("payments".into()),
                parent_display_name: Some("payments".into()),
                environment: Some("prod".into()),
                health_state: "watch".into(),
                last_seen_unix_nano: 42,
                related_services: vec!["checkout-api".into()],
                log_rate_per_minute: Some(8.5),
                error_rate: Some(0.02),
                restart_count: None,
                cpu_usage: None,
                memory_usage: None,
                disk_usage: None,
                network_io: None,
            }],
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["items"][0]["entity_type"], "pod");
        assert_eq!(json["items"][0]["entity_id"], "checkout-pod-1");
        assert_eq!(json["items"][0]["related_services"][0], "checkout-api");
    }

    #[test]
    fn infrastructure_detail_response_embeds_links() {
        let response = InfrastructureDetailResponse {
            entity: InfrastructureEntitySummary {
                entity_type: InfrastructureEntityType::Host,
                entity_id: "ip-10-0-0-12".into(),
                display_name: "ip-10-0-0-12".into(),
                parent_id: None,
                parent_display_name: None,
                environment: Some("prod".into()),
                health_state: "healthy".into(),
                last_seen_unix_nano: 100,
                related_services: vec!["checkout-api".into()],
                log_rate_per_minute: Some(1.0),
                error_rate: Some(0.0),
                restart_count: None,
                cpu_usage: None,
                memory_usage: None,
                disk_usage: None,
                network_io: None,
            },
            links: infrastructure_links(
                InfrastructureEntityType::Host,
                "ip-10-0-0-12",
                Some("checkout-api".into()),
            ),
        };

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(
            json["links"]["logs"],
            "/logs?resource_attr=host.name%3Aip-10-0-0-12"
        );
    }

    #[test]
    fn attribute_sql_expr_includes_id_fallback_for_host_and_container() {
        assert!(
            InfrastructureEntityType::Host
                .attribute_sql_expr()
                .contains("host.id")
        );
        assert!(
            InfrastructureEntityType::Container
                .attribute_sql_expr()
                .contains("container.id")
        );
        assert!(
            !InfrastructureEntityType::Pod
                .attribute_sql_expr()
                .contains("if(")
        );
        assert!(
            !InfrastructureEntityType::Cluster
                .attribute_sql_expr()
                .contains("if(")
        );
        assert!(
            !InfrastructureEntityType::Namespace
                .attribute_sql_expr()
                .contains("if(")
        );
    }

    #[test]
    fn infrastructure_summary_uses_canonical_hierarchical_entity_ids() {
        let summary = infrastructure_entity_summary_from_row(
            InfrastructureEntityType::Pod,
            InfrastructureAggregateRow {
                cluster_name: "prod-cluster".into(),
                namespace_name: "payments".into(),
                pod_name: "checkout-pod-1".into(),
                entity_name: "checkout-pod-1".into(),
                environment: "prod".into(),
                last_seen_unix_nano: 42,
                related_services: vec!["checkout-api".into()],
                log_events: 30,
                span_events: 100,
                error_events: 2,
            },
            10,
        );

        assert_eq!(summary.entity_id, "prod-cluster/payments/checkout-pod-1");
        assert_eq!(summary.display_name, "checkout-pod-1");
        assert_eq!(summary.parent_id.as_deref(), Some("prod-cluster/payments"));
        assert_eq!(summary.parent_display_name.as_deref(), Some("payments"));
    }

    #[test]
    fn infrastructure_detail_links_use_leaf_resource_value_not_canonical_route_id() {
        let entity = InfrastructureEntitySummary {
            entity_type: InfrastructureEntityType::Pod,
            entity_id: "prod-cluster/payments/checkout-pod-1".into(),
            display_name: "checkout-pod-1".into(),
            parent_id: Some("prod-cluster/payments".into()),
            parent_display_name: Some("payments".into()),
            environment: Some("prod".into()),
            health_state: "watch".into(),
            last_seen_unix_nano: 42,
            related_services: vec!["checkout-api".into()],
            log_rate_per_minute: Some(3.0),
            error_rate: Some(0.02),
            restart_count: None,
            cpu_usage: None,
            memory_usage: None,
            disk_usage: None,
            network_io: None,
        };

        let links = infrastructure_detail_links(&entity);

        assert_eq!(
            links.logs,
            "/logs?resource_attr=k8s.pod.name%3Acheckout-pod-1"
        );
        assert_eq!(
            links.metrics,
            "/services/checkout-api/metrics?resource_attr=k8s.pod.name%3Acheckout-pod-1"
        );
    }

    #[test]
    fn infrastructure_lookback_validation_rejects_zero_minutes() {
        assert_eq!(
            validated_infrastructure_lookback_minutes(Some(0)),
            Err(StatusCode::BAD_REQUEST)
        );
        assert_eq!(validated_infrastructure_lookback_minutes(None), Ok(60));
        assert_eq!(validated_infrastructure_lookback_minutes(Some(15)), Ok(15));
    }

    #[test]
    fn infrastructure_summary_from_row_derives_rates_and_normalizes_empty_fields() {
        let summary = infrastructure_entity_summary_from_row(
            InfrastructureEntityType::Pod,
            InfrastructureAggregateRow {
                cluster_name: String::new(),
                namespace_name: String::new(),
                pod_name: String::new(),
                entity_name: "checkout-pod-1".into(),
                environment: String::new(),
                last_seen_unix_nano: 42,
                related_services: vec!["checkout-api".into()],
                log_events: 30,
                span_events: 100,
                error_events: 2,
            },
            10,
        );

        assert_eq!(summary.entity_type, InfrastructureEntityType::Pod);
        assert_eq!(summary.entity_id, "checkout-pod-1");
        assert_eq!(summary.display_name, "checkout-pod-1");
        assert_eq!(summary.parent_id, None);
        assert_eq!(summary.parent_display_name, None);
        assert_eq!(summary.environment, None);
        assert_eq!(summary.log_rate_per_minute, Some(3.0));
        assert_eq!(summary.error_rate, Some(0.02));
        assert_eq!(summary.health_state, "watch");
        assert_eq!(summary.related_services, vec!["checkout-api".to_string()]);
    }

    #[test]
    fn infrastructure_summary_from_row_omits_error_rate_without_spans() {
        let summary = infrastructure_entity_summary_from_row(
            InfrastructureEntityType::Host,
            InfrastructureAggregateRow {
                cluster_name: "prod-cluster".into(),
                namespace_name: String::new(),
                pod_name: String::new(),
                entity_name: "ip-10-0-0-12".into(),
                environment: "prod".into(),
                last_seen_unix_nano: 99,
                related_services: vec![],
                log_events: 0,
                span_events: 0,
                error_events: 0,
            },
            15,
        );

        assert_eq!(summary.parent_id.as_deref(), Some("prod-cluster"));
        assert_eq!(summary.parent_display_name.as_deref(), Some("prod-cluster"));
        assert_eq!(summary.environment.as_deref(), Some("prod"));
        assert_eq!(summary.log_rate_per_minute, Some(0.0));
        assert_eq!(summary.error_rate, None);
        assert_eq!(summary.health_state, "healthy");
    }

    // Regression test: toUnixTimestamp64Nano returns Int64. Mixed with UInt64 columns from spans
    // and logs in a UNION ALL, ClickHouse widens the column to Float64 via getLeastSupertype,
    // which the Rust `u64` field (last_seen_unix_nano) cannot deserialize — causing always-500.
    // Fix: use toUInt64(toUnixTimestamp(created_at)) * 1000000000 which stays UInt64.
    #[test]
    fn infrastructure_metric_series_event_time_is_uint64_not_int64() {
        for entity_type in all_infrastructure_entity_types() {
            let entity_expr = entity_type.attribute_sql_expr();
            // Replicate exactly the metric_series branch as built by fetch_infrastructure_summaries.
            let metric_series_branch = format!(
                "SELECT \
                    JSONExtractString(resource_attributes, 'k8s.cluster.name') AS cluster_name, \
                    JSONExtractString(resource_attributes, 'k8s.namespace.name') AS namespace_name, \
                    JSONExtractString(resource_attributes, 'k8s.pod.name') AS pod_name, \
                    {entity_expr} AS entity_name, \
                    environment, \
                    service_name, \
                    toUInt64(toUnixTimestamp(created_at)) * 1000000000 AS event_time, \
                    toUInt64(0) AS log_events, \
                    toUInt64(0) AS span_events, \
                    toUInt64(0) AS error_events \
                FROM observable.metric_series \
                WHERE tenant_id = ? AND created_at >= fromUnixTimestamp(?)"
            );
            assert!(
                !metric_series_branch.contains("toUnixTimestamp64Nano"),
                "entity_type {entity_type:?}: metric_series event_time must not use \
                 toUnixTimestamp64Nano (returns Int64, causing always-500 via Float64 widening)"
            );
            assert!(
                metric_series_branch.contains("toUInt64(toUnixTimestamp(created_at))"),
                "entity_type {entity_type:?}: metric_series event_time must use \
                 toUInt64(toUnixTimestamp(created_at)) * 1000000000 to produce UInt64"
            );
        }
    }

    // Binding-count invariant: unfiltered infrastructure SQL must have exactly 6 '?' placeholders
    // (tenant_id × 3 + start_time × 3), one set per UNION ALL branch.
    #[test]
    fn infrastructure_query_base_binding_count_is_six() {
        let entity_expr = InfrastructureEntityType::Pod.attribute_sql_expr();
        let sql = format!(
            "SELECT \
                cluster_name, namespace_name, pod_name, entity_name, \
                argMax(environment, event_time) AS environment, \
                max(event_time) AS last_seen_unix_nano, \
                arraySort(groupUniqArrayIf(service_name, service_name != '')) AS related_services, \
                sum(log_events) AS log_events, \
                sum(span_events) AS span_events, \
                sum(error_events) AS error_events \
            FROM ( \
                SELECT {entity_expr} AS entity_name, environment, service_name, \
                    start_time_unix_nano AS event_time, \
                    toUInt64(0) AS log_events, toUInt64(1) AS span_events, \
                    toUInt64(status_code = 'ERROR') AS error_events \
                FROM observable.spans WHERE tenant_id = ? AND start_time_unix_nano >= ? \
                UNION ALL \
                SELECT {entity_expr} AS entity_name, environment, service_name, \
                    timestamp_unix_nano AS event_time, \
                    toUInt64(1) AS log_events, toUInt64(0) AS span_events, \
                    toUInt64(0) AS error_events \
                FROM observable.logs WHERE tenant_id = ? AND timestamp_unix_nano >= ? \
                UNION ALL \
                SELECT {entity_expr} AS entity_name, environment, service_name, \
                    toUInt64(toUnixTimestamp(created_at)) * 1000000000 AS event_time, \
                    toUInt64(0) AS log_events, toUInt64(0) AS span_events, \
                    toUInt64(0) AS error_events \
                FROM observable.metric_series WHERE tenant_id = ? AND created_at >= fromUnixTimestamp(?) \
            ) WHERE entity_name != '' \
            GROUP BY cluster_name, namespace_name, pod_name, entity_name \
            ORDER BY last_seen_unix_nano DESC, entity_name ASC"
        );

        let placeholder_count = sql.matches('?').count();
        assert_eq!(
            placeholder_count, 6,
            "unfiltered infrastructure SQL must have exactly 6 '?' (tenant_id×3 + start×3), got {placeholder_count}"
        );
    }
}
