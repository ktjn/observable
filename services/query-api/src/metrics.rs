use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use domain::{MetricPoint, MetricPointRow, MetricSeries, MetricSeriesRow};
use serde::Serialize;

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

pub async fn list_metrics(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<MetricSeriesListResponse>, StatusCode> {
    let mut cursor = state
        .ch
        .query("SELECT ?fields FROM metric_series WHERE tenant_id = ?")
        .bind(ctx.tenant_id)
        .fetch::<MetricSeriesRow>()
        .map_err(|e| {
            tracing::error!("ClickHouse query error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut series = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        series.push(MetricSeries::from(row));
    }

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

    let mut points = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        tracing::error!("ClickHouse fetch error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        points.push(MetricPoint::from(row));
    }

    Ok(Json(MetricPointsResponse { points }))
}
