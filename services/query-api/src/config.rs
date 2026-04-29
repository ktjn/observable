// Platform configuration endpoints (P8-S6 Setup UI).
//
// Provides a runtime way to set the LLM_API_KEY without requiring a container
// restart. The key is stored in the `platform_config` PostgreSQL table.
//
// GET  /v1/config            — returns {llm_key_configured: bool}; never echoes the key.
// PUT  /v1/config/llm-key    — upserts key=llm_api_key; accepts {key: "..."}
use crate::traces::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConfigStatus {
    pub llm_key_configured: bool,
}

#[derive(Deserialize)]
pub struct SetLlmKeyRequest {
    pub key: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /v1/config
/// Returns whether the LLM key is configured (env var OR database).
/// Never returns the key value itself.
pub async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigStatus>, StatusCode> {
    // Check env var first (fastest path, avoids DB).
    if env_key_present() {
        return Ok(Json(ConfigStatus {
            llm_key_configured: true,
        }));
    }
    // Fall back to DB.
    let db_configured = db_key_present(&state.db).await.unwrap_or(false);
    Ok(Json(ConfigStatus {
        llm_key_configured: db_configured,
    }))
}

/// PUT /v1/config/llm-key
/// Upserts the LLM API key into the `platform_config` table.
pub async fn put_llm_key(
    State(state): State<AppState>,
    Json(body): Json<SetLlmKeyRequest>,
) -> Result<StatusCode, StatusCode> {
    let key = body.key.trim().to_string();
    if key.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    upsert_db_key(&state.db, &key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Internal helpers (pub(crate) for llm_adapter fallback) ───────────────────

pub fn env_key_present() -> bool {
    std::env::var("LLM_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

pub async fn db_key_present(db: &PgPool) -> Result<bool, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM platform_config WHERE key = 'llm_api_key'")
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(v,)| !v.is_empty()).unwrap_or(false))
}

/// Fetches the LLM API key from the database. Returns None if not present.
pub async fn fetch_db_key(db: &PgPool) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM platform_config WHERE key = 'llm_api_key'")
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(v,)| v).filter(|v| !v.is_empty()))
}

pub async fn upsert_db_key(db: &PgPool, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO platform_config (key, value, updated_at)
         VALUES ('llm_api_key', $1, now())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .execute(db)
    .await?;
    Ok(())
}
