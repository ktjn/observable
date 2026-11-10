//! `GET /v1/health/services/{id}?environment_class=` (assess-service-health 0.1.0).
//!
//! Order of checks, each failing closed:
//! 1. `validate-credential`, then the class parameter.
//! 2. `decide-access` on `health.assessment` for the id, before any lookup, so a policy outage
//!    answers missing and existing IDs with the same 503.
//! 3. The caller's own catalog read of the service: missing, hidden and non-live are one 404.
//! 4. `decide-access` on each `health.evidence` contributor of the current rule. A denied
//!    contributor withholds the whole assessment (restricted unknown).
//! 5. The stored snapshot, accepted only for the current rule and scope; freshness is
//!    evaluated now. An approved, current maintenance window discloses failing evidence as
//!    `planned_maintenance` with `underlying_state`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use observable_auth::authz::{
    AccessDecider, Classification, DecideRequest, Decision, FilterRequest, ResourceRef,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::HealthState;
use crate::assessment::{current_rule, load, scope_key};
use crate::maintenance;
use crate::ports::{Caller, EnvironmentClass, ServiceReadError};
use crate::scope::{Scope, scope};

pub const ASSESSMENT: &str = "health.assessment";
pub const EVIDENCE: &str = "health.evidence";

#[derive(Deserialize)]
pub struct HealthQuery {
    environment_class: Option<String>,
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "not_found")
}

fn decision_unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "access_decision_unavailable",
    )
}

/// A visible service whose assessment cannot be disclosed: no state, no times.
fn withheld(id: Uuid, class: EnvironmentClass, evidence_status: &str) -> Response {
    Json(json!({
        "service_id": id,
        "environment_class": class.as_str(),
        "state": "unknown",
        "evaluated_at": null,
        "last_evidence_at": null,
        "evidence_status": evidence_status,
        "conditions": [],
    }))
    .into_response()
}

pub async fn get_health(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(q): Query<HealthQuery>,
) -> Response {
    let caller = match state.credentials.verify(&headers).await {
        Ok(c) => c,
        Err(e) => {
            let status = StatusCode::from(e);
            let code = if status == StatusCode::BAD_REQUEST {
                "invalid_request"
            } else {
                "unauthorized"
            };
            return error(status, code);
        }
    };
    let Some(class) = q
        .environment_class
        .as_deref()
        .and_then(EnvironmentClass::parse)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid_request",
                "message": "environment_class must be production or non_production",
            })),
        )
            .into_response();
    };
    // An id that is not a UUID still goes through the request-level decision.
    let id = raw_id.parse::<Uuid>().ok();

    match decide(&state, &caller, &raw_id).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(()) => return decision_unavailable(),
    }
    let Some(id) = id else {
        return not_found();
    };

    let service = match state.services.read(&caller, &headers, id).await {
        Ok(s) => s,
        Err(ServiceReadError::NotFound) => return not_found(),
        Err(ServiceReadError::Unauthorized) => {
            return error(StatusCode::UNAUTHORIZED, "unauthorized");
        }
        Err(ServiceReadError::Unavailable) => return decision_unavailable(),
    };
    if !matches!(service.lifecycle.as_str(), "live" | "deprecated") {
        return not_found();
    }
    // Unknown service classification is treated as the strongest.
    let service_class =
        Classification::parse(&service.classification).unwrap_or(Classification::Restricted);

    let rule = match current_rule(&state.db, caller.tenant_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return withheld(id, class, "evaluation_unavailable"),
        Err(e) => {
            tracing::warn!(error = %e, "health rule store unavailable");
            return withheld(id, class, "evaluation_unavailable");
        }
    };

    // Every contributor of the current rule must be readable, whether or not it has evidence.
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
        Ok(keys) if keys.len() == wanted => {}
        Ok(_) => return withheld(id, class, "restricted"),
        Err(_) => return decision_unavailable(),
    }

    let snapshot = match load(&state.db, caller.tenant_id, id, class).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "health snapshot store unavailable");
            return withheld(id, class, "evaluation_unavailable");
        }
    };
    let Some(snapshot) = snapshot else {
        return withheld(id, class, "missing");
    };
    if snapshot.rule_id != rule.rule_id || snapshot.rule_revision != rule.revision {
        return withheld(id, class, "missing");
    }
    // The stored scope must still be the service's scope in this class.
    let current = match state.inputs.inputs(caller.tenant_id).await {
        Ok(inputs) => scope(&inputs, id, class),
        Err(_) => Scope::Unavailable,
    };
    match current {
        Scope::Bound(identities) if scope_key(&identities) == snapshot.scope_key => {}
        Scope::Bound(_) => return withheld(id, class, "missing"),
        // The worker cannot read the service (retired since, or hidden from the worker).
        Scope::NotEvaluated => return not_found(),
        Scope::Unavailable => return withheld(id, class, "evaluation_unavailable"),
    }

    let now = Utc::now();
    let (underlying, evidence_status) = snapshot.freshness(now);
    let window = match maintenance::active(&state.db, caller.tenant_id, id, class, now).await {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "health maintenance store unavailable");
            return withheld(id, class, "evaluation_unavailable");
        }
    };
    let health = maintenance::apply(underlying, window.as_ref());
    Json(json!({
        "service_id": id,
        "environment_class": class.as_str(),
        "state": health,
        "underlying_state": (health != underlying).then_some(underlying),
        "maintenance": window.map(|w| json!({
            "id": w.id,
            "starts_at": w.starts_at,
            "ends_at": w.ends_at,
            "reason": w.reason,
        })),
        "evaluated_at": snapshot.evaluated_at,
        "last_evidence_at": snapshot.last_evidence_at(),
        "evidence_status": evidence_status.as_str(),
        "conditions": snapshot.condition_states(now),
    }))
    .into_response()
}

async fn decide(state: &HealthState, caller: &Caller, id: &str) -> Result<bool, ()> {
    let req = DecideRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some("read".into()),
        resource: Some(ResourceRef::new(ASSESSMENT, id, Classification::Public)),
        purpose: None,
        elevation_id: None,
    };
    match state.decider.decide(&req).await {
        Ok(Decision::Permit) => Ok(true),
        Ok(Decision::Deny | Decision::PurposeMissing) => Ok(false),
        Err(_) => Err(()),
    }
}
