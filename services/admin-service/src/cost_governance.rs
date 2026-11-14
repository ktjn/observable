//! Cost attribution reporting (`OBS-17-01`) and budget overrun warnings with
//! fallback escalation (`OBS-17-02`).

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::AdminServiceAppState;

#[derive(Deserialize)]
pub struct RecordExpenditureRequest {
    pub tenant_id: Uuid,
    pub service_name: String,
    pub owner_team: Option<String>,
    pub cost_center: Option<String>,
    pub expenditure_cents: i64,
    pub period: String,
}

#[derive(Deserialize)]
pub struct AttributionQuery {
    pub tenant_id: Uuid,
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
    pub tenant_id: Uuid,
    pub cost_scope: String,
    pub budget_cents: i64,
    pub primary_owner: String,
    pub backup_owner: String,
}

#[derive(Deserialize)]
pub struct EvaluateBudgetRequest {
    pub tenant_id: Uuid,
    pub cost_scope: String,
    pub actual_cents: i64,
    pub primary_webhook_url: String,
    pub backup_webhook_url: String,
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

pub fn router(state: AdminServiceAppState) -> Router {
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

fn db_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "cost_store_unavailable" })),
    )
        .into_response()
}

pub async fn record_expenditure_handler(
    State(state): State<AdminServiceAppState>,
    Json(req): Json<RecordExpenditureRequest>,
) -> Response {
    if req.service_name.trim().is_empty()
        || req.period.trim().is_empty()
        || req.expenditure_cents < 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid expenditure payload" })),
        )
            .into_response();
    }

    let owner = req
        .owner_team
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let cost_center = req
        .cost_center
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    match sqlx::query(
        "INSERT INTO control.service_cost_attribution \
         (tenant_id, service_name, owner_team, cost_center, expenditure_cents, period) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(req.tenant_id)
    .bind(&req.service_name)
    .bind(owner)
    .bind(cost_center)
    .bind(req.expenditure_cents)
    .bind(&req.period)
    .execute(&state.db)
    .await
    {
        Ok(_) => (StatusCode::CREATED, Json(json!({ "recorded": true }))).into_response(),
        Err(_) => db_unavailable(),
    }
}

pub async fn attribution_report_handler(
    State(state): State<AdminServiceAppState>,
    Query(query): Query<AttributionQuery>,
) -> Response {
    let rows = match sqlx::query(
        "SELECT service_name, owner_team, cost_center, expenditure_cents \
         FROM control.service_cost_attribution \
         WHERE tenant_id = $1 AND period = $2 \
         ORDER BY service_name",
    )
    .bind(query.tenant_id)
    .bind(&query.period)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(_) => return db_unavailable(),
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

        match (
            owner_opt.filter(|s| !s.trim().is_empty()),
            cc_opt.filter(|s| !s.trim().is_empty()),
        ) {
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

    (
        StatusCode::OK,
        Json(CostAttributionReport {
            tenant_id: query.tenant_id,
            period: query.period,
            total_cents: attributed_total_cents + unattributed_total_cents,
            attributed_total_cents,
            unattributed_total_cents,
            attributed,
            unattributed,
        }),
    )
        .into_response()
}

pub async fn upsert_budget_handler(
    State(state): State<AdminServiceAppState>,
    Json(req): Json<UpsertBudgetRequest>,
) -> Response {
    if req.cost_scope.trim().is_empty()
        || req.primary_owner.trim().is_empty()
        || req.backup_owner.trim().is_empty()
        || req.budget_cents < 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid budget fields" })),
        )
            .into_response();
    }

    match sqlx::query(
        "INSERT INTO control.cost_budgets \
         (tenant_id, cost_scope, budget_cents, primary_owner, backup_owner, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (tenant_id, cost_scope) \
         DO UPDATE SET budget_cents = EXCLUDED.budget_cents, \
         primary_owner = EXCLUDED.primary_owner, backup_owner = EXCLUDED.backup_owner, updated_at = now()",
    )
    .bind(req.tenant_id)
    .bind(&req.cost_scope)
    .bind(req.budget_cents)
    .bind(&req.primary_owner)
    .bind(&req.backup_owner)
    .execute(&state.db)
    .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({ "saved": true }))).into_response(),
        Err(_) => db_unavailable(),
    }
}

pub async fn evaluate_budget_handler(
    State(state): State<AdminServiceAppState>,
    Json(req): Json<EvaluateBudgetRequest>,
) -> Response {
    let budget_row = match sqlx::query(
        "SELECT budget_cents, primary_owner, backup_owner \
         FROM control.cost_budgets WHERE tenant_id = $1 AND cost_scope = $2",
    )
    .bind(req.tenant_id)
    .bind(&req.cost_scope)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "budget_not_found" })),
            )
                .into_response();
        }
        Err(_) => return db_unavailable(),
    };

    let budget_cents: i64 = budget_row.get("budget_cents");
    let primary_owner: String = budget_row.get("primary_owner");
    let backup_owner: String = budget_row.get("backup_owner");

    if req.actual_cents <= budget_cents {
        return (
            StatusCode::OK,
            Json(BudgetAlertResult {
                breached: false,
                cost_scope: req.cost_scope,
                actual_cents: req.actual_cents,
                budget_cents,
                recipient: None,
                delivery_status: "within_budget".into(),
                escalated_to_backup: false,
            }),
        )
            .into_response();
    }

    let alert_payload = json!({
        "tenant_id": req.tenant_id,
        "cost_scope": req.cost_scope,
        "actual_cents": req.actual_cents,
        "budget_cents": budget_cents,
        "reason": req.reason,
    });

    let primary_ok = matches!(
        state
            .http_client
            .post(&req.primary_webhook_url)
            .json(&alert_payload)
            .send()
            .await,
        Ok(r) if r.status().is_success()
    );

    let (recipient, delivery_status, escalated) = if primary_ok {
        (primary_owner, "delivered", false)
    } else {
        let backup_ok = matches!(
            state
                .http_client
                .post(&req.backup_webhook_url)
                .json(&alert_payload)
                .send()
                .await,
            Ok(r) if r.status().is_success()
        );
        if backup_ok {
            (backup_owner, "escalated_delivered", true)
        } else {
            (backup_owner, "failed", true)
        }
    };

    let _ = sqlx::query(
        "INSERT INTO control.cost_budget_alerts \
         (tenant_id, cost_scope, actual_cents, budget_cents, recipient, delivery_status, escalated_to_backup, reason) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(req.tenant_id)
    .bind(&req.cost_scope)
    .bind(req.actual_cents)
    .bind(budget_cents)
    .bind(&recipient)
    .bind(delivery_status)
    .bind(escalated)
    .bind(&req.reason)
    .execute(&state.db)
    .await;

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
