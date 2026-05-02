mod audit;

use auth_service::{lookup_api_key, validate};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::{Instrument as _, Level};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: PgPool,
}

#[derive(Deserialize)]
struct ValidateRequest {
    api_key: String,
}

#[derive(Serialize)]
struct ValidateResponse {
    tenant_id: Uuid,
    role: String,
    environment: String,
}

async fn validate_handler(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let hash = validate::sha256_hex(&req.api_key);
    async move {
        match lookup_api_key(&state.db, &req.api_key).await {
            Ok((tenant_id, role, environment)) => {
                audit::write(&state.db, &audit::AuditEntry::allow(hash, tenant_id)).await;
                Ok(Json(ValidateResponse {
                    tenant_id,
                    role,
                    environment,
                }))
            }
            Err(e) => {
                let reason = if e.to_string().contains("revoked") {
                    "revoked"
                } else if e.to_string().contains("not found") {
                    audit::write(&state.db, &audit::AuditEntry::deny_not_found(hash)).await;
                    return Err(StatusCode::UNAUTHORIZED);
                } else {
                    "hash_mismatch"
                };
                audit::write(
                    &state.db,
                    &audit::AuditEntry::deny(hash, Uuid::nil(), reason),
                )
                .await;
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
    .instrument(tracing::info_span!("auth.validate"))
    .await
}

/// Set the OTel parent context on the current (TraceLayer) span by extracting
/// the W3C `traceparent` header. Must run INSIDE the TraceLayer span.
async fn extract_otel_context(request: Request, next: Next) -> Response {
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    let carrier: std::collections::HashMap<String, String> = request
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();
    let parent_cx =
        opentelemetry::global::get_text_map_propagator(|propagator| propagator.extract(&carrier));
    let _ = tracing::Span::current().set_parent(parent_cx);
    next.run(request).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("auth-service")?;
    let db_url = std::env::var("DATABASE_URL")?;
    let db = PgPool::connect(&db_url).await?;
    let port: u16 = std::env::var("AUTH_SERVICE_PORT")
        .unwrap_or_else(|_| "4319".into())
        .parse()?;
    let state = AppState { db };
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/internal/validate", post(validate_handler))
        .layer(middleware::from_fn::<_, (axum::extract::Request,)>(
            extract_otel_context,
        ))
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().level(Level::INFO)))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "auth-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
