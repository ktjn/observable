//! Client for auth-service's directory views and tenant purge (internal token). API keys and
//! memberships are auth data (`auth.*`, closed to the control role), so observable-control reads
//! them here. Any transport error or unexpected status is 503: callers fail closed.

use std::sync::Arc;

use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthDirectory {
    auth_url: Arc<String>,
    internal_token: Arc<String>,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct KeyEnvironmentList {
    environments: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Membership {
    pub tenant_id: Uuid,
    pub role: String,
}

#[derive(Deserialize)]
struct MembershipList {
    tenants: Vec<Membership>,
}

impl AuthDirectory {
    pub fn new(auth_url: &str, internal_token: &str, client: reqwest::Client) -> Self {
        Self {
            auth_url: Arc::new(auth_url.trim_end_matches('/').to_owned()),
            internal_token: Arc::new(internal_token.to_owned()),
            client,
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, StatusCode> {
        let unavailable = |reason: &str| {
            tracing::warn!(reason, path, "auth directory unavailable");
            StatusCode::SERVICE_UNAVAILABLE
        };
        let response = self
            .client
            .get(format!("{}{path}", self.auth_url))
            .query(query)
            .header("X-Internal-Token", self.internal_token.as_str())
            .send()
            .await
            .map_err(|_| unavailable("request_failed"))?;
        if !response.status().is_success() {
            return Err(unavailable("auth_refused"));
        }
        response
            .json::<T>()
            .await
            .map_err(|_| unavailable("invalid_response"))
    }

    /// Distinct environments of the tenant's active API keys.
    pub async fn key_environments(&self, tenant_id: Uuid) -> Result<Vec<String>, StatusCode> {
        self.get::<KeyEnvironmentList>(
            "/internal/key-environments",
            &[("tenant_id", tenant_id.to_string())],
        )
        .await
        .map(|l| l.environments)
    }

    /// The user's tenant memberships.
    pub async fn user_tenants(&self, user_id: Uuid) -> Result<Vec<Membership>, StatusCode> {
        self.get::<MembershipList>(&format!("/internal/users/{user_id}/tenants"), &[])
            .await
            .map(|l| l.tenants)
    }

    /// Purges the tenant's auth data (sessions, memberships, keys, worker credentials, tenant
    /// policy) and records the purge in auth's audit log. Idempotent.
    pub async fn purge_tenant(
        &self,
        tenant_id: Uuid,
        actor: &str,
    ) -> Result<Vec<String>, StatusCode> {
        #[derive(Deserialize)]
        struct Purged {
            purged_tables: Vec<String>,
        }
        let path = format!("/internal/tenants/{tenant_id}/purge");
        let unavailable = |reason: &str| {
            tracing::warn!(reason, %tenant_id, "auth tenant purge unavailable");
            StatusCode::SERVICE_UNAVAILABLE
        };
        let response = self
            .client
            .post(format!("{}{path}", self.auth_url))
            .header("X-Internal-Token", self.internal_token.as_str())
            .json(&serde_json::json!({ "actor": actor }))
            .send()
            .await
            .map_err(|_| unavailable("request_failed"))?;
        if !response.status().is_success() {
            return Err(unavailable("auth_refused"));
        }
        response
            .json::<Purged>()
            .await
            .map(|p| p.purged_tables)
            .map_err(|_| unavailable("invalid_response"))
    }
}
