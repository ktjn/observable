//! Required-capability ports of the health worker (phase 1 C2.2), each with a real adapter and
//! a programmable substitute:
//!
//! - `manage-service-catalog`: lifecycle, telemetry bindings and approved environment classes
//!   for one tenant, read with that tenant's worker credential.
//! - `store-telemetry`: the latest evidence per signal for one exact telemetry identity, read
//!   through a read-only ClickHouse session.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::HeaderMap;
use axum::http::header::{AUTHORIZATION, COOKIE};
use observable_auth::authz::{
    AccessDecider, DecideRequest, Decision, DecisionUnavailable, FilterRequest, HttpAccessDecider,
    MockAccessDecider, Principal, ResourceKey,
};
use observable_auth::{
    AuthError, extract_bearer_token, extract_session_cookie, extract_tenant_id_header,
};
use serde::Deserialize;
use uuid::Uuid;

/// Approved environment class of a binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentClass {
    Production,
    NonProduction,
}

impl EnvironmentClass {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "production" => Some(Self::Production),
            "non_production" => Some(Self::NonProduction),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::NonProduction => "non_production",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BindingInput {
    pub service_name: String,
    pub environment: String,
    /// None for an empty or unclassified environment.
    pub environment_class: Option<EnvironmentClass>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ServiceInputs {
    pub id: Uuid,
    pub lifecycle: String,
    pub criticality: String,
    pub classification: String,
    pub bindings: Vec<BindingInput>,
}

/// Catalog inputs of one tenant as the worker is authorized to read them.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TenantInputs {
    pub tenant_id: Uuid,
    pub services: Vec<ServiceInputs>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputsUnavailable;

/// Substitute state: per-tenant inputs, or every read unavailable when `None`.
pub type InputsTable = HashMap<Uuid, Vec<ServiceInputs>>;

/// `manage-service-catalog` port (`GET /internal/catalog/health-inputs`).
#[derive(Clone)]
pub enum CatalogInputs {
    Http {
        base_url: Arc<String>,
        internal_token: Arc<String>,
        /// Tenant -> worker credential issued for that tenant. A tenant without one is never
        /// read, so a credential can only reach the tenant it belongs to.
        workers: Arc<HashMap<Uuid, String>>,
        client: reqwest::Client,
    },
    Static(Arc<Mutex<Option<InputsTable>>>),
}

impl CatalogInputs {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(Some(HashMap::new()))))
    }

    /// Substitute only: replaces a tenant's inputs.
    pub fn set(&self, tenant_id: Uuid, services: Vec<ServiceInputs>) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .get_or_insert_with(HashMap::new)
                .insert(tenant_id, services);
        }
    }

    /// Substitute only: makes every read unavailable.
    pub fn set_unavailable(&self) {
        if let Self::Static(s) = self {
            *s.lock().expect("substitute poisoned") = None;
        }
    }

    /// Tenants the worker holds a credential for (substitute: tenants with inputs).
    pub fn tenants(&self) -> Vec<Uuid> {
        let mut out: Vec<Uuid> = match self {
            Self::Http { workers, .. } => workers.keys().copied().collect(),
            Self::Static(s) => s
                .lock()
                .expect("substitute poisoned")
                .as_ref()
                .map(|m| m.keys().copied().collect())
                .unwrap_or_default(),
        };
        out.sort();
        out
    }

    pub async fn inputs(&self, tenant_id: Uuid) -> Result<TenantInputs, InputsUnavailable> {
        match self {
            Self::Static(s) => s
                .lock()
                .expect("substitute poisoned")
                .as_ref()
                .map(|m| TenantInputs {
                    tenant_id,
                    services: m.get(&tenant_id).cloned().unwrap_or_default(),
                })
                .ok_or(InputsUnavailable),
            Self::Http {
                base_url,
                internal_token,
                workers,
                client,
            } => {
                let worker = workers.get(&tenant_id).ok_or(InputsUnavailable)?;
                let response = client
                    .get(format!(
                        "{}/internal/catalog/health-inputs?tenant_id={tenant_id}",
                        base_url.trim_end_matches('/')
                    ))
                    .header("X-Internal-Token", internal_token.as_str())
                    .header("X-Health-Worker-Token", worker.as_str())
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|_| InputsUnavailable)?;
                if response.status().as_u16() != 200 {
                    tracing::warn!(%tenant_id, status = %response.status(), "catalog inputs refused");
                    return Err(InputsUnavailable);
                }
                let body: TenantInputs = response.json().await.map_err(|_| InputsUnavailable)?;
                // Inputs for another tenant are never used.
                if body.tenant_id != tenant_id {
                    return Err(InputsUnavailable);
                }
                Ok(body)
            }
        }
    }
}

/// One telemetry identity: tenant-scoped `service.name` and an exact nonempty environment.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TelemetryIdentity {
    pub service_name: String,
    pub environment: String,
}

/// Latest observation per signal in Unix nanoseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvidenceTimes {
    pub traces: Option<u64>,
    pub logs: Option<u64>,
    pub metrics: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceUnavailable;

/// Substitute state: `(tenant, service.name, environment)` -> latest evidence.
pub type EvidenceTable = HashMap<(Uuid, String, String), EvidenceTimes>;

/// `store-telemetry` port: latest evidence for an exact identity.
#[derive(Clone)]
pub enum Evidence {
    ClickHouse(Box<clickhouse::Client>),
    Static(Arc<Mutex<Option<EvidenceTable>>>),
}

#[derive(clickhouse::Row, Deserialize)]
struct SignalRow {
    signal: String,
    last_seen: u64,
}

impl Evidence {
    /// Wraps a client in a read-only session: every query runs with `readonly = 2`, so the
    /// evidence port cannot write or alter telemetry even if its login could.
    pub fn clickhouse(client: clickhouse::Client) -> Self {
        Self::ClickHouse(Box::new(client.with_setting("readonly", "2")))
    }

    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(Some(HashMap::new()))))
    }

    /// Substitute only: records evidence for an identity.
    pub fn record(&self, tenant_id: Uuid, identity: &TelemetryIdentity, times: EvidenceTimes) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .get_or_insert_with(HashMap::new)
                .insert(
                    (
                        tenant_id,
                        identity.service_name.clone(),
                        identity.environment.clone(),
                    ),
                    times,
                );
        }
    }

    /// Substitute only: makes every read unavailable.
    pub fn set_unavailable(&self) {
        if let Self::Static(s) = self {
            *s.lock().expect("substitute poisoned") = None;
        }
    }

    /// Latest evidence at or after `since_ns`. An empty environment is not an identity and has
    /// no evidence.
    pub async fn latest_since(
        &self,
        tenant_id: Uuid,
        identity: &TelemetryIdentity,
        since_ns: u64,
    ) -> Result<EvidenceTimes, EvidenceUnavailable> {
        if identity.environment.is_empty() || identity.service_name.is_empty() {
            return Ok(EvidenceTimes::default());
        }
        match self {
            Self::Static(s) => {
                let guard = s.lock().expect("substitute poisoned");
                let table = guard.as_ref().ok_or(EvidenceUnavailable)?;
                let keep = |v: Option<u64>| v.filter(|ns| *ns >= since_ns);
                Ok(table
                    .get(&(
                        tenant_id,
                        identity.service_name.clone(),
                        identity.environment.clone(),
                    ))
                    .map(|t| EvidenceTimes {
                        traces: keep(t.traces),
                        logs: keep(t.logs),
                        metrics: keep(t.metrics),
                    })
                    .unwrap_or_default())
            }
            Self::ClickHouse(ch) => {
                let (name, env) = (&identity.service_name, &identity.environment);
                let rows: Vec<SignalRow> = ch
                    .query(
                        "SELECT 'traces' AS signal, max(start_time_unix_nano) AS last_seen FROM observable.spans \
                         WHERE tenant_id = ? AND service_name = ? AND environment = ? AND start_time_unix_nano >= ? \
                         UNION ALL SELECT 'logs', max(timestamp_unix_nano) FROM observable.logs \
                         WHERE tenant_id = ? AND service_name = ? AND environment = ? AND timestamp_unix_nano >= ? \
                         UNION ALL SELECT 'metrics', max(time_unix_nano) FROM observable.metric_points \
                         WHERE tenant_id = ? AND service_name = ? AND time_unix_nano >= ? AND metric_series_id IN \
                         (SELECT metric_series_id FROM observable.metric_series WHERE tenant_id = ? AND service_name = ? AND environment = ?)",
                    )
                    .bind(tenant_id)
                    .bind(name)
                    .bind(env)
                    .bind(since_ns)
                    .bind(tenant_id)
                    .bind(name)
                    .bind(env)
                    .bind(since_ns)
                    .bind(tenant_id)
                    .bind(name)
                    .bind(since_ns)
                    .bind(tenant_id)
                    .bind(name)
                    .bind(env)
                    .fetch_all()
                    .await
                    .map_err(|e| {
                        tracing::warn!(error = %e, "health evidence read failed");
                        EvidenceUnavailable
                    })?;
                let mut out = EvidenceTimes::default();
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

// --- Caller-facing ports of the read contract ------------------------------------------------

/// Verified caller of a health read.
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

/// Same resolution order as the catalog: API key with `X-Tenant-ID` (must match the key's
/// tenant), API key alone, then session cookie or bearer session.
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

/// The caller's view of one catalog service: what health needs to authorize and scope a read.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ServiceView {
    pub id: Uuid,
    pub lifecycle: String,
    pub classification: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceReadError {
    /// Missing or hidden from the caller; the catalog does not distinguish them.
    NotFound,
    Unauthorized,
    Unavailable,
}

/// Substitute state: `(tenant, service id)` -> record, ids hidden from every caller, and
/// availability.
#[derive(Default)]
pub struct ServiceTable {
    pub records: HashMap<(Uuid, Uuid), ServiceView>,
    pub hidden: std::collections::HashSet<Uuid>,
    pub unavailable: bool,
}

/// `manage-service-catalog` port for the caller's own read of a service
/// (`GET /v1/catalog/services/{id}`). The caller's credential is forwarded, so the catalog
/// authorizes the service record under the caller's tenant and role.
#[derive(Clone)]
pub enum ServiceReads {
    Http {
        base_url: Arc<String>,
        client: reqwest::Client,
    },
    Static(Arc<Mutex<ServiceTable>>),
}

impl ServiceReads {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(ServiceTable::default())))
    }

    /// Substitute only: stores or replaces a record.
    pub fn put(&self, tenant_id: Uuid, view: ServiceView) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .records
                .insert((tenant_id, view.id), view);
        }
    }

    /// Substitute only: hides a record from every caller.
    pub fn hide(&self, id: Uuid) {
        if let Self::Static(s) = self {
            s.lock().expect("substitute poisoned").hidden.insert(id);
        }
    }

    /// Substitute only: makes every read unavailable, or available again.
    pub fn set_unavailable(&self, unavailable: bool) {
        if let Self::Static(s) = self {
            s.lock().expect("substitute poisoned").unavailable = unavailable;
        }
    }

    pub async fn read(
        &self,
        caller: &Caller,
        headers: &HeaderMap,
        id: Uuid,
    ) -> Result<ServiceView, ServiceReadError> {
        match self {
            Self::Static(s) => {
                let table = s.lock().expect("substitute poisoned");
                if table.unavailable {
                    return Err(ServiceReadError::Unavailable);
                }
                match table.records.get(&(caller.tenant_id, id)) {
                    Some(v) if !table.hidden.contains(&id) => Ok(v.clone()),
                    _ => Err(ServiceReadError::NotFound),
                }
            }
            Self::Http { base_url, client } => {
                let mut req = client.get(format!(
                    "{}/v1/catalog/services/{id}",
                    base_url.trim_end_matches('/')
                ));
                for name in [AUTHORIZATION, COOKIE] {
                    if let Some(v) = headers.get(&name) {
                        req = req.header(name, v.clone());
                    }
                }
                if let Some(v) = headers.get("x-tenant-id") {
                    req = req.header("x-tenant-id", v.clone());
                }
                let response = req
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                    .map_err(|_| ServiceReadError::Unavailable)?;
                match response.status().as_u16() {
                    200 => {
                        let view: ServiceView = response
                            .json()
                            .await
                            .map_err(|_| ServiceReadError::Unavailable)?;
                        // A record for another id is never trusted.
                        if view.id != id {
                            return Err(ServiceReadError::Unavailable);
                        }
                        Ok(view)
                    }
                    404 => Err(ServiceReadError::NotFound),
                    401 => Err(ServiceReadError::Unauthorized),
                    _ => Err(ServiceReadError::Unavailable),
                }
            }
        }
    }
}
