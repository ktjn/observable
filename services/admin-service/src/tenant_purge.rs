//! Hard purge of one tenant's rows in the `control` schema (tenant lifecycle).
//!
//! Runs as the scoped `observable_control` login, in one transaction, child tables first
//! and the tenant row last. Idempotent: purging an absent tenant deletes nothing.
//! Other components purge their own schemas; control never deletes outside `control`.
//! Auth is called directly. Catalog, health and alerting depend on control, so they pull
//! pending requests from `GET /internal/tenant-purges` and acknowledge each purge.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AdminServiceAppState;

/// `(table, statement)` in delete order; each statement binds the tenant as `$1`.
pub const STEPS: &[(&str, &str)] = &[
    (
        "change_events",
        "DELETE FROM control.change_events WHERE tenant_id = $1",
    ),
    (
        "deployment_markers",
        "DELETE FROM control.deployment_markers WHERE tenant_id = $1",
    ),
    (
        "semantic_annotations",
        "DELETE FROM control.semantic_annotations WHERE tenant_id = $1",
    ),
    (
        "environment_classes",
        "DELETE FROM control.environment_classes WHERE tenant_id = $1",
    ),
    (
        "saved_views",
        "DELETE FROM control.saved_views WHERE tenant_id = $1",
    ),
    (
        "dashboards",
        "DELETE FROM control.dashboards WHERE tenant_id = $1",
    ),
    (
        "projects",
        "DELETE FROM control.projects WHERE tenant_id = $1",
    ),
    ("tenants", "DELETE FROM control.tenants WHERE id = $1"),
];

/// Tenant-scoped tables kept after a purge: the audit trail outlives the tenant.
pub const RETAINED: &[&str] = &["query_audit_log", "tenant_purge_acks", "tenant_purges"];

/// Components that purge their own schema from the request feed.
pub const PULL_COMPONENTS: &[&str] = &["alerting", "catalog", "health"];

/// `(component, prerequisite)`: the component sees a request only after the prerequisite
/// acknowledged it. Alerting pushes asset syncs into the catalog, so the catalog purges after
/// alerting has stopped syncing the tenant; otherwise a sync could recreate catalog rows.
pub const PURGE_AFTER: &[(&str, &str)] = &[("catalog", "alerting")];

/// ClickHouse telemetry tables purged by `ALTER TABLE ... DELETE` (asynchronous mutations).
pub const CLICKHOUSE_TABLES: &[&str] = &[
    "observable.spans",
    "observable.span_events",
    "observable.logs",
    "observable.metric_points",
    "observable.metric_series",
];

/// Deletes the tenant's rows and returns `(table, rows deleted)` per step.
pub async fn purge_tenant(
    db: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<(&'static str, u64)>, sqlx::Error> {
    let mut tx = db.begin().await?;
    let mut deleted = Vec::with_capacity(STEPS.len());
    for (table, sql) in STEPS {
        let rows = sqlx::query(*sql)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        deleted.push((*table, rows));
    }
    tx.commit().await?;
    Ok(deleted)
}

/// Schedules the deletion of the tenant's telemetry; ClickHouse applies it asynchronously.
pub async fn purge_telemetry(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
) -> Result<(), clickhouse::error::Error> {
    for table in CLICKHOUSE_TABLES {
        ch.query(&format!("ALTER TABLE {table} DELETE WHERE tenant_id = ?"))
            .bind(tenant_id.to_string())
            .execute()
            .await?;
    }
    Ok(())
}

/// Records the purge request before anything is deleted, so pulling components see it even
/// when a later step fails. Repeated requests keep the first.
pub async fn record_request(
    db: &PgPool,
    tenant_id: Uuid,
    requested_by: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO control.tenant_purges (tenant_id, requested_by) VALUES ($1, $2) \
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(requested_by)
    .execute(db)
    .await
    .map(|_| ())
}

/// Pulling components that have not acknowledged the tenant's purge.
pub async fn pending_components(db: &PgPool, tenant_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT c FROM unnest($2::text[]) AS c \
         WHERE NOT EXISTS (SELECT 1 FROM control.tenant_purge_acks a WHERE a.tenant_id = $1 AND a.component = c) \
         ORDER BY c",
    )
    .bind(tenant_id)
    .bind(PULL_COMPONENTS)
    .fetch_all(db)
    .await
}

fn known_component(component: &str) -> Result<(), StatusCode> {
    if PULL_COMPONENTS.contains(&component) {
        Ok(())
    } else {
        Err(StatusCode::UNPROCESSABLE_ENTITY)
    }
}

fn store_unavailable(e: sqlx::Error) -> StatusCode {
    tracing::error!(error = %e, "tenant purge store unavailable");
    StatusCode::SERVICE_UNAVAILABLE
}

#[derive(Deserialize)]
pub struct FeedQuery {
    pub component: String,
}

#[derive(Serialize, Deserialize)]
pub struct PurgeFeed {
    pub tenants: Vec<Uuid>,
}

/// `GET /internal/tenant-purges?component=` (internal token): tenants whose purge the
/// component has not acknowledged, oldest request first, respecting `PURGE_AFTER`.
pub async fn handle_feed(
    State(state): State<AdminServiceAppState>,
    Query(q): Query<FeedQuery>,
) -> Result<Json<PurgeFeed>, StatusCode> {
    known_component(&q.component)?;
    let after = PURGE_AFTER
        .iter()
        .find(|(c, _)| *c == q.component)
        .map(|(_, p)| *p);
    sqlx::query_scalar(
        "SELECT p.tenant_id FROM control.tenant_purges p \
         WHERE NOT EXISTS (SELECT 1 FROM control.tenant_purge_acks a \
                           WHERE a.tenant_id = p.tenant_id AND a.component = $1) \
           AND ($2::text IS NULL OR EXISTS (SELECT 1 FROM control.tenant_purge_acks a \
                           WHERE a.tenant_id = p.tenant_id AND a.component = $2)) \
         ORDER BY p.requested_at, p.tenant_id LIMIT 100",
    )
    .bind(&q.component)
    .bind(after)
    .fetch_all(&state.db)
    .await
    .map(|tenants| Json(PurgeFeed { tenants }))
    .map_err(store_unavailable)
}

#[derive(Deserialize)]
pub struct AckBody {
    pub component: String,
}

/// `POST /internal/tenant-purges/{tenant_id}/acks` (internal token): the component purged
/// its schema. Idempotent; the request is completed once every pulling component acked.
/// An unknown request is 404, so an acknowledgement never invents a purge.
pub async fn handle_ack(
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<AckBody>,
) -> Result<StatusCode, StatusCode> {
    known_component(&body.component)?;
    let mut tx = state.db.begin().await.map_err(store_unavailable)?;
    let requested: Option<Uuid> = sqlx::query_scalar(
        "SELECT tenant_id FROM control.tenant_purges WHERE tenant_id = $1 FOR UPDATE",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_unavailable)?;
    if requested.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query(
        "INSERT INTO control.tenant_purge_acks (tenant_id, component) VALUES ($1, $2) \
         ON CONFLICT (tenant_id, component) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(&body.component)
    .execute(&mut *tx)
    .await
    .map_err(store_unavailable)?;
    sqlx::query(
        "UPDATE control.tenant_purges SET completed_at = now() \
         WHERE tenant_id = $1 AND completed_at IS NULL \
           AND (SELECT count(*) FROM control.tenant_purge_acks WHERE tenant_id = $1) = cardinality($2::text[])",
    )
    .bind(tenant_id)
    .bind(PULL_COMPONENTS)
    .execute(&mut *tx)
    .await
    .map_err(store_unavailable)?;
    tx.commit().await.map_err(store_unavailable)?;
    Ok(StatusCode::NO_CONTENT)
}
