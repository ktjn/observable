// Bootstrap & Tenant management endpoints.
//
// GET    /v1/tenants                  — list tenants (bootstrap, no tenant-auth)
// GET    /v1/tenants/:id/environments — list environments for a tenant (bootstrap)
// GET    /v1/tenants/:id/config       — get tenant retention and quota config (requires admin)
// PUT    /v1/tenants/:id/config       — update tenant retention and quota config (requires admin)
// DELETE /v1/tenants/:id              — hard-purge tenant data across PG and ClickHouse (requires admin)

use crate::AdminServiceAppState;
use crate::middleware::auth::{TenantContext, require_admin};
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct TenantListResponse {
    pub tenants: Vec<TenantRecord>,
}

#[derive(Serialize)]
pub struct EnvironmentRecord {
    pub environment: String,
}

#[derive(Serialize)]
pub struct EnvironmentListResponse {
    pub environments: Vec<EnvironmentRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TenantConfigResponse {
    pub tenant_id: Uuid,
    pub retention_days: u16,
    pub ingest_quota_per_sec: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTenantConfigRequest {
    pub retention_days: u16,
    pub ingest_quota_per_sec: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeTenantResponse {
    pub status: String,
    pub tenant_id: Uuid,
    pub purged_postgres_tables: Vec<String>,
    pub purged_clickhouse_tables: Vec<String>,
}

/// GET /v1/tenants
/// Without a session cookie: returns all tenants (backwards-compatible for API-key callers).
/// With a session cookie: filters to only the tenants the authenticated user belongs to.
pub async fn list_tenants(
    State(state): State<AdminServiceAppState>,
    headers: HeaderMap,
) -> Result<Json<TenantListResponse>, StatusCode> {
    let session_token = match observable_auth::extract_session_cookie(&headers) {
        Some(cookie) => Some(cookie),
        None => observable_auth::extract_bearer_token(&headers).map_err(StatusCode::from)?,
    };

    if let Some(session_token) = session_token {
        let session = observable_auth::verify_session(
            &state.http_client,
            &state.auth_service_url,
            &session_token,
        )
        .await
        .map_err(StatusCode::from)?;

        let rows = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT t.id, t.name
            FROM tenants t
            JOIN user_tenant_roles utr ON utr.tenant_id = t.id
            WHERE utr.user_id = $1
            ORDER BY t.name ASC
            "#,
        )
        .bind(session.user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list user tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        return Ok(Json(TenantListResponse {
            tenants: rows
                .into_iter()
                .map(|(id, name)| TenantRecord { id, name })
                .collect(),
        }));
    }

    // No session cookie — legacy path: return all tenants (API key callers).
    let rows =
        sqlx::query_as::<_, (Uuid, String)>(r#"SELECT id, name FROM tenants ORDER BY name ASC"#)
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Failed to list tenants");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    Ok(Json(TenantListResponse {
        tenants: rows
            .into_iter()
            .map(|(id, name)| TenantRecord { id, name })
            .collect(),
    }))
}

/// GET /v1/tenants/:id/environments
pub async fn list_tenant_environments(
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<EnvironmentListResponse>, StatusCode> {
    // If a session is present, verify the user has access to the requested tenant.
    let session_token = match observable_auth::extract_session_cookie(&headers) {
        Some(cookie) => Some(cookie),
        None => observable_auth::extract_bearer_token(&headers).map_err(StatusCode::from)?,
    };
    if let Some(session_token) = session_token {
        let session = observable_auth::verify_session(
            &state.http_client,
            &state.auth_service_url,
            &session_token,
        )
        .await
        .map_err(StatusCode::from)?;

        let has_access = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(session.user_id)
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if has_access == 0 {
            tracing::warn!(user_id = %session.user_id, %tenant_id, "User attempted to access unauthorized tenant environments");
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT environment
        FROM api_keys
        WHERE tenant_id = $1
          AND revoked_at IS NULL
          AND environment != ''
        ORDER BY environment ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to list tenant environments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(EnvironmentListResponse {
        environments: rows
            .into_iter()
            .map(|e| EnvironmentRecord { environment: e })
            .collect(),
    }))
}

/// GET /v1/tenants/:id/config
/// Requires tenant_admin role. Tenant ID must match TenantContext.
pub async fn handle_get_tenant_config(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<TenantConfigResponse>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = sqlx::query_as::<_, (i32, i32)>(
        r#"
        SELECT retention_days, ingest_quota_per_sec
        FROM tenants
        WHERE id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to fetch tenant config");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(TenantConfigResponse {
        tenant_id,
        retention_days: row.0 as u16,
        ingest_quota_per_sec: row.1 as u32,
    }))
}

/// PUT /v1/tenants/:id/config
/// Updates retention_days and ingest_quota_per_sec. Requires tenant_admin.
pub async fn handle_update_tenant_config(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<UpdateTenantConfigRequest>,
) -> Result<Json<TenantConfigResponse>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = sqlx::query_as::<_, (i32, i32)>(
        r#"
        UPDATE tenants
        SET retention_days = $1,
            ingest_quota_per_sec = $2,
            updated_at = NOW()
        WHERE id = $3
        RETURNING retention_days, ingest_quota_per_sec
        "#,
    )
    .bind(payload.retention_days as i32)
    .bind(payload.ingest_quota_per_sec as i32)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to update tenant config");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(TenantConfigResponse {
        tenant_id,
        retention_days: row.0 as u16,
        ingest_quota_per_sec: row.1 as u32,
    }))
}

/// DELETE /v1/tenants/:id
/// Hard purges all tenant telemetry from ClickHouse and all tenant records from PostgreSQL.
/// Writes an audit record in `credential_audit_log` with action = "tenant_purge".
pub async fn handle_purge_tenant(
    Extension(ctx): Extension<TenantContext>,
    State(state): State<AdminServiceAppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<PurgeTenantResponse>, StatusCode> {
    require_admin(&ctx)?;
    if ctx.tenant_id != tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // 1. Purge ClickHouse telemetry
    let ch_tables = vec![
        "observable.spans",
        "observable.span_events",
        "observable.logs",
        "observable.metric_points",
        "observable.metric_series",
    ];

    let mut purged_ch = Vec::new();
    for table in &ch_tables {
        let mutation = format!("ALTER TABLE {table} DELETE WHERE tenant_id = '{tenant_id}'");
        if let Err(e) = state.ch.query(&mutation).execute().await {
            tracing::warn!(error = %e, table = %table, "ClickHouse mutation failed or ignored");
        } else {
            purged_ch.push(table.to_string());
        }
    }

    // 2. Postgres purge in transaction
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!(error = ?e, "Failed to begin transaction for tenant purge");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Delete dependent tables without FK or cross-schema references first
    let pg_tables_cleaned = vec![
        "notification_audit_log",
        "notification_channels",
        "incident_events",
        "incidents",
        "alert_firings",
        "alert_rules",
        "slo_definitions",
        "deployment_markers",
        "change_events",
        "semantic_annotations",
        "saved_view_grants",
        "saved_views",
        "dashboard_grants",
        "dashboard_panels",
        "dashboards",
        "user_sessions",
        "user_tenant_roles",
        "api_keys",
        "projects",
        "tenants",
    ];

    // Helper function to run delete and log error if any
    async fn run_delete(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        query: &'static str,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<(), StatusCode> {
        if let Err(e) = sqlx::query(query).bind(tenant_id).execute(&mut **tx).await {
            tracing::error!(error = ?e, step = %name, "Purge step failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(())
    }

    // Notification audit log & channels
    run_delete(
        &mut tx,
        "DELETE FROM notification_audit_log WHERE tenant_id = $1",
        tenant_id,
        "notification_audit_log",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM notification_channels WHERE tenant_id = $1",
        tenant_id,
        "notification_channels",
    )
    .await?;

    // Incidents & events
    run_delete(
        &mut tx,
        "DELETE FROM incident_events WHERE incident_id IN (SELECT incident_id FROM incidents WHERE tenant_id = $1)",
        tenant_id,
        "incident_events",
    ).await?;
    run_delete(
        &mut tx,
        "DELETE FROM incidents WHERE tenant_id = $1",
        tenant_id,
        "incidents",
    )
    .await?;

    // Alerting
    run_delete(
        &mut tx,
        "DELETE FROM alert_firings WHERE tenant_id = $1",
        tenant_id,
        "alert_firings",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM alert_rules WHERE tenant_id = $1",
        tenant_id,
        "alert_rules",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM slo_definitions WHERE tenant_id = $1",
        tenant_id,
        "slo_definitions",
    )
    .await?;

    // Deployment markers & change events
    run_delete(
        &mut tx,
        "DELETE FROM deployment_markers WHERE tenant_id = $1",
        tenant_id,
        "deployment_markers",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM change_events WHERE tenant_id = $1",
        tenant_id,
        "change_events",
    )
    .await?;

    // Semantic annotations
    run_delete(
        &mut tx,
        "DELETE FROM semantic_annotations WHERE tenant_id = $1",
        tenant_id,
        "semantic_annotations",
    )
    .await?;

    // Saved views & dashboards
    run_delete(
        &mut tx,
        "DELETE FROM saved_view_grants WHERE saved_view_id IN (SELECT saved_view_id FROM saved_views WHERE tenant_id = $1)",
        tenant_id,
        "saved_view_grants",
    ).await?;
    run_delete(
        &mut tx,
        "DELETE FROM saved_views WHERE tenant_id = $1",
        tenant_id,
        "saved_views",
    )
    .await?;

    run_delete(
        &mut tx,
        "DELETE FROM dashboard_grants WHERE dashboard_id IN (SELECT dashboard_id FROM dashboards WHERE tenant_id = $1)",
        tenant_id,
        "dashboard_grants",
    ).await?;
    run_delete(
        &mut tx,
        "DELETE FROM dashboard_panels WHERE dashboard_id IN (SELECT dashboard_id FROM dashboards WHERE tenant_id = $1)",
        tenant_id,
        "dashboard_panels",
    ).await?;
    run_delete(
        &mut tx,
        "DELETE FROM dashboards WHERE tenant_id = $1",
        tenant_id,
        "dashboards",
    )
    .await?;

    // Auth & credentials
    run_delete(
        &mut tx,
        "DELETE FROM user_sessions WHERE tenant_id = $1",
        tenant_id,
        "user_sessions",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM user_tenant_roles WHERE tenant_id = $1",
        tenant_id,
        "user_tenant_roles",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM api_keys WHERE tenant_id = $1",
        tenant_id,
        "api_keys",
    )
    .await?;
    run_delete(
        &mut tx,
        "DELETE FROM projects WHERE tenant_id = $1",
        tenant_id,
        "projects",
    )
    .await?;

    // Finally delete tenant
    let res = sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to delete tenant row");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if res.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    // Record audit event in credential_audit_log
    // Note: credential_audit_log has tenant_id column which can record purged tenant's id.
    // If foreign keys existed, it would have to be written first or with NULL tenant_id,
    // but credential_audit_log is append-only audit without FK.
    let audit_actor = ctx
        .user_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| "admin_api".to_string());
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO credential_audit_log
        (action, outcome, credential_hash, tenant_id, auth_method)
        VALUES ('tenant_purge', 'allow', $1, $2, 'tenant_admin')
        "#,
    )
    .bind(audit_actor)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!(error = ?e, "Failed to insert credential_audit_log for tenant_purge");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(error = ?e, "Failed to commit tenant purge transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(PurgeTenantResponse {
        status: "purged".to_string(),
        tenant_id,
        purged_postgres_tables: pg_tables_cleaned.into_iter().map(String::from).collect(),
        purged_clickhouse_tables: purged_ch,
    }))
}
