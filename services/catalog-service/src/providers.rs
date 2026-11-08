//! `manage-providers` (phase 1 S7, OBS-36-01): material third-party providers, the business
//! services and critical journeys whose functions depend on them, and provider gaps.
//!
//! - A provider record names the provider, its kind, materiality, contract reference and the
//!   accountable contract owner (a user or team), and lists the dependent functions the caller
//!   may see.
//! - A dependency links a service or journey and a function to a provider. It may name a
//!   provider that is not registered yet; a critical function depending on an unregistered
//!   provider is the gap `unregistered_provider`. Registering a provider of that name links the
//!   dependency; deleting a provider makes its dependencies unregistered again.
//! - Material providers without an active contract owner, or with no dependent function, are
//!   provider gaps too.
//!
//! Access follows `api`: `decide-access` on `catalog.provider`, `catalog.service` and
//! `catalog.journey`. Hidden and missing records return the same 404, lists filter before
//! counting, dependents the caller may not see are never identified or counted, and a
//! registered provider the caller may not read is shown as withheld. An unavailable decision
//! point fails closed.

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
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::CatalogState;
use crate::api::{
    ApiError, ApiResult, Page, PrincipalRef, READ, ServiceRow, ServiceSummary, WRITE,
    check_principal, classification, decide, load_services, non_empty, one_of, paginate, permitted,
    readable_service, resolve_principals, visible, writable_service,
};
use crate::journeys::{
    JourneyRow, JourneySummary, load_journeys, readable_journey, visible_journeys, writable_journey,
};
use crate::ports::Caller;

pub const PROVIDER: &str = "catalog.provider";
const KINDS: &[&str] = &["cloud", "saas", "network", "payment", "data", "other"];

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, FromRow)]
pub struct ProviderRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub material: bool,
    pub contract_ref: String,
    pub contract_owner_kind: Option<String>,
    pub contract_owner_id: Option<Uuid>,
    pub classification: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const PROVIDER_COLUMNS: &str = "id, name, kind, material, contract_ref, contract_owner_kind, \
     contract_owner_id, classification, created_at, updated_at";

async fn load_providers(
    db: &PgPool,
    tenant_id: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<ProviderRow>, sqlx::Error> {
    sqlx::query_as::<_, ProviderRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {PROVIDER_COLUMNS} FROM catalog.providers WHERE tenant_id = $1 \
         AND ($2::uuid IS NULL OR id = $2) ORDER BY lower(name), id"
    )))
    .bind(tenant_id)
    .bind(id)
    .fetch_all(db)
    .await
}

#[derive(Clone, Debug, FromRow)]
pub struct DependencyRow {
    pub id: Uuid,
    pub provider_id: Option<Uuid>,
    pub provider_name: String,
    pub service_id: Option<Uuid>,
    pub journey_id: Option<Uuid>,
    pub function: String,
}

/// Dependencies of the tenant, optionally only those of one provider or one service.
async fn load_dependencies(
    db: &PgPool,
    tenant_id: Uuid,
    provider_id: Option<Uuid>,
    service_id: Option<Uuid>,
) -> Result<Vec<DependencyRow>, sqlx::Error> {
    sqlx::query_as::<_, DependencyRow>(
        "SELECT id, provider_id, provider_name, service_id, journey_id, function \
         FROM catalog.provider_dependencies WHERE tenant_id = $1 \
         AND ($2::uuid IS NULL OR provider_id = $2) AND ($3::uuid IS NULL OR service_id = $3) \
         ORDER BY lower(provider_name), lower(function), id",
    )
    .bind(tenant_id)
    .bind(provider_id)
    .bind(service_id)
    .fetch_all(db)
    .await
}

async fn visible_providers(
    state: &CatalogState,
    caller: &Caller,
    rows: Vec<ProviderRow>,
) -> ApiResult<Vec<ProviderRow>> {
    let keys: Vec<(Uuid, &str)> = rows
        .iter()
        .map(|r| (r.id, r.classification.as_str()))
        .collect();
    let allowed = permitted(state, caller, PROVIDER, &keys).await?;
    Ok(rows
        .into_iter()
        .filter(|r| allowed.contains(&r.id))
        .collect())
}

async fn readable_provider(
    state: &CatalogState,
    caller: &Caller,
    id: Uuid,
) -> ApiResult<ProviderRow> {
    let row = load_providers(&state.db, caller.tenant_id, Some(id))
        .await?
        .pop()
        .ok_or(ApiError::NotFound)?;
    if decide(
        state,
        caller,
        READ,
        PROVIDER,
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

async fn writable_provider(
    state: &CatalogState,
    caller: &Caller,
    id: Uuid,
) -> ApiResult<ProviderRow> {
    let row = readable_provider(state, caller, id).await?;
    if decide(
        state,
        caller,
        WRITE,
        PROVIDER,
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
// Wire types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ProviderSummary {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub material: bool,
    pub classification: String,
}

impl From<&ProviderRow> for ProviderSummary {
    fn from(r: &ProviderRow) -> Self {
        Self {
            id: r.id,
            name: r.name.clone(),
            kind: r.kind.clone(),
            material: r.material,
            classification: r.classification.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct ContractOwner {
    principal_kind: String,
    principal_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_name: Option<String>,
    /// `active`, `departed` or `unknown` (directory unavailable).
    state: String,
}

/// The function and the service or journey that performs it.
#[derive(Serialize)]
pub struct Dependent {
    dependency_id: Uuid,
    function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<ServiceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    journey: Option<JourneySummary>,
}

#[derive(Serialize)]
pub struct ProviderDetail {
    #[serde(flatten)]
    summary: ProviderSummary,
    contract_ref: String,
    contract_owner: Option<ContractOwner>,
    /// Dependent functions the caller may see.
    dependents: Vec<Dependent>,
    /// `true` when functions the caller may not see depend on the provider; they are never
    /// identified or counted.
    dependents_withheld: bool,
    /// Provider gap reasons (empty when complete); see `/v1/catalog/providers/gaps`.
    exceptions: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Targets of dependencies, loaded once and filtered by the caller's access.
struct Targets {
    services: HashMap<Uuid, ServiceRow>,
    journeys: HashMap<Uuid, JourneyRow>,
    visible_services: HashSet<Uuid>,
    visible_journeys: HashSet<Uuid>,
}

impl Targets {
    async fn load(
        state: &CatalogState,
        caller: &Caller,
        deps: &[DependencyRow],
    ) -> ApiResult<Self> {
        let service_ids: Vec<Uuid> = deps.iter().filter_map(|d| d.service_id).collect();
        let services = if service_ids.is_empty() {
            Vec::new()
        } else {
            load_services(&state.db, caller.tenant_id, Some(&service_ids)).await?
        };
        let journey_ids: HashSet<Uuid> = deps.iter().filter_map(|d| d.journey_id).collect();
        let journeys: Vec<JourneyRow> = if journey_ids.is_empty() {
            Vec::new()
        } else {
            load_journeys(&state.db, caller.tenant_id, None)
                .await?
                .into_iter()
                .filter(|j| journey_ids.contains(&j.id))
                .collect()
        };
        let visible_services = visible(state, caller, services.clone())
            .await?
            .iter()
            .map(|s| s.id)
            .collect();
        let visible_journeys = visible_journeys(state, caller, journeys.clone())
            .await?
            .iter()
            .map(|j| j.id)
            .collect();
        Ok(Self {
            services: services.into_iter().map(|s| (s.id, s)).collect(),
            journeys: journeys.into_iter().map(|j| (j.id, j)).collect(),
            visible_services,
            visible_journeys,
        })
    }

    fn visible(&self, d: &DependencyRow) -> bool {
        match (d.service_id, d.journey_id) {
            (Some(s), _) => self.visible_services.contains(&s),
            (_, Some(j)) => self.visible_journeys.contains(&j),
            _ => false,
        }
    }

    /// A critical function: a critical, non-retired service or a critical journey.
    fn critical(&self, d: &DependencyRow) -> bool {
        match (d.service_id, d.journey_id) {
            (Some(s), _) => self
                .services
                .get(&s)
                .is_some_and(|r| r.criticality == "critical" && r.lifecycle != "retired"),
            (_, Some(j)) => self
                .journeys
                .get(&j)
                .is_some_and(|r| r.criticality == "critical"),
            _ => false,
        }
    }

    fn dependent(&self, d: &DependencyRow) -> Dependent {
        Dependent {
            dependency_id: d.id,
            function: d.function.clone(),
            service: d
                .service_id
                .and_then(|s| self.services.get(&s))
                .map(ServiceSummary::from),
            journey: d
                .journey_id
                .and_then(|j| self.journeys.get(&j))
                .map(JourneySummary::from),
        }
    }
}

/// Provider gap reasons. `dependents` counts all dependencies, also hidden ones, which only
/// contribute to a reason.
fn provider_exceptions(
    p: &ProviderRow,
    owner_state: Option<&str>,
    dependents: usize,
) -> Vec<String> {
    if !p.material {
        return Vec::new();
    }
    let mut reasons = Vec::new();
    match owner_state {
        None => reasons.push("missing_contract_owner".to_owned()),
        Some("departed") => reasons.push("contract_owner_departed".to_owned()),
        Some("unknown") => reasons.push("contract_owner_unresolved".to_owned()),
        Some(_) => {}
    }
    if dependents == 0 {
        reasons.push("unmapped_material_provider".to_owned());
    }
    reasons
}

async fn contract_owners(
    state: &CatalogState,
    tenant_id: Uuid,
    rows: &[ProviderRow],
) -> ApiResult<HashMap<Uuid, ContractOwner>> {
    let refs: Vec<(String, Uuid)> = rows
        .iter()
        .filter_map(|p| Some((p.contract_owner_kind.clone()?, p.contract_owner_id?)))
        .collect();
    let resolution = resolve_principals(state, tenant_id, &refs).await?;
    Ok(rows
        .iter()
        .filter_map(|p| {
            let (kind, id) = (p.contract_owner_kind.clone()?, p.contract_owner_id?);
            Some((
                p.id,
                ContractOwner {
                    state: resolution.state(&kind, id).to_owned(),
                    team_name: resolution.teams.get(&id).map(|(n, _)| n.clone()),
                    principal_kind: kind,
                    principal_id: id,
                },
            ))
        })
        .collect())
}

async fn provider_detail(
    state: &CatalogState,
    caller: &Caller,
    row: ProviderRow,
) -> ApiResult<ProviderDetail> {
    let deps = load_dependencies(&state.db, caller.tenant_id, Some(row.id), None).await?;
    let targets = Targets::load(state, caller, &deps).await?;
    let mut owners = contract_owners(state, caller.tenant_id, std::slice::from_ref(&row)).await?;
    let owner = owners.remove(&row.id);
    let shown: Vec<Dependent> = deps
        .iter()
        .filter(|d| targets.visible(d))
        .map(|d| targets.dependent(d))
        .collect();
    Ok(ProviderDetail {
        exceptions: provider_exceptions(&row, owner.as_ref().map(|o| o.state.as_str()), deps.len()),
        dependents_withheld: shown.len() != deps.len(),
        dependents: shown,
        contract_owner: owner,
        contract_ref: row.contract_ref.clone(),
        summary: ProviderSummary::from(&row),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

// ---------------------------------------------------------------------------
// Handlers: providers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ProviderBody {
    name: String,
    kind: String,
    #[serde(default = "default_true")]
    material: bool,
    #[serde(default)]
    contract_ref: String,
    contract_owner: Option<PrincipalRef>,
    #[serde(default = "default_classification")]
    classification: String,
}

fn default_true() -> bool {
    true
}

fn default_classification() -> String {
    "internal".into()
}

async fn validate(
    state: &CatalogState,
    caller: &Caller,
    body: &ProviderBody,
) -> ApiResult<(String, Classification)> {
    let name = non_empty("name", &body.name)?;
    one_of("kind", &body.kind, KINDS)?;
    if body.contract_ref.len() > 500 {
        return Err(ApiError::BadRequest(
            "contract_ref must be at most 500 characters".into(),
        ));
    }
    let class = classification(&body.classification)?;
    if let Some(o) = &body.contract_owner {
        check_principal(state, caller, &o.principal_kind, o.principal_id).await?;
    }
    Ok((name, class))
}

/// POST /v1/catalog/providers. Dependencies already naming the provider are linked to it.
pub async fn create_provider(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<ProviderBody>,
) -> ApiResult<Response> {
    let (name, class) = validate(&state, &caller, &body).await?;
    if !decide(&state, &caller, WRITE, PROVIDER, "new", Some(class)).await? {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO catalog.providers (tenant_id, name, kind, material, contract_ref, \
         contract_owner_kind, contract_owner_id, classification) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(caller.tenant_id)
    .bind(&name)
    .bind(&body.kind)
    .bind(body.material)
    .bind(body.contract_ref.trim())
    .bind(
        body.contract_owner
            .as_ref()
            .map(|o| o.principal_kind.as_str()),
    )
    .bind(body.contract_owner.as_ref().map(|o| o.principal_id))
    .bind(class.as_str())
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE catalog.provider_dependencies SET provider_id = $1, provider_name = $3 \
         WHERE tenant_id = $2 AND provider_id IS NULL AND lower(provider_name) = lower($3)",
    )
    .bind(id)
    .bind(caller.tenant_id)
    .bind(&name)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = readable_provider(&state, &caller, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(provider_detail(&state, &caller, row).await?),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct ProviderQuery {
    material: Option<bool>,
    kind: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

/// GET /v1/catalog/providers
pub async fn list_providers(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<ProviderQuery>,
) -> ApiResult<Json<Page<ProviderSummary>>> {
    let rows = load_providers(&state.db, caller.tenant_id, None).await?;
    let items: Vec<ProviderSummary> = visible_providers(&state, &caller, rows)
        .await?
        .iter()
        .filter(|p| q.material.is_none_or(|m| p.material == m))
        .filter(|p| q.kind.as_deref().is_none_or(|k| p.kind == k))
        .map(ProviderSummary::from)
        .collect();
    Ok(Json(paginate(items, q.limit, q.offset)))
}

/// GET /v1/catalog/providers/{id}
pub async fn get_provider(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProviderDetail>> {
    let row = readable_provider(&state, &caller, id).await?;
    Ok(Json(provider_detail(&state, &caller, row).await?))
}

/// PUT /v1/catalog/providers/{id}: replaces the record. A rename carries the provider's
/// dependencies along.
pub async fn update_provider(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
    Json(body): Json<ProviderBody>,
) -> ApiResult<Json<ProviderDetail>> {
    writable_provider(&state, &caller, id).await?;
    let (name, class) = validate(&state, &caller, &body).await?;
    if !decide(
        &state,
        &caller,
        WRITE,
        PROVIDER,
        &id.to_string(),
        Some(class),
    )
    .await?
    {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE catalog.providers SET name = $3, kind = $4, material = $5, contract_ref = $6, \
         contract_owner_kind = $7, contract_owner_id = $8, classification = $9, updated_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(&name)
    .bind(&body.kind)
    .bind(body.material)
    .bind(body.contract_ref.trim())
    .bind(
        body.contract_owner
            .as_ref()
            .map(|o| o.principal_kind.as_str()),
    )
    .bind(body.contract_owner.as_ref().map(|o| o.principal_id))
    .bind(class.as_str())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE catalog.provider_dependencies SET provider_name = $2 WHERE provider_id = $1",
    )
    .bind(id)
    .bind(&name)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let row = readable_provider(&state, &caller, id).await?;
    Ok(Json(provider_detail(&state, &caller, row).await?))
}

/// DELETE /v1/catalog/providers/{id}: its dependencies stay, as unregistered.
pub async fn delete_provider(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    writable_provider(&state, &caller, id).await?;
    sqlx::query("DELETE FROM catalog.providers WHERE tenant_id = $1 AND id = $2")
        .bind(caller.tenant_id)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Handlers: dependencies
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DependencyBody {
    provider_id: Option<Uuid>,
    /// A provider that may not be registered yet.
    provider_name: Option<String>,
    service_id: Option<Uuid>,
    journey_id: Option<Uuid>,
    function: String,
}

/// How a dependency's provider is shown to the caller.
#[derive(Serialize)]
struct ProviderRefView {
    registered: bool,
    /// Present when unregistered (the declared name) or when the caller may read the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_name: Option<String>,
    /// A registered provider the caller may not read; it is never identified.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    withheld: bool,
}

#[derive(Serialize)]
struct DependencyView {
    id: Uuid,
    function: String,
    #[serde(flatten)]
    provider: ProviderRefView,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    journey_id: Option<Uuid>,
}

async fn provider_refs(
    state: &CatalogState,
    caller: &Caller,
    deps: &[DependencyRow],
) -> ApiResult<HashMap<Uuid, ProviderRow>> {
    let ids: HashSet<Uuid> = deps.iter().filter_map(|d| d.provider_id).collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<ProviderRow> = load_providers(&state.db, caller.tenant_id, None)
        .await?
        .into_iter()
        .filter(|p| ids.contains(&p.id))
        .collect();
    Ok(visible_providers(state, caller, rows)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect())
}

fn dependency_view(d: &DependencyRow, readable: &HashMap<Uuid, ProviderRow>) -> DependencyView {
    let provider = match d.provider_id {
        None => ProviderRefView {
            registered: false,
            provider: None,
            provider_name: Some(d.provider_name.clone()),
            withheld: false,
        },
        Some(p) => match readable.get(&p) {
            Some(row) => ProviderRefView {
                registered: true,
                provider: Some(ProviderSummary::from(row)),
                provider_name: None,
                withheld: false,
            },
            None => ProviderRefView {
                registered: true,
                provider: None,
                provider_name: None,
                withheld: true,
            },
        },
    };
    DependencyView {
        id: d.id,
        function: d.function.clone(),
        provider,
        service_id: d.service_id,
        journey_id: d.journey_id,
    }
}

/// The caller must be able to change the service or journey that performs the function.
async fn writable_target(
    state: &CatalogState,
    caller: &Caller,
    service_id: Option<Uuid>,
    journey_id: Option<Uuid>,
) -> ApiResult<()> {
    match (service_id, journey_id) {
        (Some(s), None) => writable_service(state, caller, s).await.map(|_| ()),
        (None, Some(j)) => writable_journey(state, caller, j).await.map(|_| ()),
        _ => Err(ApiError::BadRequest(
            "exactly one of service_id, journey_id is required".into(),
        )),
    }
}

/// POST /v1/catalog/providers/dependencies
pub async fn create_dependency(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<DependencyBody>,
) -> ApiResult<Response> {
    let function = non_empty("function", &body.function)?;
    writable_target(&state, &caller, body.service_id, body.journey_id)
        .await
        .map_err(|e| match e {
            ApiError::NotFound => ApiError::BadRequest("unknown service or journey".into()),
            other => other,
        })?;
    let (provider_id, provider_name) = match (body.provider_id, body.provider_name.as_deref()) {
        (Some(id), None) => {
            let p = readable_provider(&state, &caller, id)
                .await
                .map_err(|e| match e {
                    ApiError::NotFound => ApiError::BadRequest("unknown provider".into()),
                    other => other,
                })?;
            (Some(p.id), p.name)
        }
        (None, Some(name)) => {
            let name = non_empty("provider_name", name)?;
            let registered: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM catalog.providers WHERE tenant_id = $1 AND lower(name) = lower($2)",
            )
            .bind(caller.tenant_id)
            .bind(&name)
            .fetch_optional(&state.db)
            .await?;
            (registered, name)
        }
        _ => {
            return Err(ApiError::BadRequest(
                "exactly one of provider_id, provider_name is required".into(),
            ));
        }
    };
    let by = match caller.user_id {
        Some(u) => format!("user:{u}"),
        None => format!("api_key:{}", caller.role),
    };
    let row: DependencyRow = sqlx::query_as(
        "INSERT INTO catalog.provider_dependencies (tenant_id, provider_id, provider_name, \
         service_id, journey_id, function, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id, provider_id, provider_name, service_id, journey_id, function",
    )
    .bind(caller.tenant_id)
    .bind(provider_id)
    .bind(&provider_name)
    .bind(body.service_id)
    .bind(body.journey_id)
    .bind(&function)
    .bind(by)
    .fetch_one(&state.db)
    .await?;
    let readable = provider_refs(&state, &caller, std::slice::from_ref(&row)).await?;
    Ok((StatusCode::CREATED, Json(dependency_view(&row, &readable))).into_response())
}

/// DELETE /v1/catalog/providers/dependencies/{id}
pub async fn delete_dependency(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let target: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT service_id, journey_id FROM catalog.provider_dependencies \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (service_id, journey_id) = target.ok_or(ApiError::NotFound)?;
    writable_target(&state, &caller, service_id, journey_id).await?;
    sqlx::query("DELETE FROM catalog.provider_dependencies WHERE tenant_id = $1 AND id = $2")
        .bind(caller.tenant_id)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /v1/catalog/services/{id}/dependencies: the providers a service's functions depend on.
pub async fn service_dependencies(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    readable_service(&state, &caller, id).await?;
    let deps = load_dependencies(&state.db, caller.tenant_id, None, Some(id)).await?;
    let readable = provider_refs(&state, &caller, &deps).await?;
    let items: Vec<DependencyView> = deps.iter().map(|d| dependency_view(d, &readable)).collect();
    let total = items.len();
    Ok(Json(serde_json::json!({ "items": items, "total": total })))
}

/// GET /v1/catalog/journeys/{id}/dependencies
pub async fn journey_dependencies(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    readable_journey(&state, &caller, id).await?;
    let deps: Vec<DependencyRow> = load_dependencies(&state.db, caller.tenant_id, None, None)
        .await?
        .into_iter()
        .filter(|d| d.journey_id == Some(id))
        .collect();
    let readable = provider_refs(&state, &caller, &deps).await?;
    let items: Vec<DependencyView> = deps.iter().map(|d| dependency_view(d, &readable)).collect();
    let total = items.len();
    Ok(Json(serde_json::json!({ "items": items, "total": total })))
}

// ---------------------------------------------------------------------------
// Gaps
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ProviderGap {
    /// `unregistered_provider` for a critical function, or a provider reason
    /// (`missing_contract_owner`, `contract_owner_departed`, `contract_owner_unresolved`,
    /// `unmapped_material_provider`).
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependent: Option<Dependent>,
}

/// Unregistered providers of critical functions the caller may see.
async fn unregistered(
    state: &CatalogState,
    caller: &Caller,
) -> ApiResult<(Vec<DependencyRow>, Targets)> {
    let deps: Vec<DependencyRow> = load_dependencies(&state.db, caller.tenant_id, None, None)
        .await?
        .into_iter()
        .filter(|d| d.provider_id.is_none())
        .collect();
    let targets = Targets::load(state, caller, &deps).await?;
    let gaps = deps
        .into_iter()
        .filter(|d| targets.visible(d) && targets.critical(d))
        .collect();
    Ok((gaps, targets))
}

/// Critical services (by id) whose functions depend on an unregistered provider, for the
/// exception list.
pub(crate) async fn unregistered_provider_services(
    state: &CatalogState,
    caller: &Caller,
) -> ApiResult<HashSet<Uuid>> {
    Ok(unregistered(state, caller)
        .await?
        .0
        .iter()
        .filter_map(|d| d.service_id)
        .collect())
}

/// GET /v1/catalog/providers/gaps
pub async fn provider_gaps(
    State(state): State<CatalogState>,
    Extension(caller): Extension<Caller>,
) -> ApiResult<Json<Page<ProviderGap>>> {
    let (deps, targets) = unregistered(&state, &caller).await?;
    let mut items: Vec<ProviderGap> = deps
        .iter()
        .map(|d| ProviderGap {
            reason: "unregistered_provider".into(),
            provider: None,
            provider_name: Some(d.provider_name.clone()),
            dependent: Some(targets.dependent(d)),
        })
        .collect();

    let providers: Vec<ProviderRow> = visible_providers(
        &state,
        &caller,
        load_providers(&state.db, caller.tenant_id, None).await?,
    )
    .await?
    .into_iter()
    .filter(|p| p.material)
    .collect();
    let counts: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT provider_id, count(*) FROM catalog.provider_dependencies \
         WHERE tenant_id = $1 AND provider_id IS NOT NULL GROUP BY provider_id",
    )
    .bind(caller.tenant_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .collect();
    let owners = contract_owners(&state, caller.tenant_id, &providers).await?;
    for p in &providers {
        let reasons = provider_exceptions(
            p,
            owners.get(&p.id).map(|o| o.state.as_str()),
            counts.get(&p.id).copied().unwrap_or(0) as usize,
        );
        items.extend(reasons.into_iter().map(|reason| ProviderGap {
            reason,
            provider: Some(ProviderSummary::from(p)),
            provider_name: None,
            dependent: None,
        }));
    }
    Ok(Json(paginate(items, Some(200), None)))
}
