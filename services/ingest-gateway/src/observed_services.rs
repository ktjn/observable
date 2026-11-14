//! Asynchronous, deduplicated reporter that notifies `catalog-service` when
//! a new `service_name` is seen in ingested telemetry (`POST /internal/catalog/observed`).

use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const MAX_CACHED_SERVICES: usize = 10_000;

#[derive(Serialize)]
struct ObservedServicePayload<'a> {
    tenant_id: Uuid,
    operational_name: &'a str,
}

/// Non-blocking reporter that caches `(tenant_id, service_name)` pairs so each
/// discovered service is reported to `catalog-service` once per process lifetime.
pub struct ObservedServiceReporter {
    catalog_url: Option<String>,
    internal_token: Option<String>,
    http_client: reqwest::Client,
    seen: Mutex<HashSet<(Uuid, String)>>,
}

impl ObservedServiceReporter {
    pub fn from_env(http_client: reqwest::Client) -> Arc<Self> {
        let catalog_url = std::env::var("CATALOG_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        let internal_token = std::env::var("INTERNAL_SERVICE_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        Arc::new(Self {
            catalog_url,
            internal_token,
            http_client,
            seen: Mutex::new(HashSet::new()),
        })
    }

    pub fn disabled() -> Arc<Self> {
        Arc::new(Self {
            catalog_url: None,
            internal_token: None,
            http_client: reqwest::Client::new(),
            seen: Mutex::new(HashSet::new()),
        })
    }

    /// Records any unseen non-empty service names in the background.
    pub fn observe_names<'a, I>(&self, tenant_id: Uuid, names: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let (Some(catalog_url), Some(token)) = (&self.catalog_url, &self.internal_token) else {
            return;
        };

        let mut new_names = Vec::new();
        {
            let mut guard = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            for raw in names {
                let name = raw.trim();
                if name.is_empty() {
                    continue;
                }
                if guard.len() >= MAX_CACHED_SERVICES {
                    guard.clear();
                }
                let key = (tenant_id, name.to_string());
                if guard.insert(key) {
                    new_names.push(name.to_string());
                }
            }
        }

        if new_names.is_empty() {
            return;
        }

        let client = self.http_client.clone();
        let url = format!("{catalog_url}/internal/catalog/observed");
        let token = token.clone();
        tokio::spawn(async move {
            for name in new_names {
                let res = client
                    .post(&url)
                    .header("X-Internal-Token", &token)
                    .json(&ObservedServicePayload {
                        tenant_id,
                        operational_name: &name,
                    })
                    .send()
                    .await;
                if let Err(err) = res {
                    tracing::debug!(error = %err, service = %name, "failed to report observed service to catalog");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_reporter_ignores_names_without_panicking() {
        let reporter = ObservedServiceReporter::disabled();
        reporter.observe_names(Uuid::nil(), ["checkout", "", "payment"]);
    }
}
