//! `manage-service-catalog` HTTP API (phase 1 S3; journeys and contexts in `journeys`): service records, identifiers and aliases,
//! criticality and lifecycle, owners and support route, teams, business outcomes, search and
//! the exception list.
//!
//! Access (req-catalog-access-control): every read goes through `decide-access`. Lists, search
//! and exceptions call `filter` before counting or paginating; single-record reads call
//! `decide`, and a denied record returns the same `404 {"error":"not_found"}` as a missing one.
//! When the decision point is unavailable every read fails closed with
//! `503 {"error":"access_decision_unavailable"}`.

use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::authz::{
    AccessDecider, Classification, DecideRequest, Decision, FilterRequest, ResourceRef,
};
use observable_auth::principals::{PrincipalDirectory, PrincipalState};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::CatalogState;
use crate::ports::Caller;

pub const SERVICE: &str = "catalog.service";
pub const TEAM: &str = "catalog.team";
pub(crate) const READ: &str = "read";
pub(crate) const WRITE: &str = "write";

pub(crate) const CRITICALITY: &[&str] = &["critical", "high", "standard", "low"];
const LIFECYCLE: &[&str] = &["planned", "live", "deprecated", "retired"];
const OWNER_ROLES: &[&str] = &["business_owner", "operational_owner"];

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    Unauthorized,
    /// 422 with a machine-readable error code.
    Unprocessable(&'static str),
    Forbidden,
    BadRequest(String),
    Conflict(String),
    Unavailable,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response()
            }
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response(),
            Self::Unprocessable(code) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": code})),
            )
                .into_response(),
            Self::Forbidden => {
                (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))).into_response()
            }
            Self::BadRequest(m) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_request", "message": m})),
            )
                .into_response(),
            Self::Conflict(m) => (
                StatusCode::CONFLICT,
                Json(json!({"error": "conflict", "message": m})),
            )
                .into_response(),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "access_decision_unavailable"})),
            )
                .into_response(),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response(),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e
            && db.is_unique_violation()
        {
            return Self::Conflict("an identifier is already in use".into());
        }
        tracing::error!(error = %e, "catalog store error");
        Self::Internal
    }
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

pub(crate) fn one_of(field: &str, value: &str, allowed: &[&str]) -> ApiResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "{field} must be one of {}",
            allowed.join(", ")
        )))
    }
}

pub(crate) fn non_empty(field: &str, value: &str) -> ApiResult<String> {
    let v = value.trim();
    if v.is_empty() || v.len() > 200 {
        return Err(ApiError::BadRequest(format!(
            "{field} must be 1-200 characters"
        )));
    }
    Ok(v.to_owned())
}

pub(crate) fn classification(value: &str) -> ApiResult<Classification> {
    Classification::parse(value).ok_or_else(|| {
        ApiError::BadRequest(
            "classification must be one of public, internal, confidential, restricted".into(),
        )
    })
}

// ---------------------------------------------------------------------------
// Access helpers
// ---------------------------------------------------------------------------

pub(crate) async fn decide(
    state: &CatalogState,
    caller: &Caller,
    action: &str,
    resource_type: &str,
    id: &str,
    classification: Option<Classification>,
) -> ApiResult<bool> {
    let req = DecideRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some(action.into()),
        resource: Some(ResourceRef {
            resource_type: resource_type.into(),
            id: id.into(),
            classification,
        }),
        purpose: None,
        elevation_id: None,
    };
    match state.decider.decide(&req).await {
        Ok(Decision::Permit) => Ok(true),
        Ok(Decision::Deny | Decision::PurposeMissing) => Ok(false),
        Err(_) => Err(ApiError::Unavailable),
    }
}

pub(crate) async fn permitted(
    state: &CatalogState,
    caller: &Caller,
    resource_type: &str,
    rows: &[(Uuid, &str)],
) -> ApiResult<HashSet<Uuid>> {
    if rows.is_empty() {
        return Ok(HashSet::new());
    }
    let req = FilterRequest {
        tenant_id: Some(caller.tenant_id),
        principal: Some(caller.principal()),
        action: Some(READ.into()),
        resources: rows
            .iter()
            .map(|(id, c)| ResourceRef {
                resource_type: resource_type.into(),
                id: id.to_string(),
                classification: Classification::parse(c),
            })
            .collect(),
        purpose: None,
        elevation_id: None,
    };
    let keys = state
        .decider
        .filter(&req)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    Ok(keys
        .into_iter()
        .filter(|k| k.resource_type == resource_type)
        .filter_map(|k| k.id.parse().ok())
        .collect())
}

/// The error for a record that does not exist. The policy is still asked, so during a
/// decision-point outage a missing ID fails like an existing one (503) instead of a 404
/// that would reveal which IDs exist.
pub(crate) async fn missing(
    state: &CatalogState,
    caller: &Caller,
    resource_type: &str,
    id: &str,
) -> ApiError {
    match decide(state, caller, READ, resource_type, id, None).await {
        Err(e) => e,
        Ok(_) => ApiError::NotFound,
    }
}

/// Loads a service the caller may read; missing and denied are the same `NotFound`.
pub(crate) async fn readable_service(
    state: &CatalogState,
    caller: &Caller,
    id: Uuid,
) -> ApiResult<ServiceRow> {
    let Some(row) = load_service(&state.db, caller.tenant_id, id).await? else {
        return Err(missing(state, caller, SERVICE, &id.to_string()).await);
    };
    if decide(
        state,
        caller,
        READ,
        SERVICE,
        &id.to_string(),
        Classification::parse(&row.classification),
    )
    .await?
    {
        Ok(row)
    } else {
        Err(ApiError::NotFound)
    }
}

/// A readable service the caller may also change; a readable but read-only record is 403.
pub(crate) async fn writable_service(
    state: &CatalogState,
    caller: &Caller,
    id: Uuid,
) -> ApiResult<ServiceRow> {
    let row = readable_service(state, caller, id).await?;
    if decide(
        state,
        caller,
        WRITE,
        SERVICE,
        &id.to_string(),
        Classification::parse(&row.classification),
    )
    .await?
    {
        Ok(row)
    } else {
        Err(ApiError::Forbidden)
    }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, FromRow)]
pub struct ServiceRow {
    pub id: Uuid,
    pub business_name: String,
    pub operational_name: String,
    pub domain: String,
    pub description: String,
    pub criticality: String,
    pub lifecycle: String,
    pub classification: String,
    pub support_route: String,
    pub operating_hours: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SERVICE_COLUMNS: &str = "id, business_name, operational_name, domain, description, criticality, \
     lifecycle, classification, support_route, operating_hours, created_at, updated_at";

async fn load_service(
    db: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<ServiceRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {SERVICE_COLUMNS} FROM catalog.services WHERE tenant_id = $1 AND id = $2"
    )))
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(db)
    .await
}

pub(crate) async fn load_services(
    db: &PgPool,
    tenant_id: Uuid,
    ids: Option<&[Uuid]>,
) -> Result<Vec<ServiceRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {SERVICE_COLUMNS} FROM catalog.services WHERE tenant_id = $1 \
         AND ($2::uuid[] IS NULL OR id = ANY($2)) ORDER BY lower(business_name), id"
    )))
    .bind(tenant_id)
    .bind(ids)
    .fetch_all(db)
    .await
}

#[derive(Clone, Debug, FromRow)]
struct OwnerRow {
    service_id: Uuid,
    role: String,
    principal_kind: String,
    principal_id: Uuid,
    assigned_at: DateTime<Utc>,
}

#[derive(Clone, Debug, FromRow)]
struct OutcomeRow {
    id: Uuid,
    service_id: Uuid,
    statement: String,
    expectation: String,
    owner_kind: String,
    owner_id: Uuid,
    approved_by: Option<Uuid>,
    approved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ServiceSummary {
    pub id: Uuid,
    pub business_name: String,
    pub operational_name: String,
    pub domain: String,
    pub criticality: String,
    pub lifecycle: String,
    /// `true` only for lifecycle `live`; retired, deprecated and planned services never
    /// present as live.
    pub live: bool,
    pub classification: String,
    /// `true` when the caller (directly or via team membership) is an owner of the service.
    #[serde(default)]
    pub accountable: bool,
}

impl From<&ServiceRow> for ServiceSummary {
    fn from(r: &ServiceRow) -> Self {
        Self {
            id: r.id,
            business_name: r.business_name.clone(),
            operational_name: r.operational_name.clone(),
            domain: r.domain.clone(),
            criticality: r.criticality.clone(),
            lifecycle: r.lifecycle.clone(),
            live: r.lifecycle == "live",
            classification: r.classification.clone(),
            accountable: false,
        }
    }
}

#[derive(Serialize)]
struct AliasView {
    alias: String,
    state: String,
}

#[derive(Serialize, Clone)]
struct OwnerView {
    role: String,
    principal_kind: String,
    principal_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_name: Option<String>,
    /// `active`, `departed` (left the tenant, or a team without active members) or `unknown`
    /// (the principal directory could not resolve it).
    state: String,
    assigned_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct OutcomeView {
    id: Uuid,
    statement: String,
    expectation: String,
    owner_kind: String,
    owner_id: Uuid,
    approved: bool,
    approved_by: Option<Uuid>,
    approved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ServiceDetail {
    #[serde(flatten)]
    summary: ServiceSummary,
    description: String,
    support_route: String,
    operating_hours: String,
    maintenance_windows: Vec<crate::governance::MaintenanceWindowView>,
    aliases: Vec<AliasView>,
    owners: Vec<OwnerView>,
    outcomes: Vec<OutcomeView>,
    /// Exception reasons (empty when the record is complete); see `/v1/catalog/exceptions`.
    exceptions: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Owner resolution and exceptions
// ---------------------------------------------------------------------------

pub(crate) struct Resolution {
    pub(crate) users: HashMap<Uuid, PrincipalState>,
    pub(crate) teams: HashMap<Uuid, (String, Vec<Uuid>)>,
    pub(crate) available: bool,
}

impl Resolution {
    pub(crate) fn state(&self, kind: &str, id: Uuid) -> &'static str {
        if !self.available {
            return "unknown";
        }
        let user = |u: &Uuid| self.users.get(u).copied();
        match kind {
            "user" => match user(&id) {
                Some(PrincipalState::Active) => "active",
                Some(PrincipalState::Departed | PrincipalState::Unknown) => "departed",
                None => "unknown",
            },
            _ => match self.teams.get(&id) {
                Some((_, members))
                    if members
                        .iter()
                        .any(|m| user(m) == Some(PrincipalState::Active)) =>
                {
                    "active"
                }
                Some(_) => "departed",
                None => "departed",
            },
        }
    }
}

pub(crate) async fn resolve_principals(
    state: &CatalogState,
    tenant_id: Uuid,
    owners: &[(String, Uuid)],
) -> ApiResult<Resolution> {
    let team_ids: Vec<Uuid> = owners
        .iter()
        .filter(|(k, _)| k == "team")
        .map(|(_, id)| *id)
        .collect();
    let mut teams: HashMap<Uuid, (String, Vec<Uuid>)> = HashMap::new();
    if !team_ids.is_empty() {
        let rows: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT t.id, t.name, m.user_id FROM catalog.teams t \
             LEFT JOIN catalog.team_members m ON m.team_id = t.id \
             WHERE t.tenant_id = $1 AND t.id = ANY($2)",
        )
        .bind(tenant_id)
        .bind(&team_ids)
        .fetch_all(&state.db)
        .await?;
        for (id, name, member) in rows {
            let entry = teams.entry(id).or_insert_with(|| (name, Vec::new()));
            entry.1.extend(member);
        }
    }
    let mut user_ids: Vec<Uuid> = owners
        .iter()
        .filter(|(k, _)| k == "user")
        .map(|(_, id)| *id)
        .collect();
    user_ids.extend(teams.values().flat_map(|(_, m)| m.iter().copied()));
    let (users, available) = if user_ids.is_empty() {
        (HashMap::new(), true)
    } else {
        match state.directory.resolve(tenant_id, &user_ids).await {
            Ok(r) => (r.into_iter().map(|p| (p.user_id, p.state)).collect(), true),
            Err(_) => (HashMap::new(), false),
        }
    };
    Ok(Resolution {
        users,
        teams,
        available,
    })
}

fn exceptions_for(
    service: &ServiceRow,
    owners: &[OwnerView],
    outcomes: &[OutcomeRow],
) -> Vec<String> {
    if service.criticality != "critical" || service.lifecycle == "retired" {
        return Vec::new();
    }
    let mut reasons = Vec::new();
    for role in OWNER_ROLES {
        match owners.iter().find(|o| o.role == *role) {
            None => reasons.push(format!("missing_{role}")),
            Some(o) if o.state == "departed" => reasons.push(format!("{role}_departed")),
            Some(o) if o.state == "unknown" => reasons.push(format!("{role}_unresolved")),
            Some(_) => {}
        }
    }
    if service.support_route.trim().is_empty() {
        reasons.push("missing_support_route".into());
    }
    if !outcomes.iter().any(|o| o.approved_at.is_some()) {
        reasons.push("no_approved_outcome".into());
    }
    reasons
}

async fn owners_and_outcomes(
    state: &CatalogState,
    tenant_id: Uuid,
    service_ids: &[Uuid],
) -> ApiResult<(
    HashMap<Uuid, Vec<OwnerView>>,
    HashMap<Uuid, Vec<OutcomeRow>>,
)> {
    let owner_rows: Vec<OwnerRow> = sqlx::query_as(
        "SELECT service_id, role, principal_kind, principal_id, assigned_at FROM catalog.service_owners \
         WHERE tenant_id = $1 AND service_id = ANY($2) ORDER BY role",
    )
    .bind(tenant_id)
    .bind(service_ids)
    .fetch_all(&state.db)
    .await?;
    let outcome_rows: Vec<OutcomeRow> = sqlx::query_as(
        "SELECT id, service_id, statement, expectation, owner_kind, owner_id, approved_by, approved_at, created_at \
         FROM catalog.service_outcomes WHERE tenant_id = $1 AND service_id = ANY($2) ORDER BY created_at, id",
    )
    .bind(tenant_id)
    .bind(service_ids)
    .fetch_all(&state.db)
    .await?;
    let principals: Vec<(String, Uuid)> = owner_rows
        .iter()
        .map(|o| (o.principal_kind.clone(), o.principal_id))
        .collect();
    let resolution = resolve_principals(state, tenant_id, &principals).await?;
    let mut owners: HashMap<Uuid, Vec<OwnerView>> = HashMap::new();
    for o in owner_rows {
        owners.entry(o.service_id).or_default().push(OwnerView {
            team_name: (o.principal_kind == "team")
                .then(|| resolution.teams.get(&o.principal_id).map(|t| t.0.clone()))
                .flatten(),
            state: resolution.state(&o.principal_kind, o.principal_id).into(),
            role: o.role,
            principal_kind: o.principal_kind,
            principal_id: o.principal_id,
            assigned_at: o.assigned_at,
        });
    }
    let mut outcomes: HashMap<Uuid, Vec<OutcomeRow>> = HashMap::new();
    for o in outcome_rows {
        outcomes.entry(o.service_id).or_default().push(o);
    }
    Ok((owners, outcomes))
}

async fn detail(
    state: &CatalogState,
    caller: &Caller,
    row: ServiceRow,
) -> ApiResult<ServiceDetail> {
    let aliases: Vec<(String, String)> = sqlx::query_as(
        "SELECT alias, state FROM catalog.service_aliases WHERE tenant_id = $1 AND service_id = $2 \
         ORDER BY state, lower(alias)",
    )
    .bind(caller.tenant_id)
    .bind(row.id)
    .fetch_all(&state.db)
    .await?;
    let (mut owners, mut outcomes) =
        owners_and_outcomes(state, caller.tenant_id, &[row.id]).await?;
    let owners = owners.remove(&row.id).unwrap_or_default();
    let outcomes = outcomes.remove(&row.id).unwrap_or_default();
    let mut exceptions = exceptions_for(&row, &owners, &outcomes);
    exceptions.extend(
        crate::coverage::coverage_exceptions(&state.db, caller.tenant_id, &[&row])
            .await?
            .remove(&row.id)
            .unwrap_or_default(),
    );
    let conflicts = crate::governance::open_conflict_services(&state.db, caller.tenant_id).await?;
    if conflicts.contains(&row.id) {
        exceptions.push("authoritative_owner_conflict".into());
    }
    let maintenance_windows =
        crate::governance::load_maintenance_windows(&state.db, caller.tenant_id, row.id).await?;
    Ok(ServiceDetail {
        summary: ServiceSummary::from(&row),
        description: row.description,
        support_route: row.support_route,
        operating_hours: row.operating_hours,
        maintenance_windows,
        aliases: aliases
            .into_iter()
            .map(|(alias, state)| AliasView { alias, state })
            .collect(),
        owners,
        outcomes: outcomes
            .into_iter()
            .map(|o| OutcomeView {
                id: o.id,
                statement: o.statement,
                expectation: o.expectation,
                owner_kind: o.owner_kind,
                owner_id: o.owner_id,
                approved: o.approved_at.is_some(),
                approved_by: o.approved_by,
                approved_at: o.approved_at,
                created_at: o.created_at,
            })
            .collect(),
        exceptions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

// ---------------------------------------------------------------------------
// Handlers: services
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateService {
    business_name: String,
    operational_name: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    description: String,
    criticality: String,
    #[serde(default = "default_lifecycle")]
    lifecycle: String,
    #[serde(default = "default_classification")]
    classification: String,
    #[serde(default)]
    support_route: String,
    #[serde(default)]
    operating_hours: String,
    #[serde(default)]
    aliases: Vec<String>,
}

fn default_lifecycle() -> String {
    "live".into()
}

fn default_classification() -> String {
    "internal".into()
}

pub async fn create_service(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<CreateService>,
) -> ApiResult<Response> {
    let business_name = non_empty("business_name", &body.business_name)?;
    let operational_name = non_empty("operational_name", &body.operational_name)?;
    one_of("criticality", &body.criticality, CRITICALITY)?;
    one_of("lifecycle", &body.lifecycle, LIFECYCLE)?;
    let class = classification(&body.classification)?;
    if !decide(&state, &caller, WRITE, SERVICE, "new", Some(class)).await? {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO catalog.services (tenant_id, business_name, operational_name, domain, description, \
         criticality, lifecycle, classification, support_route, operating_hours) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(caller.tenant_id)
    .bind(&business_name)
    .bind(&operational_name)
    .bind(body.domain.trim())
    .bind(body.description.trim())
    .bind(&body.criticality)
    .bind(&body.lifecycle)
    .bind(class.as_str())
    .bind(body.support_route.trim())
    .bind(body.operating_hours.trim())
    .fetch_one(&mut *tx)
    .await?;
    for alias in &body.aliases {
        let alias = non_empty("alias", alias)?;
        sqlx::query("INSERT INTO catalog.service_aliases (service_id, tenant_id, alias) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(caller.tenant_id)
            .bind(alias)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let row = load_service(&state.db, caller.tenant_id, id)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok((
        StatusCode::CREATED,
        Json(detail(&state, &caller, row).await?),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct ListQuery {
    lifecycle: Option<String>,
    /// `business_critical` restricts to critical, non-retired services (OBS-01-03).
    context: Option<String>,
    criticality: Option<String>,
    /// When `true`, restricts to services where the caller is an accountable owner (OBS-13-01).
    accountable: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
pub struct Page<T> {
    pub(crate) items: Vec<T>,
    /// Number of permitted records matching the query (never counts denied records).
    pub(crate) total: usize,
}

pub(crate) fn paginate<T>(items: Vec<T>, limit: Option<usize>, offset: Option<usize>) -> Page<T> {
    let total = items.len();
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let items = items
        .into_iter()
        .skip(offset.unwrap_or(0))
        .take(limit)
        .collect();
    Page { items, total }
}

/// Permitted rows in their original order.
pub(crate) async fn visible(
    state: &CatalogState,
    caller: &Caller,
    rows: Vec<ServiceRow>,
) -> ApiResult<Vec<ServiceRow>> {
    let keys: Vec<(Uuid, &str)> = rows
        .iter()
        .map(|r| (r.id, r.classification.as_str()))
        .collect();
    let allowed = permitted(state, caller, SERVICE, &keys).await?;
    Ok(rows
        .into_iter()
        .filter(|r| allowed.contains(&r.id))
        .collect())
}

async fn accountable_service_ids(
    db: &PgPool,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
) -> Result<HashSet<Uuid>, sqlx::Error> {
    let Some(user_id) = user_id else {
        return Ok(HashSet::new());
    };
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT service_id FROM catalog.service_owners \
         WHERE tenant_id = $1 AND ( \
           (principal_kind = 'user' AND principal_id = $2) \
           OR (principal_kind = 'team' AND principal_id IN ( \
             SELECT team_id FROM catalog.team_members WHERE user_id = $2 \
           )) \
         )",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(ids.into_iter().collect())
}

pub async fn list_services(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Page<ServiceSummary>>> {
    let critical_only = crate::journeys::catalog_context(q.context.as_deref())?;
    let rows = load_services(&state.db, caller.tenant_id, None).await?;
    let rows = visible(&state, &caller, rows).await?;
    let accountable_ids =
        accountable_service_ids(&state.db, caller.tenant_id, caller.user_id).await?;
    let only_accountable = q.accountable == Some(true);
    let items: Vec<ServiceSummary> = rows
        .iter()
        .filter(|r| !critical_only || (r.criticality == "critical" && r.lifecycle != "retired"))
        .filter(|r| !only_accountable || accountable_ids.contains(&r.id))
        .filter(|r| q.lifecycle.as_deref().is_none_or(|l| r.lifecycle == l))
        .filter(|r| q.criticality.as_deref().is_none_or(|c| r.criticality == c))
        .map(|r| {
            let mut s = ServiceSummary::from(r);
            s.accountable = accountable_ids.contains(&r.id);
            s
        })
        .collect();
    Ok(Json(paginate(items, q.limit, q.offset)))
}

pub async fn get_service(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = readable_service(&state, &caller, id).await?;
    Ok(Json(
        serde_json::to_value(detail(&state, &caller, row).await?)
            .map_err(|_| ApiError::Internal)?,
    ))
}

#[derive(Deserialize)]
pub struct UpdateService {
    business_name: Option<String>,
    operational_name: Option<String>,
    domain: Option<String>,
    description: Option<String>,
    criticality: Option<String>,
    lifecycle: Option<String>,
    classification: Option<String>,
    support_route: Option<String>,
    operating_hours: Option<String>,
    #[serde(default)]
    add_aliases: Vec<String>,
    #[serde(default)]
    retire_aliases: Vec<String>,
}

pub async fn update_service(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateService>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = writable_service(&state, &caller, id).await?;
    let business_name = body
        .business_name
        .as_deref()
        .map(|v| non_empty("business_name", v))
        .transpose()?;
    let operational_name = body
        .operational_name
        .as_deref()
        .map(|v| non_empty("operational_name", v))
        .transpose()?;
    if let Some(c) = body.criticality.as_deref() {
        one_of("criticality", c, CRITICALITY)?;
    }
    if let Some(l) = body.lifecycle.as_deref() {
        one_of("lifecycle", l, LIFECYCLE)?;
    }
    let class = body
        .classification
        .as_deref()
        .map(classification)
        .transpose()?;
    if let Some(c) = class
        && c.as_str() != row.classification
        && !decide(&state, &caller, WRITE, SERVICE, &id.to_string(), Some(c)).await?
    {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE catalog.services SET business_name = COALESCE($3, business_name), \
         operational_name = COALESCE($4, operational_name), domain = COALESCE($5, domain), \
         description = COALESCE($6, description), criticality = COALESCE($7, criticality), \
         lifecycle = COALESCE($8, lifecycle), classification = COALESCE($9, classification), \
         support_route = COALESCE($10, support_route), \
         operating_hours = COALESCE($11, operating_hours), updated_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(business_name)
    .bind(operational_name)
    .bind(body.domain.as_deref().map(str::trim))
    .bind(body.description.as_deref().map(str::trim))
    .bind(body.criticality.as_deref())
    .bind(body.lifecycle.as_deref())
    .bind(class.map(Classification::as_str))
    .bind(body.support_route.as_deref().map(str::trim))
    .bind(body.operating_hours.as_deref().map(str::trim))
    .execute(&mut *tx)
    .await?;
    for alias in &body.retire_aliases {
        sqlx::query(
            "UPDATE catalog.service_aliases SET state = 'retired', retired_at = now() \
             WHERE tenant_id = $1 AND service_id = $2 AND lower(alias) = lower($3) AND state = 'active'",
        )
        .bind(caller.tenant_id)
        .bind(id)
        .bind(alias.trim())
        .execute(&mut *tx)
        .await?;
    }
    for alias in &body.add_aliases {
        let alias = non_empty("alias", alias)?;
        sqlx::query(
            "INSERT INTO catalog.service_aliases (service_id, tenant_id, alias) VALUES ($1, $2, $3) \
             ON CONFLICT (service_id, alias) DO UPDATE SET state = 'active', retired_at = NULL",
        )
        .bind(id)
        .bind(caller.tenant_id)
        .bind(alias)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let row = load_service(&state.db, caller.tenant_id, id)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok(Json(
        serde_json::to_value(detail(&state, &caller, row).await?)
            .map_err(|_| ApiError::Internal)?,
    ))
}

// ---------------------------------------------------------------------------
// Owners and outcomes
// ---------------------------------------------------------------------------

#[derive(Clone, Deserialize)]
pub struct PrincipalRef {
    pub(crate) principal_kind: String,
    pub(crate) principal_id: Uuid,
}

/// Validates an owner principal: a team of this tenant, or a user who is active in it.
pub(crate) async fn check_principal(
    state: &CatalogState,
    caller: &Caller,
    kind: &str,
    id: Uuid,
) -> ApiResult<()> {
    match kind {
        "team" => {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM catalog.teams WHERE tenant_id = $1 AND id = $2)",
            )
            .bind(caller.tenant_id)
            .bind(id)
            .fetch_one(&state.db)
            .await?;
            if exists {
                Ok(())
            } else {
                Err(ApiError::BadRequest("unknown team".into()))
            }
        }
        "user" => match state.directory.resolve(caller.tenant_id, &[id]).await {
            Ok(r) if r.first().is_some_and(|p| p.state == PrincipalState::Active) => Ok(()),
            Ok(_) => Err(ApiError::BadRequest(
                "owner must be an active user of this tenant".into(),
            )),
            Err(_) => Err(ApiError::Unavailable),
        },
        _ => Err(ApiError::BadRequest(
            "principal_kind must be one of user, team".into(),
        )),
    }
}

pub async fn put_owner(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path((id, role)): Path<(Uuid, String)>,
    Json(body): Json<PrincipalRef>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = writable_service(&state, &caller, id).await?;
    one_of("role", &role, OWNER_ROLES)?;
    check_principal(&state, &caller, &body.principal_kind, body.principal_id).await?;
    sqlx::query(
        "INSERT INTO catalog.service_owners (service_id, tenant_id, role, principal_kind, principal_id) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (service_id, role) DO UPDATE \
         SET principal_kind = EXCLUDED.principal_kind, principal_id = EXCLUDED.principal_id, assigned_at = now()",
    )
    .bind(id)
    .bind(caller.tenant_id)
    .bind(&role)
    .bind(&body.principal_kind)
    .bind(body.principal_id)
    .execute(&state.db)
    .await?;
    Ok(Json(
        serde_json::to_value(detail(&state, &caller, row).await?)
            .map_err(|_| ApiError::Internal)?,
    ))
}

pub async fn delete_owner(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path((id, role)): Path<(Uuid, String)>,
) -> ApiResult<StatusCode> {
    writable_service(&state, &caller, id).await?;
    one_of("role", &role, OWNER_ROLES)?;
    sqlx::query(
        "DELETE FROM catalog.service_owners WHERE tenant_id = $1 AND service_id = $2 AND role = $3",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(&role)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct CreateOutcome {
    statement: String,
    #[serde(default)]
    expectation: String,
    owner: PrincipalRef,
}

pub async fn create_outcome(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateOutcome>,
) -> ApiResult<Response> {
    writable_service(&state, &caller, id).await?;
    let statement = body.statement.trim();
    if statement.is_empty() || statement.len() > 2000 {
        return Err(ApiError::BadRequest(
            "statement must be 1-2000 characters".into(),
        ));
    }
    check_principal(
        &state,
        &caller,
        &body.owner.principal_kind,
        body.owner.principal_id,
    )
    .await?;
    let outcome_id: Uuid = sqlx::query_scalar(
        "INSERT INTO catalog.service_outcomes (service_id, tenant_id, statement, expectation, owner_kind, owner_id) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(id)
    .bind(caller.tenant_id)
    .bind(statement)
    .bind(body.expectation.trim())
    .bind(&body.owner.principal_kind)
    .bind(body.owner.principal_id)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": outcome_id}))).into_response())
}

/// Approval is a human decision: only a session user may approve, and is recorded as approver.
pub async fn approve_outcome(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path((id, outcome_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    writable_service(&state, &caller, id).await?;
    let Some(approver) = caller.user_id else {
        return Err(ApiError::Forbidden);
    };
    let updated = sqlx::query(
        "UPDATE catalog.service_outcomes SET approved_by = $4, approved_at = now() \
         WHERE tenant_id = $1 AND service_id = $2 AND id = $3",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(outcome_id)
    .bind(approver)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Search and exceptions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
pub struct SearchHit {
    #[serde(flatten)]
    service: ServiceSummary,
    matched_on: Vec<String>,
}

#[derive(Serialize)]
pub struct SearchResult {
    items: Vec<SearchHit>,
    total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'static str>,
}

fn like_prefix(q: &str) -> String {
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{}%", escaped.to_lowercase())
}

/// Matches business name, operational name or active alias by case-insensitive prefix, domain
/// by exact name, and owner by team name or user id. Retired aliases never match.
pub async fn search(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<SearchResult>> {
    let term = q.q.trim();
    if term.chars().count() < 2 || term.len() > 200 {
        return Err(ApiError::BadRequest("q must be 2-200 characters".into()));
    }
    let owner_user = term.parse::<Uuid>().ok();
    let hits: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, 'business_name' FROM catalog.services WHERE tenant_id = $1 AND lower(business_name) LIKE $2 \
         UNION ALL SELECT id, 'operational_name' FROM catalog.services WHERE tenant_id = $1 AND lower(operational_name) LIKE $2 \
         UNION ALL SELECT id, 'domain' FROM catalog.services WHERE tenant_id = $1 AND domain <> '' AND lower(domain) = lower($3) \
         UNION ALL SELECT service_id, 'alias' FROM catalog.service_aliases WHERE tenant_id = $1 AND state = 'active' AND lower(alias) LIKE $2 \
         UNION ALL SELECT o.service_id, 'owner' FROM catalog.service_owners o JOIN catalog.teams t ON t.id = o.principal_id \
           WHERE o.tenant_id = $1 AND o.principal_kind = 'team' AND lower(t.name) = lower($3) \
         UNION ALL SELECT service_id, 'owner' FROM catalog.service_owners WHERE tenant_id = $1 AND principal_kind = 'user' AND principal_id = $4",
    )
    .bind(caller.tenant_id)
    .bind(like_prefix(term))
    .bind(term)
    .bind(owner_user)
    .fetch_all(&state.db)
    .await?;
    let mut matched: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (id, field) in hits {
        let fields = matched.entry(id).or_default();
        if !fields.contains(&field) {
            fields.push(field);
        }
    }
    let ids: Vec<Uuid> = matched.keys().copied().collect();
    let rows = if ids.is_empty() {
        Vec::new()
    } else {
        load_services(&state.db, caller.tenant_id, Some(&ids)).await?
    };
    let rows = visible(&state, &caller, rows).await?;
    let items: Vec<SearchHit> = rows
        .iter()
        .map(|r| SearchHit {
            service: ServiceSummary::from(r),
            matched_on: matched.remove(&r.id).unwrap_or_default(),
        })
        .collect();
    let page = paginate(items, q.limit, q.offset);
    let message = (page.total == 0).then_some("No service matches this identifier.");
    Ok(Json(SearchResult {
        items: page.items,
        total: page.total,
        message,
    }))
}

#[derive(Serialize)]
pub struct ExceptionItem {
    #[serde(flatten)]
    service: ServiceSummary,
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation_owner: Option<OwnerView>,
}

#[derive(Serialize)]
pub struct Exceptions {
    items: Vec<ExceptionItem>,
    total: usize,
    /// Journey exceptions (OBS-01-01), see `journeys::journey_exceptions_for`.
    journeys: Vec<crate::journeys::JourneyException>,
    /// Observed services/components/providers not registered in the catalog (OBS-02-07).
    unregistered_observed: Vec<crate::governance::UnregisteredObservedItem>,
}

/// Critical, non-retired services without accountable owners (missing, departed or
/// unresolvable), support route, approved outcome (OBS-01-02, OBS-02-02), approved coverage
/// policy or full coverage (OBS-27-01, OBS-02-03), or with a function depending on an
/// unregistered provider (OBS-36-01), plus journey exceptions (OBS-01-01),
/// authoritative owner conflicts (OBS-02-08), and unregistered observed components (OBS-02-07).
pub async fn exceptions(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
) -> ApiResult<Json<Exceptions>> {
    let rows = load_services(&state.db, caller.tenant_id, None).await?;
    let rows: Vec<ServiceRow> = visible(&state, &caller, rows)
        .await?
        .into_iter()
        .filter(|r| r.criticality == "critical" && r.lifecycle != "retired")
        .collect();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let (mut owners, mut outcomes) = owners_and_outcomes(&state, caller.tenant_id, &ids).await?;
    let refs: Vec<&ServiceRow> = rows.iter().collect();
    let mut coverage =
        crate::coverage::coverage_exceptions(&state.db, caller.tenant_id, &refs).await?;
    let unregistered = crate::providers::unregistered_provider_services(&state, &caller).await?;
    let conflicts = crate::governance::open_conflict_services(&state.db, caller.tenant_id).await?;
    let items: Vec<ExceptionItem> = rows
        .iter()
        .filter_map(|r| {
            let service_owners = owners.remove(&r.id).unwrap_or_default();
            let mut reasons = exceptions_for(
                r,
                &service_owners,
                &outcomes.remove(&r.id).unwrap_or_default(),
            );
            reasons.extend(coverage.remove(&r.id).unwrap_or_default());
            if unregistered.contains(&r.id) {
                reasons.push("unregistered_provider".into());
            }
            if conflicts.contains(&r.id) {
                reasons.push("authoritative_owner_conflict".into());
            }
            let remediation_owner = service_owners
                .iter()
                .find(|o| o.role == "operational_owner" && o.state == "active")
                .or_else(|| service_owners.iter().find(|o| o.state == "active"))
                .cloned();
            (!reasons.is_empty()).then(|| ExceptionItem {
                service: ServiceSummary::from(r),
                reasons,
                remediation_owner,
            })
        })
        .collect();
    let page = paginate(items, Some(200), None);
    let unregistered_observed =
        crate::governance::load_unregistered_observed(&state.db, caller.tenant_id).await?;
    Ok(Json(Exceptions {
        items: page.items,
        total: page.total,
        journeys: crate::journeys::journey_exceptions_for(&state, &caller).await?,
        unregistered_observed,
    }))
}

// ---------------------------------------------------------------------------
// Teams
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TeamBody {
    name: Option<String>,
    #[serde(default)]
    members: Vec<Uuid>,
}

#[derive(Serialize)]
struct TeamView {
    id: Uuid,
    name: String,
    members: Vec<Uuid>,
}

async fn may_write_teams(state: &CatalogState, caller: &Caller) -> ApiResult<()> {
    if decide(
        state,
        caller,
        WRITE,
        TEAM,
        "teams",
        Some(Classification::Internal),
    )
    .await?
    {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn replace_members(
    db: &mut sqlx::PgConnection,
    team_id: Uuid,
    members: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM catalog.team_members WHERE team_id = $1")
        .bind(team_id)
        .execute(&mut *db)
        .await?;
    sqlx::query("INSERT INTO catalog.team_members (team_id, user_id) SELECT $1, unnest($2::uuid[]) ON CONFLICT DO NOTHING")
        .bind(team_id)
        .bind(members)
        .execute(&mut *db)
        .await?;
    Ok(())
}

pub async fn create_team(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<TeamBody>,
) -> ApiResult<Response> {
    may_write_teams(&state, &caller).await?;
    let name = non_empty("name", body.name.as_deref().unwrap_or_default())?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO catalog.teams (tenant_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(caller.tenant_id)
    .bind(&name)
    .fetch_one(&mut *tx)
    .await?;
    replace_members(&mut tx, id, &body.members).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(TeamView {
            id,
            name,
            members: body.members,
        }),
    )
        .into_response())
}

pub async fn list_teams(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
) -> ApiResult<Json<Page<serde_json::Value>>> {
    if !decide(
        &state,
        &caller,
        READ,
        TEAM,
        "teams",
        Some(Classification::Internal),
    )
    .await?
    {
        return Ok(Json(Page {
            items: vec![],
            total: 0,
        }));
    }
    let rows: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT t.id, t.name, m.user_id FROM catalog.teams t LEFT JOIN catalog.team_members m ON m.team_id = t.id \
         WHERE t.tenant_id = $1 ORDER BY lower(t.name), t.id, m.user_id",
    )
    .bind(caller.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let mut teams: Vec<TeamView> = Vec::new();
    for (id, name, member) in rows {
        match teams.last_mut() {
            Some(t) if t.id == id => t.members.extend(member),
            _ => teams.push(TeamView {
                id,
                name,
                members: member.into_iter().collect(),
            }),
        }
    }
    let items = teams
        .into_iter()
        .map(|t| serde_json::to_value(t).unwrap_or_default())
        .collect();
    Ok(Json(paginate(items, Some(200), None)))
}

pub async fn put_team_members(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<TeamBody>,
) -> ApiResult<StatusCode> {
    may_write_teams(&state, &caller).await?;
    let mut tx = state.db.begin().await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM catalog.teams WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(ApiError::NotFound);
    }
    replace_members(&mut tx, id, &body.members).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
