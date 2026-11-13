//! Hard purge of one tenant's rows in the `auth` schema (tenant lifecycle).
//!
//! Runs as this component's own scoped login, in one transaction, child tables first.
//! Idempotent: purging an absent tenant deletes nothing and succeeds.
//! Users are global and kept; audit logs are retained (see `RETAINED`).

use sqlx::PgPool;
use uuid::Uuid;

/// `(table, statement)` in delete order; each statement binds the tenant as `$1`.
pub const STEPS: &[(&str, &str)] = &[
    (
        "user_sessions",
        "DELETE FROM auth.user_sessions WHERE tenant_id = $1",
    ),
    (
        "user_tenant_roles",
        "DELETE FROM auth.user_tenant_roles WHERE tenant_id = $1",
    ),
    ("api_keys", "DELETE FROM auth.api_keys WHERE tenant_id = $1"),
    (
        "health_worker_credentials",
        "DELETE FROM auth.health_worker_credentials WHERE tenant_id = $1",
    ),
    (
        "authz_elevations",
        "DELETE FROM auth.authz_elevations WHERE tenant_id = $1",
    ),
    (
        "authz_purpose_rules",
        "DELETE FROM auth.authz_purpose_rules WHERE tenant_id = $1",
    ),
    (
        "authz_role_grants",
        "DELETE FROM auth.authz_role_grants WHERE tenant_id = $1",
    ),
];

/// Tenant-scoped tables kept after a purge: the audit trail outlives the tenant.
pub const RETAINED: &[&str] = &["credential_audit_log", "access_decision_log"];

/// Deletes the tenant's rows and records `tenant_purge` in `credential_audit_log` in the
/// same transaction (actor in `credential_hash`, as the other tenant-admin actions do).
/// Returns `(table, rows deleted)` per step.
pub async fn purge_tenant(
    db: &PgPool,
    tenant_id: Uuid,
    actor: &str,
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
    sqlx::query(
        "INSERT INTO auth.credential_audit_log (action, outcome, credential_hash, tenant_id, auth_method) \
         VALUES ('tenant_purge', 'allow', $1, $2, 'tenant_admin')",
    )
    .bind(actor)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(deleted)
}

#[derive(serde::Deserialize)]
pub(crate) struct PurgeRequest {
    actor: String,
}

#[derive(serde::Serialize)]
pub(crate) struct PurgeResult {
    tenant_id: Uuid,
    purged_tables: Vec<&'static str>,
}

/// `POST /internal/tenants/{tenant_id}/purge` (internal token): observable-control's tenant
/// purge. Idempotent; a store failure is 503 and nothing is deleted.
pub(crate) async fn purge_handler(
    axum::extract::State(state): axum::extract::State<crate::authz::AuthzState>,
    axum::extract::Path(tenant_id): axum::extract::Path<Uuid>,
    axum::Json(body): axum::Json<PurgeRequest>,
) -> Result<axum::Json<PurgeResult>, axum::http::StatusCode> {
    let actor = body.actor.trim();
    if actor.is_empty() {
        return Err(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }
    let deleted = purge_tenant(&state.db, tenant_id, actor)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, %tenant_id, "tenant purge failed");
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        })?;
    Ok(axum::Json(PurgeResult {
        tenant_id,
        purged_tables: deleted.into_iter().map(|(t, _)| t).collect(),
    }))
}
