//! Read-only link from a locally-set alert-rule owner to the catalog's actual registered
//! owner for the rule's free-text `service_name` (see `alerts::OwnerRef`). Distinct from
//! `objectives::ObjectiveCatalog`, which resolves by catalog service id; this resolves by
//! `service_name` string against the catalog's owner-by-binding lookup.
//!
//! A catalog outage must never fail an alert rule read: every error (not found, hidden,
//! unauthorized, unreachable) resolves to `None`, same as "no owner" -- mirrors
//! `catalog_sync.rs`'s "a failing tenant does not stop the others" resilience doctrine and
//! `main.rs`'s "without CATALOG_URL the sync is off" posture.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, header};
use serde::Deserialize;
use uuid::Uuid;

use crate::alerts::OwnerRef;
use crate::objectives::CatalogError;

#[derive(Default)]
pub struct CatalogOwnerTable {
    pub owners: HashMap<String, Option<OwnerRef>>,
    pub unavailable: bool,
}

/// `GET /v1/catalog/services/owner-by-binding?service_name=` reads made with the caller's
/// own credential.
#[derive(Clone)]
pub enum CatalogOwnerPort {
    Http {
        base_url: Arc<String>,
        client: reqwest::Client,
    },
    Static(Arc<Mutex<CatalogOwnerTable>>),
    /// No catalog configured: every lookup resolves to `None`.
    Disabled,
}

#[derive(Deserialize)]
struct OwnerBody {
    principal_kind: String,
    principal_id: Uuid,
}

#[derive(Deserialize)]
struct OwnerByBindingResponse {
    owner: Option<OwnerBody>,
}

impl CatalogOwnerPort {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(CatalogOwnerTable::default())))
    }

    /// Substitute only.
    pub fn put(&self, service_name: &str, owner: Option<OwnerRef>) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .owners
                .insert(service_name.to_string(), owner);
        }
    }

    /// Substitute only.
    pub fn set_unavailable(&self, unavailable: bool) {
        if let Self::Static(s) = self {
            s.lock().expect("substitute poisoned").unavailable = unavailable;
        }
    }

    fn forward(req: reqwest::RequestBuilder, headers: &HeaderMap) -> reqwest::RequestBuilder {
        let mut req = req.timeout(Duration::from_secs(5));
        for name in [header::AUTHORIZATION, header::COOKIE] {
            if let Some(v) = headers.get(&name) {
                req = req.header(name, v.clone());
            }
        }
        req
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        req: reqwest::RequestBuilder,
    ) -> Result<T, CatalogError> {
        let resp = req.send().await.map_err(|_| CatalogError::Unavailable)?;
        match resp.status().as_u16() {
            200 => resp.json().await.map_err(|_| CatalogError::Unavailable),
            404 => Err(CatalogError::NotFound),
            401 => Err(CatalogError::Unauthorized),
            _ => Err(CatalogError::Unavailable),
        }
    }

    /// The catalog's registered owner for `service_name`, caller-credential-forwarded.
    /// `None` for no binding, a hidden binding, any catalog error, or a disabled port --
    /// this is a soft dependency, not a source of alert-rule-read failure.
    pub async fn owner(&self, headers: &HeaderMap, service_name: &str) -> Option<OwnerRef> {
        match self {
            Self::Disabled => None,
            Self::Static(s) => {
                let t = s.lock().expect("substitute poisoned");
                if t.unavailable {
                    return None;
                }
                t.owners.get(service_name).cloned().flatten()
            }
            Self::Http { base_url, client } => {
                let base = base_url.trim_end_matches('/');
                let Ok(mut url) =
                    reqwest::Url::parse(&format!("{base}/v1/catalog/services/owner-by-binding"))
                else {
                    return None;
                };
                url.query_pairs_mut()
                    .append_pair("service_name", service_name);
                let req = Self::forward(client.get(url), headers);
                Self::get_json::<OwnerByBindingResponse>(req)
                    .await
                    .ok()
                    .and_then(|body| body.owner)
                    .map(|o| OwnerRef {
                        principal_kind: o.principal_kind,
                        principal_id: o.principal_id,
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn owner(kind: &str, id: Uuid) -> OwnerRef {
        OwnerRef {
            principal_kind: kind.into(),
            principal_id: id,
        }
    }

    #[tokio::test]
    async fn disabled_port_resolves_to_none() {
        let port = CatalogOwnerPort::Disabled;
        assert_eq!(port.owner(&HeaderMap::new(), "payments-api").await, None);
    }

    #[tokio::test]
    async fn static_port_resolves_the_seeded_owner() {
        let port = CatalogOwnerPort::substitute();
        let id = Uuid::new_v4();
        port.put("payments-api", Some(owner("team", id)));
        assert_eq!(
            port.owner(&HeaderMap::new(), "payments-api").await,
            Some(owner("team", id))
        );
    }

    #[tokio::test]
    async fn static_port_resolves_to_none_for_an_unbound_service_name() {
        let port = CatalogOwnerPort::substitute();
        assert_eq!(port.owner(&HeaderMap::new(), "unknown-service").await, None);
    }

    #[tokio::test]
    async fn static_port_resolves_to_none_for_a_bound_but_ownerless_service() {
        let port = CatalogOwnerPort::substitute();
        port.put("payments-api", None);
        assert_eq!(port.owner(&HeaderMap::new(), "payments-api").await, None);
    }

    #[tokio::test]
    async fn an_unavailable_catalog_resolves_to_none_rather_than_failing() {
        let port = CatalogOwnerPort::substitute();
        let id = Uuid::new_v4();
        port.put("payments-api", Some(owner("user", id)));
        port.set_unavailable(true);
        assert_eq!(port.owner(&HeaderMap::new(), "payments-api").await, None);
    }
}
