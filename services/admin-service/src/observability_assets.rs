//! Control's observability assets for the catalog asset registry (phase 1 S6, OBS-34-01). The
//! catalog pulls them with the internal token, as it does environment classes: control never
//! calls the catalog, which keeps the component graph acyclic.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Serialize;
use uuid::Uuid;

use crate::AdminServiceAppState;
use crate::environments::InternalParams;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Owner {
    pub principal_kind: &'static str,
    pub principal_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservabilityAsset {
    pub kind: &'static str,
    pub external_id: String,
    pub service_name: String,
    pub environment: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
}

#[derive(Serialize)]
pub struct ObservabilityAssetList {
    pub assets: Vec<ObservabilityAsset>,
}

type DashboardRow = (Uuid, String, String, Option<String>, Option<Uuid>);

/// A tenant's dashboards that can be attributed to one service: all panels that name a service
/// name the same one. The environment is the panels' single `environment` filter, if any, and
/// the owner the earliest owner grant. Other dashboards are left out.
pub async fn dashboards(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<ObservabilityAsset>, sqlx::Error> {
    let rows: Vec<DashboardRow> = sqlx::query_as(
        "SELECT d.dashboard_id, d.name, min(btrim(p.service)), \
                CASE WHEN count(DISTINCT p.filters->>'environment') = 1 \
                     THEN min(p.filters->>'environment') END, \
                (SELECT g.user_id FROM dashboard_grants g WHERE g.dashboard_id = d.dashboard_id \
                   AND g.relation = 'owner' ORDER BY g.granted_at, g.user_id LIMIT 1) \
         FROM dashboards d JOIN dashboard_panels p ON p.dashboard_id = d.dashboard_id \
         WHERE d.tenant_id = $1 AND p.service IS NOT NULL AND btrim(p.service) <> '' \
         GROUP BY d.dashboard_id, d.name \
         HAVING count(DISTINCT btrim(p.service)) = 1 \
         ORDER BY d.dashboard_id",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, service, env, owner)| ObservabilityAsset {
            kind: "dashboard",
            external_id: id.to_string(),
            service_name: service,
            environment: env.unwrap_or_default().trim().to_owned(),
            name: name.chars().take(200).collect(),
            owner: owner.map(|principal_id| Owner {
                principal_kind: "user",
                principal_id,
            }),
        })
        .collect())
}

/// GET /internal/observability-assets?tenant_id= (internal token; catalog consumer).
pub async fn handle_internal_observability_assets(
    State(state): State<AdminServiceAppState>,
    Query(params): Query<InternalParams>,
) -> Result<Json<ObservabilityAssetList>, StatusCode> {
    let assets = dashboards(&state.db, params.tenant_id).await.map_err(|e| {
        tracing::error!(error = %e, "failed to list observability assets");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ObservabilityAssetList { assets }))
}
