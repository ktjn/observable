//! Monitoring coverage (phase 1 S5): telemetry bindings, coverage policies, the observability
//! asset registry (`govern-observability-assets`), assessment of required against demonstrated
//! coverage, and the gap workflow.
//!
//! - Required coverage is the coverage policy for the service's criticality. Without a stored
//!   policy the default (traces, logs, metrics within 60 minutes) applies and the report says so;
//!   only an approved policy counts as agreed coverage (OBS-27-01).
//! - Demonstrated coverage per item: telemetry signals need data within the policy window for a
//!   bound `service.name`; asset items need a registered, non-retired asset for the service with
//!   an active owner. A demonstrated item always carries an evidence reference; an owner claim is
//!   recorded but never counts as evidence (OBS-27-02).
//! - Status is `covered`, `partial`, `uncovered` or `unknown` (telemetry unavailable, never
//!   assessed, or stale). Coverage never reports health (OBS-02-03).
//! - Gaps: unmet items of critical services and critical journeys without full coverage open a
//!   gap. A gap closes only when remediated (the item is demonstrated now) or under an approved,
//!   unexpired risk acceptance (OBS-27-04).

use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, NaiveDate, Utc};
use observable_auth::authz::Classification;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::CatalogState;
use crate::api::{
    ApiError, ApiResult, CRITICALITY, PrincipalRef, READ, ServiceRow, ServiceSummary, WRITE,
    check_principal, decide, load_services, one_of, readable_service, resolve_principals, visible,
    writable_service,
};
use crate::journeys::{JourneyRow, load_journeys, visible_journeys, writable_journey};
use crate::ports::Caller;

pub const COVERAGE_POLICY: &str = "catalog.coverage_policy";
pub const SIGNALS: &[&str] = &["traces", "logs", "metrics"];
pub const ASSET_KINDS: &[&str] = &[
    "alert_rule",
    "objective",
    "dashboard",
    "proactive_check",
    "health_rule",
    "runbook",
];
const DEFAULT_ITEMS: &[&str] = &["traces", "logs", "metrics"];
const DEFAULT_WINDOW_MINUTES: i32 = 60;
pub const JOURNEY_ITEM: &str = "journey_coverage";

fn is_item(item: &str) -> bool {
    SIGNALS.contains(&item) || ASSET_KINDS.contains(&item)
}

// ---------------------------------------------------------------------------
// Policies
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct Policy {
    pub criticality: String,
    pub items: Vec<String>,
    pub window_minutes: i32,
    /// `approved`, `unapproved` (stored, awaiting approval) or `default` (nothing stored).
    pub source: String,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct PolicyRow {
    criticality: String,
    items: Vec<String>,
    window_minutes: i32,
    approved_by: Option<Uuid>,
    approved_at: Option<DateTime<Utc>>,
}

async fn policies(db: &PgPool, tenant_id: Uuid) -> Result<HashMap<String, Policy>, sqlx::Error> {
    let rows: Vec<PolicyRow> = sqlx::query_as(
        "SELECT criticality, items, window_minutes, approved_by, approved_at \
         FROM catalog.coverage_policies WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;
    let mut out: HashMap<String, Policy> = CRITICALITY
        .iter()
        .map(|c| {
            (
                (*c).to_owned(),
                Policy {
                    criticality: (*c).to_owned(),
                    items: DEFAULT_ITEMS.iter().map(|s| (*s).to_owned()).collect(),
                    window_minutes: DEFAULT_WINDOW_MINUTES,
                    source: "default".into(),
                    approved_by: None,
                    approved_at: None,
                },
            )
        })
        .collect();
    for r in rows {
        out.insert(
            r.criticality.clone(),
            Policy {
                source: if r.approved_at.is_some() {
                    "approved".into()
                } else {
                    "unapproved".into()
                },
                criticality: r.criticality,
                items: r.items,
                window_minutes: r.window_minutes,
                approved_by: r.approved_by,
                approved_at: r.approved_at,
            },
        );
    }
    Ok(out)
}

async fn policy_access(state: &CatalogState, caller: &Caller, action: &str) -> ApiResult<bool> {
    decide(
        state,
        caller,
        action,
        COVERAGE_POLICY,
        "policies",
        Some(Classification::Internal),
    )
    .await
}

pub async fn list_policies(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
) -> ApiResult<Json<serde_json::Value>> {
    if !policy_access(&state, &caller, READ).await? {
        return Err(ApiError::NotFound);
    }
    let all = policies(&state.db, caller.tenant_id).await?;
    let items: Vec<&Policy> = CRITICALITY.iter().filter_map(|c| all.get(*c)).collect();
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
pub struct PolicyBody {
    items: Vec<String>,
    window_minutes: Option<i32>,
}

/// Replacing a policy withdraws its approval: changed expectations must be agreed again.
pub async fn put_policy(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(criticality): Path<String>,
    Json(body): Json<PolicyBody>,
) -> ApiResult<Json<Policy>> {
    one_of("criticality", &criticality, CRITICALITY)?;
    if !policy_access(&state, &caller, WRITE).await? {
        return Err(ApiError::Forbidden);
    }
    let mut items: Vec<String> = Vec::new();
    for i in &body.items {
        if !is_item(i) {
            return Err(ApiError::BadRequest(format!(
                "items must be drawn from {}, {}",
                SIGNALS.join(", "),
                ASSET_KINDS.join(", ")
            )));
        }
        if !items.contains(i) {
            items.push(i.clone());
        }
    }
    if items.is_empty() {
        return Err(ApiError::BadRequest("items must not be empty".into()));
    }
    let window = body.window_minutes.unwrap_or(DEFAULT_WINDOW_MINUTES);
    if !(5..=10080).contains(&window) {
        return Err(ApiError::BadRequest(
            "window_minutes must be 5-10080".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO catalog.coverage_policies (tenant_id, criticality, items, window_minutes) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (tenant_id, criticality) DO UPDATE \
         SET items = EXCLUDED.items, window_minutes = EXCLUDED.window_minutes, \
         approved_by = NULL, approved_at = NULL, updated_at = now()",
    )
    .bind(caller.tenant_id)
    .bind(&criticality)
    .bind(&items)
    .bind(window)
    .execute(&state.db)
    .await?;
    let mut all = policies(&state.db, caller.tenant_id).await?;
    all.remove(&criticality).map(Json).ok_or(ApiError::Internal)
}

/// Approval is a human decision: only a session user may approve.
pub async fn approve_policy(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(criticality): Path<String>,
) -> ApiResult<StatusCode> {
    one_of("criticality", &criticality, CRITICALITY)?;
    if !policy_access(&state, &caller, WRITE).await? {
        return Err(ApiError::Forbidden);
    }
    let Some(approver) = caller.user_id else {
        return Err(ApiError::Forbidden);
    };
    let updated = sqlx::query(
        "UPDATE catalog.coverage_policies SET approved_by = $3, approved_at = now() \
         WHERE tenant_id = $1 AND criticality = $2",
    )
    .bind(caller.tenant_id)
    .bind(&criticality)
    .bind(approver)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::BadRequest(
            "store a policy for this criticality before approving it".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct Binding {
    pub service_name: String,
    #[serde(default)]
    pub environment: String,
}

async fn bindings_of(
    db: &PgPool,
    tenant_id: Uuid,
    service_id: Uuid,
) -> Result<Vec<Binding>, sqlx::Error> {
    sqlx::query_as(
        "SELECT service_name, environment FROM catalog.service_bindings \
         WHERE tenant_id = $1 AND service_id = $2 ORDER BY service_name, environment",
    )
    .bind(tenant_id)
    .bind(service_id)
    .fetch_all(db)
    .await
}

#[derive(Deserialize)]
pub struct BindingsBody {
    bindings: Vec<Binding>,
}

/// PUT /v1/catalog/services/{id}/bindings: replaces the telemetry identities of a service.
pub async fn put_bindings(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<BindingsBody>,
) -> ApiResult<Json<serde_json::Value>> {
    writable_service(&state, &caller, id).await?;
    let mut seen = HashSet::new();
    for b in &body.bindings {
        if b.service_name.trim().is_empty()
            || b.service_name.len() > 255
            || b.environment.len() > 64
            || !seen.insert((b.service_name.trim(), b.environment.trim()))
        {
            return Err(ApiError::BadRequest(
                "bindings need a distinct service_name (1-255) and environment (0-64)".into(),
            ));
        }
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM catalog.service_bindings WHERE tenant_id = $1 AND service_id = $2")
        .bind(caller.tenant_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for b in &body.bindings {
        sqlx::query(
            "INSERT INTO catalog.service_bindings (service_id, tenant_id, service_name, environment) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(caller.tenant_id)
        .bind(b.service_name.trim())
        .bind(b.environment.trim())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({ "bindings": bindings_of(&state.db, caller.tenant_id, id).await? }),
    ))
}

// ---------------------------------------------------------------------------
// Asset registry (govern-observability-assets)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, FromRow)]
struct AssetRow {
    kind: String,
    external_id: String,
    owning_component: String,
    service_id: Uuid,
    name: String,
    owner_kind: Option<String>,
    owner_id: Option<Uuid>,
    registered_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const ASSET_COLUMNS: &str = "kind, external_id, owning_component, service_id, name, owner_kind, \
     owner_id, registered_at, updated_at";

async fn live_assets(
    db: &PgPool,
    tenant_id: Uuid,
    service_ids: &[Uuid],
) -> Result<Vec<AssetRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {ASSET_COLUMNS} FROM catalog.observability_assets \
         WHERE tenant_id = $1 AND service_id = ANY($2) AND retired_at IS NULL \
         ORDER BY kind, external_id"
    )))
    .bind(tenant_id)
    .bind(service_ids)
    .fetch_all(db)
    .await
}

#[derive(Serialize)]
pub struct AssetView {
    kind: String,
    external_id: String,
    owning_component: String,
    service_id: Uuid,
    name: String,
    owner_kind: Option<String>,
    owner_id: Option<Uuid>,
    /// `active`, `departed`, `unknown`, or `none` when no owner is registered.
    owner_state: String,
    registered_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn asset_views(
    state: &CatalogState,
    tenant_id: Uuid,
    rows: Vec<AssetRow>,
) -> ApiResult<Vec<AssetView>> {
    let owners: Vec<(String, Uuid)> = rows
        .iter()
        .filter_map(|a| Some((a.owner_kind.clone()?, a.owner_id?)))
        .collect();
    let resolution = resolve_principals(state, tenant_id, &owners).await?;
    Ok(rows
        .into_iter()
        .map(|a| AssetView {
            owner_state: match (&a.owner_kind, a.owner_id) {
                (Some(k), Some(id)) => resolution.state(k, id).into(),
                _ => "none".into(),
            },
            kind: a.kind,
            external_id: a.external_id,
            owning_component: a.owning_component,
            service_id: a.service_id,
            name: a.name,
            owner_kind: a.owner_kind,
            owner_id: a.owner_id,
            registered_at: a.registered_at,
            updated_at: a.updated_at,
        })
        .collect())
}

#[derive(Deserialize)]
pub struct AssetBody {
    service_id: Option<Uuid>,
    /// Internal producers may name the service by telemetry identity instead of id.
    service_name: Option<String>,
    #[serde(default)]
    environment: String,
    #[serde(default)]
    name: String,
    owner: Option<PrincipalRef>,
    /// Internal producers only.
    tenant_id: Option<Uuid>,
    owning_component: Option<String>,
}

fn asset_key(kind: &str, external_id: &str) -> ApiResult<()> {
    one_of("kind", kind, ASSET_KINDS)?;
    if external_id.is_empty() || external_id.len() > 200 {
        return Err(ApiError::BadRequest(
            "external_id must be 1-200 characters".into(),
        ));
    }
    Ok(())
}

async fn upsert_asset(
    db: &PgPool,
    tenant_id: Uuid,
    kind: &str,
    external_id: &str,
    owning_component: &str,
    service_id: Uuid,
    body: &AssetBody,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO catalog.observability_assets (tenant_id, kind, external_id, owning_component, \
         service_id, name, owner_kind, owner_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (tenant_id, kind, external_id) DO UPDATE SET owning_component = EXCLUDED.owning_component, \
         service_id = EXCLUDED.service_id, name = EXCLUDED.name, owner_kind = EXCLUDED.owner_kind, \
         owner_id = EXCLUDED.owner_id, retired_at = NULL, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(external_id)
    .bind(owning_component)
    .bind(service_id)
    .bind(body.name.trim())
    .bind(body.owner.as_ref().map(|o| o.principal_kind.as_str()))
    .bind(body.owner.as_ref().map(|o| o.principal_id))
    .execute(db)
    .await?;
    Ok(())
}

/// PUT /v1/catalog/assets/{kind}/{external_id}: registers an asset (typically a runbook) for a
/// service the caller may change. The owner must be an active user or a team of the tenant.
pub async fn put_asset(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path((kind, external_id)): Path<(String, String)>,
    Json(body): Json<AssetBody>,
) -> ApiResult<StatusCode> {
    asset_key(&kind, &external_id)?;
    let service_id = body
        .service_id
        .ok_or_else(|| ApiError::BadRequest("service_id is required".into()))?;
    writable_service(&state, &caller, service_id)
        .await
        .map_err(|e| match e {
            ApiError::NotFound => ApiError::BadRequest("unknown service".into()),
            other => other,
        })?;
    if let Some(o) = &body.owner {
        check_principal(&state, &caller, &o.principal_kind, o.principal_id).await?;
    }
    if let Some(existing) =
        asset_owner_service(&state.db, caller.tenant_id, &kind, &external_id).await?
        && existing != service_id
    {
        writable_service(&state, &caller, existing).await?;
    }
    upsert_asset(
        &state.db,
        caller.tenant_id,
        &kind,
        &external_id,
        "catalog",
        service_id,
        &body,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn asset_owner_service(
    db: &PgPool,
    tenant_id: Uuid,
    kind: &str,
    external_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT service_id FROM catalog.observability_assets \
         WHERE tenant_id = $1 AND kind = $2 AND external_id = $3",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(external_id)
    .fetch_optional(db)
    .await
}

/// DELETE /v1/catalog/assets/{kind}/{external_id}: retires an asset.
pub async fn delete_asset(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path((kind, external_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    asset_key(&kind, &external_id)?;
    let service = asset_owner_service(&state.db, caller.tenant_id, &kind, &external_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    writable_service(&state, &caller, service).await?;
    retire_asset(&state.db, caller.tenant_id, &kind, &external_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn retire_asset(
    db: &PgPool,
    tenant_id: Uuid,
    kind: &str,
    external_id: &str,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE catalog.observability_assets SET retired_at = now(), updated_at = now() \
         WHERE tenant_id = $1 AND kind = $2 AND external_id = $3 AND retired_at IS NULL",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(external_id)
    .execute(db)
    .await?
    .rows_affected())
}

#[derive(Deserialize)]
pub struct AssetQuery {
    service_id: Option<Uuid>,
    #[serde(default)]
    ownerless: bool,
}

/// GET /v1/catalog/assets: registered assets of services the caller may read. `ownerless=true`
/// keeps assets without an owner or whose owner is no longer active.
pub async fn list_assets(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<AssetQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let services = load_services(
        &state.db,
        caller.tenant_id,
        q.service_id.as_ref().map(std::slice::from_ref),
    )
    .await?;
    let ids: Vec<Uuid> = visible(&state, &caller, services)
        .await?
        .iter()
        .map(|s| s.id)
        .collect();
    let rows = live_assets(&state.db, caller.tenant_id, &ids).await?;
    let items: Vec<AssetView> = asset_views(&state, caller.tenant_id, rows)
        .await?
        .into_iter()
        .filter(|a| !q.ownerless || a.owner_state != "active")
        .collect();
    let total = items.len();
    Ok(Json(json!({ "items": items, "total": total })))
}

fn internal_auth(state: &CatalogState, headers: &HeaderMap) -> ApiResult<()> {
    match state.internal_token.as_deref() {
        Some(token) => observable_auth::verify_internal_token(headers, token)
            .map_err(|_| ApiError::Unauthorized),
        None => Err(ApiError::Unauthorized),
    }
}

/// PUT /internal/catalog/assets/{kind}/{external_id} (internal token): producers (alerting,
/// control) register an asset by service id or bound telemetry identity.
pub async fn internal_put_asset(
    State(state): State<CatalogState>,
    headers: HeaderMap,
    Path((kind, external_id)): Path<(String, String)>,
    Json(body): Json<AssetBody>,
) -> ApiResult<StatusCode> {
    internal_auth(&state, &headers)?;
    asset_key(&kind, &external_id)?;
    let tenant_id = body
        .tenant_id
        .ok_or_else(|| ApiError::BadRequest("tenant_id is required".into()))?;
    let component = body
        .owning_component
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty() && c.len() <= 100)
        .ok_or_else(|| ApiError::BadRequest("owning_component is required".into()))?
        .to_owned();
    let service_id: Option<Uuid> = match (body.service_id, body.service_name.as_deref()) {
        (Some(id), _) => {
            sqlx::query_scalar("SELECT id FROM catalog.services WHERE tenant_id = $1 AND id = $2")
                .bind(tenant_id)
                .bind(id)
                .fetch_optional(&state.db)
                .await?
        }
        (None, Some(name)) => {
            sqlx::query_scalar(
                "SELECT service_id FROM catalog.service_bindings WHERE tenant_id = $1 \
             AND service_name = $2 AND environment IN ($3, '') ORDER BY environment DESC LIMIT 1",
            )
            .bind(tenant_id)
            .bind(name)
            .bind(body.environment.trim())
            .fetch_optional(&state.db)
            .await?
        }
        (None, None) => None,
    };
    let service_id = service_id.ok_or(ApiError::Unprocessable("unknown_service"))?;
    upsert_asset(
        &state.db,
        tenant_id,
        &kind,
        &external_id,
        &component,
        service_id,
        &body,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TenantParam {
    tenant_id: Uuid,
}

/// DELETE /internal/catalog/assets/{kind}/{external_id}?tenant_id= (internal token).
pub async fn internal_delete_asset(
    State(state): State<CatalogState>,
    headers: HeaderMap,
    Path((kind, external_id)): Path<(String, String)>,
    Query(q): Query<TenantParam>,
) -> ApiResult<StatusCode> {
    internal_auth(&state, &headers)?;
    asset_key(&kind, &external_id)?;
    retire_asset(&state.db, q.tenant_id, &kind, &external_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ClaimBody {
    #[serde(default)]
    note: String,
}

/// PUT /v1/catalog/services/{id}/coverage/claims/{item}: records that an owner says an item is
/// met. A claim is shown on the report but is never evidence.
pub async fn put_claim(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path((id, item)): Path<(Uuid, String)>,
    Json(body): Json<ClaimBody>,
) -> ApiResult<StatusCode> {
    writable_service(&state, &caller, id).await?;
    if !is_item(&item) {
        return Err(ApiError::BadRequest("unknown coverage item".into()));
    }
    let by = match caller.user_id {
        Some(u) => format!("user:{u}"),
        None => format!("api_key:{}", caller.role),
    };
    sqlx::query(
        "INSERT INTO catalog.coverage_claims (service_id, tenant_id, item, claimed_by, note) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (service_id, item) DO UPDATE \
         SET claimed_by = EXCLUDED.claimed_by, note = EXCLUDED.note, created_at = now()",
    )
    .bind(id)
    .bind(caller.tenant_id)
    .bind(&item)
    .bind(by)
    .bind(body.note.trim())
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Assessment
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ItemResult {
    pub item: String,
    pub demonstrated: bool,
    pub evidence_ref: Option<String>,
    /// Why an item is unmet, or what demonstrated it.
    pub detail: String,
    /// An owner claimed the item is met; only evidence counts.
    pub claimed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoverageReport {
    pub service: ServiceSummary,
    /// `covered`, `partial`, `uncovered` or `unknown`. Coverage is never a health state.
    pub status: String,
    pub policy: Policy,
    pub bindings: Vec<Binding>,
    pub items: Vec<ItemResult>,
    /// Required items without evidence (OBS-27-02).
    pub unmet: Vec<String>,
    pub assessed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn status_of(items: &[ItemResult], failed: bool) -> &'static str {
    if failed {
        return "unknown";
    }
    let met = items.iter().filter(|i| i.demonstrated).count();
    if met == items.len() {
        "covered"
    } else if met == 0 {
        "uncovered"
    } else {
        "partial"
    }
}

/// Assesses one service against its policy and stores the result. Critical services open a gap
/// for each unmet item.
pub async fn assess(
    state: &CatalogState,
    tenant_id: Uuid,
    service: &ServiceRow,
    policy: &Policy,
) -> ApiResult<CoverageReport> {
    let now = Utc::now();
    let since_ns = (now - chrono::Duration::minutes(policy.window_minutes.into()))
        .timestamp_nanos_opt()
        .unwrap_or(0)
        .max(0) as u64;
    let bindings = bindings_of(&state.db, tenant_id, service.id).await?;
    let claims: HashSet<String> = sqlx::query_scalar(
        "SELECT item FROM catalog.coverage_claims WHERE tenant_id = $1 AND service_id = $2",
    )
    .bind(tenant_id)
    .bind(service.id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .collect();

    // Telemetry: latest data per signal across all bindings.
    let mut seen: HashMap<&str, (u64, String)> = HashMap::new();
    let mut failed = false;
    if policy.items.iter().any(|i| SIGNALS.contains(&i.as_str())) {
        for b in &bindings {
            match state
                .telemetry
                .seen_since(tenant_id, &b.service_name, &b.environment, since_ns)
                .await
            {
                Ok(s) => {
                    for signal in SIGNALS {
                        if let Some(ns) = s.get(signal)
                            && seen.get(signal).is_none_or(|(prev, _)| ns > *prev)
                        {
                            let env = if b.environment.is_empty() {
                                "*"
                            } else {
                                &b.environment
                            };
                            seen.insert(signal, (ns, format!("{signal}:{}@{env}", b.service_name)));
                        }
                    }
                }
                Err(_) => failed = true,
            }
        }
    }

    // Assets: the newest live asset per kind whose owner is active.
    let assets = asset_views(
        state,
        tenant_id,
        live_assets(&state.db, tenant_id, &[service.id]).await?,
    )
    .await?;

    let items: Vec<ItemResult> = policy
        .items
        .iter()
        .map(|item| {
            let claimed = claims.contains(item);
            let (demonstrated, evidence_ref, mut detail) = if SIGNALS.contains(&item.as_str()) {
                match seen.get(item.as_str()) {
                    Some((ns, evidence)) => (
                        true,
                        Some(evidence.clone()),
                        format!(
                            "last data {}",
                            DateTime::<Utc>::from_timestamp_nanos(*ns as i64).to_rfc3339()
                        ),
                    ),
                    None if failed => (false, None, "telemetry_unavailable".into()),
                    None if bindings.is_empty() => (false, None, "no_telemetry_binding".into()),
                    None => (
                        false,
                        None,
                        format!("no_data_within_{}m", policy.window_minutes),
                    ),
                }
            } else {
                let of_kind: Vec<&AssetView> = assets.iter().filter(|a| a.kind == *item).collect();
                match of_kind.iter().find(|a| a.owner_state == "active") {
                    Some(a) => (
                        true,
                        Some(format!("asset:{}/{}", a.kind, a.external_id)),
                        format!("registered by {}", a.owning_component),
                    ),
                    None if of_kind.is_empty() => (false, None, "no_registered_asset".into()),
                    None => (false, None, "asset_without_active_owner".into()),
                }
            };
            if claimed && !demonstrated {
                detail = format!("claimed_without_evidence; {detail}");
            }
            ItemResult {
                item: item.clone(),
                demonstrated,
                evidence_ref,
                detail,
                claimed,
            }
        })
        .collect();

    let status = status_of(&items, failed);
    let error = failed.then(|| "telemetry_unavailable".to_owned());
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "DELETE FROM catalog.coverage_assessments WHERE tenant_id = $1 AND service_id = $2",
    )
    .bind(tenant_id)
    .bind(service.id)
    .execute(&mut *tx)
    .await?;
    for i in &items {
        sqlx::query(
            "INSERT INTO catalog.coverage_assessments (service_id, tenant_id, item, demonstrated, \
             evidence_ref, detail, assessed_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(service.id)
        .bind(tenant_id)
        .bind(&i.item)
        .bind(i.demonstrated)
        .bind(&i.evidence_ref)
        .bind(&i.detail)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO catalog.coverage_runs (service_id, tenant_id, status, policy_source, window_minutes, error, assessed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (service_id) DO UPDATE SET status = EXCLUDED.status, \
         policy_source = EXCLUDED.policy_source, window_minutes = EXCLUDED.window_minutes, \
         error = EXCLUDED.error, assessed_at = EXCLUDED.assessed_at",
    )
    .bind(service.id)
    .bind(tenant_id)
    .bind(status)
    .bind(&policy.source)
    .bind(policy.window_minutes)
    .bind(&error)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if service.criticality == "critical" && !failed {
        for i in items.iter().filter(|i| !i.demonstrated) {
            open_gap(&mut tx, tenant_id, Some(service.id), None, &i.item).await?;
        }
    }
    tx.commit().await?;

    Ok(CoverageReport {
        service: ServiceSummary::from(service),
        status: status.into(),
        policy: policy.clone(),
        bindings,
        unmet: items
            .iter()
            .filter(|i| !i.demonstrated)
            .map(|i| i.item.clone())
            .collect(),
        items,
        assessed_at: now,
        error,
    })
}

async fn open_gap(
    tx: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    service_id: Option<Uuid>,
    journey_id: Option<Uuid>,
    item: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO catalog.coverage_gaps (tenant_id, service_id, journey_id, item) \
         SELECT $1, $2, $3, $4 WHERE NOT EXISTS (SELECT 1 FROM catalog.coverage_gaps \
         WHERE tenant_id = $1 AND coalesce(service_id, journey_id) = coalesce($2, $3) \
         AND item = $4 AND state <> 'closed')",
    )
    .bind(tenant_id)
    .bind(service_id)
    .bind(journey_id)
    .bind(item)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// GET /v1/catalog/services/{id}/coverage: assesses now and returns the report.
pub async fn service_coverage(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CoverageReport>> {
    let service = readable_service(&state, &caller, id).await?;
    if service.lifecycle == "retired" {
        return Err(ApiError::BadRequest(
            "retired services are not assessed".into(),
        ));
    }
    let all = policies(&state.db, caller.tenant_id).await?;
    let policy = all.get(&service.criticality).ok_or(ApiError::Internal)?;
    Ok(Json(
        assess(&state, caller.tenant_id, &service, policy).await?,
    ))
}

#[derive(Serialize)]
pub struct CoverageSummary {
    #[serde(flatten)]
    service: ServiceSummary,
    /// `covered`, `partial`, `uncovered` or `unknown` (never assessed, stale, or failed).
    coverage_status: String,
    assessed_at: Option<DateTime<Utc>>,
    stale: bool,
}

#[derive(FromRow)]
struct RunRow {
    service_id: Uuid,
    status: String,
    window_minutes: i32,
    assessed_at: DateTime<Utc>,
}

/// Last stored status per service; never assessed or older than twice the policy window is
/// `unknown`.
pub async fn coverage_statuses(
    db: &PgPool,
    tenant_id: Uuid,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, (String, Option<DateTime<Utc>>, bool)>, sqlx::Error> {
    let rows: Vec<RunRow> = sqlx::query_as(
        "SELECT service_id, status, window_minutes, assessed_at FROM catalog.coverage_runs \
         WHERE tenant_id = $1 AND service_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(ids)
    .fetch_all(db)
    .await?;
    let now = Utc::now();
    let mut out: HashMap<Uuid, (String, Option<DateTime<Utc>>, bool)> = ids
        .iter()
        .map(|id| (*id, ("unknown".to_owned(), None, false)))
        .collect();
    for r in rows {
        let stale =
            now - r.assessed_at > chrono::Duration::minutes(2 * i64::from(r.window_minutes));
        let status = if stale {
            "unknown".to_owned()
        } else {
            r.status
        };
        out.insert(r.service_id, (status, Some(r.assessed_at), stale));
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct CoverageQuery {
    status: Option<String>,
}

/// GET /v1/catalog/coverage: coverage status of every non-retired service the caller may read.
pub async fn list_coverage(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<CoverageQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = load_services(&state.db, caller.tenant_id, None).await?;
    let rows: Vec<ServiceRow> = visible(&state, &caller, rows)
        .await?
        .into_iter()
        .filter(|r| r.lifecycle != "retired")
        .collect();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut statuses = coverage_statuses(&state.db, caller.tenant_id, &ids).await?;
    let items: Vec<CoverageSummary> = rows
        .iter()
        .filter_map(|r| {
            let (status, at, stale) = statuses.remove(&r.id)?;
            q.status
                .as_deref()
                .is_none_or(|s| s == status)
                .then(|| CoverageSummary {
                    service: ServiceSummary::from(r),
                    coverage_status: status,
                    assessed_at: at,
                    stale,
                })
        })
        .collect();
    let total = items.len();
    Ok(Json(json!({ "items": items, "total": total })))
}

/// POST /v1/catalog/coverage/assess: assesses every non-retired service the caller may read,
/// then opens gaps for critical journeys that are out of scope or not fully covered.
pub async fn assess_all(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = load_services(&state.db, caller.tenant_id, None).await?;
    let rows: Vec<ServiceRow> = visible(&state, &caller, rows)
        .await?
        .into_iter()
        .filter(|r| r.lifecycle != "retired")
        .collect();
    let all = policies(&state.db, caller.tenant_id).await?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in &rows {
        let policy = all.get(&r.criticality).ok_or(ApiError::Internal)?;
        let report = assess(&state, caller.tenant_id, r, policy).await?;
        *counts.entry(report.status).or_default() += 1;
    }
    let journeys = load_journeys(&state.db, caller.tenant_id, None).await?;
    let journeys = visible_journeys(&state, &caller, journeys).await?;
    for j in journeys.iter().filter(|j| j.criticality == "critical") {
        if !journey_covered(&state.db, caller.tenant_id, j).await? {
            let mut conn = state.db.acquire().await?;
            open_gap(&mut conn, caller.tenant_id, None, Some(j.id), JOURNEY_ITEM).await?;
        }
    }
    Ok(Json(json!({ "assessed": rows.len(), "by_status": counts })))
}

/// A journey is covered when it is in scope, has services, and every service's current
/// coverage status is `covered`.
async fn journey_covered(db: &PgPool, tenant_id: Uuid, j: &JourneyRow) -> ApiResult<bool> {
    if !j.in_scope {
        return Ok(false);
    }
    let services: Vec<Uuid> = sqlx::query_scalar(
        "SELECT service_id FROM catalog.journey_services WHERE tenant_id = $1 AND journey_id = $2",
    )
    .bind(tenant_id)
    .bind(j.id)
    .fetch_all(db)
    .await?;
    if services.is_empty() {
        return Ok(false);
    }
    let statuses = coverage_statuses(db, tenant_id, &services).await?;
    Ok(statuses.values().all(|(s, _, _)| s == "covered"))
}

/// Coverage exception reasons for critical, non-retired services (OBS-27-01, OBS-02-03).
pub async fn coverage_exceptions(
    db: &PgPool,
    tenant_id: Uuid,
    services: &[&ServiceRow],
) -> Result<HashMap<Uuid, Vec<String>>, sqlx::Error> {
    let critical: Vec<Uuid> = services
        .iter()
        .filter(|s| s.criticality == "critical" && s.lifecycle != "retired")
        .map(|s| s.id)
        .collect();
    if critical.is_empty() {
        return Ok(HashMap::new());
    }
    let approved: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM catalog.coverage_policies WHERE tenant_id = $1 \
         AND criticality = 'critical' AND approved_at IS NOT NULL)",
    )
    .bind(tenant_id)
    .fetch_one(db)
    .await?;
    let statuses = coverage_statuses(db, tenant_id, &critical).await?;
    Ok(critical
        .into_iter()
        .map(|id| {
            let mut reasons = Vec::new();
            if !approved {
                reasons.push("no_approved_coverage_policy".to_owned());
            }
            match statuses.get(&id).map(|s| s.0.as_str()) {
                Some("covered") => {}
                Some(s) => reasons.push(format!("coverage_{s}")),
                None => reasons.push("coverage_unknown".to_owned()),
            }
            (id, reasons)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Gaps
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct GapRow {
    id: Uuid,
    service_id: Option<Uuid>,
    journey_id: Option<Uuid>,
    item: String,
    state: String,
    remediation_owner_kind: Option<String>,
    remediation_owner_id: Option<Uuid>,
    due_date: Option<NaiveDate>,
    review_date: Option<NaiveDate>,
    acceptance_approver: Option<Uuid>,
    acceptance_rationale: Option<String>,
    closed_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const GAP_COLUMNS: &str = "id, service_id, journey_id, item, state, remediation_owner_kind, \
     remediation_owner_id, due_date, review_date, acceptance_approver, acceptance_rationale, \
     closed_reason, created_at, updated_at";

#[derive(Deserialize)]
pub struct GapQuery {
    state: Option<String>,
}

/// GET /v1/catalog/gaps: gaps of services and journeys the caller may read.
pub async fn list_gaps(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<GapQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let gaps: Vec<GapRow> = sqlx::query_as::<_, GapRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {GAP_COLUMNS} FROM catalog.coverage_gaps WHERE tenant_id = $1 \
         AND ($2::text IS NULL OR state = $2) ORDER BY created_at, id"
    )))
    .bind(caller.tenant_id)
    .bind(q.state.as_deref())
    .fetch_all(&state.db)
    .await?;
    let services = load_services(&state.db, caller.tenant_id, None).await?;
    let seen_services: HashSet<Uuid> = visible(&state, &caller, services)
        .await?
        .iter()
        .map(|s| s.id)
        .collect();
    let journeys = load_journeys(&state.db, caller.tenant_id, None).await?;
    let seen_journeys: HashSet<Uuid> = visible_journeys(&state, &caller, journeys)
        .await?
        .iter()
        .map(|j| j.id)
        .collect();
    let items: Vec<GapRow> = gaps
        .into_iter()
        .filter(|g| {
            g.service_id.is_some_and(|s| seen_services.contains(&s))
                || g.journey_id.is_some_and(|j| seen_journeys.contains(&j))
        })
        .collect();
    let total = items.len();
    Ok(Json(json!({ "items": items, "total": total })))
}

/// Loads a gap whose service or journey the caller may change; hidden gaps are not found.
async fn writable_gap(state: &CatalogState, caller: &Caller, id: Uuid) -> ApiResult<GapRow> {
    let gap: GapRow = sqlx::query_as::<_, GapRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {GAP_COLUMNS} FROM catalog.coverage_gaps WHERE tenant_id = $1 AND id = $2"
    )))
    .bind(caller.tenant_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    match (gap.service_id, gap.journey_id) {
        (Some(s), _) => {
            writable_service(state, caller, s).await?;
        }
        (None, Some(j)) => {
            writable_journey(state, caller, j).await?;
        }
        _ => return Err(ApiError::Internal),
    }
    Ok(gap)
}

#[derive(Deserialize)]
pub struct RemediateBody {
    owner: PrincipalRef,
    due_date: NaiveDate,
}

/// POST /v1/catalog/gaps/{id}/remediate: assigns a remediation owner and due date.
pub async fn remediate_gap(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<RemediateBody>,
) -> ApiResult<Json<GapRow>> {
    let gap = writable_gap(&state, &caller, id).await?;
    if gap.state == "closed" {
        return Err(ApiError::Conflict("gap is closed".into()));
    }
    check_principal(
        &state,
        &caller,
        &body.owner.principal_kind,
        body.owner.principal_id,
    )
    .await?;
    update_gap(
        &state.db,
        caller.tenant_id,
        id,
        "UPDATE catalog.coverage_gaps SET state = 'remediating', remediation_owner_kind = $3, \
         remediation_owner_id = $4, due_date = $5, updated_at = now() WHERE tenant_id = $1 AND id = $2",
        |q| {
            q.bind(body.owner.principal_kind.clone())
                .bind(body.owner.principal_id)
                .bind(body.due_date)
        },
    )
    .await
}

#[derive(Deserialize)]
pub struct AcceptBody {
    rationale: String,
    review_date: NaiveDate,
}

/// POST /v1/catalog/gaps/{id}/accept: documents a deferral as risk acceptance. A human
/// decision: only a session user may accept, and the review date must be in the future.
pub async fn accept_gap(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<AcceptBody>,
) -> ApiResult<Json<GapRow>> {
    let gap = writable_gap(&state, &caller, id).await?;
    let Some(approver) = caller.user_id else {
        return Err(ApiError::Forbidden);
    };
    if gap.state == "closed" {
        return Err(ApiError::Conflict("gap is closed".into()));
    }
    let rationale = body.rationale.trim().to_owned();
    if rationale.is_empty() || rationale.len() > 2000 {
        return Err(ApiError::BadRequest(
            "rationale must be 1-2000 characters".into(),
        ));
    }
    if body.review_date <= Utc::now().date_naive() {
        return Err(ApiError::BadRequest(
            "review_date must be in the future".into(),
        ));
    }
    update_gap(
        &state.db,
        caller.tenant_id,
        id,
        "UPDATE catalog.coverage_gaps SET state = 'accepted', acceptance_approver = $3, \
         acceptance_rationale = $4, review_date = $5, updated_at = now() WHERE tenant_id = $1 AND id = $2",
        |q| q.bind(approver).bind(rationale.clone()).bind(body.review_date),
    )
    .await
}

/// POST /v1/catalog/gaps/{id}/close: closes a gap only when it is remediated (the item is
/// demonstrated by the current assessment) or under an approved, unexpired risk acceptance.
pub async fn close_gap(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    let gap = writable_gap(&state, &caller, id).await?;
    if gap.state == "closed" {
        return Err(ApiError::Conflict("gap is closed".into()));
    }
    let remediated = match (gap.service_id, gap.journey_id) {
        (Some(s), _) => {
            let statuses = coverage_statuses(&state.db, caller.tenant_id, &[s]).await?;
            let fresh = statuses.get(&s).is_some_and(|(st, _, _)| st != "unknown");
            let demonstrated: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM catalog.coverage_assessments WHERE tenant_id = $1 \
                 AND service_id = $2 AND item = $3 AND demonstrated AND evidence_ref IS NOT NULL)",
            )
            .bind(caller.tenant_id)
            .bind(s)
            .bind(&gap.item)
            .fetch_one(&state.db)
            .await?;
            fresh && demonstrated
        }
        (None, Some(j)) => {
            let journey = load_journeys(&state.db, caller.tenant_id, Some(j))
                .await?
                .pop()
                .ok_or(ApiError::NotFound)?;
            journey_covered(&state.db, caller.tenant_id, &journey).await?
        }
        _ => false,
    };
    let accepted = gap.state == "accepted"
        && gap.acceptance_approver.is_some()
        && gap.review_date.is_some_and(|d| d > Utc::now().date_naive());
    let reason = if remediated {
        "remediated"
    } else if accepted {
        "risk_accepted"
    } else {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "gap_not_closable",
                "message": "A gap closes only when remediated with evidence or under an approved, unexpired risk acceptance."
            })),
        )
            .into_response());
    };
    let row = update_gap(
        &state.db,
        caller.tenant_id,
        id,
        "UPDATE catalog.coverage_gaps SET state = 'closed', closed_reason = $3, updated_at = now() \
         WHERE tenant_id = $1 AND id = $2",
        |q| q.bind(reason),
    )
    .await?;
    Ok(row.into_response())
}

async fn update_gap<'q, F>(
    db: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
    sql: &'static str,
    bind: F,
) -> ApiResult<Json<GapRow>>
where
    F: FnOnce(
        sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
{
    bind(sqlx::query(sql).bind(tenant_id).bind(id))
        .execute(db)
        .await?;
    sqlx::query_as::<_, GapRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {GAP_COLUMNS} FROM catalog.coverage_gaps WHERE tenant_id = $1 AND id = $2"
    )))
    .bind(tenant_id)
    .bind(id)
    .fetch_one(db)
    .await
    .map(Json)
    .map_err(ApiError::from)
}
