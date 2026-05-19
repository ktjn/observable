use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use domain::{MetricPoint, MetricPointRow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

#[derive(Serialize)]
pub struct MetricCatalogResponse {
    pub metrics: Vec<MetricCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricCatalogEntry {
    pub tenant_id: Uuid,
    pub metric_name: String,
    pub description: String,
    pub unit: String,
    pub metric_type: String,
    pub is_monotonic: Option<bool>,
    pub aggregation_temporality: Option<String>,
    pub service_name: String,
    pub environment: String,
    pub series_count: u64,
}

#[derive(Debug, Clone, Deserialize, clickhouse::Row)]
struct MetricCatalogRow {
    #[serde(with = "clickhouse::serde::uuid")]
    tenant_id: Uuid,
    metric_name: String,
    description: String,
    unit: String,
    metric_type: String,
    is_monotonic: Option<u8>,
    aggregation_temporality: Option<String>,
    service_name: String,
    environment: String,
    series_count: u64,
}

#[derive(Serialize)]
pub struct MetricPointsResponse {
    pub points: Vec<MetricPoint>,
}

#[derive(Debug, Clone, Deserialize, clickhouse::Row)]
struct MetricGroupPointRow {
    #[serde(with = "clickhouse::serde::uuid")]
    tenant_id: Uuid,
    metric_name: String,
    service_name: String,
    time_unix_nano: u64,
    start_time_unix_nano: Option<u64>,
    value_double: f64,
    value_int: Option<i64>,
    histogram_count: Option<u64>,
    histogram_sum: Option<f64>,
}

#[derive(Deserialize)]
pub struct MetricListParams {
    pub service: Option<String>,
}

#[derive(Deserialize)]
pub struct MetricGroupPointParams {
    pub metric_name: String,
    pub service: String,
    pub environment: String,
    pub metric_type: String,
    pub unit: Option<String>,
}

pub async fn list_metrics(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<MetricListParams>,
) -> Result<Json<MetricCatalogResponse>, StatusCode> {
    let sql = if params.service.is_some() {
        "SELECT
            tenant_id,
            metric_name,
            any(description) AS description,
            unit,
            metric_type,
            is_monotonic,
            aggregation_temporality,
            service_name,
            environment,
            countDistinct(metric_series_id) AS series_count
         FROM observable.metric_series
         WHERE tenant_id = ? AND service_name = ?
         GROUP BY
            tenant_id,
            metric_name,
            unit,
            metric_type,
            is_monotonic,
            aggregation_temporality,
            service_name,
            environment
         ORDER BY metric_name ASC"
    } else {
        "SELECT
            tenant_id,
            metric_name,
            any(description) AS description,
            unit,
            metric_type,
            is_monotonic,
            aggregation_temporality,
            service_name,
            environment,
            countDistinct(metric_series_id) AS series_count
         FROM observable.metric_series
         WHERE tenant_id = ?
         GROUP BY
            tenant_id,
            metric_name,
            unit,
            metric_type,
            is_monotonic,
            aggregation_temporality,
            service_name,
            environment
         ORDER BY service_name ASC, metric_name ASC"
    };
    let mut query = state.ch.query(sql).bind(ctx.tenant_id);
    if let Some(service) = &params.service {
        query = query.bind(service);
    }

    let mut cursor = query.fetch::<MetricCatalogRow>().map_err(|e| {
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

    validate_metric_catalog_rows_for_tenant(&rows, ctx.tenant_id)?;

    let result_count = rows.len() as i64;
    let metrics = rows.into_iter().map(MetricCatalogEntry::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "metric_series_list",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(MetricCatalogResponse { metrics }))
}

pub async fn get_metric_group_points(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Query(params): Query<MetricGroupPointParams>,
) -> Result<Json<MetricPointsResponse>, StatusCode> {
    let unit = params.unit.unwrap_or_default();
    let mut cursor = state
        .ch
        .query(
            "SELECT
                mp.tenant_id AS tenant_id,
                any(ms.metric_name) AS metric_name,
                any(ms.service_name) AS service_name,
                mp.time_unix_nano AS time_unix_nano,
                min(mp.start_time_unix_nano) AS start_time_unix_nano,
                if(
                    ? = 'gauge',
                    avg(ifNull(mp.value_double, toFloat64(ifNull(mp.value_int, 0)))),
                    sum(ifNull(mp.value_double, toFloat64(ifNull(mp.value_int, 0))))
                ) AS value_double,
                CAST(NULL, 'Nullable(Int64)') AS value_int,
                if(
                    sum(ifNull(mp.histogram_count, 0)) = 0,
                    NULL,
                    sum(ifNull(mp.histogram_count, 0))
                ) AS histogram_count,
                if(
                    sum(ifNull(mp.histogram_sum, 0.0)) = 0,
                    NULL,
                    sum(ifNull(mp.histogram_sum, 0.0))
                ) AS histogram_sum
             FROM observable.metric_points mp
             INNER JOIN observable.metric_series ms
                ON ms.tenant_id = mp.tenant_id
               AND ms.metric_series_id = mp.metric_series_id
             WHERE ms.tenant_id = ?
               AND ms.metric_name = ?
               AND ms.service_name = ?
               AND ms.environment = ?
               AND ms.metric_type = ?
               AND ms.unit = ?
             GROUP BY mp.tenant_id, mp.time_unix_nano
             ORDER BY mp.time_unix_nano ASC",
        )
        .bind(params.metric_type.clone())
        .bind(ctx.tenant_id)
        .bind(params.metric_name)
        .bind(params.service)
        .bind(params.environment)
        .bind(params.metric_type)
        .bind(unit)
        .fetch::<MetricGroupPointRow>()
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

    validate_metric_group_point_rows_for_tenant(&rows, ctx.tenant_id)?;

    let result_count = rows.len() as i64;
    let points = rows.into_iter().map(MetricPoint::from).collect();

    crate::audit::write(
        &state.db,
        &crate::audit::QueryAuditEntry {
            action: "metric_group_points_get",
            tenant_id: ctx.tenant_id,
            result_count,
        },
    )
    .await;

    Ok(Json(MetricPointsResponse { points }))
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

fn validate_metric_catalog_rows_for_tenant(
    rows: &[MetricCatalogRow],
    tenant_id: Uuid,
) -> Result<(), StatusCode> {
    if rows.iter().all(|row| row.tenant_id == tenant_id) {
        return Ok(());
    }
    tracing::error!(
        expected_tenant_id = %tenant_id,
        "metric catalog query returned rows outside tenant context"
    );
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

impl From<MetricCatalogRow> for MetricCatalogEntry {
    fn from(row: MetricCatalogRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            metric_name: row.metric_name,
            description: row.description,
            unit: row.unit,
            metric_type: row.metric_type,
            is_monotonic: row.is_monotonic.map(|value| value != 0),
            aggregation_temporality: row.aggregation_temporality,
            service_name: row.service_name,
            environment: row.environment,
            series_count: row.series_count,
        }
    }
}

impl From<MetricGroupPointRow> for MetricPoint {
    fn from(row: MetricGroupPointRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            metric_series_id: Uuid::nil(),
            metric_name: row.metric_name,
            service_name: row.service_name,
            time_unix_nano: row.time_unix_nano,
            start_time_unix_nano: row.start_time_unix_nano,
            value_double: Some(row.value_double),
            value_int: row.value_int,
            histogram_count: row.histogram_count,
            histogram_sum: row.histogram_sum,
            histogram_bucket_counts: None,
            histogram_explicit_bounds: None,
        }
    }
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

fn validate_metric_group_point_rows_for_tenant(
    rows: &[MetricGroupPointRow],
    tenant_id: Uuid,
) -> Result<(), StatusCode> {
    if rows.iter().all(|row| row.tenant_id == tenant_id) {
        return Ok(());
    }
    tracing::error!(
        expected_tenant_id = %tenant_id,
        "metric group point query returned rows outside tenant context"
    );
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_series_row(tenant_id: Uuid) -> MetricCatalogRow {
        MetricCatalogRow {
            tenant_id,
            metric_name: "requests_total".into(),
            description: String::new(),
            unit: String::new(),
            metric_type: "sum".into(),
            is_monotonic: None,
            aggregation_temporality: None,
            service_name: "svc".into(),
            environment: String::new(),
            series_count: 1,
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
            validate_metric_catalog_rows_for_tenant(&rows, tenant_id),
            Ok(())
        );
    }

    #[test]
    fn metric_series_rows_reject_cross_tenant_result() {
        let tenant_id = Uuid::new_v4();
        let other = Uuid::new_v4();
        let rows = vec![make_series_row(tenant_id), make_series_row(other)];
        assert_eq!(
            validate_metric_catalog_rows_for_tenant(&rows, tenant_id),
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        );
    }

    #[test]
    fn empty_metric_series_rows_are_valid() {
        let tenant_id = Uuid::new_v4();
        assert_eq!(
            validate_metric_catalog_rows_for_tenant(&[], tenant_id),
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
