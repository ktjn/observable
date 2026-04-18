mod audit;
mod validate;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
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

    let row = sqlx::query(
        "SELECT tenant_id, key_hash, revoked_at, role FROM api_keys WHERE key_hash = $1",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let Some(row) = row else {
        audit::write(&state.db, &audit::AuditEntry::deny_not_found(hash)).await;
        return Err(StatusCode::UNAUTHORIZED);
    };

    let tenant_id: Uuid = row
        .try_get("tenant_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("revoked_at").unwrap_or(None);
    let role: String = row
        .try_get("role")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entry = validate::ApiKeyEntry {
        tenant_id,
        key_hash: hash.clone(),
        revoked_at,
        role,
    };

    match validate::validate_key_against_entry(&req.api_key, &entry) {
        Ok((tid, role)) => {
            audit::write(&state.db, &audit::AuditEntry::allow(hash, tid)).await;
            Ok(Json(ValidateResponse {
                tenant_id: tid,
                role,
            }))
        }
        Err(_) => {
            let reason = if entry.revoked_at.is_some() {
                "revoked"
            } else {
                "hash_mismatch"
            };
            audit::write(&state.db, &audit::AuditEntry::deny(hash, tenant_id, reason)).await;
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    domain::telemetry::init_telemetry("auth-service", otlp.as_deref())?;
    let db_url = std::env::var("DATABASE_URL")?;
    let db = PgPool::connect(&db_url).await?;
    let port: u16 = std::env::var("AUTH_SERVICE_PORT")
        .unwrap_or_else(|_| "4318".into())
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
