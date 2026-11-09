//! Catalog inputs for the health worker (phase 1 C2.2): lifecycle, telemetry bindings and
//! their approved environment classes for one tenant.
//!
//! The route needs both the internal service token and a health worker credential issued for
//! the requested tenant. A cluster token alone, a tenant session or a worker credential of
//! another tenant is refused. The worker then reads as the `health_worker` service principal,
//! so `decide-access` still filters every service record.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::CatalogState;
use crate::api::{ApiError, ApiResult, load_services, visible};
use crate::coverage::internal_auth;
use crate::ports::{Caller, WorkerRejected};

/// Header carrying the tenant-bound worker credential.
pub const WORKER_HEADER: &str = "x-health-worker-token";
/// Service principal role the worker reads under (migration 060).
pub const WORKER_ROLE: &str = "health_worker";

#[derive(Deserialize)]
pub struct InputsQuery {
    tenant_id: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InputBinding {
    pub service_name: String,
    pub environment: String,
    /// `production` or `non_production`; null for an empty or unclassified environment.
    pub environment_class: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct InputService {
    pub id: Uuid,
    pub lifecycle: String,
    pub criticality: String,
    pub classification: String,
    pub bindings: Vec<InputBinding>,
}

/// Approved class of a bound environment. Empty environments match every environment and so
/// belong to no class; anything control does not classify is unclassified.
pub fn class_of(environment: &str, classes: &HashMap<String, String>) -> Option<&'static str> {
    if environment.is_empty() {
        return None;
    }
    match classes.get(environment).map(String::as_str) {
        Some("production") => Some("production"),
        Some("non_production") => Some("non_production"),
        _ => None,
    }
}

fn unavailable(code: &'static str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": code })),
    )
        .into_response()
}

/// GET /internal/catalog/health-inputs?tenant_id= (internal token and health worker credential).
pub async fn health_inputs(
    State(state): State<CatalogState>,
    headers: HeaderMap,
    Query(q): Query<InputsQuery>,
) -> ApiResult<Response> {
    internal_auth(&state, &headers)?;
    let token = headers
        .get(WORKER_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|t| !t.is_empty())
        .ok_or(ApiError::Unauthorized)?;
    // An unparsable tenant is an invalid credential binding, not a request-shape hint.
    let tenant_id = q
        .tenant_id
        .as_deref()
        .and_then(|t| t.parse::<Uuid>().ok())
        .ok_or(ApiError::Unauthorized)?;
    match state.workers.validate(tenant_id, token).await {
        Ok(()) => {}
        Err(WorkerRejected::Unauthorized) => return Err(ApiError::Unauthorized),
        Err(WorkerRejected::Unavailable) => {
            return Ok(unavailable("credential_validation_unavailable"));
        }
    }

    let worker = Caller {
        tenant_id,
        user_id: None,
        role: WORKER_ROLE.to_owned(),
    };
    let services = visible(
        &state,
        &worker,
        load_services(&state.db, tenant_id, None).await?,
    )
    .await?;
    let Ok(classes) = state.environments.classes(tenant_id).await else {
        return Ok(unavailable("control_plane_unavailable"));
    };
    let classes: HashMap<String, String> = classes
        .into_iter()
        .map(|c| (c.environment, c.class))
        .collect();

    let ids: Vec<Uuid> = services.iter().map(|s| s.id).collect();
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT service_id, service_name, environment FROM catalog.service_bindings \
         WHERE tenant_id = $1 AND service_id = ANY($2) ORDER BY service_name, environment",
    )
    .bind(tenant_id)
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let mut bindings: HashMap<Uuid, Vec<InputBinding>> = HashMap::new();
    for (service_id, service_name, environment) in rows {
        let environment_class = class_of(&environment, &classes);
        bindings.entry(service_id).or_default().push(InputBinding {
            service_name,
            environment,
            environment_class,
        });
    }
    let services: Vec<InputService> = services
        .into_iter()
        .map(|s| InputService {
            bindings: bindings.remove(&s.id).unwrap_or_default(),
            id: s.id,
            lifecycle: s.lifecycle,
            criticality: s.criticality,
            classification: s.classification,
        })
        .collect();
    Ok(Json(json!({ "tenant_id": tenant_id, "services": services })).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_classified_environments_have_a_class() {
        let classes = HashMap::from([
            ("prod".to_owned(), "production".to_owned()),
            ("stage".to_owned(), "non_production".to_owned()),
            ("lab".to_owned(), "unclassified".to_owned()),
        ]);
        assert_eq!(class_of("prod", &classes), Some("production"));
        assert_eq!(class_of("stage", &classes), Some("non_production"));
        assert_eq!(class_of("lab", &classes), None);
        assert_eq!(class_of("unknown", &classes), None);
        assert_eq!(class_of("", &classes), None);
        assert_eq!(class_of("Prod", &classes), None);
    }
}
