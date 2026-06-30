pub mod convert;
pub mod proto;

use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use prost::Message;

use crate::AppState;
use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn write(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Content-type guard
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or("")
        .eq_ignore_ascii_case("application/x-protobuf")
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    // Compressed body size cap — prevents allocate-on-header OOM attack
    const MAX_COMPRESSED_BODY_BYTES: usize = 4 * 1024 * 1024; // 4 MB
    if body.len() > MAX_COMPRESSED_BODY_BYTES {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }

    // Snappy decompress
    let mut decoder = snap::raw::Decoder::new();
    let proto_bytes = match decoder.decompress_vec(&body) {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Post-decompress size guard
    const MAX_DECOMPRESSED_BYTES: usize = 32 * 1024 * 1024; // 32 MB
    if proto_bytes.len() > MAX_DECOMPRESSED_BYTES {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }

    // Proto decode
    let req = match proto::WriteRequest::decode(proto_bytes.as_slice()) {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Empty request — nothing to do
    if req.timeseries.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    // Rate limit
    if state.metric_rate_limiter.check_key(&ctx.tenant_id).is_err() {
        tracing::warn!(tenant_id = %ctx.tenant_id, "prometheus remote_write rate limit exceeded");
        return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "1")]).into_response();
    }

    // Translate
    let (series, points) = convert::write_request_to_metrics(req, ctx.tenant_id, &ctx.environment);

    state
        .metric_cardinality
        .observe(ctx.tenant_id, series.len());

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        series_count = series.len(),
        point_count = points.len(),
        "received prometheus remote_write request"
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

    StatusCode::NO_CONTENT.into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use prost::Message;

    use crate::AppState;
    use crate::http_json::build_platform_router;
    use crate::readyz::IngestGatewayProbeState;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

    fn auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
        )
    }

    fn prom_content_type() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/x-protobuf"),
        )
    }

    fn make_snappy_body(timeseries: Vec<super::proto::TimeSeries>) -> Vec<u8> {
        use super::proto::WriteRequest;
        let req = WriteRequest { timeseries };
        let mut proto_bytes = Vec::new();
        req.encode(&mut proto_bytes).unwrap();
        let mut encoder = snap::raw::Encoder::new();
        encoder.compress_vec(&proto_bytes).unwrap()
    }

    fn one_gauge_body() -> Vec<u8> {
        use super::proto::{Label, Sample, TimeSeries};
        make_snappy_body(vec![TimeSeries {
            labels: vec![
                Label {
                    name: "__name__".into(),
                    value: "cpu_usage".into(),
                },
                Label {
                    name: "job".into(),
                    value: "node".into(),
                },
            ],
            samples: vec![Sample {
                value: 0.5,
                timestamp: 1_700_000_000_000,
            }],
        }])
    }

    fn platform_server() -> TestServer {
        let state = AppState::with_stub_auth(TENANT);
        let db = state.db.clone();
        let probe = IngestGatewayProbeState { db };
        TestServer::new(build_platform_router(state, probe))
    }

    #[tokio::test]
    async fn valid_body_returns_204() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn wrong_content_type_returns_415() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )
            .bytes(b"{}".as_ref().into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn malformed_snappy_body_returns_400() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_auth_returns_401() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rate_limit_exceeded_returns_429() {
        let state = AppState::with_stub_auth_and_rate_limit(TENANT, 1);
        let db = state.db.clone();
        let probe = IngestGatewayProbeState { db };
        let server = TestServer::new(build_platform_router(state, probe));

        let first = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(first.status_code(), StatusCode::NO_CONTENT);

        let second = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(second.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()["retry-after"], "1");
    }

    #[tokio::test]
    async fn oversized_body_returns_413() {
        let server = platform_server();
        // Body larger than 4 MB compressed cap
        let big_body = vec![0u8; 5 * 1024 * 1024];
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(big_body.into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn empty_write_request_returns_204() {
        use super::proto::WriteRequest;
        let req = WriteRequest { timeseries: vec![] };
        let mut proto_bytes = Vec::new();
        req.encode(&mut proto_bytes).unwrap();
        let mut encoder = snap::raw::Encoder::new();
        let body = encoder.compress_vec(&proto_bytes).unwrap();

        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(body.into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);
    }
}
