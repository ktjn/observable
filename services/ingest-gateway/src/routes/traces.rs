use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_traces(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let resource_spans = body
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let spans = parse_otlp_traces(&body, ctx.tenant_id)?;

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        span_count = resource_spans.len(),
        "received trace export request"
    );

    if let Some(producer) = &state.producer {
        let envelope = build_envelope(ctx.tenant_id, domain::EnvelopePayload::Spans(spans));
        producer
            .publish(&envelope)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(serde_json::json!({ "partialSuccess": {} })))
}

fn parse_otlp_traces(body: &Value, tenant_id: uuid::Uuid) -> Result<Vec<domain::Span>, StatusCode> {
    let resource_spans = body
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut spans = Vec::new();
    for rs in resource_spans {
        let resource_attrs = rs
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();
        for scope_spans in rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
        {
            for s in scope_spans
                .get("spans")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                let start: u64 = s
                    .get("startTimeUnixNano")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let end: u64 = s
                    .get("endTimeUnixNano")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                spans.push(domain::Span {
                    tenant_id,
                    trace_id: s
                        .get("traceId")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    span_id: s
                        .get("spanId")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    service_name: service_name.clone(),
                    operation_name: s
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    start_time_unix_nano: start,
                    end_time_unix_nano: end,
                    duration_ns: end.saturating_sub(start),
                    ..Default::default()
                });
            }
        }
    }
    Ok(spans)
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

    #[tokio::test]
    async fn missing_auth_returns_401() {
        let app = build_router(AppState::test_stub());
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/traces")
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_empty_payload_returns_200() {
        let app = build_router(AppState::with_stub_auth(
            "00000000-0000-0000-0000-000000000001",
        ));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/traces")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
            )
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }
}
