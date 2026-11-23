//! Cost attribution reporting (`OBS-17-01`) and budget overrun warnings with
//! fallback escalation (`OBS-17-02`), behind `require_tenant`.
//!
//! The tenant and the acting principal come from the caller. Writes need the
//! `tenant_admin` role. Budget owner URLs are part of the budget (set by a tenant
//! admin) and go through the guarded outbound sender at save and at every send;
//! nothing is sent to a URL taken from an evaluate request.

#![allow(clippy::result_large_err)]

use axum::{
    Extension, Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;
use crate::response::net::{SendOutcome, TargetPolicy};

/// State for the cost routes: the control pool and the outbound policy.
#[derive(Clone)]
pub struct CostState {
    pub db: PgPool,
    pub targets: TargetPolicy,
}

#[derive(Deserialize)]
pub struct RecordExpenditureRequest {
    pub service_name: String,
    pub owner_team: Option<String>,
    pub cost_center: Option<String>,
    pub expenditure_cents: i64,
    pub period: String,
}

#[derive(Deserialize)]
pub struct AttributionQuery {
    pub period: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttributedOwnerSummary {
    pub owner_team: String,
    pub cost_center: String,
    pub total_cents: i64,
    pub services: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnattributedCostItem {
    pub service_name: String,
    pub expenditure_cents: i64,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CostAttributionReport {
    pub tenant_id: Uuid,
    pub period: String,
    pub total_cents: i64,
    pub attributed_total_cents: i64,
    pub unattributed_total_cents: i64,
    pub attributed: Vec<AttributedOwnerSummary>,
    pub unattributed: Vec<UnattributedCostItem>,
}

#[derive(Deserialize)]
pub struct UpsertBudgetRequest {
    pub cost_scope: String,
    pub budget_cents: i64,
    pub primary_owner: String,
    pub backup_owner: String,
    pub primary_url: String,
    pub backup_url: String,
}

#[derive(Deserialize)]
pub struct EvaluateBudgetRequest {
    pub cost_scope: String,
    pub actual_cents: i64,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BudgetAlertResult {
    pub breached: bool,
    pub cost_scope: String,
    pub actual_cents: i64,
    pub budget_cents: i64,
    pub recipient: Option<String>,
    pub delivery_status: String,
    pub escalated_to_backup: bool,
}

pub fn router<S: Clone + Send + Sync + 'static>(state: CostState) -> Router<S> {
    Router::new()
        .route("/v1/cost/expenditures", post(record_expenditure_handler))
        .route(
            "/v1/cost/attribution-report",
            get(attribution_report_handler),
        )
        .route("/v1/cost/budgets", put(upsert_budget_handler))
        .route("/v1/cost/budgets/evaluate", post(evaluate_budget_handler))
        .with_state(state)
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn db_unavailable(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "cost store unavailable");
    error(StatusCode::SERVICE_UNAVAILABLE, "cost_store_unavailable")
}

fn require_tenant_admin(ctx: &TenantContext) -> Result<(), Response> {
    if ctx.role == "tenant_admin" {
        Ok(())
    } else {
        Err(error(StatusCode::FORBIDDEN, "tenant_admin_required"))
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn valid_text(value: &str, max: usize) -> bool {
    let v = value.trim();
    !v.is_empty() && v.chars().count() <= max
}

pub async fn record_expenditure_handler(
    State(state): State<CostState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<RecordExpenditureRequest>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    if !valid_text(&req.service_name, 200)
        || !valid_text(&req.period, 32)
        || req.expenditure_cents < 0
    {
        return error(StatusCode::BAD_REQUEST, "invalid_expenditure");
    }
    match sqlx::query(
        "INSERT INTO control.service_cost_attribution \
         (tenant_id, service_name, owner_team, cost_center, expenditure_cents, period, recorded_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(ctx.tenant_id)
    .bind(req.service_name.trim())
    .bind(clean(req.owner_team))
    .bind(clean(req.cost_center))
    .bind(req.expenditure_cents)
    .bind(req.period.trim())
    .bind(ctx.user_id)
    .execute(&state.db)
    .await
    {
        Ok(_) => (StatusCode::CREATED, Json(json!({ "recorded": true }))).into_response(),
        Err(e) => db_unavailable(e),
    }
}

pub async fn attribution_report_handler(
    State(state): State<CostState>,
    Extension(ctx): Extension<TenantContext>,
    Query(query): Query<AttributionQuery>,
) -> Response {
    let rows = match sqlx::query(
        "SELECT service_name, owner_team, cost_center, expenditure_cents \
         FROM control.service_cost_attribution \
         WHERE tenant_id = $1 AND period = $2 \
         ORDER BY service_name",
    )
    .bind(ctx.tenant_id)
    .bind(&query.period)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return db_unavailable(e),
    };

    let mut grouped: BTreeMap<(String, String), (i64, Vec<String>)> = BTreeMap::new();
    let mut unattributed = Vec::new();
    let mut attributed_total_cents = 0i64;
    let mut unattributed_total_cents = 0i64;

    for row in rows {
        let service: String = row.get("service_name");
        let owner_opt: Option<String> = row.get("owner_team");
        let cc_opt: Option<String> = row.get("cost_center");
        let cents: i64 = row.get("expenditure_cents");

        match (clean(owner_opt), clean(cc_opt)) {
            (Some(owner), Some(cc)) => {
                attributed_total_cents += cents;
                let entry = grouped
                    .entry((owner, cc))
                    .or_insert_with(|| (0, Vec::new()));
                entry.0 += cents;
                if !entry.1.contains(&service) {
                    entry.1.push(service);
                }
            }
            _ => {
                unattributed_total_cents += cents;
                unattributed.push(UnattributedCostItem {
                    service_name: service,
                    expenditure_cents: cents,
                    reason: "missing attributable owner_team or cost_center".into(),
                });
            }
        }
    }

    let attributed = grouped
        .into_iter()
        .map(
            |((owner_team, cost_center), (total_cents, services))| AttributedOwnerSummary {
                owner_team,
                cost_center,
                total_cents,
                services,
            },
        )
        .collect();

    Json(CostAttributionReport {
        tenant_id: ctx.tenant_id,
        period: query.period,
        total_cents: attributed_total_cents + unattributed_total_cents,
        attributed_total_cents,
        unattributed_total_cents,
        attributed,
        unattributed,
    })
    .into_response()
}

pub async fn upsert_budget_handler(
    State(state): State<CostState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<UpsertBudgetRequest>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    if !valid_text(&req.cost_scope, 200)
        || !valid_text(&req.primary_owner, 200)
        || !valid_text(&req.backup_owner, 200)
        || req.budget_cents < 0
    {
        return error(StatusCode::BAD_REQUEST, "invalid_budget");
    }
    let mut urls = Vec::with_capacity(2);
    for raw in [&req.primary_url, &req.backup_url] {
        match state.targets.validate(raw).await {
            Ok(u) => urls.push(u.to_string()),
            Err(r) => return error(StatusCode::BAD_REQUEST, r.code()),
        }
    }
    match sqlx::query(
        "INSERT INTO control.cost_budgets \
         (tenant_id, cost_scope, budget_cents, primary_owner, backup_owner, primary_url, backup_url, \
          updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now()) \
         ON CONFLICT (tenant_id, cost_scope) \
         DO UPDATE SET budget_cents = EXCLUDED.budget_cents, \
         primary_owner = EXCLUDED.primary_owner, backup_owner = EXCLUDED.backup_owner, \
         primary_url = EXCLUDED.primary_url, backup_url = EXCLUDED.backup_url, \
         updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(ctx.tenant_id)
    .bind(req.cost_scope.trim())
    .bind(req.budget_cents)
    .bind(req.primary_owner.trim())
    .bind(req.backup_owner.trim())
    .bind(&urls[0])
    .bind(&urls[1])
    .bind(ctx.user_id)
    .execute(&state.db)
    .await
    {
        Ok(_) => Json(json!({ "saved": true })).into_response(),
        Err(e) => db_unavailable(e),
    }
}

/// Evaluates spend against the stored budget. A breach goes to the primary owner's URL
/// and, when that fails, to the backup owner's. Every breach is recorded; a breach that
/// cannot be recorded is a 503, so an overrun is never reported as handled.
pub async fn evaluate_budget_handler(
    State(state): State<CostState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<EvaluateBudgetRequest>,
) -> Response {
    if let Err(r) = require_tenant_admin(&ctx) {
        return r;
    }
    if req.actual_cents < 0 || !valid_text(&req.reason, 2000) {
        return error(StatusCode::BAD_REQUEST, "invalid_evaluation");
    }
    let budget_row = match sqlx::query(
        "SELECT budget_cents, primary_owner, backup_owner, primary_url, backup_url \
         FROM control.cost_budgets WHERE tenant_id = $1 AND cost_scope = $2",
    )
    .bind(ctx.tenant_id)
    .bind(&req.cost_scope)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "budget_not_found"),
        Err(e) => return db_unavailable(e),
    };

    let budget_cents: i64 = budget_row.get("budget_cents");
    let primary_owner: String = budget_row.get("primary_owner");
    let backup_owner: String = budget_row.get("backup_owner");
    let primary_url: String = budget_row.get("primary_url");
    let backup_url: String = budget_row.get("backup_url");

    if req.actual_cents <= budget_cents {
        return Json(BudgetAlertResult {
            breached: false,
            cost_scope: req.cost_scope,
            actual_cents: req.actual_cents,
            budget_cents,
            recipient: None,
            delivery_status: "within_budget".into(),
            escalated_to_backup: false,
        })
        .into_response();
    }

    let alert_payload = json!({
        "tenant_id": ctx.tenant_id,
        "cost_scope": req.cost_scope,
        "actual_cents": req.actual_cents,
        "budget_cents": budget_cents,
        "reason": req.reason,
    });

    let (recipient, delivery_status, escalated, last_error) =
        match state.targets.post_json(&primary_url, &alert_payload).await {
            SendOutcome::Delivered { .. } => (primary_owner, "delivered", false, None),
            SendOutcome::Failed {
                error: primary_err, ..
            } => match state.targets.post_json(&backup_url, &alert_payload).await {
                SendOutcome::Delivered { .. } => {
                    (backup_owner, "escalated_delivered", true, Some(primary_err))
                }
                SendOutcome::Failed {
                    error: backup_err, ..
                } => (
                    backup_owner,
                    "failed",
                    true,
                    Some(format!("primary: {primary_err}; backup: {backup_err}")),
                ),
            },
        };

    if let Err(e) = sqlx::query(
        "INSERT INTO control.cost_budget_alerts \
         (tenant_id, cost_scope, actual_cents, budget_cents, recipient, delivery_status, \
          escalated_to_backup, reason, last_error, requested_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(ctx.tenant_id)
    .bind(&req.cost_scope)
    .bind(req.actual_cents)
    .bind(budget_cents)
    .bind(&recipient)
    .bind(delivery_status)
    .bind(escalated)
    .bind(&req.reason)
    .bind(last_error.as_deref())
    .bind(ctx.user_id)
    .execute(&state.db)
    .await
    {
        return db_unavailable(e);
    }

    let status_code = if delivery_status == "failed" {
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::OK
    };

    (
        status_code,
        Json(BudgetAlertResult {
            breached: true,
            cost_scope: req.cost_scope,
            actual_cents: req.actual_cents,
            budget_cents,
            recipient: Some(recipient),
            delivery_status: delivery_status.into(),
            escalated_to_backup: escalated,
        }),
    )
        .into_response()
}

/// Reconciles observability cost categories for a period (OBS-17-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostCategoryEntry {
    pub category: String, // "ingest" | "storage" | "query" | "retention"
    pub cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostReconciliationResult {
    pub total_cents: i64,
    pub reported_cents: i64,
    pub reconciled: bool,
}

pub fn reconcile_cost_categories(
    categories: &[CostCategoryEntry],
    reported_total_cents: i64,
) -> CostReconciliationResult {
    let sum: i64 = categories.iter().map(|c| c.cents).sum();
    CostReconciliationResult {
        total_cents: sum,
        reported_cents: reported_total_cents,
        reconciled: sum == reported_total_cents,
    }
}

/// Compares business-cost implications of two retention/sampling policy choices (OBS-17-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyCostComparison {
    pub option_a_monthly_cents: i64,
    pub option_b_monthly_cents: i64,
    pub monthly_savings_cents: i64,
}

pub fn compare_policy_cost_implications(
    option_a_monthly_cents: i64,
    option_b_monthly_cents: i64,
) -> PolicyCostComparison {
    PolicyCostComparison {
        option_a_monthly_cents,
        option_b_monthly_cents,
        monthly_savings_cents: option_a_monthly_cents - option_b_monthly_cents,
    }
}

/// Rationalization candidate with risk and benefit (OBS-17-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RationalizationCandidate {
    pub asset_id: String,
    pub monthly_savings_cents: i64,
    pub risk_level: String,
}

pub fn identify_rationalization_candidates(
    candidates: Vec<RationalizationCandidate>,
) -> Vec<RationalizationCandidate> {
    candidates
        .into_iter()
        .filter(|c| c.monthly_savings_cents > 0 && !c.risk_level.is_empty())
        .collect()
}

#[cfg(test)]
mod p1_cost_governance_tests {
    use super::*;

    #[test]
    fn obs_17_03_cost_report_reconciles_categories_for_period() {
        let res = reconcile_cost_categories(
            &[
                CostCategoryEntry {
                    category: "ingest".to_string(),
                    cents: 4000,
                },
                CostCategoryEntry {
                    category: "storage".to_string(),
                    cents: 3500,
                },
                CostCategoryEntry {
                    category: "query".to_string(),
                    cents: 2500,
                },
            ],
            10000,
        );
        assert!(res.reconciled);
        assert_eq!(res.total_cents, 10000);
    }

    #[test]
    fn obs_17_04_planning_compares_two_policy_choices() {
        let cmp = compare_policy_cost_implications(120_000, 75_000);
        assert_eq!(cmp.monthly_savings_cents, 45_000);
    }

    #[test]
    fn obs_17_05_surfaces_rationalization_candidates_with_risk_and_benefit() {
        let candidates = identify_rationalization_candidates(vec![RationalizationCandidate {
            asset_id: "unused-debug-dashboard".to_string(),
            monthly_savings_cents: 18_000,
            risk_level: "low".to_string(),
        }]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].risk_level, "low");
        assert_eq!(candidates[0].monthly_savings_cents, 18_000);
    }

    #[test]
    fn obs_17_08_retention_reduction_blocks_required_evidence_without_exception() {
        let blocked = evaluate_retention_reduction(90, 7, true, false, 120_000, 40_000);
        assert!(blocked.blocked_pending_exception);
        let allowed = evaluate_retention_reduction(90, 30, false, false, 120_000, 70_000);
        assert!(!allowed.blocked_pending_exception);
        assert_eq!(allowed.projected_monthly_cents, 70_000);
    }
}

/// Retention reduction impact assessment (OBS-17-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionReductionAssessment {
    pub projected_monthly_cents: i64,
    pub lost_evidence_window_days: i64,
    pub blocked_pending_exception: bool,
}

pub fn evaluate_retention_reduction(
    current_days: i64,
    proposed_days: i64,
    required_for_audit_or_slo: bool,
    has_approved_exception: bool,
    _current_monthly_cents: i64,
    projected_monthly_cents: i64,
) -> RetentionReductionAssessment {
    RetentionReductionAssessment {
        projected_monthly_cents,
        lost_evidence_window_days: (current_days - proposed_days).max(0),
        blocked_pending_exception: required_for_audit_or_slo && !has_approved_exception,
    }
}
