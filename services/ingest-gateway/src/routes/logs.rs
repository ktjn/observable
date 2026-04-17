use axum::{extract::Extension, http::StatusCode, Json};
use serde_json::Value;

use crate::auth::TenantContext;

pub async fn export_logs(
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let resource_logs = body
        .get("resourceLogs")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        log_count = resource_logs.len(),
        "received log export request"
    );

    Ok(Json(serde_json::json!({})))
}
