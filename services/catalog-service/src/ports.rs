//! Required-capability ports with real (HTTP to observable-auth and observable-control) and
//! substitute adapters, so
//! the catalog runs standalone against the provider-owned mocks (ISO-02).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
use serde::{Deserialize, Serialize};
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

/// One environment and its class as `manage-control-plane` reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentClass {
    pub environment: String,
    /// `production`, `non_production` or `unclassified`.
    pub class: String,
}

#[derive(Deserialize)]
struct EnvironmentClassList {
    environments: Vec<EnvironmentClass>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlUnavailable;

/// Substitute state: per-tenant environment classes.
pub type ClassTable = HashMap<Uuid, Vec<EnvironmentClass>>;

/// `manage-control-plane` port for environment classes (`GET /internal/environment-classes`).
#[derive(Clone)]
pub enum Environments {
    Http {
        base_url: Arc<String>,
        internal_token: Arc<String>,
        client: reqwest::Client,
    },
    /// Programmable substitute: per-tenant classes, or unavailable when `None`.
    Static(Arc<Mutex<Option<ClassTable>>>),
}

impl Environments {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(Some(HashMap::new()))))
    }

    /// Substitute only: replaces a tenant's classes.
    pub fn set(&self, tenant_id: Uuid, classes: Vec<EnvironmentClass>) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .get_or_insert_with(HashMap::new)
                .insert(tenant_id, classes);
        }
    }

    /// Substitute only: makes every call fail as unavailable.
    pub fn set_unavailable(&self) {
        if let Self::Static(s) = self {
            *s.lock().expect("substitute poisoned") = None;
        }
    }

    pub async fn classes(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<EnvironmentClass>, ControlUnavailable> {
        match self {
            Self::Static(s) => s
                .lock()
                .expect("substitute poisoned")
                .as_ref()
                .map(|m| m.get(&tenant_id).cloned().unwrap_or_default())
                .ok_or(ControlUnavailable),
            Self::Http {
                base_url,
                internal_token,
                client,
            } => {
                let response = client
                    .get(format!(
                        "{}/internal/environment-classes?tenant_id={tenant_id}",
                        base_url.trim_end_matches('/')
                    ))
                    .header("X-Internal-Token", internal_token.as_str())
                    .send()
                    .await
                    .map_err(|_| ControlUnavailable)?;
                if response.status().as_u16() != 200 {
                    return Err(ControlUnavailable);
                }
                let body: EnvironmentClassList =
                    response.json().await.map_err(|_| ControlUnavailable)?;
                Ok(body.environments)
            }
        }
    }
}

/// Most recent data per signal for one telemetry identity, in Unix nanoseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SignalsSeen {
    pub traces: Option<u64>,
    pub logs: Option<u64>,
    pub metrics: Option<u64>,
}

impl SignalsSeen {
    pub fn get(&self, signal: &str) -> Option<u64> {
        match signal {
            "traces" => self.traces,
            "logs" => self.logs,
            "metrics" => self.metrics,
            _ => None,
        }
    }

    fn merge(&mut self, other: SignalsSeen) {
        self.traces = self.traces.max(other.traces);
        self.logs = self.logs.max(other.logs);
        self.metrics = self.metrics.max(other.metrics);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TelemetryUnavailable;

/// Substitute state: `(tenant, service.name, environment)` -> signals seen.
pub type SignalTable = HashMap<(Uuid, String, String), SignalsSeen>;

/// `store-telemetry` port: recent data per signal for a telemetry identity. An empty
/// environment matches every environment.
#[derive(Clone)]
pub enum Telemetry {
    ClickHouse(Box<clickhouse::Client>),
    /// Programmable substitute; `None` makes every read fail.
    Static(Arc<Mutex<Option<SignalTable>>>),
}

#[derive(clickhouse::Row, Deserialize)]
struct SignalRow {
    signal: String,
    last_seen: u64,
}

impl Telemetry {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(Some(HashMap::new()))))
    }

    /// Substitute only: records data for an identity.
    pub fn record(
        &self,
        tenant_id: Uuid,
        service_name: &str,
        environment: &str,
        seen: SignalsSeen,
    ) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .get_or_insert_with(HashMap::new)
                .insert((tenant_id, service_name.into(), environment.into()), seen);
        }
    }

    /// Substitute only: makes every read fail.
    pub fn set_unavailable(&self) {
        if let Self::Static(s) = self {
            *s.lock().expect("substitute poisoned") = None;
        }
    }

    /// Latest data at or after `since_ns` per signal.
    pub async fn seen_since(
        &self,
        tenant_id: Uuid,
        service_name: &str,
        environment: &str,
        since_ns: u64,
    ) -> Result<SignalsSeen, TelemetryUnavailable> {
        match self {
            Self::Static(s) => {
                let guard = s.lock().expect("substitute poisoned");
                let table = guard.as_ref().ok_or(TelemetryUnavailable)?;
                let mut out = SignalsSeen::default();
                for ((t, name, env), seen) in table {
                    if *t == tenant_id
                        && name == service_name
                        && (environment.is_empty() || env == environment)
                    {
                        let keep = |v: Option<u64>| v.filter(|ns| *ns >= since_ns);
                        out.merge(SignalsSeen {
                            traces: keep(seen.traces),
                            logs: keep(seen.logs),
                            metrics: keep(seen.metrics),
                        });
                    }
                }
                Ok(out)
            }
            Self::ClickHouse(ch) => {
                let rows: Vec<SignalRow> = ch
                    .query(
                        "SELECT 'traces' AS signal, max(start_time_unix_nano) AS last_seen FROM observable.spans \
                         WHERE tenant_id = ? AND service_name = ? AND (? = '' OR environment = ?) AND start_time_unix_nano >= ? \
                         UNION ALL SELECT 'logs', max(timestamp_unix_nano) FROM observable.logs \
                         WHERE tenant_id = ? AND service_name = ? AND (? = '' OR environment = ?) AND timestamp_unix_nano >= ? \
                         UNION ALL SELECT 'metrics', max(time_unix_nano) FROM observable.metric_points \
                         WHERE tenant_id = ? AND service_name = ? AND time_unix_nano >= ? AND (? = '' OR metric_series_id IN \
                         (SELECT metric_series_id FROM observable.metric_series WHERE tenant_id = ? AND service_name = ? AND environment = ?))",
                    )
                    .bind(tenant_id)
                    .bind(service_name)
                    .bind(environment)
                    .bind(environment)
                    .bind(since_ns)
                    .bind(tenant_id)
                    .bind(service_name)
                    .bind(environment)
                    .bind(environment)
                    .bind(since_ns)
                    .bind(tenant_id)
                    .bind(service_name)
                    .bind(since_ns)
                    .bind(environment)
                    .bind(tenant_id)
                    .bind(service_name)
                    .bind(environment)
                    .fetch_all()
                    .await
                    .map_err(|e| {
                        tracing::warn!(error = %e, "telemetry coverage read failed");
                        TelemetryUnavailable
                    })?;
                let mut out = SignalsSeen::default();
                for r in rows {
                    let v = (r.last_seen > 0).then_some(r.last_seen);
                    match r.signal.as_str() {
                        "traces" => out.traces = v,
                        "logs" => out.logs = v,
                        "metrics" => out.metrics = v,
                        _ => {}
                    }
                }
                Ok(out)
            }
        }
    }
}

/// An asset a producer reports, with its kind.
#[derive(Clone, serde::Deserialize)]
pub struct ProducerAsset {
    pub kind: String,
    #[serde(flatten)]
    pub asset: crate::coverage::SyncAsset,
}

#[derive(serde::Deserialize)]
struct ProducerAssetList {
    assets: Vec<ProducerAsset>,
}

/// Substitute state: per-tenant control assets.
pub type ControlAssetTable = HashMap<Uuid, Vec<serde_json::Value>>;

/// `manage-control-plane` port for control's observability assets, i.e. dashboards
/// (`GET /internal/observability-assets`).
#[derive(Clone)]
pub enum ControlAssets {
    Http {
        base_url: Arc<String>,
        internal_token: Arc<String>,
        client: reqwest::Client,
    },
    /// Programmable substitute: per-tenant assets as JSON, or unavailable when `None`.
    Static(Arc<Mutex<Option<ControlAssetTable>>>),
}

impl ControlAssets {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(Some(HashMap::new()))))
    }

    /// Substitute only: replaces a tenant's assets.
    pub fn set(&self, tenant_id: Uuid, assets: Vec<serde_json::Value>) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .get_or_insert_with(HashMap::new)
                .insert(tenant_id, assets);
        }
    }

    /// Substitute only: makes every call fail as unavailable.
    pub fn set_unavailable(&self) {
        if let Self::Static(s) = self {
            *s.lock().expect("substitute poisoned") = None;
        }
    }

    pub async fn assets(&self, tenant_id: Uuid) -> Result<Vec<ProducerAsset>, ControlUnavailable> {
        match self {
            Self::Static(s) => {
                let rows = s
                    .lock()
                    .expect("substitute poisoned")
                    .as_ref()
                    .map(|m| m.get(&tenant_id).cloned().unwrap_or_default())
                    .ok_or(ControlUnavailable)?;
                serde_json::from_value(serde_json::Value::Array(rows))
                    .map_err(|_| ControlUnavailable)
            }
            Self::Http {
                base_url,
                internal_token,
                client,
            } => {
                let response = client
                    .get(format!(
                        "{}/internal/observability-assets?tenant_id={tenant_id}",
                        base_url.trim_end_matches('/')
                    ))
                    .header("X-Internal-Token", internal_token.as_str())
                    .send()
                    .await
                    .map_err(|_| ControlUnavailable)?;
                if response.status().as_u16() != 200 {
                    return Err(ControlUnavailable);
                }
                let body: ProducerAssetList =
                    response.json().await.map_err(|_| ControlUnavailable)?;
                Ok(body.assets)
            }
        }
    }
}
