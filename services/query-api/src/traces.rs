use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use clickhouse::Client;
use domain::{Span, SpanEvent, SpanEventRow, SpanRow};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::llm_adapter::LlmCaller;
use crate::middleware::auth::TenantContext;
use crate::planner::QueryPlanner;

#[derive(Clone)]
pub struct AppState {
    pub ch: Client,
    pub db: PgPool,
    pub planner: Arc<QueryPlanner>,
    /// Optional LLM caller. None when LLM_API_KEY is not set.
    pub llm: Option<Arc<dyn LlmCaller>>,
    /// Base URL for the auth-service internal API.
    pub auth_service_url: String,
}

#[derive(Serialize)]
pub struct TraceResponse {
    pub trace_id: String,
    pub spans: Vec<Span>,
    pub events: Vec<SpanEvent>,
}

#[derive(Serialize)]
pub struct FacetValue {
    pub value: String,
    pub count: u64,
}

#[derive(Serialize)]
pub struct TraceListResponse {
    pub traces: Vec<TraceResponse>,
    pub total: u64,
    pub facets: HashMap<String, Vec<FacetValue>>,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub service: Option<String>,
    pub limit: Option<u32>,
    pub facets: Option<String>, // Comma-separated list of fields to facet
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct TraceHistogramParams {
    pub service: Option<String>,
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub buckets: Option<u32>,
}

#[derive(Serialize)]
pub struct TraceHistogramBucket {
    pub start_ms: u64,
    pub end_ms: u64,
    pub count: u64,
}

#[derive(Serialize)]
pub struct TraceHistogramResponse {
    pub buckets: Vec<TraceHistogramBucket>,
}

pub(crate) const SELECT_COLS: &str = "tenant_id, trace_id, span_id, parent_span_id, service_name, \
    service_namespace, service_version, operation_name, span_kind, \
    start_time_unix_nano, end_time_unix_nano, duration_ns, \
    status_code, status_message, attributes, resource_attributes, \
    environment, host_id, workload, deployment_id";

pub async fn get_trace(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(trace_id): Path<String>,
) -> Result<Json<TraceResponse>, StatusCode> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM observable.spans \
         WHERE tenant_id = ? AND trace_id = ? \
         ORDER BY start_time_unix_nano \
         LIMIT 1000"
    );
    let rows: Vec<SpanRow> = state
        .ch
        .query(&sql)
        .bind(ctx.tenant_id)
        .bind(trace_id.as_str())
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "ClickHouse query error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    validate_trace_rows_for_tenant(&rows, ctx.tenant_id)?;
    if rows.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let span_count = rows.len() as i64;
    let spans: Vec<Span> = rows.into_iter().map(Span::from).collect();

    // Fetch span events for this trace
    let events_sql = "SELECT tenant_id, trace_id, span_id, event_index, name, \
        timestamp_unix_nano, attributes \
        FROM observable.span_events \
        WHERE tenant_id = ? AND trace_id = ? \
        ORDER BY span_id, event_index \
        LIMIT 10000";

    let event_rows: Vec<SpanEventRow> = state
        .ch
        .query(events_sql)
        .bind(ctx.tenant_id)
        .bind(trace_id.as_str())
        .fetch_all()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "span_events query error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let events: Vec<SpanEvent> = event_rows.into_iter().map(SpanEvent::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "trace_get",
            tenant_id: ctx.tenant_id,
            result_count: span_count,
        },
    )
    .await;
    Ok(Json(TraceResponse {
        trace_id,
        spans,
        events,
    }))
}

pub async fn search_traces(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<TraceListResponse>, StatusCode> {
    let plan = state.planner.plan_trace_search(&params);

    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let from_ns = if let Some(dt) = params.from {
        dt.timestamp_nanos_opt().unwrap_or(0) as u64
    } else {
        now_ns.saturating_sub(3_600_000_000_000) // Default 1h
    };
    let to_ns = params
        .to
        .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
        .unwrap_or(now_ns);

    // Bind the common parameters in the correct order as defined by trace_search_where_clause:
    // tenant_id, from_ns (if any), to_ns (if any), service_name (if any)
    let bind_common = |mut q: clickhouse::query::Query| {
        q = q.bind(ctx.tenant_id);
        if params.from.is_some() {
            q = q.bind(from_ns);
        }
        if params.to.is_some() {
            q = q.bind(to_ns);
        }
        if let Some(ref svc) = params.service {
            q = q.bind(svc.as_str());
        }
        q
    };

    // First, count total distinct traces.
    let count_query = bind_common(state.ch.query(&plan.count_sql));
    let total: u64 = count_query.fetch_one().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse total count error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Handle facets.
    let mut facet_results = HashMap::new();
    if let Some(ref facets_str) = params.facets {
        let requested_facets: Vec<&str> = facets_str.split(',').map(|s| s.trim()).collect();

        for field in requested_facets {
            // Validate field name to prevent SQL injection.
            let valid_fields = [
                "service_name",
                "status_code",
                "span_kind",
                "environment",
                "host_id",
            ];
            if !valid_fields.contains(&field) {
                continue;
            }

            let mut facet_sql = format!(
                "SELECT toString({field}) as value, count(DISTINCT trace_id) as count FROM observable.spans WHERE tenant_id = ?"
            );
            if params.from.is_some() {
                facet_sql.push_str(" AND start_time_unix_nano >= ?");
            }
            if params.to.is_some() {
                facet_sql.push_str(" AND start_time_unix_nano <= ?");
            }
            if params.service.is_some() {
                facet_sql.push_str(" AND service_name = ?");
            }
            facet_sql.push_str(&format!(" GROUP BY {field} ORDER BY count DESC LIMIT 10"));

            let facet_query = bind_common(state.ch.query(&facet_sql));

            let mut cursor = facet_query.fetch::<(String, u64)>().map_err(|e| {
                tracing::error!("ClickHouse facet query error: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let mut values = Vec::new();
            while let Some((value, count)) = cursor.next().await.map_err(|e| {
                tracing::error!("ClickHouse facet fetch error: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })? {
                values.push(FacetValue { value, count });
            }
            facet_results.insert(field.to_string(), values);
        }
    }

    // Then, fetch the newest span for each of the newest N traces.
    debug_assert_eq!(
        trace_search_common_bind_count(&params) + 1,
        plan.spans_sql.matches('?').count()
    );
    let mut query = state.ch.query(&plan.spans_sql);
    query = bind_common(query);
    query = query.bind(plan.limit);

    let rows: Vec<SpanRow> = query.fetch_all().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse search traces error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    validate_trace_rows_for_tenant(&rows, ctx.tenant_id)?;

    // Grouping by trace_id to handle potential (though rare) duplicate timestamps.
    let mut seen: HashSet<String> = HashSet::new();
    let mut traces: Vec<TraceResponse> = Vec::new();
    for row in rows {
        let trace_id = row.trace_id.clone();
        if seen.insert(trace_id.clone()) {
            traces.push(TraceResponse {
                trace_id,
                spans: vec![Span::from(row)],
                events: vec![],
            });
        }
    }

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "trace_search",
            tenant_id: ctx.tenant_id,
            result_count: traces.len() as i64,
        },
    )
    .await;

    Ok(Json(TraceListResponse {
        traces,
        total,
        facets: facet_results,
    }))
}

pub async fn trace_histogram(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<TraceHistogramParams>,
) -> Result<Json<TraceHistogramResponse>, StatusCode> {
    use chrono::Utc;
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let from_ns = params
        .from
        .unwrap_or_else(|| now_ns.saturating_sub(3_600_000_000_000));
    let to_ns = params.to.unwrap_or(now_ns);
    let bucket_count = params.buckets.unwrap_or(30).clamp(1, 200);

    let plan =
        state
            .planner
            .plan_trace_histogram(from_ns, to_ns, params.service.as_deref(), bucket_count);

    let mut query = state
        .ch
        .query(&plan.sql)
        .bind(plan.from_ns)
        .bind(plan.interval_ns)
        .bind(ctx.tenant_id)
        .bind(from_ns)
        .bind(to_ns);
    if let Some(service) = &params.service {
        query = query.bind(service);
    }

    let mut cursor = query.fetch::<(i64, i32, u64)>().map_err(|e| {
        tracing::error!("ClickHouse trace histogram query error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut raw: HashMap<i64, u64> = HashMap::new();
    while let Some((bucket_idx, _dummy, count)) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse trace histogram fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        if bucket_idx >= 0 && bucket_idx < bucket_count as i64 {
            *raw.entry(bucket_idx).or_default() += count;
        }
    }

    let buckets = (0..bucket_count)
        .map(|i| {
            let start_ns = plan.from_ns + i as u64 * plan.interval_ns;
            let end_ns = start_ns + plan.interval_ns;
            TraceHistogramBucket {
                start_ms: start_ns / 1_000_000,
                end_ms: end_ns / 1_000_000,
                count: raw.remove(&(i as i64)).unwrap_or_default(),
            }
        })
        .collect();

    Ok(Json(TraceHistogramResponse { buckets }))
}

/// Repository-level fetch used by integration tests to verify tenant-filter correctness.
#[allow(dead_code)]
pub async fn fetch_trace_spans(
    ch: &Client,
    tenant_id: uuid::Uuid,
    trace_id: &str,
) -> anyhow::Result<Vec<SpanRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM observable.spans \
         WHERE tenant_id = ? AND trace_id = ? \
         ORDER BY start_time_unix_nano \
         LIMIT 1000"
    );
    let rows: Vec<SpanRow> = ch
        .query(&sql)
        .bind(tenant_id)
        .bind(trace_id)
        .fetch_all()
        .await?;
    Ok(rows)
}

fn validate_trace_rows_for_tenant(
    rows: &[SpanRow],
    tenant_id: uuid::Uuid,
) -> Result<(), StatusCode> {
    if rows.iter().all(|row| row.tenant_id == tenant_id) {
        return Ok(());
    }

    tracing::error!(
        expected_tenant_id = %tenant_id,
        "trace query returned rows outside tenant context"
    );
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

fn trace_search_common_bind_count(params: &SearchParams) -> usize {
    let mut count = 1;
    if params.from.is_some() {
        count += 1;
    }
    if params.to.is_some() {
        count += 1;
    }
    if params.service.is_some() {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{SpanKind, StatusCode as SpanStatus};
    use uuid::Uuid;

    #[test]
    fn span_row_converts_status() {
        let mut row = make_row("INTERNAL", "ERROR");
        row.status_code = "ERROR".into();
        let span = Span::from(row);
        assert!(matches!(span.status_code, SpanStatus::Error));
    }

    #[test]
    fn span_row_converts_span_kind() {
        let row = make_row("SERVER", "OK");
        let span = Span::from(row);
        assert!(matches!(span.span_kind, SpanKind::Server));
    }

    #[test]
    fn trace_response_serializes() {
        let resp = TraceResponse {
            trace_id: "abc123".into(),
            spans: vec![],
            events: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("events"));
    }

    #[test]
    fn trace_rows_validate_for_same_tenant() {
        let tenant_id = Uuid::new_v4();
        let rows = vec![make_tenant_row(tenant_id), make_tenant_row(tenant_id)];

        let result = validate_trace_rows_for_tenant(&rows, tenant_id);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn trace_rows_reject_cross_tenant_result() {
        let tenant_id = Uuid::new_v4();
        let other_tenant_id = Uuid::new_v4();
        let rows = vec![make_tenant_row(tenant_id), make_tenant_row(other_tenant_id)];

        let result = validate_trace_rows_for_tenant(&rows, tenant_id);

        assert_eq!(result, Err(StatusCode::INTERNAL_SERVER_ERROR));
    }

    fn make_row(span_kind: &str, status_code: &str) -> SpanRow {
        let mut row = make_tenant_row(Uuid::new_v4());
        row.span_kind = span_kind.into();
        row.status_code = status_code.into();
        row
    }

    fn make_tenant_row(tenant_id: Uuid) -> SpanRow {
        SpanRow {
            tenant_id,
            trace_id: "abc".into(),
            span_id: "def".into(),
            parent_span_id: None,
            service_name: "svc".into(),
            service_namespace: "".into(),
            service_version: "".into(),
            operation_name: "op".into(),
            span_kind: "INTERNAL".into(),
            start_time_unix_nano: 0,
            end_time_unix_nano: 1_000_000,
            duration_ns: 1_000_000,
            status_code: "OK".into(),
            status_message: "".into(),
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            environment: "".into(),
            host_id: "".into(),
            workload: "".into(),
            deployment_id: "".into(),
        }
    }

    // SELECT_COLS must name exactly the same columns that SpanRow deserializes.  A mismatch
    // means the query-api reads different fields than the storage-writer inserts, producing
    // runtime deserialization failures (500s) that are invisible at compile time.
    #[test]
    fn select_cols_names_match_span_row_field_count() {
        let col_count = SELECT_COLS.split(',').count();
        // SpanRow has 20 fields: tenant_id trace_id span_id parent_span_id service_name
        // service_namespace service_version operation_name span_kind start_time_unix_nano
        // end_time_unix_nano duration_ns status_code status_message attributes
        // resource_attributes environment host_id workload deployment_id
        assert_eq!(
            col_count, 20,
            "SELECT_COLS has {col_count} columns but SpanRow has 20 fields; keep them in sync"
        );
    }

    #[test]
    fn trace_response_includes_events_field() {
        let resp = TraceResponse {
            trace_id: "abc".into(),
            spans: vec![],
            events: vec![SpanEvent {
                tenant_id: Uuid::new_v4(),
                trace_id: "abc".into(),
                span_id: "def".into(),
                event_index: 0,
                name: "exception".into(),
                timestamp_unix_nano: 1_700_000_000_000_000_000,
                attributes: std::collections::HashMap::new(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"events\""));
        assert!(json.contains("exception"));
    }

    // The count query used by search_traces must not include LIMIT so that it returns a
    // true total, not just the count within the current page.
    #[test]
    fn trace_count_sql_has_no_limit_clause() {
        use crate::planner::QueryPlanner;
        let planner = QueryPlanner;
        for service in [None, Some("checkout".to_string())] {
            let params = SearchParams {
                service,
                limit: Some(10),
                facets: None,
                from: None,
                to: None,
            };
            let plan = planner.plan_trace_search(&params);
            assert!(
                !plan.count_sql.to_uppercase().contains("LIMIT"),
                "count_sql must not contain LIMIT: {}",
                plan.count_sql
            );
        }
    }

    #[test]
    fn trace_search_spans_bind_count_matches_generated_sql() {
        use crate::planner::QueryPlanner;
        let planner = QueryPlanner;
        let params = SearchParams {
            service: Some("checkout".to_string()),
            limit: Some(10),
            facets: None,
            from: Some(Utc::now()),
            to: Some(Utc::now()),
        };
        let plan = planner.plan_trace_search(&params);

        assert_eq!(
            trace_search_common_bind_count(&params) + 1,
            plan.spans_sql.matches('?').count(),
            "spans_sql must bind one common filter group plus the subquery LIMIT: {}",
            plan.spans_sql
        );
    }
}
