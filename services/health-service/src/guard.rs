//! Shared read checks for per-service health disclosures (history, issues), in the order of
//! the C2.3 read contract, each failing closed:
//!
//! 1. `decide-access` read on the request's resource type for the raw id, before any lookup,
//!    so a policy outage answers missing and existing ids with the same 503.
//! 2. The caller's own catalog read of the service: missing, hidden and other-tenant are 404.
//! 3. `decide-access` on every `health.evidence` contributor of the current rule. A denied
//!    contributor withholds the disclosure (`restricted`).
//!
//! The caller's credential and the class parameter are checked by the handler first.

use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use observable_auth::authz::{
    AccessDecider, Classification, DecideRequest, Decision, FilterRequest, ResourceRef,
};
use serde_json::json;
use uuid::Uuid;

use crate::HealthState;
use crate::api::EVIDENCE;
use crate::assessment::current_rule;
use crate::ports::{Caller, EnvironmentClass, ServiceReadError, ServiceView};

/// Why a disclosure is refused: an error response, or a withheld body with this status.
pub enum Refusal {
    Respond(Response),
    Withhold(&'static str),
}

fn error(status: StatusCode, code: &str) -> Refusal {
    Refusal::Respond((status, Json(json!({ "error": code }))).into_response())
}

/// Verifies the caller's credential.
pub async fn caller(state: &HealthState, headers: &HeaderMap) -> Result<Caller, Response> {
    state.credentials.verify(headers).await.map_err(|e| {
        let status = StatusCode::from(e);
        let code = if status == StatusCode::BAD_REQUEST {
            "invalid_request"
        } else {
            "unauthorized"
        };
        (status, Json(json!({ "error": code }))).into_response()
    })
}

/// Steps 1–3 for `raw_id` in `class`. Returns the service id and the caller's view of it.
pub async fn evidence_read(
    state: &HealthState,
    caller: &Caller,
    headers: &HeaderMap,
    raw_id: &str,
    resource_type: &str,
    class: EnvironmentClass,
) -> Result<(Uuid, ServiceView), Refusal> {
    let req = DecideRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some("read".into()),
        resource: Some(ResourceRef::new(
            resource_type,
            raw_id,
            Classification::Public,
        )),
        purpose: None,
        elevation_id: None,
    };
    match state.decider.decide(&req).await {
        Ok(Decision::Permit) => {}
        Ok(Decision::Deny | Decision::PurposeMissing) => {
            return Err(error(StatusCode::NOT_FOUND, "not_found"));
        }
        Err(_) => {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "access_decision_unavailable",
            ));
        }
    }
    let id = raw_id
        .parse::<Uuid>()
        .map_err(|_| error(StatusCode::NOT_FOUND, "not_found"))?;
    let service = match state.services.read(caller, headers, id).await {
        Ok(s) => s,
        Err(ServiceReadError::NotFound) => return Err(error(StatusCode::NOT_FOUND, "not_found")),
        Err(ServiceReadError::Unauthorized) => {
            return Err(error(StatusCode::UNAUTHORIZED, "unauthorized"));
        }
        Err(ServiceReadError::Unavailable) => {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "access_decision_unavailable",
            ));
        }
    };
    let service_class =
        Classification::parse(&service.classification).unwrap_or(Classification::Restricted);
    let rule = match current_rule(&state.db, caller.tenant_id).await {
        Ok(Some(r)) => r,
        _ => return Err(Refusal::Withhold("evaluation_unavailable")),
    };
    let contributors: Vec<ResourceRef> = rule
        .conditions
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let c = Classification::parse(&c.classification)
                .unwrap_or(Classification::Restricted)
                .max(service_class);
            ResourceRef::new(EVIDENCE, &format!("{id}/{}/{i}", class.as_str()), c)
        })
        .collect();
    let wanted = contributors.len();
    let filter = FilterRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some("read".into()),
        resources: contributors,
        purpose: None,
        elevation_id: None,
    };
    match state.decider.filter(&filter).await {
        Ok(keys) if keys.len() == wanted => Ok((id, service)),
        Ok(_) => Err(Refusal::Withhold("restricted")),
        Err(_) => Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "access_decision_unavailable",
        )),
    }
}
