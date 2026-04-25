use axum::{
    extract::{Extension, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_traces(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Response {
    if state.trace_rate_limiter.check_key(&ctx.tenant_id).is_err() {
        tracing::warn!(tenant_id = %ctx.tenant_id, "trace ingest rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(serde_json::json!({
                "error": "rate_limit_exceeded",
                "message": "Trace ingest rate limit exceeded"
            })),
        )
            .into_response();
    }

    let resource_spans = match body.get("resourceSpans").and_then(|v| v.as_array()) {
        Some(s) => s,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let spans = match parse_otlp_traces(&body, ctx.tenant_id) {
        Ok(s) => s,
        Err(status) => return status.into_response(),
    };

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        span_count = resource_spans.len(),
        "received trace export request"
    );

    if let Some(producer) = &state.producer {
        if producer
            .publish(&build_envelope(
                ctx.tenant_id,
                domain::EnvelopePayload::Spans(spans),
            ))
            .await
            .is_err()
        {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    Json(serde_json::json!({ "partialSuccess": {} })).into_response()
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
        let service_name = super::convert::extract_string_attr(&resource_attrs, "service.name")
            .unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;

    use crate::build_router;
    use crate::AppState;

    fn auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
        )
    }

    fn viewer_auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-viewer-key-0000"),
        )
    }

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

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
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn within_rate_limit_returns_200() {
        // quota of 2/s: first two requests succeed
        let app = build_router(AppState::with_stub_auth_and_rate_limit(TENANT, 2));
        let server = TestServer::new(app).unwrap();
        for _ in 0..2 {
            let resp = server
                .post("/v1/traces")
                .add_header(auth_header().0, auth_header().1)
                .json(&serde_json::json!({"resourceSpans": []}))
                .await;
            assert_eq!(resp.status_code(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn exceeding_rate_limit_returns_429() {
        // quota of 1/s: second request in same second is rejected
        let app = build_router(AppState::with_stub_auth_and_rate_limit(TENANT, 1));
        let server = TestServer::new(app).unwrap();

        let first = server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);

        let second = server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(second.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()["retry-after"], "1");
        let body: serde_json::Value = second.json();
        assert_eq!(body["error"], "rate_limit_exceeded");
    }

    #[tokio::test]
    async fn rate_limit_is_per_tenant() {
        // tenant A exhausts its quota; tenant B should still succeed
        let state_a = AppState::with_stub_auth_and_rate_limit(TENANT, 1);
        let app = build_router(state_a);
        let server = TestServer::new(app).unwrap();

        // exhaust tenant A
        server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        let rate_limited = server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(rate_limited.status_code(), StatusCode::TOO_MANY_REQUESTS);

        // tenant B uses a separate limiter instance (different AppState)
        const TENANT_B: &str = "00000000-0000-0000-0000-000000000002";
        let state_b = AppState::with_stub_auth_and_rate_limit(TENANT_B, 1);
        let app_b = build_router(state_b);
        let server_b = TestServer::new(app_b).unwrap();
        let resp_b = server_b
            .post("/v1/traces")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
            )
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(resp_b.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn viewer_role_cannot_ingest_traces() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/traces")
            .add_header(viewer_auth_header().0, viewer_auth_header().1)
            .json(&serde_json::json!({"resourceSpans": []}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::FORBIDDEN);
    }
}
