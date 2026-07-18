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

// ── Provider selection (Remote vs. WebLLM) ───────────────────────────────────

/// Which LLM path a tenant is configured to use: the existing server-side call to a
/// remote OpenAI-compatible endpoint, or client-side inference via WebLLM (the browser
/// runs the model; the server only prepares prompts via `/v1/nlq/prepare` +
/// `/v1/nlq/complete`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    Remote,
    Webllm,
}

/// Parses a raw provider string ("remote"/"webllm", case-insensitive). Any other value
/// (including empty) is treated as unset.
fn parse_provider(raw: &str) -> Option<LlmProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "remote" => Some(LlmProvider::Remote),
        "webllm" => Some(LlmProvider::Webllm),
        _ => None,
    }
}

/// Resolves the configured LLM provider: `LLM_PROVIDER` env var first, then
/// `platform_config.llm_provider` DB value, defaulting to `LlmProvider::Remote` when
/// neither is set (or either is an unrecognized value). Mirrors the env-then-DB
/// lookup pattern used by `env_llm_model`/`env_llm_url`/`fetch_db_value` above: any
/// lookup failure (including a DB error) is treated as "not configured" rather than
/// propagated, since an unreachable provider config should never block the NLQ path.
pub async fn fetch_provider(db: &PgPool) -> LlmProvider {
    if let Ok(raw) = std::env::var("LLM_PROVIDER") {
        match parse_provider(&raw) {
            Some(provider) => return provider,
            None if !raw.trim().is_empty() => {
                tracing::warn!(value = %raw, "LLM_PROVIDER set to an unrecognized value — ignoring");
            }
            None => {}
        }
    }

    if let Some(raw) = fetch_db_value(db, "llm_provider").await.ok().flatten() {
        match parse_provider(&raw) {
            Some(provider) => return provider,
            None => {
                tracing::warn!(value = %raw, "platform_config.llm_provider is an unrecognized value — falling back to remote");
            }
        }
    }

    LlmProvider::Remote
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only mirror of the obfuscation step (the real obfuscation helper lives in
    /// admin-service now and isn't needed in query-api outside of round-trip testing
    /// `xor_deobfuscate`).
    fn xor_obfuscate(s: &str) -> String {
        s.bytes()
            .enumerate()
            .map(|(i, b)| b ^ XOR_KEY[i % XOR_KEY.len()])
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn xor_roundtrip() {
        let key = "sk-test-api-key-1234567890abcdef";
        assert_eq!(xor_deobfuscate(&xor_obfuscate(key)), Some(key.to_string()));
    }

    #[test]
    fn xor_deobfuscate_invalid_hex_returns_none() {
        assert_eq!(xor_deobfuscate("not-hex!"), None);
    }

    #[test]
    fn xor_deobfuscate_odd_length_returns_none() {
        assert_eq!(xor_deobfuscate("abc"), None);
    }

    // ── fetch_provider ────────────────────────────────────────────────────────
    //
    // These tests read/write the process-wide `LLM_PROVIDER` env var, so they're
    // serialized via `ENV_LOCK` to avoid racing each other under the default
    // parallel test runner (no other test in this crate touches `LLM_PROVIDER`).

    // A `tokio::sync::Mutex` (not `std::sync::Mutex`) so the guard can be held across the
    // `.await` in `fetch_provider` without tripping `clippy::await_holding_lock`.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// A pool that is never actually connected — `fetch_db_value` against it fails
    /// immediately, which `fetch_provider` treats as "not configured", matching the
    /// existing `fetch_db_key`/`fetch_db_value` fallback convention used elsewhere in
    /// this module and in `llm_adapter.rs`'s DB-fallback tests.
    fn fake_db() -> PgPool {
        PgPool::connect_lazy("postgres://x:x@127.0.0.1:5432/x").expect("valid url")
    }

    #[tokio::test]
    async fn fetch_provider_defaults_to_remote_with_no_env_no_db() {
        let _guard = ENV_LOCK.lock().await;
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
        assert_eq!(fetch_provider(&fake_db()).await, LlmProvider::Remote);
    }

    #[tokio::test]
    async fn fetch_provider_env_webllm_wins_regardless_of_db() {
        let _guard = ENV_LOCK.lock().await;
        unsafe {
            std::env::set_var("LLM_PROVIDER", "webllm");
        }
        let result = fetch_provider(&fake_db()).await;
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
        assert_eq!(result, LlmProvider::Webllm);
    }

    #[tokio::test]
    async fn fetch_provider_env_remote_is_case_insensitive() {
        let _guard = ENV_LOCK.lock().await;
        unsafe {
            std::env::set_var("LLM_PROVIDER", "ReMoTe");
        }
        let result = fetch_provider(&fake_db()).await;
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
        assert_eq!(result, LlmProvider::Remote);
    }

    #[tokio::test]
    async fn fetch_provider_invalid_env_value_falls_back_to_default() {
        let _guard = ENV_LOCK.lock().await;
        unsafe {
            std::env::set_var("LLM_PROVIDER", "not-a-real-provider");
        }
        let result = fetch_provider(&fake_db()).await;
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
        assert_eq!(result, LlmProvider::Remote);
    }
}
