use axum::{
    extract::{Extension, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use uuid::Uuid;

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_logs(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Response {
    if state.log_rate_limiter.check_key(&ctx.tenant_id).is_err() {
        tracing::warn!(tenant_id = %ctx.tenant_id, "log ingest rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(serde_json::json!({
                "error": "rate_limit_exceeded",
                "message": "Log ingest rate limit exceeded"
            })),
        )
            .into_response();
    }

    let resource_logs = match body.get("resourceLogs").and_then(|v| v.as_array()) {
        Some(s) => s,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let logs = match parse_otlp_logs(&body, ctx.tenant_id) {
        Ok(l) => l,
        Err(status) => return status.into_response(),
    };

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        log_count = resource_logs.len(),
        "received log export request"
    );

    if let Some(producer) = &state.producer {
        let envelope = build_envelope(ctx.tenant_id, domain::EnvelopePayload::Logs(logs));
        if producer.publish(&envelope).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    Json(serde_json::json!({})).into_response()
}

fn parse_otlp_logs(body: &Value, tenant_id: Uuid) -> Result<Vec<domain::LogRecord>, StatusCode> {
    let resource_logs = body
        .get("resourceLogs")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut logs = Vec::new();
    for rl in resource_logs {
        let resource_attrs = rl
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();
        for scope_logs in rl
            .get("scopeLogs")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
        {
            for lr in scope_logs
                .get("logRecords")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                logs.push(domain::LogRecord {
                    tenant_id,
                    log_id: Uuid::new_v4(),
                    timestamp_unix_nano: lr
                        .get("timeUnixNano")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    observed_timestamp_unix_nano: 0,
                    severity_number: lr
                        .get("severityNumber")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32,
                    severity_text: lr
                        .get("severityText")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    body: lr.get("body").cloned().unwrap_or(Value::Null),
                    service_name: service_name.clone(),
                    ..Default::default()
                });
            }
        }
    }
    Ok(logs)
}

fn extract_string_attr(attrs: &Value, key: &str) -> Option<String> {
    attrs
        .as_array()?
        .iter()
        .find(|a| a.get("key").and_then(|k| k.as_str()) == Some(key))?
        .get("value")?
        .get("stringValue")?
        .as_str()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;

    use crate::build_router;
    use crate::AppState;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

    fn auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
        )
    }

    fn simple_log_payload() -> serde_json::Value {
        serde_json::json!({
            "resourceLogs": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "test-svc"}}]},
                "scopeLogs": [{
                    "logRecords": [{"timeUnixNano": "1000", "severityText": "INFO", "body": {"stringValue": "hello"}}]
                }]
            }]
        })
    }

    #[tokio::test]
    async fn logs_export_returns_200() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/logs")
            .add_header(auth_header().0, auth_header().1)
            .json(&simple_log_payload())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn exceeding_rate_limit_returns_429() {
        let app = build_router(AppState::with_stub_auth_and_rate_limit(TENANT, 1));
        let server = TestServer::new(app).unwrap();

        let first = server
            .post("/v1/logs")
            .add_header(auth_header().0, auth_header().1)
            .json(&simple_log_payload())
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);

        let second = server
            .post("/v1/logs")
            .add_header(auth_header().0, auth_header().1)
            .json(&simple_log_payload())
            .await;
        assert_eq!(second.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()["retry-after"], "1");
        let body: serde_json::Value = second.json();
        assert_eq!(body["error"], "rate_limit_exceeded");
    }
}
