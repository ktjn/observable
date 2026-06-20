use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};
use chrono::Utc;
use clickhouse::Client;
use serde::Serialize;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

/// How far back to look for a tenant's first ingested signal — matches the
/// onboarding wizard's original client-side polling window (60 minutes).
const LOOKBACK_NANOS: u64 = 3_600_000_000_000;

#[derive(Serialize, PartialEq, Debug)]
pub struct SetupStatusResponse {
    pub state: &'static str,
    pub traces: u64,
    pub logs: u64,
    pub metrics: u64,
}

pub async fn compute_setup_status(
    ch: &Client,
    tenant_id: Uuid,
) -> anyhow::Result<SetupStatusResponse> {
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
    let since_ns = now_ns.saturating_sub(LOOKBACK_NANOS);
    let since_secs = since_ns / 1_000_000_000;

    let traces: u64 = ch
        .query(
            "SELECT count(DISTINCT trace_id) FROM observable.spans \
             WHERE tenant_id = ? AND start_time_unix_nano >= ?",
        )
        .bind(tenant_id)
        .bind(since_ns)
        .fetch_one()
        .await?;

    let logs: u64 = ch
        .query(
            "SELECT count() FROM observable.logs \
             WHERE tenant_id = ? AND timestamp_unix_nano >= ?",
        )
        .bind(tenant_id)
        .bind(since_ns)
        .fetch_one()
        .await?;

    let metrics: u64 = ch
        .query(
            "SELECT count() FROM observable.metric_series \
             WHERE tenant_id = ? AND created_at >= fromUnixTimestamp(?)",
        )
        .bind(tenant_id)
        .bind(since_secs)
        .fetch_one()
        .await?;

    let state = if traces + logs + metrics > 0 {
        "detected"
    } else {
        "waiting"
    };

    Ok(SetupStatusResponse {
        state,
        traces,
        logs,
        metrics,
    })
}

pub async fn get_setup_status(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<SetupStatusResponse>, StatusCode> {
    let status = compute_setup_status(&state.ch, ctx.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "setup_status query error");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_serializes_detected_state() {
        let r = SetupStatusResponse {
            state: "detected",
            traces: 2,
            logs: 0,
            metrics: 1,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["state"], "detected");
        assert_eq!(v["traces"], 2);
        assert_eq!(v["logs"], 0);
        assert_eq!(v["metrics"], 1);
    }

    #[test]
    fn response_serializes_waiting_state() {
        let r = SetupStatusResponse {
            state: "waiting",
            traces: 0,
            logs: 0,
            metrics: 0,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["state"], "waiting");
    }
}
