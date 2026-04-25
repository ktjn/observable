use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_traces(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    body: Bytes,
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

    let (span_count, spans) = match super::decode_json_otlp_request(&headers, body) {
        Ok(body) => {
            let resource_spans = match body.get("resourceSpans").and_then(|v| v.as_array()) {
                Some(s) => s,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };

            let spans = match super::convert::parse_otlp_traces(&body, ctx.tenant_id) {
                Ok(s) => s,
                Err(status) => return status.into_response(),
            };

            (resource_spans.len(), spans)
        }
        Err(status) => return status.into_response(),
    };

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        span_count,
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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;

    use crate::http_json::build_router;
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

    fn gzip_json(value: serde_json::Value) -> Vec<u8> {
        let expected = serde_json::json!({"resourceSpans": []});
        assert_eq!(value, expected);
        vec![
            31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 170, 86, 42, 74, 45, 206, 47, 45, 74, 78, 13, 46, 72,
            204, 43, 86, 178, 138, 142, 173, 5, 0, 0, 0, 255, 255, 3, 0, 149, 227, 176, 76, 20, 0,
            0, 0,
        ]
    }

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
    async fn protobuf_trace_payload_returns_415() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/x-protobuf"),
            )
            .bytes(vec![0, 1, 2].into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn gzip_compressed_trace_payload_returns_200() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/traces")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )
            .add_header(
                axum::http::header::CONTENT_ENCODING,
                axum::http::HeaderValue::from_static("gzip"),
            )
            .bytes(gzip_json(serde_json::json!({"resourceSpans": []})).into())
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
