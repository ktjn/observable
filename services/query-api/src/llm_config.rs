// Shared LLM configuration helpers used by the `/v1/nlq` chat path (see `llm_adapter`).
//
// Env vars take priority over DB values (LLM_API_KEY, LLM_URL / OPENAI_BASE_URL,
// LLM_MODEL / OPENAI_MODEL). DB values live in the `platform_config` PostgreSQL table
// and are managed via the admin-service config endpoints.

use sqlx::PgPool;

/// Returns the LLM model from env vars (LLM_MODEL, then OPENAI_MODEL as fallback).
pub fn env_llm_model() -> Option<String> {
    std::env::var("LLM_MODEL")
        .or_else(|_| std::env::var("OPENAI_MODEL"))
        .ok()
        .filter(|v| !v.is_empty())
}

/// Returns the LLM URL from env vars (LLM_URL, then OPENAI_BASE_URL as fallback).
pub fn env_llm_url() -> Option<String> {
    std::env::var("LLM_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .ok()
        .filter(|v| !v.is_empty())
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

// ── XOR obfuscation (key-at-rest helper for `fetch_db_key`) ──────────────────
//
// Provides basic obfuscation for the API key at rest. This is NOT cryptographically
// secure — it only prevents casual reads of the key from a database dump. The XOR
// key is embedded in source code; real encryption would require an operator-managed
// secret, which is out of scope for this local-dev tool.

const XOR_KEY: &[u8; 32] = b"ObservableLLMKeyProtect!!KeyXOR!";

/// Reverses the obfuscation applied when the key was stored. Returns `None` if the
/// input is not valid hex. Falls back gracefully: if decoding fails, returns `None`
/// so callers can treat the stored value as missing (rather than returning garbage).
fn xor_deobfuscate(hex: &str) -> Option<String> {
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
