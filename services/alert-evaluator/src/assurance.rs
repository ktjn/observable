//! W5a (docs/assurance-relocations.md): confirmed incident causes and similar incidents
//! (OBS-09-07), and the delivery (OBS-11-05) and benefits (OBS-17-06) reviews, which keep
//! measures from different sources apart and never combine them into one figure.

use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::guidance::GuidanceSources;
use crate::middleware::auth::TenantContext;
use crate::objectives::ObjectiveCatalog;

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "assurance store error");
    error(StatusCode::SERVICE_UNAVAILABLE, "store_unavailable")
}

// --- Confirmed causes and similar incidents (OBS-09-07) ---------------------------------------

#[derive(Debug, Deserialize)]
pub struct CauseBody {
    pub confirmed_cause: String,
    pub cause_category: Option<String>,
}

/// `PUT /v1/incidents/{id}/cause`: the incident owner or a tenant admin records the confirmed
/// cause of a resolved incident.
pub async fn handle_put_cause(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<CauseBody>,
) -> Response {
    let Some(user) = ctx.user_id else {
        return error(StatusCode::FORBIDDEN, "forbidden");
    };
    let row: Option<(String, Option<String>)> = match sqlx::query_as(
        "SELECT status, owner FROM alerting.incidents WHERE tenant_id = $1 AND incident_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return store_error(e),
    };
    let Some((status, owner)) = row else {
        return error(StatusCode::NOT_FOUND, "not_found");
    };
    let is_owner = owner
        .as_deref()
        .is_some_and(|o| o == user.to_string() || o == format!("user:{user}"));
    if ctx.role != "tenant_admin" && !is_owner {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    let cause = body.confirmed_cause.trim();
    let category = body
        .cause_category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    if cause.is_empty()
        || cause.chars().count() > 2000
        || category.is_some_and(|c| c.chars().count() > 100)
    {
        return error(StatusCode::BAD_REQUEST, "invalid_cause");
    }
    if !matches!(status.as_str(), "resolved" | "post_mortem") {
        return error(StatusCode::CONFLICT, "incident_not_resolved");
    }
    let res = sqlx::query(
        "UPDATE alerting.incidents SET confirmed_cause = $3, cause_category = $4, \
         cause_confirmed_by = $5, cause_confirmed_at = now(), updated_at = now() \
         WHERE tenant_id = $1 AND incident_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(cause)
    .bind(category)
    .bind(user)
    .execute(&state.db)
    .await;
    match res {
        Ok(_) => Json(json!({
            "incident_id": id,
            "confirmed_cause": cause,
            "cause_category": category,
            "cause_confirmed_by": user,
        }))
        .into_response(),
        Err(e) => store_error(e),
    }
}

#[derive(sqlx::FromRow)]
struct IncidentSignature {
    incident_id: Uuid,
    title: String,
    severity: String,
    status: String,
    triggered_by_rule_id: Option<Uuid>,
    triggered_at: DateTime<Utc>,
    affected_services: Vec<String>,
    confirmed_cause: Option<String>,
    cause_category: Option<String>,
}

const SIGNATURE: &str = "incident_id, title, severity, status, triggered_by_rule_id, triggered_at, \
     affected_services, confirmed_cause, cause_category";

#[derive(Debug, Deserialize)]
pub struct SimilarQuery {
    pub limit: Option<i64>,
}

/// `GET /v1/incidents/{id}/similar?limit=`: prior incidents in the caller's tenant that share
/// a symptom signature (the same triggering rule, or an affected service at the same
/// severity). Confirmed causes rank first.
pub async fn handle_similar(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(id): Path<Uuid>,
    Query(q): Query<SimilarQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let target: Option<IncidentSignature> = match sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SIGNATURE} FROM alerting.incidents WHERE tenant_id = $1 AND incident_id = $2"
    )))
    .bind(ctx.tenant_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(t) => t,
        Err(e) => return store_error(e),
    };
    let Some(target) = target else {
        return error(StatusCode::NOT_FOUND, "not_found");
    };
    let candidates: Vec<IncidentSignature> = match sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SIGNATURE} FROM alerting.incidents \
         WHERE tenant_id = $1 AND incident_id <> $2 AND triggered_at < $3 \
           AND ((triggered_by_rule_id IS NOT NULL AND triggered_by_rule_id = $4) \
             OR (severity = $5 AND affected_services && $6)) \
         ORDER BY triggered_at DESC LIMIT 500"
    )))
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(target.triggered_at)
    .bind(target.triggered_by_rule_id)
    .bind(&target.severity)
    .bind(&target.affected_services)
    .fetch_all(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => return store_error(e),
    };
    let mine: HashSet<&str> = target
        .affected_services
        .iter()
        .map(String::as_str)
        .collect();
    let mut scored: Vec<(bool, usize, DateTime<Utc>, Value)> = candidates
        .iter()
        .map(|c| {
            let mut signals = Vec::new();
            if c.triggered_by_rule_id.is_some()
                && c.triggered_by_rule_id == target.triggered_by_rule_id
            {
                signals.push(json!({"signal": "same_rule", "rule_id": c.triggered_by_rule_id}));
            }
            let shared: Vec<&str> = c
                .affected_services
                .iter()
                .map(String::as_str)
                .filter(|s| mine.contains(s))
                .collect();
            if c.severity == target.severity && !shared.is_empty() {
                signals.push(json!({"signal": "shared_services", "services": shared,
                                    "severity": c.severity}));
            }
            let confirmed = c.confirmed_cause.is_some();
            (
                confirmed,
                signals.len(),
                c.triggered_at,
                json!({
                    "incident_id": c.incident_id,
                    "title": c.title,
                    "severity": c.severity,
                    "status": c.status,
                    "triggered_at": c.triggered_at,
                    "matched": signals,
                    "confirmed_cause": c.confirmed_cause,
                    "cause_category": c.cause_category,
                }),
            )
        })
        .filter(|(_, n, _, _)| *n > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    let items: Vec<Value> = scored
        .into_iter()
        .take(limit as usize)
        .map(|(_, _, _, v)| v)
        .collect();
    Json(json!({"incident_id": id, "items": items})).into_response()
}

// --- Approved review measures -----------------------------------------------------------------

pub const DELIVERY_MEASURES: &[&str] = &[
    "deployment_count",
    "change_failure_rate",
    "median_restore_minutes",
    "attainment",
    "error_budget_remaining",
];
pub const BENEFITS_MEASURES: &[&str] = &[
    "incident_count",
    "critical_incident_count",
    "mean_time_to_resolve_minutes",
];

#[derive(Debug, Deserialize)]
pub struct MeasuresBody {
    pub review: String,
    pub measures: Vec<String>,
}

/// `PUT /v1/objectives/review-measures`: a signed-in tenant admin approves which measures each
/// review reports.
pub async fn handle_put_measures(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<MeasuresBody>,
) -> Response {
    let Some(admin) = ctx.user_id.filter(|_| ctx.role == "tenant_admin") else {
        return error(StatusCode::FORBIDDEN, "forbidden");
    };
    let known = match body.review.as_str() {
        "delivery" => DELIVERY_MEASURES,
        "benefits" => BENEFITS_MEASURES,
        _ => return error(StatusCode::BAD_REQUEST, "unknown_review"),
    };
    let mut measures = body.measures;
    measures.sort();
    measures.dedup();
    if measures.is_empty() || measures.iter().any(|m| !known.contains(&m.as_str())) {
        return error(StatusCode::BAD_REQUEST, "unknown_measure");
    }
    let res = sqlx::query(
        "INSERT INTO alerting.review_measures (tenant_id, review, measures, approved_by) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (tenant_id, review) DO UPDATE SET \
         measures = EXCLUDED.measures, approved_by = EXCLUDED.approved_by, updated_at = now()",
    )
    .bind(ctx.tenant_id)
    .bind(&body.review)
    .bind(&measures)
    .bind(admin)
    .execute(&state.db)
    .await;
    match res {
        Ok(_) => Json(json!({"review": body.review, "measures": measures, "approved_by": admin}))
            .into_response(),
        Err(e) => store_error(e),
    }
}

async fn approved(
    state: &AppState,
    tenant: Uuid,
    review: &str,
) -> sqlx::Result<Option<Vec<String>>> {
    sqlx::query_scalar(
        "SELECT measures FROM alerting.review_measures WHERE tenant_id = $1 AND review = $2",
    )
    .bind(tenant)
    .bind(review)
    .fetch_optional(&state.db)
    .await
}

fn measure(name: &str, value: Value, source: &str, basis: &str) -> Value {
    json!({"measure": name, "value": value, "source": source, "basis": basis})
}

// --- Delivery review (OBS-11-05) --------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

/// `GET /v1/objectives/{id}/delivery-review?from=&to=`: delivery measures from control's
/// deployments of the objective's service and reliability measures from the objective, in
/// separate sections. Only approved measures are reported.
pub async fn handle_delivery_review(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(catalog): Extension<ObjectiveCatalog>,
    Extension(sources): Extension<GuidanceSources>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(q): Query<PeriodQuery>,
) -> Response {
    let now = Utc::now();
    if q.from >= q.to || q.from > now || q.to - q.from > chrono::Duration::days(93) {
        return error(StatusCode::BAD_REQUEST, "invalid_period");
    }
    let measures = match approved(&state, ctx.tenant_id, "delivery").await {
        Ok(Some(m)) => m,
        Ok(None) => return error(StatusCode::CONFLICT, "review_measures_not_approved"),
        Err(e) => return store_error(e),
    };
    let reliability = match crate::objectives::period_attainment(
        &state,
        &catalog,
        &ctx,
        &headers,
        id,
        q.from,
        q.to.min(now),
    )
    .await
    {
        Ok(r) => r,
        Err(r) => return r,
    };
    let names: HashSet<(String, String)> = reliability.identities.iter().cloned().collect();
    let deployments = match sources.deployments(ctx.tenant_id, q.from).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "delivery review: control unavailable");
            return error(StatusCode::SERVICE_UNAVAILABLE, "control_unavailable");
        }
    };
    let mut ours: Vec<_> = deployments
        .into_iter()
        .filter(|d| {
            d.started_at < q.to && names.contains(&(d.service_name.clone(), d.environment.clone()))
        })
        .collect();
    ours.sort_by_key(|d| d.started_at);
    let finished: Vec<_> = ours.iter().filter(|d| d.status != "in_progress").collect();
    let failed = finished
        .iter()
        .filter(|d| matches!(d.status.as_str(), "failed" | "rolled_back"))
        .count();
    let mut restores: Vec<i64> = Vec::new();
    for (i, d) in ours.iter().enumerate() {
        if !matches!(d.status.as_str(), "failed" | "rolled_back") {
            continue;
        }
        if let Some(next) = ours[i + 1..].iter().find(|n| {
            n.status == "success"
                && n.service_name == d.service_name
                && n.environment == d.environment
        }) {
            restores.push((next.started_at - d.started_at).num_minutes());
        }
    }
    restores.sort_unstable();
    let median = (!restores.is_empty()).then(|| restores[restores.len() / 2]);
    let all_delivery: Vec<(&str, Value)> = vec![
        ("deployment_count", json!(ours.len())),
        (
            "change_failure_rate",
            if finished.is_empty() {
                Value::Null
            } else {
                json!(failed as f64 / finished.len() as f64)
            },
        ),
        ("median_restore_minutes", json!(median)),
    ];
    let budget = reliability
        .attainment
        .map(|a| 1.0 - (1.0 - a) / (1.0 - reliability.target).max(1e-9));
    let all_reliability: Vec<(&str, Value)> = vec![
        ("attainment", json!(reliability.attainment)),
        ("error_budget_remaining", json!(budget)),
    ];
    let pick = |all: Vec<(&str, Value)>, source: &str, basis: &str| -> Vec<Value> {
        all.into_iter()
            .filter(|(n, _)| measures.iter().any(|m| m == n))
            .map(|(n, v)| {
                let b = if v.is_null() {
                    "insufficient_data"
                } else {
                    basis
                };
                measure(n, v, source, b)
            })
            .collect()
    };
    Json(json!({
        "objective_id": id,
        "period": {"from": q.from, "to": q.to},
        "delivery": pick(all_delivery, "control", "deployment_markers"),
        "reliability": pick(all_reliability, "alerting", reliability.basis),
        "target": reliability.target,
        "note": "delivery and reliability measures are reported separately and never combined",
    }))
    .into_response()
}

// --- Benefits review (OBS-17-06) --------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BenefitsQuery {
    pub baseline: String,
    pub period: String,
}

fn month(p: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = NaiveDate::parse_from_str(&format!("{p}-01"), "%Y-%m-%d").ok()?;
    let next = if start.month() == 12 {
        NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)?
    };
    Some((
        Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0)?),
        Utc.from_utc_datetime(&next.and_hms_opt(0, 0, 0)?),
    ))
}

async fn outcomes(
    state: &AppState,
    tenant: Uuid,
    (from, to): (DateTime<Utc>, DateTime<Utc>),
) -> sqlx::Result<HashMap<&'static str, Value>> {
    let (count, critical, mttr): (i64, i64, Option<f64>) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE severity = 'critical'), \
         (avg(EXTRACT(EPOCH FROM (resolved_at - triggered_at))) \
            FILTER (WHERE resolved_at IS NOT NULL) / 60.0)::float8 \
         FROM alerting.incidents WHERE tenant_id = $1 AND triggered_at >= $2 AND triggered_at < $3",
    )
    .bind(tenant)
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await?;
    Ok(HashMap::from([
        ("incident_count", json!(count)),
        ("critical_incident_count", json!(critical)),
        ("mean_time_to_resolve_minutes", json!(mttr)),
    ]))
}

/// `GET /v1/objectives/benefits-review?baseline=YYYY-MM&period=YYYY-MM`: the observability
/// cost change from control next to the approved outcome indicators from alerting, each with
/// its source. No ratio of the two is computed.
pub async fn handle_benefits_review(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(sources): Extension<GuidanceSources>,
    Query(q): Query<BenefitsQuery>,
) -> Response {
    let (Some(base), Some(cur)) = (month(&q.baseline), month(&q.period)) else {
        return error(StatusCode::BAD_REQUEST, "invalid_period");
    };
    if base.0 >= cur.0 {
        return error(StatusCode::BAD_REQUEST, "baseline_must_precede_period");
    }
    let measures = match approved(&state, ctx.tenant_id, "benefits").await {
        Ok(Some(m)) => m,
        Ok(None) => return error(StatusCode::CONFLICT, "review_measures_not_approved"),
        Err(e) => return store_error(e),
    };
    let (base_cost, cur_cost) = match (
        sources.cost_total(ctx.tenant_id, &q.baseline).await,
        sources.cost_total(ctx.tenant_id, &q.period).await,
    ) {
        (Ok(b), Ok(c)) => (b, c),
        (Err(e), _) | (_, Err(e)) => {
            tracing::warn!(error = %e, "benefits review: control unavailable");
            return error(StatusCode::SERVICE_UNAVAILABLE, "control_unavailable");
        }
    };
    let (b, c) = match (
        outcomes(&state, ctx.tenant_id, base).await,
        outcomes(&state, ctx.tenant_id, cur).await,
    ) {
        (Ok(b), Ok(c)) => (b, c),
        (Err(e), _) | (_, Err(e)) => return store_error(e),
    };
    let indicators: Vec<Value> = BENEFITS_MEASURES
        .iter()
        .filter(|m| measures.iter().any(|x| x == *m))
        .map(|m| {
            let (bv, cv) = (b[m].as_f64(), c[m].as_f64());
            json!({
                "indicator": m,
                "baseline": b[m],
                "period": c[m],
                "change": bv.zip(cv).map(|(x, y)| y - x),
                "source": "alerting",
            })
        })
        .collect();
    let cost = |v: Option<i64>| json!(v);
    Json(json!({
        "baseline": q.baseline,
        "period": q.period,
        "cost": {
            "baseline_cents": cost(base_cost),
            "period_cents": cost(cur_cost),
            "change_cents": base_cost.zip(cur_cost).map(|(x, y)| y - x),
            "source": "control",
            "basis": if base_cost.is_some() && cur_cost.is_some() { "recorded" } else { "insufficient_data" },
        },
        "outcome_indicators": indicators,
        "note": "cost and outcome indicators are compared side by side; no combined value is computed",
    }))
    .into_response()
}
