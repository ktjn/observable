//! Approved measurable objectives per catalog service and environment class
//! (`manage-reliability-objectives`, phase 1 C3, OBS-01-04).
//!
//! An objective binds an availability target to a catalog service id and one approved
//! environment class. It is deliberately separate from the catalog's approved outcome
//! statement. Compliance is evaluated on request against the current scoped evidence:
//! spans of the service's bindings whose exact environment the catalog classifies in the
//! objective's class. Every read forwards the caller's credential to the catalog, which
//! authorizes the service record; a hidden service is the same 404 as a missing one.
//!
//! Results: `compliant`, `at_risk` (budget consumption at or above the objective's
//! threshold), `breached` (attainment below target) or `unknown` (no evidence, or none newer
//! than the freshness bound). An unapproved objective yields no result at all.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::middleware::auth::TenantContext;

pub const CLASSES: &[&str] = &["production", "non_production"];

// --- Catalog port -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogError {
    /// Missing or hidden from the caller.
    NotFound,
    Unauthorized,
    Unavailable,
}

/// One binding with the class the catalog's contexts give its environment (substitute state).
#[derive(Clone, Debug)]
pub struct ScopedBinding {
    pub service_name: String,
    pub environment: String,
    pub class: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct CatalogRecord {
    pub lifecycle: String,
    pub bindings: Vec<ScopedBinding>,
}

#[derive(Default)]
pub struct CatalogTable {
    pub records: HashMap<(Uuid, Uuid), CatalogRecord>,
    pub hidden: HashSet<Uuid>,
    pub unavailable: bool,
}

/// `manage-service-catalog` reads made with the caller's own credential:
/// `GET /v1/catalog/services/{id}`, `/bindings` and `GET /v1/catalog/contexts`.
#[derive(Clone)]
pub enum ObjectiveCatalog {
    Http {
        base_url: Arc<String>,
        client: reqwest::Client,
    },
    Static(Arc<Mutex<CatalogTable>>),
    /// No catalog configured: every read is unavailable.
    Disabled,
}

#[derive(Deserialize)]
struct ServiceBody {
    id: Uuid,
    lifecycle: String,
}

#[derive(Deserialize)]
struct BindingBody {
    service_name: String,
    #[serde(default)]
    environment: String,
}

#[derive(Deserialize)]
struct BindingsBody {
    bindings: Vec<BindingBody>,
}

#[derive(Deserialize)]
struct ContextBody {
    context: String,
    #[serde(default)]
    environments: Vec<String>,
}

#[derive(Deserialize)]
struct ContextsBody {
    contexts: Vec<ContextBody>,
}

impl ObjectiveCatalog {
    pub fn substitute() -> Self {
        Self::Static(Arc::new(Mutex::new(CatalogTable::default())))
    }

    /// Substitute only.
    pub fn put(&self, tenant_id: Uuid, id: Uuid, record: CatalogRecord) {
        if let Self::Static(s) = self {
            s.lock()
                .expect("substitute poisoned")
                .records
                .insert((tenant_id, id), record);
        }
    }

    /// Substitute only.
    pub fn hide(&self, id: Uuid) {
        if let Self::Static(s) = self {
            s.lock().expect("substitute poisoned").hidden.insert(id);
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
        if let Some(v) = headers.get("x-tenant-id") {
            req = req.header("x-tenant-id", v.clone());
        }
        req
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        client: &reqwest::Client,
        url: String,
        headers: &HeaderMap,
    ) -> Result<T, CatalogError> {
        let resp = Self::forward(client.get(url), headers)
            .send()
            .await
            .map_err(|_| CatalogError::Unavailable)?;
        match resp.status().as_u16() {
            200 => resp.json().await.map_err(|_| CatalogError::Unavailable),
            404 => Err(CatalogError::NotFound),
            401 => Err(CatalogError::Unauthorized),
            _ => Err(CatalogError::Unavailable),
        }
    }

    /// The service's lifecycle as the caller may read it.
    pub async fn lifecycle(
        &self,
        ctx: &TenantContext,
        headers: &HeaderMap,
        id: Uuid,
    ) -> Result<String, CatalogError> {
        match self {
            Self::Disabled => Err(CatalogError::Unavailable),
            Self::Static(s) => {
                let t = s.lock().expect("substitute poisoned");
                if t.unavailable {
                    return Err(CatalogError::Unavailable);
                }
                match t.records.get(&(ctx.tenant_id, id)) {
                    Some(r) if !t.hidden.contains(&id) => Ok(r.lifecycle.clone()),
                    _ => Err(CatalogError::NotFound),
                }
            }
            Self::Http { base_url, client } => {
                let base = base_url.trim_end_matches('/');
                let body: ServiceBody =
                    Self::get_json(client, format!("{base}/v1/catalog/services/{id}"), headers)
                        .await?;
                if body.id != id {
                    return Err(CatalogError::Unavailable);
                }
                Ok(body.lifecycle)
            }
        }
    }

    /// Identities of the service whose exact, nonempty environment is in `class`.
    pub async fn scope(
        &self,
        ctx: &TenantContext,
        headers: &HeaderMap,
        id: Uuid,
        class: &str,
    ) -> Result<Vec<(String, String)>, CatalogError> {
        let keep = |name: &str, env: &str, c: Option<&str>| {
            !name.trim().is_empty() && !env.trim().is_empty() && c == Some(class)
        };
        let mut out: Vec<(String, String)> = match self {
            Self::Disabled => return Err(CatalogError::Unavailable),
            Self::Static(s) => {
                let t = s.lock().expect("substitute poisoned");
                if t.unavailable {
                    return Err(CatalogError::Unavailable);
                }
                let r = t
                    .records
                    .get(&(ctx.tenant_id, id))
                    .filter(|_| !t.hidden.contains(&id))
                    .ok_or(CatalogError::NotFound)?;
                r.bindings
                    .iter()
                    .filter(|b| keep(&b.service_name, &b.environment, b.class))
                    .map(|b| (b.service_name.clone(), b.environment.clone()))
                    .collect()
            }
            Self::Http { base_url, client } => {
                let base = base_url.trim_end_matches('/');
                let bindings: BindingsBody = Self::get_json(
                    client,
                    format!("{base}/v1/catalog/services/{id}/bindings"),
                    headers,
                )
                .await?;
                let contexts: ContextsBody =
                    Self::get_json(client, format!("{base}/v1/catalog/contexts"), headers).await?;
                let in_class: HashSet<String> = contexts
                    .contexts
                    .into_iter()
                    .filter(|c| c.context == class)
                    .flat_map(|c| c.environments)
                    .collect();
                bindings
                    .bindings
                    .into_iter()
                    .filter(|b| {
                        let c = in_class.contains(&b.environment).then_some(class);
                        keep(&b.service_name, &b.environment, c)
                    })
                    .map(|b| (b.service_name, b.environment))
                    .collect()
            }
        };
        out.sort();
        out.dedup();
        Ok(out)
    }
}

// --- Records ----------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct Objective {
    pub objective_id: Uuid,
    pub service_id: Uuid,
    pub environment_class: String,
    pub sli_type: String,
    pub target: f64,
    pub window_days: i32,
    pub at_risk_budget_fraction: f64,
    pub freshness_seconds: i32,
    pub description: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str = "objective_id, service_id, environment_class, sli_type, target, window_days, \
     at_risk_budget_fraction, freshness_seconds, description, created_by, created_at, updated_at, \
     approved_by, approved_at";

#[derive(Debug, Deserialize)]
pub struct CreateObjective {
    pub service_id: Uuid,
    pub environment_class: String,
    pub target: f64,
    pub window_days: i32,
    pub at_risk_budget_fraction: Option<f64>,
    pub freshness_seconds: Option<i32>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateObjective {
    pub target: Option<f64>,
    pub window_days: Option<i32>,
    pub at_risk_budget_fraction: Option<f64>,
    pub freshness_seconds: Option<i32>,
    pub description: Option<String>,
}

fn validate(
    target: f64,
    window_days: i32,
    at_risk: f64,
    freshness: i32,
    description: &str,
) -> Result<(), String> {
    if !(target > 0.0 && target < 1.0) {
        return Err("target must be between 0 and 1 (exclusive)".into());
    }
    if !(1..=90).contains(&window_days) {
        return Err("window_days must be between 1 and 90".into());
    }
    if !(at_risk > 0.0 && at_risk < 1.0) {
        return Err("at_risk_budget_fraction must be between 0 and 1 (exclusive)".into());
    }
    if !(60..=86_400).contains(&freshness) {
        return Err("freshness_seconds must be between 60 and 86400".into());
    }
    if description.len() > 500 {
        return Err("description must be at most 500 bytes".into());
    }
    Ok(())
}

// --- Responses --------------------------------------------------------------------------------

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn bad_request(message: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid_request", "message": message })),
    )
        .into_response()
}

fn catalog_error(e: CatalogError) -> Response {
    match e {
        CatalogError::NotFound => error(StatusCode::NOT_FOUND, "not_found"),
        CatalogError::Unauthorized => error(StatusCode::UNAUTHORIZED, "unauthorized"),
        CatalogError::Unavailable => error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable"),
    }
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "objective store error");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

fn can_write(ctx: &TenantContext) -> bool {
    matches!(ctx.role.as_str(), "member" | "tenant_admin" | "admin")
}

async fn load(state: &AppState, tenant: Uuid, id: Uuid) -> Result<Option<Objective>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM alerting.service_objectives \
         WHERE tenant_id = $1 AND objective_id = $2"
    )))
    .bind(tenant)
    .bind(id)
    .fetch_optional(&state.db)
    .await
}

/// The objective and its service, both readable by the caller. A hidden service hides its
/// objectives with the same 404.
#[allow(clippy::result_large_err)]
async fn readable(
    state: &AppState,
    catalog: &ObjectiveCatalog,
    ctx: &TenantContext,
    headers: &HeaderMap,
    id: Uuid,
) -> Result<(Objective, String), Response> {
    let objective = load(state, ctx.tenant_id, id)
        .await
        .map_err(store_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "not_found"))?;
    let lifecycle = catalog
        .lifecycle(ctx, headers, objective.service_id)
        .await
        .map_err(catalog_error)?;
    Ok((objective, lifecycle))
}

// --- Handlers ---------------------------------------------------------------------------------

/// POST /v1/objectives (member or admin): defines an unapproved objective.
pub async fn handle_create(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(catalog): Extension<ObjectiveCatalog>,
    headers: HeaderMap,
    Json(body): Json<CreateObjective>,
) -> Response {
    if !can_write(&ctx) {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    if !CLASSES.contains(&body.environment_class.as_str()) {
        return bad_request("environment_class must be production or non_production".into());
    }
    let at_risk = body.at_risk_budget_fraction.unwrap_or(0.75);
    let freshness = body.freshness_seconds.unwrap_or(900);
    let description = body.description.unwrap_or_default();
    if let Err(m) = validate(
        body.target,
        body.window_days,
        at_risk,
        freshness,
        &description,
    ) {
        return bad_request(m);
    }
    match catalog.lifecycle(&ctx, &headers, body.service_id).await {
        Ok(l) if l == "retired" => {
            return error(StatusCode::UNPROCESSABLE_ENTITY, "service_retired");
        }
        Ok(_) => {}
        Err(e) => return catalog_error(e),
    }
    let created: Result<Objective, sqlx::Error> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO alerting.service_objectives (tenant_id, service_id, environment_class, \
         sli_type, target, window_days, at_risk_budget_fraction, freshness_seconds, description, \
         created_by) VALUES ($1, $2, $3, 'availability', $4, $5, $6, $7, $8, $9) RETURNING {COLUMNS}"
    )))
    .bind(ctx.tenant_id)
    .bind(body.service_id)
    .bind(&body.environment_class)
    .bind(body.target)
    .bind(body.window_days)
    .bind(at_risk)
    .bind(freshness)
    .bind(&description)
    .bind(ctx.user_id)
    .fetch_one(&state.db)
    .await;
    match created {
        Ok(o) => (StatusCode::CREATED, Json(o)).into_response(),
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => error(
            StatusCode::CONFLICT,
            "objective_exists_for_service_and_class",
        ),
        Err(e) => store_error(e),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    service_id: Option<Uuid>,
}

/// GET /v1/objectives?service_id= : the objectives of one readable service.
pub async fn handle_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(catalog): Extension<ObjectiveCatalog>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    let Some(service_id) = q.service_id else {
        return bad_request("service_id is required".into());
    };
    if let Err(e) = catalog.lifecycle(&ctx, &headers, service_id).await {
        return catalog_error(e);
    }
    let items: Result<Vec<Objective>, sqlx::Error> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM alerting.service_objectives \
         WHERE tenant_id = $1 AND service_id = $2 ORDER BY environment_class, sli_type"
    )))
    .bind(ctx.tenant_id)
    .bind(service_id)
    .fetch_all(&state.db)
    .await;
    match items {
        Ok(items) => Json(json!({ "items": items })).into_response(),
        Err(e) => store_error(e),
    }
}

/// PATCH /v1/objectives/{id} (member or admin): changes the definition and clears approval.
pub async fn handle_update(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(catalog): Extension<ObjectiveCatalog>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateObjective>,
) -> Response {
    let (current, _) = match readable(&state, &catalog, &ctx, &headers, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if !can_write(&ctx) {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    let target = body.target.unwrap_or(current.target);
    let window = body.window_days.unwrap_or(current.window_days);
    let at_risk = body
        .at_risk_budget_fraction
        .unwrap_or(current.at_risk_budget_fraction);
    let freshness = body.freshness_seconds.unwrap_or(current.freshness_seconds);
    let description = body.description.unwrap_or(current.description);
    if let Err(m) = validate(target, window, at_risk, freshness, &description) {
        return bad_request(m);
    }
    let updated: Result<Objective, sqlx::Error> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE alerting.service_objectives SET target = $3, window_days = $4, \
         at_risk_budget_fraction = $5, freshness_seconds = $6, description = $7, \
         approved_by = NULL, approved_at = NULL, updated_at = now() \
         WHERE tenant_id = $1 AND objective_id = $2 RETURNING {COLUMNS}"
    )))
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(target)
    .bind(window)
    .bind(at_risk)
    .bind(freshness)
    .bind(&description)
    .fetch_one(&state.db)
    .await;
    match updated {
        Ok(o) => Json(o).into_response(),
        Err(e) => store_error(e),
    }
}

/// POST /v1/objectives/{id}/approve (tenant_admin): agrees the expectation as defined now.
pub async fn handle_approve(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(catalog): Extension<ObjectiveCatalog>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let (current, lifecycle) = match readable(&state, &catalog, &ctx, &headers, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if ctx.role != "tenant_admin" {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    // Approval is a person's decision; an API key cannot record one.
    let Some(approver) = ctx.user_id else {
        return error(StatusCode::FORBIDDEN, "approval_requires_a_user");
    };
    if lifecycle == "retired" {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "service_retired");
    }
    // Approve exactly the revision the approver read.
    let approved: Result<Option<Objective>, sqlx::Error> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE alerting.service_objectives SET approved_by = $3, approved_at = now() \
             WHERE tenant_id = $1 AND objective_id = $2 AND updated_at = $4 RETURNING {COLUMNS}"
        )))
        .bind(ctx.tenant_id)
        .bind(id)
        .bind(approver)
        .bind(current.updated_at)
        .fetch_optional(&state.db)
        .await;
    match approved {
        Ok(Some(o)) => Json(o).into_response(),
        Ok(None) => error(StatusCode::CONFLICT, "objective_changed"),
        Err(e) => store_error(e),
    }
}

/// Evidence totals over the objective's window for the scoped identities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowEvidence {
    pub total: u64,
    pub bad: u64,
    /// Latest span start in Unix nanoseconds.
    pub latest_ns: Option<u64>,
}

#[derive(clickhouse::Row, Deserialize)]
struct EvidenceRow {
    total: u64,
    bad: u64,
    latest: u64,
}

pub async fn window_evidence(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    identities: &[(String, String)],
    since_ns: u64,
) -> Result<WindowEvidence, clickhouse::error::Error> {
    let mut out = WindowEvidence::default();
    for (name, env) in identities {
        let row: EvidenceRow = ch
            .query(
                "SELECT count() AS total, countIf(status_code = 'ERROR') AS bad, \
                 max(start_time_unix_nano) AS latest FROM observable.spans \
                 WHERE tenant_id = ? AND service_name = ? AND environment = ? \
                 AND start_time_unix_nano >= ?",
            )
            .bind(tenant_id)
            .bind(name)
            .bind(env)
            .bind(since_ns)
            .fetch_one()
            .await?;
        out.total += row.total;
        out.bad += row.bad;
        if row.total > 0 {
            out.latest_ns = out.latest_ns.max(Some(row.latest));
        }
    }
    Ok(out)
}

/// Compliance of an approved objective for the evidence at `now`.
#[derive(Clone, Debug, PartialEq)]
pub struct Compliance {
    pub status: &'static str,
    pub evidence_status: &'static str,
    pub attainment: Option<f64>,
    pub budget_consumed: Option<f64>,
}

pub fn classify(
    target: f64,
    at_risk_budget_fraction: f64,
    freshness_seconds: i32,
    evidence: WindowEvidence,
    now: DateTime<Utc>,
) -> Compliance {
    let unknown = |evidence_status| Compliance {
        status: "unknown",
        evidence_status,
        attainment: None,
        budget_consumed: None,
    };
    let Some(latest) = evidence.latest_ns.filter(|_| evidence.total > 0) else {
        return unknown("missing");
    };
    let age_ns = (now.timestamp_nanos_opt().unwrap_or(i64::MAX) as i128) - latest as i128;
    if age_ns > i128::from(freshness_seconds) * 1_000_000_000 {
        return unknown("stale");
    }
    let error_rate = evidence.bad as f64 / evidence.total as f64;
    let attainment = 1.0 - error_rate;
    let consumed = error_rate / (1.0 - target);
    // Exact boundaries are judged with a tolerance for binary floating point.
    const EPS: f64 = 1e-9;
    let status = if attainment + EPS < target {
        "breached"
    } else if consumed + EPS >= at_risk_budget_fraction {
        "at_risk"
    } else {
        "compliant"
    };
    Compliance {
        status,
        evidence_status: "current",
        attainment: Some(attainment),
        budget_consumed: Some(consumed),
    }
}

/// GET /v1/objectives/{id}/compliance: evaluated now against current scoped evidence.
pub async fn handle_compliance(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(catalog): Extension<ObjectiveCatalog>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let (objective, lifecycle) = match readable(&state, &catalog, &ctx, &headers, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    // Only running services have a current compliance result.
    if !matches!(lifecycle.as_str(), "live" | "deprecated") {
        return error(StatusCode::NOT_FOUND, "not_found");
    }
    if objective.approved_at.is_none() {
        return error(StatusCode::CONFLICT, "objective_not_approved");
    }
    let identities = match catalog
        .scope(
            &ctx,
            &headers,
            objective.service_id,
            &objective.environment_class,
        )
        .await
    {
        Ok(i) => i,
        Err(e) => return catalog_error(e),
    };
    let now = Utc::now();
    let window_start = now - chrono::Duration::days(i64::from(objective.window_days));
    let since_ns = window_start.timestamp_nanos_opt().unwrap_or(0).max(0) as u64;
    let evidence = match window_evidence(&state.ch, ctx.tenant_id, &identities, since_ns).await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "objective evidence read failed");
            return error(StatusCode::SERVICE_UNAVAILABLE, "evidence_unavailable");
        }
    };
    let c = classify(
        objective.target,
        objective.at_risk_budget_fraction,
        objective.freshness_seconds,
        evidence,
        now,
    );
    let current = c.evidence_status == "current";
    Json(json!({
        "objective_id": objective.objective_id,
        "service_id": objective.service_id,
        "environment_class": objective.environment_class,
        "target": objective.target,
        "status": c.status,
        "evidence_status": c.evidence_status,
        "attainment": c.attainment,
        "error_budget_consumed": c.budget_consumed,
        "total_events": current.then_some(evidence.total),
        "good_events": current.then_some(evidence.total - evidence.bad),
        "last_evidence_at": evidence
            .latest_ns
            .and_then(|ns| i64::try_from(ns).ok())
            .map(DateTime::from_timestamp_nanos),
        "window_start": window_start,
        "evaluated_at": now,
        "approved_at": objective.approved_at,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(total: u64, bad: u64, age_s: i64, now: DateTime<Utc>) -> WindowEvidence {
        WindowEvidence {
            total,
            bad,
            latest_ns: Some(
                (now - chrono::Duration::seconds(age_s))
                    .timestamp_nanos_opt()
                    .unwrap() as u64,
            ),
        }
    }

    #[test]
    fn classifies_compliant_at_risk_breached_and_unknown() {
        let now = Utc::now();
        // Target 99%: budget 1%. 0.5% errors consume half; 0.8% consume 80%; 2% breach.
        let c = |total, bad, age| classify(0.99, 0.75, 900, ev(total, bad, age, now), now).status;
        assert_eq!(c(1000, 5, 10), "compliant");
        assert_eq!(c(1000, 8, 10), "at_risk");
        assert_eq!(c(1000, 20, 10), "breached");
        assert_eq!(c(1000, 0, 901), "unknown");
        let missing = classify(0.99, 0.75, 900, WindowEvidence::default(), now);
        assert_eq!(
            (missing.status, missing.evidence_status),
            ("unknown", "missing")
        );
        assert_eq!(missing.attainment, None);
        let stale = classify(0.99, 0.75, 900, ev(1000, 0, 3600, now), now);
        assert_eq!((stale.status, stale.evidence_status), ("unknown", "stale"));
        assert_eq!(stale.attainment, None);
    }

    #[test]
    fn budget_boundaries() {
        let now = Utc::now();
        let c = |bad| classify(0.99, 0.75, 900, ev(10_000, bad, 1, now), now).status;
        assert_eq!(c(74), "compliant");
        assert_eq!(c(75), "at_risk");
        assert_eq!(c(100), "at_risk");
        assert_eq!(c(101), "breached");
    }
}
