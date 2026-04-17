use axum::{extract::Extension, http::StatusCode, Json};
use serde_json::Value;

use crate::auth::TenantContext;

pub async fn export_traces(
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let resource_spans = body
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        span_count = resource_spans.len(),
        "received trace export request"
    );

    Ok(Json(serde_json::json!({ "partialSuccess": {} })))
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
