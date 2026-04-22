use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use domain::{MetricPoint, MetricPointRow, MetricSeries, MetricSeriesRow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

#[derive(Serialize)]
pub struct MetricSeriesListResponse {
    pub series: Vec<MetricSeries>,
}

#[derive(Serialize)]
pub struct MetricPointsResponse {
    pub points: Vec<MetricPoint>,
}

#[derive(Deserialize)]
pub struct MetricListParams {
    pub service: Option<String>,
}

pub async fn list_metrics(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<MetricListParams>,
) -> Result<Json<MetricSeriesListResponse>, StatusCode> {
    let sql = if params.service.is_some() {
        "SELECT ?fields FROM metric_series WHERE tenant_id = ? AND service_name = ?"
    } else {
        "SELECT ?fields FROM metric_series WHERE tenant_id = ?"
    };
    let mut query = state.ch.query(sql).bind(ctx.tenant_id);
    if let Some(service) = &params.service {
        query = query.bind(service);
    }

    let mut cursor = query.fetch::<MetricSeriesRow>().map_err(|e| {
        tracing::error!("ClickHouse query error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut rows = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        rows.push(row);
    }

    validate_metric_series_rows_for_tenant(&rows, ctx.tenant_id)?;

    let result_count = rows.len() as i64;
    let series = rows.into_iter().map(MetricSeries::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "metric_series_list",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(MetricSeriesListResponse { series }))
}

pub async fn get_metric_points(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(series_id): Path<uuid::Uuid>,
) -> Result<Json<MetricPointsResponse>, StatusCode> {
    let mut cursor = state
        .ch
        .query("SELECT ?fields FROM metric_points WHERE tenant_id = ? AND metric_series_id = ? ORDER BY time_unix_nano ASC")
        .bind(ctx.tenant_id)
        .bind(series_id)
        .fetch::<MetricPointRow>()
        .map_err(|e| {
            tracing::error!("ClickHouse query error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut rows = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        rows.push(row);
    }

    validate_metric_point_rows_for_tenant(&rows, ctx.tenant_id)?;

    let result_count = rows.len() as i64;
    let points = rows.into_iter().map(MetricPoint::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "metric_points_get",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(MetricPointsResponse { points }))
}

fn validate_metric_series_rows_for_tenant(
    rows: &[MetricSeriesRow],
    tenant_id: Uuid,
) -> Result<(), StatusCode> {
    if rows.iter().all(|row| row.tenant_id == tenant_id) {
        return Ok(());
    }
    tracing::error!(
        expected_tenant_id = %tenant_id,
        "metric series query returned rows outside tenant context"
    );
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

fn validate_metric_point_rows_for_tenant(
    rows: &[MetricPointRow],
    tenant_id: Uuid,
) -> Result<(), StatusCode> {
    if rows.iter().all(|row| row.tenant_id == tenant_id) {
        return Ok(());
    }
    tracing::error!(
        expected_tenant_id = %tenant_id,
        "metric point query returned rows outside tenant context"
    );
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_series_row(tenant_id: Uuid) -> MetricSeriesRow {
        MetricSeriesRow {
            tenant_id,
            metric_series_id: Uuid::new_v4(),
            metric_name: "requests_total".into(),
            description: String::new(),
            unit: String::new(),
            metric_type: "sum".into(),
            is_monotonic: None,
            aggregation_temporality: None,
            attributes: "{}".into(),
            resource_attributes: "{}".into(),
            service_name: "svc".into(),
            environment: String::new(),
        }
    }

    fn make_point_row(tenant_id: Uuid) -> MetricPointRow {
        MetricPointRow {
            tenant_id,
            metric_series_id: Uuid::new_v4(),
            metric_name: "requests_total".into(),
            service_name: "svc".into(),
            time_unix_nano: 0,
            start_time_unix_nano: None,
            value_double: Some(1.0),
            value_int: None,
            histogram_count: None,
            histogram_sum: None,
            histogram_bucket_counts: Vec::new(),
            histogram_explicit_bounds: Vec::new(),
        }
    }

    #[test]
    fn metric_series_rows_validate_for_same_tenant() {
        let tenant_id = Uuid::new_v4();
        let rows = vec![make_series_row(tenant_id), make_series_row(tenant_id)];
        assert_eq!(
            validate_metric_series_rows_for_tenant(&rows, tenant_id),
            Ok(())
        );
    }

    #[test]
    fn metric_series_rows_reject_cross_tenant_result() {
        let tenant_id = Uuid::new_v4();
        let other = Uuid::new_v4();
        let rows = vec![make_series_row(tenant_id), make_series_row(other)];
        assert_eq!(
            validate_metric_series_rows_for_tenant(&rows, tenant_id),
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        );
    }

    #[test]
    fn empty_metric_series_rows_are_valid() {
        let tenant_id = Uuid::new_v4();
        assert_eq!(
            validate_metric_series_rows_for_tenant(&[], tenant_id),
            Ok(())
        );
    }

    #[test]
    fn metric_point_rows_validate_for_same_tenant() {
        let tenant_id = Uuid::new_v4();
        let rows = vec![make_point_row(tenant_id), make_point_row(tenant_id)];
        assert_eq!(
            validate_metric_point_rows_for_tenant(&rows, tenant_id),
            Ok(())
        );
    }

    #[test]
    fn metric_point_rows_reject_cross_tenant_result() {
        let tenant_id = Uuid::new_v4();
        let other = Uuid::new_v4();
        let rows = vec![make_point_row(tenant_id), make_point_row(other)];
        assert_eq!(
            validate_metric_point_rows_for_tenant(&rows, tenant_id),
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        );
    }

    #[test]
    fn empty_metric_point_rows_are_valid() {
        let tenant_id = Uuid::new_v4();
        assert_eq!(
            validate_metric_point_rows_for_tenant(&[], tenant_id),
            Ok(())
        );
    }

    #[test]
    fn metric_list_params_accept_service_filter() {
        let params = MetricListParams {
            service: Some("checkout".into()),
        };

        assert_eq!(params.service.as_deref(), Some("checkout"));
    }
}
