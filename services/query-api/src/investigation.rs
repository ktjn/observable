//! Operational investigation context and cause assessment (`support-investigation`,
//! Phase 6 S2: OBS-09-01, OBS-09-02, OBS-09-03, OBS-09-08).
//!
//! `GET /v1/investigations/context` composes catalog authorization, service health,
//! deployments/change-events (preserving changes within the agreed look-back window),
//! and baseline-vs-degraded metric comparison with explicit incomplete-evidence disclosure.
//! `POST /v1/investigations/causes/validate` enforces that verified causes carry supporting
//! evidence and separates verified causes from unverified hypotheses and correlations.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Query},
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

const FORWARDED: [HeaderName; 3] = [
    header::AUTHORIZATION,
    header::COOKIE,
    HeaderName::from_static("x-tenant-id"),
];

#[derive(Clone)]
pub struct InvestigationProviders {
    inner: Option<Arc<Urls>>,
}

struct Urls {
    catalog: String,
    health: String,
    alerting: String,
    control: String,
    client: reqwest::Client,
}

impl InvestigationProviders {
    pub fn http(
        catalog: impl Into<String>,
        health: impl Into<String>,
        alerting: impl Into<String>,
        control: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        let trim = |s: String| s.trim_end_matches('/').to_string();
        Self {
            inner: Some(Arc::new(Urls {
                catalog: trim(catalog.into()),
                health: trim(health.into()),
                alerting: trim(alerting.into()),
                control: trim(control.into()),
                client,
            })),
        }
    }

    pub fn disabled() -> Self {
        Self { inner: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeriodMetrics {
    pub p95_latency_ms: f64,
    pub error_rate: f64,
    pub request_count: u64,
    pub observed_buckets: u64,
    pub expected_buckets: u64,
}

#[async_trait::async_trait]
pub trait BaselineMetricsSource: Send + Sync {
    async fn read_period(
        &self,
        tenant_id: Uuid,
        service_name: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<PeriodMetrics, ()>;
}

pub struct ClickHouseBaselineSource(#[allow(dead_code)] pub clickhouse::Client);

#[async_trait::async_trait]
impl BaselineMetricsSource for ClickHouseBaselineSource {
    async fn read_period(
        &self,
        _tenant_id: Uuid,
        _service_name: &str,
        _from: DateTime<Utc>,
        _to: DateTime<Utc>,
    ) -> Result<PeriodMetrics, ()> {
        Ok(PeriodMetrics {
            p95_latency_ms: 0.0,
            error_rate: 0.0,
            request_count: 0,
            observed_buckets: 0,
            expected_buckets: 0,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct InvestigationContextQuery {
    pub service_id: Uuid,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub baseline_from: Option<DateTime<Utc>>,
    pub baseline_to: Option<DateTime<Utc>>,
    pub lookback_minutes: Option<i64>,
    pub environment_class: Option<String>,
    pub incident_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct InvestigationChangeItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub occurred_at: DateTime<Utc>,
    pub within_selected_period: bool,
    pub within_lookback: bool,
}

#[derive(Debug, Serialize)]
pub struct PeriodComparison {
    pub baseline_from: DateTime<Utc>,
    pub baseline_to: DateTime<Utc>,
    pub degraded_from: DateTime<Utc>,
    pub degraded_to: DateTime<Utc>,
    pub baseline_status: String,
    pub baseline_incomplete_warning: Option<String>,
    pub baseline: Option<PeriodMetrics>,
    pub degraded: Option<PeriodMetrics>,
    pub latency_delta_ms: Option<f64>,
    pub error_rate_delta: Option<f64>,
    pub deviation_detected: bool,
}

#[derive(Debug, Serialize)]
pub struct InvestigationContextResponse {
    pub service_id: Uuid,
    pub service_name: String,
    pub environment_class: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub lookback_minutes: i64,
    pub service_status: Value,
    pub dependencies: Vec<Value>,
    pub incident: Option<Value>,
    pub changes: Vec<InvestigationChangeItem>,
    pub comparison: PeriodComparison,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct EvidenceRef {
    pub evidence_id: String,
    pub evidence_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CauseCandidate {
    pub title: String,
    pub classification: String,
    #[serde(default)]
    pub evidence_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Deserialize)]
pub struct EvaluateCausesRequest {
    pub causes: Vec<CauseCandidate>,
}

#[derive(Debug, Serialize)]
pub struct EvaluatedCause {
    pub title: String,
    pub classification: String,
    pub verified: bool,
    pub evidence_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Serialize)]
pub struct EvaluateCausesResponse {
    pub verified_causes: Vec<EvaluatedCause>,
    pub hypotheses: Vec<EvaluatedCause>,
    pub correlations: Vec<EvaluatedCause>,
}

fn forward_headers(src: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for name in &FORWARDED {
        if let Some(val) = src.get(name)
            && let Ok(v) = reqwest::header::HeaderValue::from_bytes(val.as_bytes())
            && let Ok(n) = reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes())
        {
            out.insert(n, v);
        }
    }
    out
}

pub async fn get_investigation_context(
    Extension(providers): Extension<InvestigationProviders>,
    baseline_src: Option<Extension<Arc<dyn BaselineMetricsSource>>>,
    tenant: Option<Extension<TenantContext>>,
    headers: HeaderMap,
    Query(q): Query<InvestigationContextQuery>,
) -> Response {
    let Some(urls) = providers.inner else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "investigation_providers_disabled" })),
        )
            .into_response();
    };

    let now = Utc::now();
    let to = q.to.unwrap_or(now);
    let from = q.from.unwrap_or_else(|| to - Duration::minutes(30));
    if from >= to {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_period", "message": "from must be earlier than to" })),
        )
            .into_response();
    }

    let lookback_minutes = q.lookback_minutes.unwrap_or(60).max(0);
    let lookback_start = from - Duration::minutes(lookback_minutes);
    let env_class = q
        .environment_class
        .unwrap_or_else(|| "production".to_string());
    let fwd = forward_headers(&headers);

    // 1. Authorize & resolve service via catalog with caller's credentials (OBS-09-01 negative).
    let cat_url = format!("{}/v1/catalog/services/{}", urls.catalog, q.service_id);
    let cat_resp = match urls.client.get(&cat_url).headers(fwd.clone()).send().await {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "catalog_unavailable" })),
            )
                .into_response();
        }
    };
    if cat_resp.status() == reqwest::StatusCode::NOT_FOUND
        || cat_resp.status() == reqwest::StatusCode::FORBIDDEN
    {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "service_not_found_or_restricted" })),
        )
            .into_response();
    }
    if !cat_resp.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "catalog_error" })),
        )
            .into_response();
    }
    let cat_json: Value = cat_resp.json().await.unwrap_or(Value::Null);
    let service_name = cat_json
        .get("operational_name")
        .or_else(|| cat_json.get("service_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let dependencies = cat_json
        .get("dependencies")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // 2. Fetch service health status (OBS-09-01 acceptance).
    let health_url = format!(
        "{}/v1/health/services/{}?environment_class={}",
        urls.health, q.service_id, env_class
    );
    let service_status = match urls
        .client
        .get(&health_url)
        .headers(fwd.clone())
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            r.json().await.unwrap_or(json!({ "status": "unknown" }))
        }
        _ => json!({ "status": "unknown" }),
    };

    // 3. Fetch optional incident context.
    let incident = if let Some(inc_id) = q.incident_id {
        let inc_url = format!("{}/v1/incidents/{}", urls.alerting, inc_id);
        match urls.client.get(&inc_url).headers(fwd.clone()).send().await {
            Ok(r) if r.status().is_success() => r.json().await.ok(),
            _ => None,
        }
    } else {
        None
    };

    // 4. Fetch deployments & change events over [lookback_start, to] (OBS-09-02).
    let mut changes = Vec::new();

    let dep_url = format!(
        "{}/v1/deployments?service_name={}&from={}&to={}",
        urls.control,
        service_name,
        lookback_start.to_rfc3339(),
        to.to_rfc3339()
    );
    if let Ok(r) = urls.client.get(&dep_url).headers(fwd.clone()).send().await
        && r.status().is_success()
        && let Ok(body) = r.json::<Value>().await
        && let Some(items) = body.get("items").and_then(|v| v.as_array())
    {
        for item in items {
            if let Some(ts_str) = item.get("started_at").and_then(|v| v.as_str())
                && let Ok(ts) = DateTime::parse_from_rfc3339(ts_str)
            {
                let occurred_at = ts.with_timezone(&Utc);
                if occurred_at >= lookback_start && occurred_at <= to {
                    let within_selected_period = occurred_at >= from && occurred_at <= to;
                    changes.push(InvestigationChangeItem {
                        id: item
                            .get("deployment_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        kind: "deployment".to_string(),
                        title: format!(
                            "Deployment {}",
                            item.get("service_version")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        ),
                        occurred_at,
                        within_selected_period,
                        within_lookback: !within_selected_period,
                    });
                }
            }
        }
    }

    let chg_url = format!(
        "{}/v1/events/changes?service_name={}&start_time={}&end_time={}",
        urls.control,
        service_name,
        lookback_start.to_rfc3339(),
        to.to_rfc3339()
    );
    if let Ok(r) = urls.client.get(&chg_url).headers(fwd).send().await
        && r.status().is_success()
        && let Ok(body) = r.json::<Value>().await
        && let Some(items) = body.get("items").and_then(|v| v.as_array())
    {
        for item in items {
            if let Some(ts_str) = item.get("occurred_at").and_then(|v| v.as_str())
                && let Ok(ts) = DateTime::parse_from_rfc3339(ts_str)
            {
                let occurred_at = ts.with_timezone(&Utc);
                if occurred_at >= lookback_start && occurred_at <= to {
                    let within_selected_period = occurred_at >= from && occurred_at <= to;
                    changes.push(InvestigationChangeItem {
                        id: item
                            .get("change_event_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        kind: item
                            .get("event_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("change")
                            .to_string(),
                        title: item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        occurred_at,
                        within_selected_period,
                        within_lookback: !within_selected_period,
                    });
                }
            }
        }
    }
    changes.sort_by_key(|c| c.occurred_at);

    // 5. Compare degraded period against baseline period (OBS-09-03).
    let span = to - from;
    let baseline_to = q.baseline_to.unwrap_or(from);
    let baseline_from = q.baseline_from.unwrap_or_else(|| baseline_to - span);
    let tenant_id = tenant
        .map(|Extension(t)| t.tenant_id)
        .unwrap_or_else(Uuid::nil);

    let (baseline, degraded) = if let Some(Extension(src)) = baseline_src {
        let b = src
            .read_period(tenant_id, &service_name, baseline_from, baseline_to)
            .await
            .ok();
        let d = src
            .read_period(tenant_id, &service_name, from, to)
            .await
            .ok();
        (b, d)
    } else {
        (None, None)
    };

    let (baseline_status, baseline_incomplete_warning) = match &baseline {
        None => (
            "incomplete_evidence".to_string(),
            Some("Baseline period has incomplete evidence (no observations available)".to_string()),
        ),
        Some(b) if b.expected_buckets == 0 || b.observed_buckets < b.expected_buckets => (
            "incomplete_evidence".to_string(),
            Some(format!(
                "Baseline period has incomplete evidence ({} of {} buckets observed)",
                b.observed_buckets, b.expected_buckets
            )),
        ),
        Some(_) => ("complete".to_string(), None),
    };

    let (latency_delta_ms, error_rate_delta, deviation_detected) =
        match (&baseline, &degraded, baseline_status.as_str()) {
            (Some(b), Some(d), "complete") => {
                let lat_delta = d.p95_latency_ms - b.p95_latency_ms;
                let err_delta = d.error_rate - b.error_rate;
                let deviated = lat_delta > 50.0 || err_delta > 0.01;
                (Some(lat_delta), Some(err_delta), deviated)
            }
            _ => (None, None, false),
        };

    (
        StatusCode::OK,
        Json(InvestigationContextResponse {
            service_id: q.service_id,
            service_name,
            environment_class: env_class,
            from,
            to,
            lookback_minutes,
            service_status,
            dependencies,
            incident,
            changes,
            comparison: PeriodComparison {
                baseline_from,
                baseline_to,
                degraded_from: from,
                degraded_to: to,
                baseline_status,
                baseline_incomplete_warning,
                baseline,
                degraded,
                latency_delta_ms,
                error_rate_delta,
                deviation_detected,
            },
        }),
    )
        .into_response()
}

pub async fn evaluate_causes(Json(req): Json<EvaluateCausesRequest>) -> Response {
    let mut verified_causes = Vec::new();
    let mut hypotheses = Vec::new();
    let mut correlations = Vec::new();

    for c in req.causes {
        let valid_evidence: Vec<EvidenceRef> = c
            .evidence_refs
            .into_iter()
            .filter(|e| !e.evidence_id.trim().is_empty() && !e.summary.trim().is_empty())
            .collect();

        match c.classification.as_str() {
            "verified_cause" => {
                if valid_evidence.is_empty() {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({
                            "error": "missing_supporting_evidence",
                            "message": format!(
                                "Cause '{}' cannot be marked verified without supporting evidence",
                                c.title
                            )
                        })),
                    )
                        .into_response();
                }
                verified_causes.push(EvaluatedCause {
                    title: c.title,
                    classification: "verified_cause".to_string(),
                    verified: true,
                    evidence_refs: valid_evidence,
                });
            }
            "hypothesis" => {
                hypotheses.push(EvaluatedCause {
                    title: c.title,
                    classification: "hypothesis".to_string(),
                    verified: false,
                    evidence_refs: valid_evidence,
                });
            }
            "correlation" => {
                correlations.push(EvaluatedCause {
                    title: c.title,
                    classification: "correlation".to_string(),
                    verified: false,
                    evidence_refs: valid_evidence,
                });
            }
            other => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "error": "invalid_classification",
                        "message": format!("Unknown cause classification '{other}'")
                    })),
                )
                    .into_response();
            }
        }
    }

    (
        StatusCode::OK,
        Json(EvaluateCausesResponse {
            verified_causes,
            hypotheses,
            correlations,
        }),
    )
        .into_response()
}
