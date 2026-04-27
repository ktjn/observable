use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use clickhouse::Client;
use domain::{Span, SpanRow};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::middleware::auth::TenantContext;
use crate::planner::QueryPlanner;

#[derive(Clone)]
pub struct AppState {
    pub ch: Client,
    pub db: PgPool,
    pub planner: Arc<QueryPlanner>,
}

#[derive(Serialize)]
pub struct TraceResponse {
    pub trace_id: String,
    pub spans: Vec<Span>,
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
        "SELECT {SELECT_COLS} FROM spans \
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
    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "trace_get",
            tenant_id: ctx.tenant_id,
            result_count: span_count,
        },
    )
    .await;
    Ok(Json(TraceResponse { trace_id, spans }))
}

pub async fn search_traces(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<TraceListResponse>, StatusCode> {
    let plan = state.planner.plan_trace_search(&params);

    // First, count total distinct traces.
    let mut count_query = state.ch.query(&plan.count_sql).bind(ctx.tenant_id);
    if let Some(ref svc) = params.service {
        count_query = count_query.bind(svc.as_str());
    }
    let total: u64 = count_query.fetch_one().await.map_err(|e| {
        tracing::error!(error = ?e, "ClickHouse total count error");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Handle facets.
    let mut facet_results = HashMap::new();
    if let Some(facets_str) = params.facets {
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
                "SELECT toString({field}) as value, count(DISTINCT trace_id) as count FROM spans WHERE tenant_id = ?"
            );
            if params.service.is_some() {
                facet_sql.push_str(" AND service_name = ?");
            }
            facet_sql.push_str(&format!(" GROUP BY {field} ORDER BY count DESC LIMIT 10"));

            let mut facet_query = state.ch.query(&facet_sql).bind(ctx.tenant_id);
            if let Some(ref svc) = params.service {
                facet_query = facet_query.bind(svc.as_str());
            }

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
    let mut query = state.ch.query(&plan.spans_sql).bind(ctx.tenant_id);
    if let Some(ref svc) = params.service {
        query = query.bind(svc.as_str());
    }
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
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
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
            };
            let plan = planner.plan_trace_search(&params);
            assert!(
                !plan.count_sql.to_uppercase().contains("LIMIT"),
                "count_sql must not contain LIMIT: {}",
                plan.count_sql
            );
        }
    }
}
