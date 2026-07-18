// Platform configuration endpoints.
//
// Provides a runtime way to set LLM connection parameters without requiring a container
// restart. Values are stored in the `platform_config` PostgreSQL table.
//
// GET  /v1/config            — returns {llm_key_configured, llm_url, llm_model, llm_provider,
//                              webllm_model}; never echoes the key.
// PUT  /v1/config/llm        — upserts api_key (XOR-obfuscated), url, model, provider,
//                              webllm_model from JSON body.
// PUT  /v1/config/llm-key    — legacy alias; accepts {key: "..."} for backwards compat.
// POST /v1/config/llm/models — tests connectivity and lists available models; accepts optional
//                              {url, api_key} body (falls back to DB/env when omitted).
//
// Env vars take priority over DB values (LLM_API_KEY, LLM_URL / OPENAI_BASE_URL,
// LLM_MODEL / OPENAI_MODEL).
use crate::AdminServiceAppState;
use crate::middleware::auth::{TenantContext, require_admin};
use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};
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
    /// "remote" | "webllm" — always present, defaults to "remote".
    pub llm_provider: String,
    /// WebLLM model identifier (e.g. a MLC model id); null when not set. Separate from
    /// `llm_model`, which is the remote-provider model.
    pub webllm_model: Option<String>,
}

/// PUT /v1/config/llm request body. All fields are optional; only provided fields are upserted.
#[derive(Deserialize)]
pub struct SetLlmConfigRequest {
    pub api_key: Option<String>,
    pub url: Option<String>,
    pub model: Option<String>,
    /// "remote" | "webllm"; any other value is rejected with 400.
    pub provider: Option<String>,
    pub webllm_model: Option<String>,
}

/// PUT /v1/config/llm-key legacy body (kept for backwards compatibility).
#[derive(Deserialize)]
pub struct SetLlmKeyRequest {
    pub key: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /v1/config
/// Returns LLM configuration status. Never echoes the API key value itself.
pub async fn get_config(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<ConfigStatus>, StatusCode> {
    require_admin(&ctx)?;
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

    // Env var takes priority over the DB value, same as the other LLM config fields;
    // default to "remote" when neither is set or the stored value is unrecognized.
    let llm_provider = env_llm_provider()
        .or(fetch_db_value(&state.db, "llm_provider")
            .await
            .unwrap_or(None)
            .filter(|v| !v.is_empty()))
        .filter(|v| is_valid_provider(v))
        .map(|v| v.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "remote".to_string());
    let webllm_model = fetch_db_value(&state.db, "webllm_model")
        .await
        .unwrap_or(None)
        .filter(|v| !v.is_empty());

    Ok(Json(ConfigStatus {
        llm_key_configured: key_configured,
        llm_url,
        llm_model,
        llm_provider,
        webllm_model,
    }))
}

/// PUT /v1/config/llm
/// Upserts any combination of api_key (XOR-obfuscated), url, and model.
pub async fn put_llm_config(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<SetLlmConfigRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
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
    if let Some(provider) = body.provider {
        if !is_valid_provider(&provider) {
            return Err(StatusCode::BAD_REQUEST);
        }
        upsert_db_value(&state.db, "llm_provider", provider.trim())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(webllm_model) = body.webllm_model {
        upsert_db_value(&state.db, "webllm_model", webllm_model.trim())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// PUT /v1/config/llm-key — legacy alias.
pub async fn put_llm_key(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<SetLlmKeyRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
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

// ── Model listing / connectivity probe ───────────────────────────────────────

/// POST /v1/config/llm/models request body. Both fields are optional.
/// When omitted the handler falls back to DB then env values.
#[derive(Deserialize)]
pub struct ListLlmModelsRequest {
    pub url: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Serialize)]
pub struct LlmModelsResult {
    pub ok: bool,
    /// Available model IDs, sorted alphabetically. Empty when `ok` is false.
    pub models: Vec<String>,
    /// Present when `ok` is false; contains the provider error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /v1/config/llm/models
///
/// Resolves the LLM configuration (request body → env → DB) and calls the
/// provider's `/models` endpoint to verify connectivity and retrieve available
/// model IDs. Always returns HTTP 200; callers inspect the `ok` field.
///
/// Using POST keeps any supplied `api_key` out of server access logs and browser
/// history (compared to query parameters).
pub async fn list_llm_models(
    State(state): State<AdminServiceAppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<ListLlmModelsRequest>,
) -> Result<Json<LlmModelsResult>, StatusCode> {
    require_admin(&ctx)?;
    use crate::llm_probe::OpenAiLlmCaller;

    // Resolve API key: request body → env → DB → empty (no-auth providers).
    let api_key = if let Some(k) = body.api_key.filter(|v| !v.trim().is_empty()) {
        k.trim().to_string()
    } else if env_key_present() {
        std::env::var("LLM_API_KEY").unwrap_or_default()
    } else {
        fetch_db_key(&state.db)
            .await
            .unwrap_or(None)
            .unwrap_or_default()
    };

    // Resolve URL: request body → env → DB → None (use provider default).
    let url: Option<String> = if let Some(u) = body.url.filter(|v| !v.trim().is_empty()) {
        Some(u.trim().to_string())
    } else if let Some(u) = env_llm_url() {
        Some(u)
    } else {
        fetch_db_value(&state.db, "llm_url")
            .await
            .unwrap_or(None)
            .filter(|v| !v.is_empty())
    };

    let caller = OpenAiLlmCaller::from_key(api_key, url, None);

    match caller.list_models().await {
        Ok(models) => Ok(Json(LlmModelsResult {
            ok: true,
            models,
            error: None,
        })),
        Err(e) => Ok(Json(LlmModelsResult {
            ok: false,
            models: vec![],
            error: Some(e),
        })),
    }
}

// ── Internal helpers (pub(crate) for llm_probe) ──────────────────────────────

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

/// Returns the LLM provider from the `LLM_PROVIDER` env var, if set to a recognized value
/// ("remote"/"webllm", case-insensitive). Mirrors `query-api`'s `llm_config::fetch_provider`
/// env lookup so both services agree on what counts as "configured".
pub fn env_llm_provider() -> Option<String> {
    std::env::var("LLM_PROVIDER")
        .ok()
        .filter(|v| is_valid_provider(v))
}

/// True when `value` (trimmed, case-insensitive) is exactly "remote" or "webllm".
pub fn is_valid_provider(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "remote" | "webllm"
    )
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

    #[test]
    fn is_valid_provider_accepts_remote_and_webllm_case_insensitively() {
        assert!(is_valid_provider("remote"));
        assert!(is_valid_provider("Remote"));
        assert!(is_valid_provider("webllm"));
        assert!(is_valid_provider("WEBLLM"));
        assert!(is_valid_provider("  webllm  "));
    }

    #[test]
    fn is_valid_provider_rejects_garbage() {
        assert!(!is_valid_provider("openai"));
        assert!(!is_valid_provider(""));
        assert!(!is_valid_provider("remotee"));
    }
}
