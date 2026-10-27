// Shared LLM configuration helpers used by the `/v1/nlq` chat path (see `llm_adapter`).
//
// Env vars take priority over admin-service platform_config values (LLM_API_KEY,
// LLM_URL / OPENAI_BASE_URL, LLM_MODEL / OPENAI_MODEL, LLM_PROVIDER).
// platform_config is owned by admin-service and queried over internal HTTP.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InternalLlmConfig {
    pub api_key: Option<String>,
    pub url: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub webllm_model: Option<String>,
}

/// Fetches LLM configuration from admin-service via internal service-to-service endpoint.
pub async fn fetch_remote_llm_config(
    client: &reqwest::Client,
    admin_service_url: &str,
    internal_token: &str,
) -> Option<InternalLlmConfig> {
    client
        .get(format!("{admin_service_url}/internal/config/llm"))
        .header("X-Internal-Token", internal_token)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
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
/// `platform_config.llm_provider` fetched from admin-service, defaulting to `LlmProvider::Remote` when
/// neither is set (or either is an unrecognized value).
pub async fn fetch_provider(
    client: &reqwest::Client,
    admin_service_url: &str,
    internal_token: &str,
) -> LlmProvider {
    if let Ok(raw) = std::env::var("LLM_PROVIDER") {
        match parse_provider(&raw) {
            Some(provider) => return provider,
            None if !raw.trim().is_empty() => {
                tracing::warn!(value = %raw, "LLM_PROVIDER set to an unrecognized value — ignoring");
            }
            None => {}
        }
    }

    if let Some(config) = fetch_remote_llm_config(client, admin_service_url, internal_token).await
        && let Some(ref raw) = config.provider
    {
        match parse_provider(raw) {
            Some(provider) => return provider,
            None => {
                tracing::warn!(value = %raw, "platform_config.llm_provider is an unrecognized value — falling back to remote");
            }
        }
    }

    LlmProvider::Remote
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn dummy_client() -> (reqwest::Client, &'static str, &'static str) {
        (reqwest::Client::new(), "http://127.0.0.1:1", "dummy-token")
    }

    #[tokio::test]
    async fn fetch_provider_defaults_to_remote_with_no_env_no_db() {
        let _guard = ENV_LOCK.lock().await;
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
        let (client, url, token) = dummy_client();
        assert_eq!(
            fetch_provider(&client, url, token).await,
            LlmProvider::Remote
        );
    }

    #[tokio::test]
    async fn fetch_provider_env_webllm_wins_regardless_of_remote() {
        let _guard = ENV_LOCK.lock().await;
        unsafe {
            std::env::set_var("LLM_PROVIDER", "webllm");
        }
        let (client, url, token) = dummy_client();
        let result = fetch_provider(&client, url, token).await;
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
        let (client, url, token) = dummy_client();
        let result = fetch_provider(&client, url, token).await;
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
        let (client, url, token) = dummy_client();
        let result = fetch_provider(&client, url, token).await;
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
        assert_eq!(result, LlmProvider::Remote);
    }
}
