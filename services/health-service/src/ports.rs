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

use serde::Deserialize;
use uuid::Uuid;

/// Approved environment class of a binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
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
