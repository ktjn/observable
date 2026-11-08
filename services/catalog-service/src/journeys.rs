//! Critical customer journeys, observability scope and operating contexts (phase 1 S4).
//!
//! - Journeys (OBS-01-01): name, criticality, accountable owner, the services they depend on and
//!   whether they are in observability scope. An out-of-scope journey needs a rationale, and a
//!   critical journey left out of scope is always an exception: it is never silently treated as
//!   covered.
//! - Scope: the in-scope services and journeys with their criticality.
//! - Contexts (OBS-01-03): production and non-production come from the environment classes of
//!   `manage-control-plane`; business-critical comes from service criticality. An unclassified
//!   environment belongs to no context.
//!
//! Access follows `api`: `decide-access` on `catalog.journey` and `catalog.service`; hidden and
//! missing journeys return the same 404; an unavailable decision point fails closed.

use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_auth::authz::Classification;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::CatalogState;
use crate::api::{
    ApiError, ApiResult, CRITICALITY, Page, PrincipalRef, READ, ServiceRow, ServiceSummary, WRITE,
    check_principal, classification, decide, load_services, non_empty, one_of, paginate, permitted,
    resolve_principals, visible,
};
use crate::ports::Caller;

pub const JOURNEY: &str = "catalog.journey";
pub const BUSINESS_CRITICAL: &str = "business_critical";
const CONTEXTS: &[&str] = &["production", "non_production", BUSINESS_CRITICAL];

#[derive(Clone, Debug, FromRow)]
pub struct JourneyRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub criticality: String,
    pub in_scope: bool,
    pub scope_rationale: String,
    pub classification: String,
    pub owner_kind: Option<String>,
    pub owner_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const JOURNEY_COLUMNS: &str = "id, name, description, criticality, in_scope, scope_rationale, \
     classification, owner_kind, owner_id, created_at, updated_at";

pub(crate) async fn load_journeys(
    db: &PgPool,
    tenant_id: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<JourneyRow>, sqlx::Error> {
    sqlx::query_as::<_, JourneyRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {JOURNEY_COLUMNS} FROM catalog.journeys WHERE tenant_id = $1 \
         AND ($2::uuid IS NULL OR id = $2) ORDER BY lower(name), id"
    )))
    .bind(tenant_id)
    .bind(id)
    .fetch_all(db)
    .await
}

/// `(journey_id, service_id)` links in step order.
async fn load_links(
    db: &PgPool,
    tenant_id: Uuid,
    journey_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<Uuid>>, sqlx::Error> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT journey_id, service_id FROM catalog.journey_services \
         WHERE tenant_id = $1 AND journey_id = ANY($2) ORDER BY journey_id, step",
    )
    .bind(tenant_id)
    .bind(journey_ids)
    .fetch_all(db)
    .await?;
    let mut links: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (j, s) in rows {
        links.entry(j).or_default().push(s);
    }
    Ok(links)
}

pub(crate) async fn visible_journeys(
    state: &CatalogState,
    caller: &Caller,
    rows: Vec<JourneyRow>,
) -> ApiResult<Vec<JourneyRow>> {
    let keys: Vec<(Uuid, &str)> = rows
        .iter()
        .map(|r| (r.id, r.classification.as_str()))
        .collect();
    let allowed = permitted(state, caller, JOURNEY, &keys).await?;
    Ok(rows
        .into_iter()
        .filter(|r| allowed.contains(&r.id))
        .collect())
}

pub(crate) async fn readable_journey(
    state: &CatalogState,
    caller: &Caller,
    id: Uuid,
) -> ApiResult<JourneyRow> {
    let row = load_journeys(&state.db, caller.tenant_id, Some(id))
        .await?
        .pop()
        .ok_or(ApiError::NotFound)?;
    if decide(
        state,
        caller,
        READ,
        JOURNEY,
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

pub(crate) async fn writable_journey(
    state: &CatalogState,
    caller: &Caller,
    id: Uuid,
) -> ApiResult<JourneyRow> {
    let row = readable_journey(state, caller, id).await?;
    if decide(
        state,
        caller,
        WRITE,
        JOURNEY,
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

/// Every referenced service must exist and be readable by the caller; hidden and missing
/// services are rejected alike so a write cannot probe for hidden records.
async fn check_services(state: &CatalogState, caller: &Caller, ids: &[Uuid]) -> ApiResult<()> {
    let distinct: HashSet<Uuid> = ids.iter().copied().collect();
    if distinct.len() != ids.len() {
        return Err(ApiError::BadRequest("service_ids must be distinct".into()));
    }
    if ids.is_empty() {
        return Ok(());
    }
    let rows = load_services(&state.db, caller.tenant_id, Some(ids)).await?;
    let rows = visible(state, caller, rows).await?;
    if rows.len() != ids.len() {
        return Err(ApiError::BadRequest("unknown service".into()));
    }
    Ok(())
}

async fn replace_links(
    tx: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    journey_id: Uuid,
    service_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM catalog.journey_services WHERE journey_id = $1")
        .bind(journey_id)
        .execute(&mut *tx)
        .await?;
    for (step, service_id) in service_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO catalog.journey_services (journey_id, service_id, tenant_id, step) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(journey_id)
        .bind(service_id)
        .bind(tenant_id)
        .bind(step as i32)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Wire types and exceptions
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct JourneySummary {
    pub id: Uuid,
    pub name: String,
    pub criticality: String,
    pub in_scope: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scope_rationale: String,
    pub classification: String,
}

impl From<&JourneyRow> for JourneySummary {
    fn from(r: &JourneyRow) -> Self {
        Self {
            id: r.id,
            name: r.name.clone(),
            criticality: r.criticality.clone(),
            in_scope: r.in_scope,
            scope_rationale: r.scope_rationale.clone(),
            classification: r.classification.clone(),
        }
    }
}

#[derive(Serialize)]
struct JourneyOwner {
    principal_kind: String,
    principal_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_name: Option<String>,
    state: String,
}

#[derive(Serialize)]
struct JourneyDetail {
    #[serde(flatten)]
    summary: JourneySummary,
    description: String,
    owner: Option<JourneyOwner>,
    /// Services the caller may read, in journey order.
    services: Vec<ServiceSummary>,
    /// `true` when the journey depends on services the caller may not see; they are never
    /// identified or counted.
    services_withheld: bool,
    exceptions: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Exception reasons for a journey. `services` are all linked services (also hidden ones, which
/// only contribute a reason, never an identifier).
fn journey_exceptions(
    j: &JourneyRow,
    services: &[&ServiceRow],
    owner_state: Option<&str>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if !j.in_scope {
        if j.criticality == "critical" {
            reasons.push("critical_journey_not_in_scope".into());
        }
        return reasons;
    }
    if services.is_empty() {
        reasons.push("no_services".into());
    }
    if services.iter().any(|s| s.lifecycle == "retired") {
        reasons.push("retired_service".into());
    }
    if j.criticality == "critical" {
        match owner_state {
            None => reasons.push("missing_owner".into()),
            Some("departed") => reasons.push("owner_departed".into()),
            Some("unknown") => reasons.push("owner_unresolved".into()),
            Some(_) => {}
        }
    }
    reasons
}

struct JourneyFacts {
    links: HashMap<Uuid, Vec<Uuid>>,
    services: HashMap<Uuid, ServiceRow>,
    owners: HashMap<Uuid, JourneyOwner>,
}

async fn facts(
    state: &CatalogState,
    tenant_id: Uuid,
    journeys: &[JourneyRow],
) -> ApiResult<JourneyFacts> {
    let ids: Vec<Uuid> = journeys.iter().map(|j| j.id).collect();
    let links = load_links(&state.db, tenant_id, &ids).await?;
    let service_ids: Vec<Uuid> = links.values().flatten().copied().collect();
    let services = if service_ids.is_empty() {
        HashMap::new()
    } else {
        load_services(&state.db, tenant_id, Some(&service_ids))
            .await?
            .into_iter()
            .map(|s| (s.id, s))
            .collect()
    };
    let principals: Vec<(String, Uuid)> = journeys
        .iter()
        .filter_map(|j| Some((j.owner_kind.clone()?, j.owner_id?)))
        .collect();
    let resolution = resolve_principals(state, tenant_id, &principals).await?;
    let owners = journeys
        .iter()
        .filter_map(|j| {
            let (kind, id) = (j.owner_kind.clone()?, j.owner_id?);
            Some((
                j.id,
                JourneyOwner {
                    team_name: (kind == "team")
                        .then(|| resolution.teams.get(&id).map(|t| t.0.clone()))
                        .flatten(),
                    state: resolution.state(&kind, id).into(),
                    principal_kind: kind,
                    principal_id: id,
                },
            ))
        })
        .collect();
    Ok(JourneyFacts {
        links,
        services,
        owners,
    })
}

impl JourneyFacts {
    fn services_of(&self, journey: Uuid) -> Vec<&ServiceRow> {
        self.links
            .get(&journey)
            .map(|ids| ids.iter().filter_map(|s| self.services.get(s)).collect())
            .unwrap_or_default()
    }

    fn exceptions(&self, j: &JourneyRow) -> Vec<String> {
        journey_exceptions(
            j,
            &self.services_of(j.id),
            self.owners.get(&j.id).map(|o| o.state.as_str()),
        )
    }
}

async fn journey_detail(
    state: &CatalogState,
    caller: &Caller,
    row: JourneyRow,
) -> ApiResult<JourneyDetail> {
    let mut f = facts(state, caller.tenant_id, std::slice::from_ref(&row)).await?;
    let linked: Vec<ServiceRow> = f.services_of(row.id).into_iter().cloned().collect();
    let shown = visible(state, caller, linked.clone()).await?;
    let exceptions = f.exceptions(&row);
    Ok(JourneyDetail {
        summary: JourneySummary::from(&row),
        description: row.description,
        owner: f.owners.remove(&row.id),
        services_withheld: shown.len() != linked.len(),
        services: shown.iter().map(ServiceSummary::from).collect(),
        exceptions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

// ---------------------------------------------------------------------------
// Handlers: journeys
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateJourney {
    name: String,
    #[serde(default)]
    description: String,
    criticality: String,
    #[serde(default = "default_true")]
    in_scope: bool,
    #[serde(default)]
    scope_rationale: String,
    #[serde(default = "default_classification")]
    classification: String,
    owner: Option<PrincipalRef>,
    #[serde(default)]
    service_ids: Vec<Uuid>,
}

fn default_true() -> bool {
    true
}

fn default_classification() -> String {
    "internal".into()
}

fn check_scope(in_scope: bool, rationale: &str) -> ApiResult<()> {
    if !in_scope && rationale.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "scope_rationale is required when in_scope is false".into(),
        ));
    }
    Ok(())
}

pub async fn create_journey(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<CreateJourney>,
) -> ApiResult<Response> {
    let name = non_empty("name", &body.name)?;
    one_of("criticality", &body.criticality, CRITICALITY)?;
    check_scope(body.in_scope, &body.scope_rationale)?;
    let class = classification(&body.classification)?;
    if !decide(&state, &caller, WRITE, JOURNEY, "new", Some(class)).await? {
        return Err(ApiError::Forbidden);
    }
    if let Some(o) = &body.owner {
        check_principal(&state, &caller, &o.principal_kind, o.principal_id).await?;
    }
    check_services(&state, &caller, &body.service_ids).await?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO catalog.journeys (tenant_id, name, description, criticality, in_scope, \
         scope_rationale, classification, owner_kind, owner_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(caller.tenant_id)
    .bind(&name)
    .bind(body.description.trim())
    .bind(&body.criticality)
    .bind(body.in_scope)
    .bind(body.scope_rationale.trim())
    .bind(class.as_str())
    .bind(body.owner.as_ref().map(|o| o.principal_kind.as_str()))
    .bind(body.owner.as_ref().map(|o| o.principal_id))
    .fetch_one(&mut *tx)
    .await?;
    replace_links(&mut tx, caller.tenant_id, id, &body.service_ids).await?;
    tx.commit().await?;
    let row = readable_journey(&state, &caller, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(journey_detail(&state, &caller, row).await?),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct JourneyQuery {
    criticality: Option<String>,
    in_scope: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
}

pub async fn list_journeys(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<JourneyQuery>,
) -> ApiResult<Json<Page<JourneySummary>>> {
    let rows = load_journeys(&state.db, caller.tenant_id, None).await?;
    let rows = visible_journeys(&state, &caller, rows).await?;
    let items: Vec<JourneySummary> = rows
        .iter()
        .filter(|r| q.criticality.as_deref().is_none_or(|c| r.criticality == c))
        .filter(|r| q.in_scope.is_none_or(|s| r.in_scope == s))
        .map(JourneySummary::from)
        .collect();
    Ok(Json(paginate(items, q.limit, q.offset)))
}

pub async fn get_journey(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = readable_journey(&state, &caller, id).await?;
    serde_json::to_value(journey_detail(&state, &caller, row).await?)
        .map(Json)
        .map_err(|_| ApiError::Internal)
}

#[derive(Deserialize)]
pub struct UpdateJourney {
    name: Option<String>,
    description: Option<String>,
    criticality: Option<String>,
    in_scope: Option<bool>,
    scope_rationale: Option<String>,
    classification: Option<String>,
    owner: Option<PrincipalRef>,
    service_ids: Option<Vec<Uuid>>,
}

pub async fn update_journey(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateJourney>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = writable_journey(&state, &caller, id).await?;
    let name = body
        .name
        .as_deref()
        .map(|v| non_empty("name", v))
        .transpose()?;
    if let Some(c) = body.criticality.as_deref() {
        one_of("criticality", c, CRITICALITY)?;
    }
    check_scope(
        body.in_scope.unwrap_or(row.in_scope),
        body.scope_rationale
            .as_deref()
            .unwrap_or(&row.scope_rationale),
    )?;
    let class = body
        .classification
        .as_deref()
        .map(classification)
        .transpose()?;
    if let Some(c) = class
        && c.as_str() != row.classification
        && !decide(&state, &caller, WRITE, JOURNEY, &id.to_string(), Some(c)).await?
    {
        return Err(ApiError::Forbidden);
    }
    if let Some(o) = &body.owner {
        check_principal(&state, &caller, &o.principal_kind, o.principal_id).await?;
    }
    if let Some(ids) = &body.service_ids {
        check_services(&state, &caller, ids).await?;
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE catalog.journeys SET name = COALESCE($3, name), description = COALESCE($4, description), \
         criticality = COALESCE($5, criticality), in_scope = COALESCE($6, in_scope), \
         scope_rationale = COALESCE($7, scope_rationale), classification = COALESCE($8, classification), \
         owner_kind = COALESCE($9, owner_kind), owner_id = COALESCE($10, owner_id), updated_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(name)
    .bind(body.description.as_deref().map(str::trim))
    .bind(body.criticality.as_deref())
    .bind(body.in_scope)
    .bind(body.scope_rationale.as_deref().map(str::trim))
    .bind(class.map(Classification::as_str))
    .bind(body.owner.as_ref().map(|o| o.principal_kind.as_str()))
    .bind(body.owner.as_ref().map(|o| o.principal_id))
    .execute(&mut *tx)
    .await?;
    if let Some(ids) = &body.service_ids {
        replace_links(&mut tx, caller.tenant_id, id, ids).await?;
    }
    tx.commit().await?;
    let row = readable_journey(&state, &caller, id).await?;
    serde_json::to_value(journey_detail(&state, &caller, row).await?)
        .map(Json)
        .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Journey exceptions (merged into /v1/catalog/exceptions)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct JourneyException {
    #[serde(flatten)]
    pub journey: JourneySummary,
    pub reasons: Vec<String>,
}

/// Permitted journeys with exception reasons: critical journeys left out of scope, and in-scope
/// journeys without services, with retired services or (critical) without an accountable owner.
pub async fn journey_exceptions_for(
    state: &CatalogState,
    caller: &Caller,
) -> ApiResult<Vec<JourneyException>> {
    let rows = load_journeys(&state.db, caller.tenant_id, None).await?;
    let rows = visible_journeys(state, caller, rows).await?;
    let f = facts(state, caller.tenant_id, &rows).await?;
    Ok(rows
        .iter()
        .filter_map(|j| {
            let reasons = f.exceptions(j);
            (!reasons.is_empty()).then(|| JourneyException {
                journey: JourneySummary::from(j),
                reasons,
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Scope and contexts
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ScopeQuery {
    context: Option<String>,
}

/// Restricts catalog records to a context. Only `business_critical` applies to catalog records;
/// production and non-production select environments, which services are bound to with coverage
/// (S5), so they are rejected here instead of being guessed.
pub(crate) fn catalog_context(context: Option<&str>) -> ApiResult<bool> {
    match context {
        None => Ok(false),
        Some(BUSINESS_CRITICAL) => Ok(true),
        Some(c) if CONTEXTS.contains(&c) => Err(ApiError::BadRequest(format!(
            "context {c} selects environments; catalog records support only {BUSINESS_CRITICAL}"
        ))),
        Some(_) => Err(ApiError::BadRequest(format!(
            "context must be one of {}",
            CONTEXTS.join(", ")
        ))),
    }
}

#[derive(Serialize)]
pub struct Scope {
    /// Non-retired services with their criticality.
    services: Vec<ServiceSummary>,
    /// Journeys in observability scope.
    journeys: Vec<JourneySummary>,
    /// Journeys explicitly excluded from scope, with their rationale.
    excluded_journeys: Vec<JourneySummary>,
}

/// GET /v1/catalog/scope?context= (OBS-01-01): everything the caller may see that observability
/// covers, and what was deliberately left out.
pub async fn scope(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<ScopeQuery>,
) -> ApiResult<Json<Scope>> {
    let critical_only = catalog_context(q.context.as_deref())?;
    let keep = |c: &str| !critical_only || c == "critical";
    let services = load_services(&state.db, caller.tenant_id, None).await?;
    let services: Vec<ServiceSummary> = visible(&state, &caller, services)
        .await?
        .iter()
        .filter(|s| s.lifecycle != "retired" && keep(&s.criticality))
        .map(ServiceSummary::from)
        .collect();
    let journeys = load_journeys(&state.db, caller.tenant_id, None).await?;
    let journeys = visible_journeys(&state, &caller, journeys).await?;
    let (inside, outside): (Vec<_>, Vec<_>) = journeys
        .iter()
        .filter(|j| keep(&j.criticality))
        .partition(|j| j.in_scope);
    Ok(Json(Scope {
        services,
        journeys: inside.into_iter().map(JourneySummary::from).collect(),
        excluded_journeys: outside.into_iter().map(JourneySummary::from).collect(),
    }))
}

/// GET /v1/catalog/contexts (OBS-01-03): each approved context and what it selects. Every
/// environment lands in exactly one of production, non_production or unclassified.
pub async fn contexts(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
) -> ApiResult<Response> {
    let classes = match state.environments.classes(caller.tenant_id).await {
        Ok(c) => c,
        Err(_) => {
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "control_plane_unavailable"})),
            )
                .into_response());
        }
    };
    let envs = |class: &str| -> Vec<String> {
        let mut v: Vec<String> = classes
            .iter()
            .filter(|c| c.class == class)
            .map(|c| c.environment.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let services = load_services(&state.db, caller.tenant_id, None).await?;
    let critical: Vec<ServiceSummary> = visible(&state, &caller, services)
        .await?
        .iter()
        .filter(|s| s.criticality == "critical" && s.lifecycle != "retired")
        .map(ServiceSummary::from)
        .collect();
    Ok(Json(json!({
        "contexts": [
            {"context": "production", "environments": envs("production")},
            {"context": "non_production", "environments": envs("non_production")},
            {"context": BUSINESS_CRITICAL, "services": critical},
        ],
        "unclassified_environments": envs("unclassified"),
    }))
    .into_response())
}
