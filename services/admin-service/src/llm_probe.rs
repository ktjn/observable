// Minimal LLM connectivity probe / model-listing client.
//
// admin-service only needs to test connectivity to an LLM provider and list
// available models (used by `config.rs::list_llm_models` for the setup-page
// model dropdown). This is a deliberately small extraction from
// `services/query-api/src/llm_adapter.rs::OpenAiLlmCaller` — admin-service has
// no use for the chat-completion (`LlmCaller` trait) path that lives there.

use async_openai::{Client as OpenAiClient, config::OpenAIConfig};
use std::collections::BTreeSet;

pub struct OpenAiLlmCaller {
    client: OpenAiClient<OpenAIConfig>,
    #[allow(dead_code)]
    model: String,
}

impl OpenAiLlmCaller {
    /// Creates a caller from an explicit API key string and optional url/model overrides.
    /// If `url` or `model` are None, falls back to env vars then hardcoded defaults.
    pub fn from_key(api_key: String, url: Option<String>, model: Option<String>) -> Self {
        let model = model
            .or_else(crate::config::env_llm_model)
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let mut config = OpenAIConfig::new().with_api_key(api_key);
        let base_url = url.or_else(crate::config::env_llm_url);
        if let Some(base_url) = base_url {
            config = config.with_api_base(base_url);
        }
        Self {
            client: OpenAiClient::with_config(config),
            model,
        }
    }

    /// Lists model IDs available at the configured endpoint.
    ///
    /// Used both as a connectivity probe and as the data source for the
    /// setup-page model dropdown.
    /// Returns a sorted list of model ID strings on success, or an error message.
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let response = self
            .client
            .models()
            .list()
            .await
            .map_err(|e| e.to_string())?;
        let ids: BTreeSet<String> = response.data.into_iter().map(|m| m.id).collect();
        Ok(ids.into_iter().collect())
    }
}
