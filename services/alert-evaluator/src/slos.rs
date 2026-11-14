use crate::AppState;
use crate::alerts::{OwnerRef, UpdateOwnerRequest, resolve_owner};
use crate::middleware::auth::{TenantContext, require_admin};
use axum::{Extension, Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical SLO definition entity. Mirrors `slos.SloDefinition@1` in
/// `models/slos.mdl` field-for-field.
/// Also serves as the Postgres `sqlx::FromRow` projection joining
/// `slo_definitions` with `alert_rules`/`alert_firings` for `firing`/
/// `last_fired_at` — timestamp fields stay `chrono::DateTime<Utc>`
/// (Phase 1 backlog item 5: modelable's `timestamp` emits as Rust `String`).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SloDefinitionItem {
    pub slo_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub sli_type: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: String,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Accountable owner (OBS-34-01): `user` or `team`, or none when ownerless.
    pub owner_kind: Option<String>,
    pub owner_id: Option<Uuid>,
    /// Phase 4 S2 (OBS-06-01..03): SLI outcome/scope, commitment tier, approval.
    pub sli_outcome: String,
    pub sli_scope: String,
    pub commitment_tier: String,
    pub approval_status: String,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSloRequest {
    pub service_name: String,
    pub environment: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: Option<String>,
    /// Defaults to the calling user (resolved by the handler); stored as given.
    #[serde(default)]
    pub owner: Option<OwnerRef>,
    #[serde(default)]
    pub sli_outcome: Option<String>,
    #[serde(default)]
    pub sli_scope: Option<String>,
    #[serde(default)]
    pub commitment_tier: Option<String>,
    #[serde(default)]
    pub approval_status: Option<String>,
    #[serde(default)]
    pub approved_by: Option<Uuid>,
}

#[derive(Serialize)]
pub struct SloListResponse {
    pub items: Vec<SloDefinitionItem>,
}

#[derive(Debug)]
pub enum CreateSloError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for CreateSloError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateSloError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            CreateSloError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for CreateSloError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CreateSloError::Db(e) => Some(e),
            CreateSloError::InvalidInput(_) => None,
        }
    }
}

pub fn validate_create_slo(req: &CreateSloRequest) -> Result<(), String> {
    if req.service_name.trim().is_empty() {
        return Err("service_name is required".into());
    }
    if req.environment.trim().is_empty() {
        return Err("environment is required".into());
    }
    if !(req.target > 0.0 && req.target < 1.0) {
        return Err("target must be between 0 and 1".into());
    }
    if req.window_days <= 0 {
        return Err("window_days must be positive".into());
    }
    if req.burn_rate_fast_threshold <= 0.0 || req.burn_rate_slow_threshold <= 0.0 {
        return Err("burn rate thresholds must be positive".into());
    }
    if let Some(tier) = req.commitment_tier.as_deref()
        && !matches!(tier, "objective" | "internal_slo" | "contractual_sla")
    {
        return Err("commitment_tier must be objective, internal_slo, or contractual_sla".into());
    }
    if let Some(status) = req.approval_status.as_deref() {
        if !matches!(status, "draft" | "approved") {
            return Err("approval_status must be draft or approved".into());
        }
        if status == "approved" {
            let has_outcome = req
                .sli_outcome
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty());
            let has_scope = req
                .sli_scope
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty());
            if !has_outcome || !has_scope {
                return Err(
                    "an SLI without a defined outcome or scope cannot be used for an approved objective"
                        .into(),
                );
            }
            if req.approved_by.is_none() {
                return Err("an SLO lacking an approver cannot be approved".into());
            }
            if req.owner.is_none() {
                return Err("an approved SLO requires an accountable owner".into());
            }
        }
    }
    Ok(())
}

pub async fn list_slos(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<SloDefinitionItem>, sqlx::Error> {
    sqlx::query_as::<_, SloDefinitionItem>(
        "SELECT slo_id, service_name, environment, sli_type, target, window_days, \
         burn_rate_fast_threshold, burn_rate_slow_threshold, description, \
         EXISTS( \
             SELECT 1 FROM alert_rules ar \
             JOIN alert_firings af ON af.rule_id = ar.rule_id \
             WHERE ar.tenant_id = slo_definitions.tenant_id \
               AND ar.alert_type = 'slo_burn_rate' \
               AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
               AND af.state = 'active' \
         ) AS firing, \
         (SELECT MAX(af.occurred_at) FROM alert_rules ar \
          JOIN alert_firings af ON af.rule_id = ar.rule_id \
          WHERE ar.tenant_id = slo_definitions.tenant_id \
            AND ar.alert_type = 'slo_burn_rate' \
            AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
            AND af.state = 'active') AS last_fired_at, \
         created_at, updated_at, owner_kind, owner_id, \
         sli_outcome, sli_scope, commitment_tier, approval_status, approved_by, approved_at \
         FROM slo_definitions WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await
}

pub async fn create_slo(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    req: &CreateSloRequest,
) -> Result<SloDefinitionItem, CreateSloError> {
    validate_create_slo(req).map_err(CreateSloError::InvalidInput)?;

    let mut tx = db.begin().await.map_err(CreateSloError::Db)?;
    let description = req.description.as_deref().unwrap_or("");
    let owner_kind = req.owner.as_ref().map(|o| o.principal_kind.as_str());
    let owner_id = req.owner.as_ref().map(|o| o.principal_id);
    let sli_outcome = req.sli_outcome.as_deref().unwrap_or("").trim();
    let sli_scope = req.sli_scope.as_deref().unwrap_or("").trim();
    let commitment_tier = req.commitment_tier.as_deref().unwrap_or("internal_slo");
    let approval_status = req.approval_status.as_deref().unwrap_or("draft");
    let approved_by = if approval_status == "approved" {
        req.approved_by
    } else {
        None
    };
    let approved_at = if approval_status == "approved" {
        Some(Utc::now())
    } else {
        None
    };
    let slo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO slo_definitions \
         (tenant_id, service_name, environment, sli_type, target, window_days, \
          burn_rate_fast_threshold, burn_rate_slow_threshold, description, owner_kind, owner_id, \
          sli_outcome, sli_scope, commitment_tier, approval_status, approved_by, approved_at) \
         VALUES ($1, $2, $3, 'availability', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
         RETURNING slo_id",
    )
    .bind(tenant_id)
    .bind(req.service_name.trim())
    .bind(req.environment.trim())
    .bind(req.target)
    .bind(req.window_days)
    .bind(req.burn_rate_fast_threshold)
    .bind(req.burn_rate_slow_threshold)
    .bind(description)
    .bind(owner_kind)
    .bind(owner_id)
    .bind(sli_outcome)
    .bind(sli_scope)
    .bind(commitment_tier)
    .bind(approval_status)
    .bind(approved_by)
    .bind(approved_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(CreateSloError::Db)?;

    let condition = serde_json::json!({
        "slo_id": slo_id,
        "fast_window_minutes": 60,
        "slow_window_minutes": 360,
    });
    let rule_name = format!(
        "{} {} availability burn rate",
        req.service_name.trim(),
        req.environment.trim()
    );
    sqlx::query(
        "INSERT INTO alert_rules \
         (tenant_id, name, alert_type, severity, condition, service_name, owner_kind, owner_id) \
         VALUES ($1, $2, 'slo_burn_rate', 'critical', $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(rule_name)
    .bind(condition)
    .bind(req.service_name.trim())
    .bind(owner_kind)
    .bind(owner_id)
    .execute(&mut *tx)
    .await
    .map_err(CreateSloError::Db)?;

    tx.commit().await.map_err(CreateSloError::Db)?;

    let items = list_slos(db, tenant_id).await.map_err(CreateSloError::Db)?;
    items
        .into_iter()
        .find(|item| item.slo_id == slo_id)
        .ok_or_else(|| CreateSloError::InvalidInput("created SLO was not found".into()))
}

pub async fn handle_list_slos(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<SloListResponse>, StatusCode> {
    let items = list_slos(&state.db, ctx.tenant_id).await.map_err(|e| {
        tracing::error!(error = %e, "failed to list SLO definitions");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SloListResponse { items }))
}

pub async fn handle_create_slo(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(mut req): Json<CreateSloRequest>,
) -> Result<(StatusCode, Json<SloDefinitionItem>), StatusCode> {
    req.owner = resolve_owner(req.owner.as_ref(), &ctx)?;
    if req.approval_status.as_deref() == Some("approved") && req.approved_by.is_none() {
        req.approved_by = ctx.user_id;
    }
    match create_slo(&state.db, ctx.tenant_id, &req).await {
        Ok(item) => Ok((StatusCode::CREATED, Json(item))),
        Err(CreateSloError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid SLO input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(CreateSloError::Db(e)) => {
            tracing::error!(error = %e, "failed to create SLO definition");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// PUT /v1/slos/{slo_id}/owner (tenant_admin): reassigns or clears the owner of an objective and
/// of its burn-rate alert rule.
pub async fn handle_update_slo_owner(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    axum::extract::Path(slo_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateOwnerRequest>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&ctx)?;
    req.validate()?;
    let kind = req.owner.as_ref().map(|o| o.principal_kind.as_str());
    let id = req.owner.as_ref().map(|o| o.principal_id);
    let fail = |e: sqlx::Error| {
        tracing::error!(error = %e, "failed to update SLO owner");
        StatusCode::INTERNAL_SERVER_ERROR
    };
    let mut tx = state.db.begin().await.map_err(fail)?;
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE slo_definitions SET owner_kind = $1, owner_id = $2, updated_at = now() \
         WHERE slo_id = $3 AND tenant_id = $4 RETURNING slo_id",
    )
    .bind(kind)
    .bind(id)
    .bind(slo_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(fail)?;
    if updated.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query(
        "UPDATE alert_rules SET owner_kind = $1, owner_id = $2 \
         WHERE tenant_id = $3 AND alert_type = 'slo_burn_rate' AND condition->>'slo_id' = $4",
    )
    .bind(kind)
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(slo_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(fail)?;
    tx.commit().await.map_err(fail)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SloBudgetEvaluation {
    pub attainment: Option<f64>,
    pub breached: bool,
    pub remaining_error_budget_ratio: Option<f64>,
    pub missing_intervals: u64,
    pub evidence_disclosure: Option<String>,
}

/// Computes SLO attainment, breach state, and remaining error budget.
/// Missing intervals are disclosed and are never counted as good events (OBS-06-04).
pub fn compute_slo_budget(
    target: f64,
    total_events: u64,
    good_events: u64,
    expected_intervals: u64,
    observed_intervals: u64,
) -> SloBudgetEvaluation {
    let missing_intervals = expected_intervals.saturating_sub(observed_intervals);
    let evidence_disclosure = if missing_intervals > 0 || total_events == 0 {
        Some(format!(
            "Missing evidence for {missing_intervals} of {expected_intervals} evaluation intervals; missing intervals are not counted as good events."
        ))
    } else {
        None
    };

    if total_events == 0 {
        return SloBudgetEvaluation {
            attainment: None,
            breached: false,
            remaining_error_budget_ratio: None,
            missing_intervals,
            evidence_disclosure,
        };
    }

    let bounded_good = good_events.min(total_events);
    let attainment = bounded_good as f64 / total_events as f64;
    let error_budget_total = (1.0 - target).max(0.0);
    let error_budget_consumed = (1.0 - attainment).max(0.0);
    let remaining_error_budget_ratio = if error_budget_total > 0.0 {
        ((error_budget_total - error_budget_consumed) / error_budget_total).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    SloBudgetEvaluation {
        attainment: Some(attainment),
        breached: attainment < target,
        remaining_error_budget_ratio: Some(remaining_error_budget_ratio),
        missing_intervals,
        evidence_disclosure,
    }
}

// ---------------------------------------------------------------------------
// Phase 4 P1: OBS-06-05, OBS-06-06, OBS-06-07, OBS-06-09
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EvaluateBudgetRiskRequest {
    pub target: f64,
    pub burn_rate: f64,
    pub burn_rate_fast_threshold: f64,
    pub remaining_budget_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct BudgetRiskEvaluation {
    pub state: &'static str,
    pub burn_rate: f64,
    pub remaining_budget_ratio: f64,
    pub reason: String,
}

/// Evaluates whether accelerated error-budget consumption puts a service `at_risk` (OBS-06-05).
pub fn evaluate_budget_risk(req: &EvaluateBudgetRiskRequest) -> BudgetRiskEvaluation {
    if req.remaining_budget_ratio <= 0.0 {
        BudgetRiskEvaluation {
            state: "exhausted",
            burn_rate: req.burn_rate,
            remaining_budget_ratio: req.remaining_budget_ratio,
            reason: "Error budget exhausted".into(),
        }
    } else if req.burn_rate >= req.burn_rate_fast_threshold || req.remaining_budget_ratio <= 0.25 {
        BudgetRiskEvaluation {
            state: "at_risk",
            burn_rate: req.burn_rate,
            remaining_budget_ratio: req.remaining_budget_ratio,
            reason: format!(
                "Accelerated budget consumption (burn_rate={:.2}x, remaining={:.1}%)",
                req.burn_rate,
                req.remaining_budget_ratio * 100.0
            ),
        }
    } else {
        BudgetRiskEvaluation {
            state: "healthy",
            burn_rate: req.burn_rate,
            remaining_budget_ratio: req.remaining_budget_ratio,
            reason: "Within error budget burn rate".into(),
        }
    }
}

pub async fn handle_evaluate_budget_risk(
    Json(req): Json<EvaluateBudgetRiskRequest>,
) -> Json<BudgetRiskEvaluation> {
    Json(evaluate_budget_risk(&req))
}

#[derive(Debug, Deserialize)]
pub struct SloReviewItemInput {
    pub service_name: String,
    pub business_area: String,
    pub reporting_period: String,
    pub target: f64,
    pub attainment: f64,
}

#[derive(Debug, Deserialize)]
pub struct MonthlyReviewRequest {
    pub reporting_period: String,
    pub business_area: Option<String>,
    pub items: Vec<SloReviewItemInput>,
}

#[derive(Debug, Serialize)]
pub struct MonthlySloReviewItem {
    pub service_name: String,
    pub business_area: String,
    pub passed: bool,
    pub attainment: f64,
    pub target: f64,
}

#[derive(Debug, Serialize)]
pub struct MonthlySloReview {
    pub reporting_period: String,
    pub passing_count: usize,
    pub failing_count: usize,
    pub items: Vec<MonthlySloReviewItem>,
}

/// Summarizes passing vs failing SLOs by service, business area, and reporting period (OBS-06-06).
pub fn summarize_monthly_slo_review(req: &MonthlyReviewRequest) -> MonthlySloReview {
    let mut items = Vec::new();
    let mut passing_count = 0usize;
    let mut failing_count = 0usize;
    for entry in &req.items {
        if entry.reporting_period != req.reporting_period {
            continue;
        }
        if let Some(area) = &req.business_area
            && &entry.business_area != area
        {
            continue;
        }
        let passed = entry.attainment >= entry.target;
        if passed {
            passing_count += 1;
        } else {
            failing_count += 1;
        }
        items.push(MonthlySloReviewItem {
            service_name: entry.service_name.clone(),
            business_area: entry.business_area.clone(),
            passed,
            attainment: entry.attainment,
            target: entry.target,
        });
    }
    MonthlySloReview {
        reporting_period: req.reporting_period.clone(),
        passing_count,
        failing_count,
        items,
    }
}

pub async fn handle_monthly_slo_review(
    Json(req): Json<MonthlyReviewRequest>,
) -> Json<MonthlySloReview> {
    Json(summarize_monthly_slo_review(&req))
}

#[derive(Debug, Deserialize)]
pub struct SloTargetRevision {
    pub effective_from: DateTime<Utc>,
    pub target: f64,
}

#[derive(Debug, Deserialize)]
pub struct ResolveEffectiveTargetRequest {
    pub at: DateTime<Utc>,
    pub revisions: Vec<SloTargetRevision>,
}

/// Resolves the SLO target in force at a historical timestamp `at` (OBS-06-07).
pub fn resolve_effective_target(revisions: &[SloTargetRevision], at: DateTime<Utc>) -> Option<f64> {
    revisions
        .iter()
        .filter(|r| r.effective_from <= at)
        .max_by_key(|r| r.effective_from)
        .map(|r| r.target)
}

pub async fn handle_resolve_effective_target(
    Json(req): Json<ResolveEffectiveTargetRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match resolve_effective_target(&req.revisions, req.at) {
        Some(target) => (
            StatusCode::OK,
            Json(serde_json::json!({ "at": req.at, "effective_target": target })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "no_revision_in_force" })),
        ),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyCompositionRule {
    AllMustPass,
    WeightedAverage,
}

#[derive(Debug, Deserialize)]
pub struct ServiceSloReading {
    pub service_name: String,
    pub attainment: f64,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct EvaluateJourneySloRequest {
    pub journey_name: String,
    pub target: f64,
    pub rule: JourneyCompositionRule,
    pub services: Vec<ServiceSloReading>,
}

#[derive(Debug, Serialize)]
pub struct JourneySloResult {
    pub journey_name: String,
    pub target: f64,
    pub journey_attainment: f64,
    pub passed: bool,
    pub failing_services: Vec<String>,
}

/// Evaluates a multi-service customer journey objective according to agreed composition rules (OBS-06-09).
pub fn evaluate_journey_slo(req: &EvaluateJourneySloRequest) -> JourneySloResult {
    let failing_services: Vec<String> = req
        .services
        .iter()
        .filter(|s| s.attainment < req.target)
        .map(|s| s.service_name.clone())
        .collect();
    let journey_attainment = if req.services.is_empty() {
        0.0
    } else {
        match req.rule {
            JourneyCompositionRule::AllMustPass => req
                .services
                .iter()
                .map(|s| s.attainment)
                .fold(f64::INFINITY, f64::min),
            JourneyCompositionRule::WeightedAverage => {
                let total_w: f64 = req.services.iter().map(|s| s.weight.max(0.0)).sum();
                if total_w > 0.0 {
                    req.services
                        .iter()
                        .map(|s| s.attainment * s.weight.max(0.0))
                        .sum::<f64>()
                        / total_w
                } else {
                    0.0
                }
            }
        }
    };
    let passed = match req.rule {
        JourneyCompositionRule::AllMustPass => {
            !req.services.is_empty() && failing_services.is_empty()
        }
        JourneyCompositionRule::WeightedAverage => {
            !req.services.is_empty() && journey_attainment >= req.target
        }
    };
    JourneySloResult {
        journey_name: req.journey_name.clone(),
        target: req.target,
        journey_attainment,
        passed,
        failing_services,
    }
}

pub async fn handle_evaluate_journey_slo(
    Json(req): Json<EvaluateJourneySloRequest>,
) -> Json<JourneySloResult> {
    Json(evaluate_journey_slo(&req))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_slo_requires_outcome_scope_and_approver() {
        let base = CreateSloRequest {
            service_name: "checkout".into(),
            environment: "prod".into(),
            target: 0.999,
            window_days: 30,
            burn_rate_fast_threshold: 14.4,
            burn_rate_slow_threshold: 1.0,
            description: None,
            owner: Some(OwnerRef {
                principal_kind: "user".into(),
                principal_id: Uuid::new_v4(),
            }),
            sli_outcome: None,
            sli_scope: None,
            commitment_tier: Some("internal_slo".into()),
            approval_status: Some("approved".into()),
            approved_by: Some(Uuid::new_v4()),
        };
        assert!(validate_create_slo(&base).is_err());

        let with_sli_no_approver = CreateSloRequest {
            sli_outcome: Some("HTTP requests without 5xx".into()),
            sli_scope: Some("ingress spans excluding /healthz".into()),
            approved_by: None,
            ..base
        };
        assert!(validate_create_slo(&with_sli_no_approver).is_err());
    }

    #[test]
    fn compute_slo_budget_reflects_failures_and_never_counts_missing_evidence_as_good() {
        let healthy = compute_slo_budget(0.99, 1000, 995, 10, 10);
        assert_eq!(healthy.attainment, Some(0.995));
        assert!(!healthy.breached);
        assert!(healthy.remaining_error_budget_ratio.unwrap() > 0.49);
        assert!(healthy.evidence_disclosure.is_none());

        let breached = compute_slo_budget(0.99, 1000, 980, 10, 10);
        assert_eq!(breached.attainment, Some(0.98));
        assert!(breached.breached);
        assert!(breached.remaining_error_budget_ratio.unwrap() < 0.0);

        let missing = compute_slo_budget(0.99, 0, 0, 10, 4);
        assert_eq!(missing.attainment, None);
        assert!(!missing.breached);
        assert_eq!(missing.missing_intervals, 6);
        assert!(
            missing
                .evidence_disclosure
                .as_deref()
                .unwrap()
                .contains("not counted as good events")
        );
    }
}
