//! Required-capability ports with real (HTTP to observable-auth) and substitute adapters, so
//! the catalog runs standalone against the provider-owned mocks (ISO-02).

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::HeaderMap;
use observable_auth::authz::{
    AccessDecider, DecideRequest, Decision, DecisionUnavailable, FilterRequest, HttpAccessDecider,
    MockAccessDecider, Principal, ResourceKey,
};
use observable_auth::principals::{
    DirectoryUnavailable, HttpPrincipalDirectory, MockPrincipalDirectory, PrincipalDirectory,
    ResolvedPrincipal,
};
use observable_auth::{
    AuthError, extract_bearer_token, extract_session_cookie, extract_tenant_id_header,
};
use uuid::Uuid;

/// Verified caller of a catalog API request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    pub tenant_id: Uuid,
    /// Present for human sessions; API-key callers have none.
    pub user_id: Option<Uuid>,
    pub role: String,
}

impl Caller {
    pub fn principal(&self) -> Principal {
        match self.user_id {
            Some(id) => Principal::user(id),
            None => Principal::service(&self.role),
        }
    }
}

/// `validate-credential` port.
#[derive(Clone)]
pub enum Credentials {
    Http {
        auth_url: Arc<String>,
        client: reqwest::Client,
    },
    /// Bearer token -> caller (standalone harness and tests).
    Static(Arc<HashMap<String, Caller>>),
}

impl Credentials {
    pub async fn verify(&self, headers: &HeaderMap) -> Result<Caller, AuthError> {
        match self {
            Self::Static(callers) => {
                let token = extract_bearer_token(headers)?.ok_or(AuthError::Unauthorized)?;
                callers.get(&token).cloned().ok_or(AuthError::Unauthorized)
            }
            Self::Http { auth_url, client } => verify_http(client, auth_url, headers).await,
        }
    }
}

/// Same resolution order as the other tenant-scoped services: API key with `X-Tenant-ID`
/// (must match the key's tenant), API key alone, then session cookie or bearer session.
async fn verify_http(
    client: &reqwest::Client,
    auth_url: &str,
    headers: &HeaderMap,
) -> Result<Caller, AuthError> {
    let bearer = extract_bearer_token(headers)?;
    let cookie = extract_session_cookie(headers);
    let tenant_hdr = match extract_tenant_id_header(headers) {
        Ok(t) => Some(t),
        Err(AuthError::BadRequest) => return Err(AuthError::BadRequest),
        Err(_) => None,
    };

    if let Some(token) = bearer.as_deref()
        && (tenant_hdr.is_some() || cookie.is_none())
    {
        match observable_auth::verify_api_key(client, auth_url, token).await {
            Ok(ctx) => {
                if tenant_hdr.is_some_and(|t| t != ctx.tenant_id) {
                    return Err(AuthError::Forbidden);
                }
                return Ok(Caller {
                    tenant_id: ctx.tenant_id,
                    user_id: None,
                    role: ctx.role,
                });
            }
            Err(AuthError::Unauthorized) => {}
            Err(e) => return Err(e),
        }
    }

    let session = cookie.or(bearer).ok_or(AuthError::Unauthorized)?;
    let ctx =
        observable_auth::verify_session_with_tenant(client, auth_url, &session, tenant_hdr).await?;
    Ok(Caller {
        tenant_id: ctx.tenant_id,
        user_id: Some(ctx.user_id),
        role: ctx.role,
    })
}

/// `decide-access` port.
#[derive(Clone)]
pub enum Decider {
    Http(HttpAccessDecider),
    Mock(MockAccessDecider),
}

impl AccessDecider for Decider {
    async fn decide(&self, request: &DecideRequest) -> Result<Decision, DecisionUnavailable> {
        match self {
            Self::Http(d) => d.decide(request).await,
            Self::Mock(d) => d.decide(request).await,
        }
    }

    async fn filter(
        &self,
        request: &FilterRequest,
    ) -> Result<Vec<ResourceKey>, DecisionUnavailable> {
        match self {
            Self::Http(d) => d.filter(request).await,
            Self::Mock(d) => d.filter(request).await,
        }
    }
}

/// `lookup-principals` port.
#[derive(Clone)]
pub enum Directory {
    Http(HttpPrincipalDirectory),
    Mock(MockPrincipalDirectory),
}

impl PrincipalDirectory for Directory {
    async fn resolve(
        &self,
        tenant_id: Uuid,
        user_ids: &[Uuid],
    ) -> Result<Vec<ResolvedPrincipal>, DirectoryUnavailable> {
        match self {
            Self::Http(d) => d.resolve(tenant_id, user_ids).await,
            Self::Mock(d) => d.resolve(tenant_id, user_ids).await,
        }
    }
}
