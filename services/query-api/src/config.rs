// Platform configuration endpoints.
//
// Provides a runtime way to set LLM connection parameters without requiring a container
// restart. Values are stored in the `platform_config` PostgreSQL table.
//
// GET  /v1/config            — returns {llm_key_configured, llm_url, llm_model}; never echoes the key.
// PUT  /v1/config/llm        — upserts api_key (XOR-obfuscated), url, model from JSON body.
// PUT  /v1/config/llm-key    — legacy alias; accepts {key: "..."} for backwards compat.
// GET  /v1/config/llm/test   — verifies LLM connectivity with a 1-token probe completion.
//
// Env vars take priority over DB values (LLM_API_KEY, LLM_URL / OPENAI_BASE_URL,
// LLM_MODEL / OPENAI_MODEL).
use crate::traces::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ── XOR obfuscation ───────────────────────────────────────────────────────────
//
// Provides basic obfuscation for the API key at rest. This is NOT cryptographically
// secure — it only prevents casual reads of the key from a database dump. The XOR
// key is embedded in source code; real encryption would require an operator-managed
// secret, which is out of scope for this local-dev tool.

const XOR_KEY: &[u8; 32] = b"ObservableLLMKeyProtect!!KeyXOR!";

/// XOR-obfuscates a string and returns a lowercase hex string.
pub fn xor_obfuscate(s: &str) -> String {
    s.bytes()
        .enumerate()
        .map(|(i, b)| b ^ XOR_KEY[i % XOR_KEY.len()])
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reverses `xor_obfuscate`. Returns `None` if the input is not valid hex.
/// Falls back gracefully: if decoding fails, returns `None` so callers can
/// treat the stored value as missing (rather than returning garbage).
pub fn xor_deobfuscate(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    let orig: Vec<u8> = bytes?
        .into_iter()
        .enumerate()
        .map(|(i, b)| b ^ XOR_KEY[i % XOR_KEY.len()])
        .collect();
    String::from_utf8(orig).ok()
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConfigStatus {
    pub llm_key_configured: bool,
    /// LLM endpoint URL; null when not set (blank = use OpenAI default).
    pub llm_url: Option<String>,
    /// LLM model identifier; null when not set.
    pub llm_model: Option<String>,
}

/// PUT /v1/config/llm request body. All fields are optional; only provided fields are upserted.
#[derive(Deserialize)]
pub struct SetLlmConfigRequest {
    pub api_key: Option<String>,
    pub url: Option<String>,
    pub model: Option<String>,
}

/// PUT /v1/config/llm-key legacy body (kept for backwards compatibility).
#[derive(Deserialize)]
pub struct SetLlmKeyRequest {
    pub key: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /v1/config
/// Returns LLM configuration status. Never echoes the API key value itself.
pub async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigStatus>, StatusCode> {
    // Env vars take priority over DB values.
    let llm_url = env_llm_url().or(fetch_db_value(&state.db, "llm_url")
        .await
        .unwrap_or(None)
        .filter(|v| !v.is_empty()));
    let llm_model = env_llm_model().or(fetch_db_value(&state.db, "llm_model")
        .await
        .unwrap_or(None)
        .filter(|v| !v.is_empty()));

    // `llm_key_configured` is true when any LLM configuration is present: a key, a custom URL,
    // or both.  No-auth providers (Ollama, local vLLM) only set a URL; they must still show as
    // "Configured" so the UI badge and the Test button are rendered correctly.
    let key_configured =
        env_key_present() || db_key_present(&state.db).await.unwrap_or(false) || llm_url.is_some();

    Ok(Json(ConfigStatus {
        llm_key_configured: key_configured,
        llm_url,
        llm_model,
    }))
}

/// PUT /v1/config/llm
/// Upserts any combination of api_key (XOR-obfuscated), url, and model.
pub async fn put_llm_config(
    State(state): State<AppState>,
    Json(body): Json<SetLlmConfigRequest>,
) -> Result<StatusCode, StatusCode> {
    if let Some(key) = body.api_key {
        let key = key.trim().to_string();
        if key.is_empty() {
            // Empty string clears the key.
            upsert_db_value(&state.db, "llm_api_key", "")
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        } else {
            let obfuscated = xor_obfuscate(&key);
            upsert_db_value(&state.db, "llm_api_key", &obfuscated)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
    if let Some(url) = body.url {
        upsert_db_value(&state.db, "llm_url", url.trim())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(model) = body.model {
        upsert_db_value(&state.db, "llm_model", model.trim())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// PUT /v1/config/llm-key — legacy alias.
pub async fn put_llm_key(
    State(state): State<AppState>,
    Json(body): Json<SetLlmKeyRequest>,
) -> Result<StatusCode, StatusCode> {
    let key = body.key.trim().to_string();
    if key.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let obfuscated = xor_obfuscate(&key);
    upsert_db_value(&state.db, "llm_api_key", &obfuscated)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Connectivity test ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LlmTestResult {
    pub ok: bool,
    /// Present when `ok` is false; contains the provider error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The model name that was probed (from DB or default).
    pub model: String,
}

/// GET /v1/config/llm/test
///
/// Resolves the LLM configuration (env → DB) and fires a single 1-token
/// probe completion to verify that the key, URL, and model are all valid.
/// Always returns HTTP 200; callers inspect the `ok` field.
///
/// Returns 503 if no LLM configuration is present at all.
pub async fn test_llm_connection(
    State(state): State<AppState>,
) -> Result<Json<LlmTestResult>, StatusCode> {
    use crate::llm_adapter::OpenAiLlmCaller;

    // Resolve the key (env first, then DB).
    let api_key: Option<String> = if crate::config::env_key_present() {
        std::env::var("LLM_API_KEY").ok().filter(|v| !v.is_empty())
    } else {
        fetch_db_key(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    // Resolve URL and model (env → DB → defaults).
    let url = env_llm_url().or(fetch_db_value(&state.db, "llm_url")
        .await
        .unwrap_or(None)
        .filter(|v| !v.is_empty()));
    let model = env_llm_model()
        .or(fetch_db_value(&state.db, "llm_model")
            .await
            .unwrap_or(None)
            .filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "gpt-4o-mini".into());

    // For providers that don't require an API key (e.g. local vLLM with no auth),
    // treat an empty / absent key as an empty string (async-openai accepts it).
    let effective_key = api_key.unwrap_or_default();

    let caller = OpenAiLlmCaller::from_key(effective_key, url, Some(model.clone()));

    match caller.probe().await {
        Ok(()) => Ok(Json(LlmTestResult {
            ok: true,
            error: None,
            model,
        })),
        Err(e) => Ok(Json(LlmTestResult {
            ok: false,
            error: Some(e),
            model,
        })),
    }
}

// ── Internal helpers (pub(crate) for llm_adapter) ────────────────────────────

pub fn env_key_present() -> bool {
    std::env::var("LLM_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Returns the LLM URL from env vars (LLM_URL, then OPENAI_BASE_URL as fallback).
pub fn env_llm_url() -> Option<String> {
    std::env::var("LLM_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .ok()
        .filter(|v| !v.is_empty())
}

/// Returns the LLM model from env vars (LLM_MODEL, then OPENAI_MODEL as fallback).
pub fn env_llm_model() -> Option<String> {
    std::env::var("LLM_MODEL")
        .or_else(|_| std::env::var("OPENAI_MODEL"))
        .ok()
        .filter(|v| !v.is_empty())
}

pub async fn db_key_present(db: &PgPool) -> Result<bool, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM platform_config WHERE key = 'llm_api_key'")
            .fetch_optional(db)
            .await?;
    // A non-empty value means key is set (obfuscated or legacy plaintext).
    Ok(row.map(|(v,)| !v.is_empty()).unwrap_or(false))
}

/// Fetches and deobfuscates the LLM API key from the database.
/// Gracefully handles legacy plaintext keys that were stored before XOR obfuscation.
pub async fn fetch_db_key(db: &PgPool) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM platform_config WHERE key = 'llm_api_key'")
            .fetch_optional(db)
            .await?;
    Ok(row.and_then(|(v,)| {
        if v.is_empty() {
            None
        } else {
            // Try deobfuscation first; fall back to treating as plaintext.
            Some(xor_deobfuscate(&v).unwrap_or(v))
        }
    }))
}

/// Fetches an arbitrary key from `platform_config`.
pub async fn fetch_db_value(db: &PgPool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM platform_config WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await?;
    Ok(row.map(|(v,)| v))
}

pub async fn upsert_db_value(db: &PgPool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO platform_config (key, value, updated_at)
         VALUES ($1, $2, now())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_roundtrip() {
        let key = "sk-test-api-key-1234567890abcdef";
        assert_eq!(xor_deobfuscate(&xor_obfuscate(key)), Some(key.to_string()));
    }

    #[test]
    fn xor_obfuscation_is_not_plaintext() {
        let key = "sk-secret";
        let obfuscated = xor_obfuscate(key);
        assert_ne!(
            obfuscated, key,
            "obfuscated value must differ from original"
        );
        assert!(
            !obfuscated.contains("sk-secret"),
            "key must not appear in obfuscated form"
        );
    }

    #[test]
    fn xor_deobfuscate_invalid_hex_returns_none() {
        assert_eq!(xor_deobfuscate("not-hex!"), None);
    }

    #[test]
    fn xor_deobfuscate_odd_length_returns_none() {
        assert_eq!(xor_deobfuscate("abc"), None);
    }
}
