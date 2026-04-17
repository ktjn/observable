use axum::{extract::Extension, http::StatusCode, Json};
use serde_json::Value;

use crate::auth::TenantContext;

pub async fn export_metrics(
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let resource_metrics = body
        .get("resourceMetrics")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        metric_count = resource_metrics.len(),
        "received metrics export request"
    );

    Ok(Json(serde_json::json!({})))
}
