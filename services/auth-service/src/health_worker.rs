use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use uuid::Uuid;

use crate::{authz::AuthzState, validate::sha256_hex};

#[derive(Deserialize)]
pub(crate) struct ValidateHealthWorker {
    tenant_id: Uuid,
    token: String,
}

pub(crate) async fn validate(
    State(state): State<AuthzState>,
    Json(request): Json<ValidateHealthWorker>,
) -> StatusCode {
    let Some(secret) = request.token.strip_prefix("ohw_") else {
        return StatusCode::UNAUTHORIZED;
    };
    if secret.len() != 64
        || !secret
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return StatusCode::UNAUTHORIZED;
    }

    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (\
            SELECT 1 FROM auth.health_worker_credentials \
            WHERE token_hash_sha256 = $1 AND tenant_id = $2 \
              AND revoked_at IS NULL AND expires_at > now()\
        )",
    )
    .bind(sha256_hex(&request.token))
    .bind(request.tenant_id)
    .fetch_one(&state.db)
    .await;

    match active {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::UNAUTHORIZED,
        Err(_) => {
            tracing::warn!("health worker credential store unavailable");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
