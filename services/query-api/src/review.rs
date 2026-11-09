//! Service review (Phase 1 C4): health and agreed expectations of catalog services in one
//! explicit environment class.
//!
//! `GET /v1/review/services/{id}?environment_class=` and
//! `GET /v1/review/services?environment_class=&context=business_critical|all` compose three
//! providers with the caller's own credential: the catalog record (lifecycle, criticality),
//! `assess-service-health` and the approved objectives of `manage-reliability-objectives`.
//! Query never widens access: a record the catalog hides is a 404, and a provider that
//! withholds or cannot evaluate yields `unknown`, never a healthy or compliant result.
//! Retired and planned services are not live and carry no health or expectation.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path, Query},
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

const FORWARDED: [HeaderName; 3] = [
    header::AUTHORIZATION,
    header::COOKIE,
    HeaderName::from_static("x-tenant-id"),
];
const CLASSES: [&str; 2] = ["production", "non_production"];
const PAGE_LIMIT: usize = 50;

/// Provider base URLs. `None` when the deployment does not run the catalog or health.
#[derive(Clone)]
pub struct ReviewProviders {
    inner: Option<Arc<Urls>>,
}

struct Urls {
    catalog: String,
    health: String,
    alerting: String,
    client: reqwest::Client,
}

impl ReviewProviders {
    pub fn http(
        catalog: impl Into<String>,
        health: impl Into<String>,
        alerting: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        let trim = |s: String| s.trim_end_matches('/').to_string();
        Self {
            inner: Some(Arc::new(Urls {
                catalog: trim(catalog.into()),
                health: trim(health.into()),
                alerting: trim(alerting.into()),
                client,
            })),
        }
    }

    pub fn disabled() -> Self {
        Self { inner: None }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Failure {
    NotFound,
    Unauthorized,
    Unavailable,
}

/// One provider read: the status class and, for 2xx, the JSON body.
async fn fetch(urls: &Urls, headers: &HeaderMap, url: String) -> Result<Value, Failure> {
    let mut req = urls.client.get(url);
    for name in FORWARDED {
        if let Some(v) = headers.get(&name) {
            req = req.header(name, v);
        }
    }
    let resp = req.send().await.map_err(|_| Failure::Unavailable)?;
    match resp.status() {
        s if s.is_success() => resp.json().await.map_err(|_| Failure::Unavailable),
        // 409 is an objective that is not (or no longer) approved: no result, like a 404.
        StatusCode::NOT_FOUND | StatusCode::CONFLICT => Err(Failure::NotFound),
        StatusCode::UNAUTHORIZED => Err(Failure::Unauthorized),
        _ => Err(Failure::Unavailable),
    }
}

#[derive(Deserialize)]
pub struct ReviewQuery {
    environment_class: Option<String>,
    context: Option<String>,
    offset: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServiceRef {
    pub id: Uuid,
    pub business_name: String,
    pub criticality: String,
    pub lifecycle: String,
}

impl ServiceRef {
    fn live(&self) -> bool {
        matches!(self.lifecycle.as_str(), "live" | "deprecated")
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Health {
    pub state: String,
    pub evidence_status: String,
    pub evaluated_at: Option<String>,
    pub last_evidence_at: Option<String>,
}

impl Health {
    fn unknown(evidence_status: &str) -> Self {
        Self {
            state: "unknown".into(),
            evidence_status: evidence_status.into(),
            evaluated_at: None,
            last_evidence_at: None,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ObjectiveReview {
    pub objective_id: Uuid,
    pub sli_type: String,
    pub target: f64,
    pub window_days: i64,
    pub description: String,
    pub approved: bool,
    /// Compliance of an approved objective; `None` while unapproved.
    pub status: Option<String>,
    pub evidence_status: Option<String>,
    pub attainment: Option<f64>,
    pub error_budget_consumed: Option<f64>,
    pub last_evidence_at: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Expectation {
    /// `compliant`, `at_risk`, `breached` or `unknown`.
    pub status: String,
    /// Why the status is what it is: `current`, `missing`, `stale`, `restricted`,
    /// `evaluation_unavailable` or `no_approved_objective`.
    pub reason: String,
    pub objectives: Vec<ObjectiveReview>,
}

#[derive(Debug, Serialize)]
pub struct ServiceReview {
    pub service: ServiceRef,
    pub environment_class: String,
    pub live: bool,
    pub health: Option<Health>,
    pub expectation: Option<Expectation>,
}

/// Combines approved objectives into one expectation status. Evidenced breaches and risks win;
/// otherwise any unknown objective keeps the whole expectation unknown, so a service lacking
/// current evidence is never reported as compliant (OBS-01-04).
pub fn combine(objectives: Vec<ObjectiveReview>) -> Expectation {
    let approved: Vec<&ObjectiveReview> = objectives.iter().filter(|o| o.approved).collect();
    let has = |s: &str| approved.iter().any(|o| o.status.as_deref() == Some(s));
    let (status, reason) = if has("breached") {
        ("breached", "current".to_string())
    } else if has("at_risk") {
        ("at_risk", "current".to_string())
    } else if let Some(u) = approved
        .iter()
        .find(|o| o.status.as_deref() != Some("compliant"))
    {
        (
            "unknown",
            u.evidence_status
                .clone()
                .unwrap_or_else(|| "evaluation_unavailable".into()),
        )
    } else if approved.is_empty() {
        ("unknown", "no_approved_objective".to_string())
    } else {
        ("compliant", "current".to_string())
    };
    Expectation {
        status: status.into(),
        reason,
        objectives,
    }
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn class_of(q: &ReviewQuery) -> Option<&str> {
    q.environment_class
        .as_deref()
        .filter(|c| CLASSES.contains(c))
}

fn invalid_class() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "invalid_request",
            "message": "environment_class must be production or non_production",
        })),
    )
        .into_response()
}

fn catalog_failure(f: Failure) -> Response {
    match f {
        Failure::NotFound => error(StatusCode::NOT_FOUND, "not_found"),
        Failure::Unauthorized => error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Failure::Unavailable => error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable"),
    }
}

async fn health(
    urls: &Urls,
    headers: &HeaderMap,
    id: Uuid,
    class: &str,
) -> Result<Health, Failure> {
    let url = format!(
        "{}/v1/health/services/{id}?environment_class={class}",
        urls.health
    );
    match fetch(urls, headers, url).await {
        Ok(v) => {
            let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
            match (text("state"), text("evidence_status")) {
                (Some(state), Some(evidence_status)) => Ok(Health {
                    state,
                    evidence_status,
                    evaluated_at: text("evaluated_at"),
                    last_evidence_at: text("last_evidence_at"),
                }),
                _ => Ok(Health::unknown("evaluation_unavailable")),
            }
        }
        // The provider hides the assessment from this caller.
        Err(Failure::NotFound) => Ok(Health::unknown("restricted")),
        Err(Failure::Unauthorized) => Err(Failure::Unauthorized),
        Err(Failure::Unavailable) => Ok(Health::unknown("evaluation_unavailable")),
    }
}

async fn expectation(
    urls: &Urls,
    headers: &HeaderMap,
    id: Uuid,
    class: &str,
) -> Result<Expectation, Failure> {
    let unknown = |reason: &str| Expectation {
        status: "unknown".into(),
        reason: reason.into(),
        objectives: vec![],
    };
    let list = match fetch(
        urls,
        headers,
        format!("{}/v1/objectives?service_id={id}", urls.alerting),
    )
    .await
    {
        Ok(v) => v,
        Err(Failure::NotFound) => return Ok(unknown("restricted")),
        Err(Failure::Unauthorized) => return Err(Failure::Unauthorized),
        Err(Failure::Unavailable) => return Ok(unknown("evaluation_unavailable")),
    };
    let items = list
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut objectives = Vec::new();
    for o in items
        .iter()
        .filter(|o| o.get("environment_class").and_then(Value::as_str) == Some(class))
    {
        let Some(objective_id) = o
            .get("objective_id")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
        else {
            continue;
        };
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let mut review = ObjectiveReview {
            objective_id,
            sli_type: text(o, "sli_type").unwrap_or_default(),
            target: o.get("target").and_then(Value::as_f64).unwrap_or_default(),
            window_days: o
                .get("window_days")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            description: text(o, "description").unwrap_or_default(),
            approved: o.get("approved_at").is_some_and(|v| !v.is_null()),
            status: None,
            evidence_status: None,
            attainment: None,
            error_budget_consumed: None,
            last_evidence_at: None,
        };
        if review.approved {
            let url = format!("{}/v1/objectives/{objective_id}/compliance", urls.alerting);
            match fetch(urls, headers, url).await {
                Ok(c) => {
                    review.status = text(&c, "status");
                    review.evidence_status = text(&c, "evidence_status");
                    review.attainment = c.get("attainment").and_then(Value::as_f64);
                    review.error_budget_consumed =
                        c.get("error_budget_consumed").and_then(Value::as_f64);
                    review.last_evidence_at = text(&c, "last_evidence_at");
                }
                Err(Failure::Unauthorized) => return Err(Failure::Unauthorized),
                // Changed or unapproved since the list was read: no result.
                Err(Failure::NotFound) => {
                    review.approved = false;
                }
                Err(Failure::Unavailable) => {
                    review.status = Some("unknown".into());
                    review.evidence_status = Some("evaluation_unavailable".into());
                }
            }
        }
        objectives.push(review);
    }
    Ok(combine(objectives))
}

async fn review(
    urls: &Urls,
    headers: &HeaderMap,
    service: ServiceRef,
    class: &str,
) -> Result<ServiceReview, Failure> {
    let live = service.live();
    let (health, expectation) = if live {
        let (h, e) = tokio::join!(
            health(urls, headers, service.id, class),
            expectation(urls, headers, service.id, class)
        );
        (Some(h?), Some(e?))
    } else {
        (None, None)
    };
    Ok(ServiceReview {
        service,
        environment_class: class.into(),
        live,
        health,
        expectation,
    })
}

fn service_ref(v: &Value) -> Option<ServiceRef> {
    serde_json::from_value(v.clone()).ok()
}

/// GET /v1/review/services/{id}?environment_class=
pub async fn get_service_review(
    Extension(providers): Extension<ReviewProviders>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ReviewQuery>,
) -> Response {
    let Some(class) = class_of(&q) else {
        return invalid_class();
    };
    let Some(urls) = providers.inner.as_deref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "review_unavailable");
    };
    let Ok(id) = id.parse::<Uuid>() else {
        return error(StatusCode::NOT_FOUND, "not_found");
    };
    let record = match fetch(
        urls,
        &headers,
        format!("{}/v1/catalog/services/{id}", urls.catalog),
    )
    .await
    {
        Ok(v) => v,
        Err(f) => return catalog_failure(f),
    };
    let Some(service) = service_ref(&record) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
    };
    match review(urls, &headers, service, class).await {
        Ok(r) => Json(r).into_response(),
        Err(f) => catalog_failure(f),
    }
}

/// GET /v1/review/services?environment_class=&context=business_critical|all&offset=
pub async fn list_service_reviews(
    Extension(providers): Extension<ReviewProviders>,
    headers: HeaderMap,
    Query(q): Query<ReviewQuery>,
) -> Response {
    let Some(class) = class_of(&q).map(str::to_string) else {
        return invalid_class();
    };
    let context = q.context.as_deref().unwrap_or("business_critical");
    let filter = match context {
        "business_critical" => "&context=business_critical",
        "all" => "",
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "invalid_request",
                    "message": "context must be business_critical or all",
                })),
            )
                .into_response();
        }
    };
    let Some(urls) = providers.inner.clone() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "review_unavailable");
    };
    let offset = q.offset.unwrap_or(0);
    let page = match fetch(
        &urls,
        &headers,
        format!(
            "{}/v1/catalog/services?limit={PAGE_LIMIT}&offset={offset}{filter}",
            urls.catalog
        ),
    )
    .await
    {
        Ok(v) => v,
        Err(f) => return catalog_failure(f),
    };
    let total = page.get("total").and_then(Value::as_u64).unwrap_or(0);
    let Some(services) = page
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(service_ref).collect::<Vec<_>>())
    else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
    };
    let headers = Arc::new(headers);
    let mut tasks = tokio::task::JoinSet::new();
    for (i, service) in services.into_iter().enumerate() {
        let (urls, headers, class) = (urls.clone(), headers.clone(), class.clone());
        tasks.spawn(async move { (i, review(&urls, &headers, service, &class).await) });
    }
    let mut items: Vec<(usize, ServiceReview)> = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((i, Ok(r))) => items.push((i, r)),
            Ok((_, Err(f))) => return catalog_failure(f),
            Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
    items.sort_by_key(|(i, _)| *i);
    let items: Vec<ServiceReview> = items.into_iter().map(|(_, r)| r).collect();
    Json(json!({
        "environment_class": class,
        "context": context,
        "items": items,
        "total": total,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objective(approved: bool, status: Option<&str>, evidence: Option<&str>) -> ObjectiveReview {
        ObjectiveReview {
            objective_id: Uuid::new_v4(),
            sli_type: "availability".into(),
            target: 0.99,
            window_days: 30,
            description: String::new(),
            approved,
            status: status.map(Into::into),
            evidence_status: evidence.map(Into::into),
            attainment: None,
            error_budget_consumed: None,
            last_evidence_at: None,
        }
    }

    #[test]
    fn combine_reports_all_four_expectation_states() {
        let c = |s| objective(true, Some(s), Some("current"));
        assert_eq!(combine(vec![c("compliant")]).status, "compliant");
        assert_eq!(
            combine(vec![c("compliant"), c("at_risk")]).status,
            "at_risk"
        );
        assert_eq!(
            combine(vec![c("at_risk"), c("breached")]).status,
            "breached"
        );
        let u = combine(vec![
            c("compliant"),
            objective(true, Some("unknown"), Some("stale")),
        ]);
        assert_eq!((u.status.as_str(), u.reason.as_str()), ("unknown", "stale"));
    }

    #[test]
    fn unapproved_or_absent_objectives_are_never_compliant() {
        let e = combine(vec![objective(false, None, None)]);
        assert_eq!(
            (e.status.as_str(), e.reason.as_str()),
            ("unknown", "no_approved_objective")
        );
        let e = combine(vec![]);
        assert_eq!(e.status, "unknown");
    }
}
