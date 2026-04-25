use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;

use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn export_logs(
    State(state): State<crate::AppState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    body: Bytes,
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

    let (resource_count, logs) =
        match super::decode_otlp_http_request::<ExportLogsServiceRequest>(&headers, body) {
            Ok(super::DecodedOtlpRequest::Json(body)) => {
                let resource_logs = match body.get("resourceLogs").and_then(|v| v.as_array()) {
                    Some(s) => s,
                    None => return StatusCode::BAD_REQUEST.into_response(),
                };

                let logs = match super::convert::parse_otlp_logs(&body, ctx.tenant_id) {
                    Ok(l) => l,
                    Err(status) => return status.into_response(),
                };

                (resource_logs.len(), logs)
            }
            Ok(super::DecodedOtlpRequest::Protobuf(request)) => {
                let resource_count = request.resource_logs.len();
                let logs = crate::grpc::convert::proto_logs_to_domain(
                    &request.resource_logs,
                    ctx.tenant_id,
                );
                (resource_count, logs)
            }
            Err(status) => return status.into_response(),
        };

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        log_count = resource_count,
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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
    use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, KeyValue};
    use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use prost013::Message;

    use crate::http_json::build_router;
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

    fn protobuf_log_payload() -> Vec<u8> {
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".into(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("test-svc".into())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![LogRecord {
                        time_unix_nano: 1_000,
                        severity_text: "INFO".into(),
                        body: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("hello".into())),
                        }),
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
        .encode_to_vec()
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
    async fn protobuf_logs_export_returns_200() {
        let app = build_router(AppState::with_stub_auth(TENANT));
        let server = TestServer::new(app).unwrap();
        let resp = server
            .post("/v1/logs")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/x-protobuf"),
            )
            .bytes(protobuf_log_payload().into())
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
