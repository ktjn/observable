use axum::{
    Json,
    body::Bytes,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::auth::TenantContext;
use crate::http_json::DecodedBody;
use crate::queue::producer::build_envelope;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use prost::Message;

pub async fn export_metrics(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if state.metric_rate_limiter.check_key(&ctx.tenant_id).is_err() {
        tracing::warn!(tenant_id = %ctx.tenant_id, "metric ingest rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(serde_json::json!({
                "error": "rate_limit_exceeded",
                "message": "Metric ingest rate limit exceeded"
            })),
        )
            .into_response();
    }

    let (resource_count, series, points) = match super::decode_otlp_request(&headers, body) {
        Ok(DecodedBody::Json(json)) => {
            let resource_metrics = match json.get("resourceMetrics").and_then(|v| v.as_array()) {
                Some(s) => s,
                None => return StatusCode::BAD_REQUEST.into_response(),
            };

            let (series, points) =
                match super::convert::parse_otlp_metrics(&json, ctx.tenant_id, &ctx.environment) {
                    Ok(m) => m,
                    Err(status) => return status.into_response(),
                };

            (resource_metrics.len(), series, points)
        }
        Ok(DecodedBody::Protobuf(bytes)) => {
            let req = match ExportMetricsServiceRequest::decode(bytes) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("failed to decode OTLP/HTTP protobuf metric request: {e}");
                    return StatusCode::BAD_REQUEST.into_response();
                }
            };
            let (series, points, _) = crate::grpc::convert::proto_metrics_to_domain(
                &req.resource_metrics,
                ctx.tenant_id,
                &ctx.environment,
            );
            (req.resource_metrics.len(), series, points)
        }
        Err(status) => return status.into_response(),
    };

    state
        .metric_cardinality
        .observe(ctx.tenant_id, series.len());

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        resource_count,
        series_count = series.len(),
        "received metrics export request"
    );

    if let Some(producer) = &state.producer {
        let envelope = build_envelope(
            ctx.tenant_id,
            &ctx.environment,
            domain::EnvelopePayload::Metrics { series, points },
        );
        if producer.publish(&envelope).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    Json(serde_json::json!({})).into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;

    use crate::AppState;
    use crate::http_json::build_router;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

    fn auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
        )
    }

    fn gzip_json(value: serde_json::Value) -> Vec<u8> {
        assert_eq!(value, two_series_payload());
        vec![
            31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 156, 205, 61, 11, 2, 49, 12, 6, 224, 255, 242, 206,
            245, 168, 142, 157, 93, 21, 23, 93, 228, 134, 90, 131, 22, 189, 86, 147, 244, 80, 142,
            254, 119, 241, 14, 68, 87, 201, 146, 143, 151, 39, 3, 152, 36, 23, 14, 180, 34, 229,
            24, 4, 110, 63, 124, 118, 112, 3, 188, 42, 199, 67, 81, 154, 78, 23, 122, 194, 65, 136,
            251, 24, 168, 73, 190, 35, 24, 244, 254, 90, 198, 176, 40, 199, 116, 218, 77, 35, 164,
            15, 51, 143, 90, 219, 106, 32, 33, 223, 126, 126, 116, 95, 253, 200, 56, 156, 85, 111,
            13, 211, 189, 144, 168, 192, 64, 74, 247, 70, 143, 94, 253, 38, 199, 164, 83, 88, 99,
            71, 219, 20, 31, 107, 159, 50, 28, 230, 214, 90, 24, 120, 89, 230, 114, 184, 18, 220,
            220, 54, 182, 182, 181, 154, 95, 151, 152, 51, 255, 175, 46, 38, 180, 29, 235, 5, 0, 0,
            255, 255, 3, 0, 100, 152, 194, 41, 54, 1, 0, 0,
        ]
    }

    fn two_series_payload() -> serde_json::Value {
        serde_json::json!({
            "resourceMetrics": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "svc-a"}}]},
                "scopeMetrics": [{
                    "metrics": [
                        {"name": "http.requests", "sum": {"dataPoints": [{"timeUnixNano": "1000", "asDouble": 10.0}]}},
                        {"name": "http.errors",   "sum": {"dataPoints": [{"timeUnixNano": "1000", "asDouble": 2.0}]}}
                    ]
                }]
            }]
        })
    }

    #[tokio::test]
    async fn metrics_export_returns_200() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app);
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn protobuf_metrics_export_returns_200() {
        use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
        use prost::Message;

        let req = ExportMetricsServiceRequest {
            resource_metrics: vec![],
        };
        let mut buf = Vec::new();
        req.encode(&mut buf).unwrap();

        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app);
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/x-protobuf"),
            )
            .bytes(buf.into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn gzip_compressed_metrics_payload_returns_200() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app);
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )
            .add_header(
                axum::http::header::CONTENT_ENCODING,
                axum::http::HeaderValue::from_static("gzip"),
            )
            .bytes(gzip_json(two_series_payload()).into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn exceeding_rate_limit_returns_429() {
        let app = build_router(AppState::with_stub_auth_and_rate_limit(TENANT, 1));
        let server = TestServer::new(app);

        let first = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);

        let second = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(second.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()["retry-after"], "1");
        let body: serde_json::Value = second.json();
        assert_eq!(body["error"], "rate_limit_exceeded");
    }

    #[tokio::test]
    async fn metrics_export_updates_cardinality_counter() {
        let state = AppState::with_stub_auth(TENANT);
        let cardinality = state.metric_cardinality.clone();
        let tenant_id = uuid::Uuid::parse_str(TENANT).unwrap();

        let app = build_router(state);
        let server = TestServer::new(app);
        server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;

        assert_eq!(cardinality.current_count(tenant_id), 2);
    }

    #[tokio::test]
    async fn metrics_export_above_budget_still_returns_200() {
        // Budget of 1; request carries 2 series — ingest must NOT be rejected.
        let state = AppState::with_stub_auth_and_metric_budget(TENANT, 1);
        let app = build_router(state);
        let server = TestServer::new(app);
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .json(&two_series_payload())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_export_missing_auth_returns_401() {
        let app = build_router(AppState::test_stub());
        let server = TestServer::new(app);
        let resp = server.post("/v1/metrics").json(&two_series_payload()).await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn zstd_compressed_metrics_payload_returns_200() {
        let json = serde_json::to_vec(&two_series_payload()).unwrap();
        let compressed = zstd::encode_all(std::io::Cursor::new(&json), 0).unwrap();

        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app);
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )
            .add_header(
                axum::http::header::CONTENT_ENCODING,
                axum::http::HeaderValue::from_static("zstd"),
            )
            .bytes(compressed.into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[tokio::test]
    async fn zstd_corrupt_payload_returns_400() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app);
        let resp = server
            .post("/v1/metrics")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )
            .add_header(
                axum::http::header::CONTENT_ENCODING,
                axum::http::HeaderValue::from_static("zstd"),
            )
            .bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    }
}
