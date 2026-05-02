mod audit;

use auth_service::{lookup_api_key, validate};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
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
}

async fn validate_handler(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let hash = validate::sha256_hex(&req.api_key);

    match lookup_api_key(&state.db, &req.api_key).await {
        Ok((tenant_id, role)) => {
            audit::write(&state.db, &audit::AuditEntry::allow(hash, tenant_id)).await;
            Ok(Json(ValidateResponse { tenant_id, role }))
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
            // Tenant ID is unknown at this point for hash mismatches; use nil UUID for audit.
            audit::write(
                &state.db,
                &audit::AuditEntry::deny(hash, Uuid::nil(), reason),
            )
            .await;
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("auth-service")?;
    let db_url = std::env::var("DATABASE_URL")?;
    let db = PgPool::connect(&db_url).await?;
    let port: u16 = std::env::var("AUTH_SERVICE_PORT")
        .unwrap_or_else(|_| "4319".into())
        .parse()?;
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/internal/validate", post(validate_handler))
        .with_state(AppState { db });
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "auth-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
