//! Hard purge of one tenant's rows in the `health` schema (tenant lifecycle).
//!
//! Runs as this component's own scoped login, in one transaction, child tables first.
//! Idempotent: purging an absent tenant deletes nothing and succeeds. Requests come from
//! observable-control's purge feed (`start_purge_worker`).
//! Platform-default rules (`tenant_id IS NULL`) are never touched.

use sqlx::PgPool;
use uuid::Uuid;

/// `(table, statement)` in delete order; each statement binds the tenant as `$1`.
pub const STEPS: &[(&str, &str)] = &[
    (
        "data_quality_issues",
        "DELETE FROM health.data_quality_issues WHERE tenant_id = $1",
    ),
    (
        "transitions",
        "DELETE FROM health.transitions WHERE tenant_id = $1",
    ),
    (
        "maintenance_windows",
        "DELETE FROM health.maintenance_windows WHERE tenant_id = $1",
    ),
    (
        "assessments",
        "DELETE FROM health.assessments WHERE tenant_id = $1",
    ),
    ("rules", "DELETE FROM health.rules WHERE tenant_id = $1"),
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

/// This component's name in observable-control's purge request feed.
pub const COMPONENT: &str = "health";

/// observable-control's tenant purge request feed (`GET /internal/tenant-purges`, internal
/// token). This component depends on control, so control cannot call it; it pulls instead.
#[derive(Clone)]
pub struct PurgeFeed {
    control_url: String,
    token: String,
    http: reqwest::Client,
}

#[derive(serde::Deserialize)]
struct Pending {
    tenants: Vec<Uuid>,
}

impl PurgeFeed {
    pub fn new(control_url: &str, token: &str, http: reqwest::Client) -> Self {
        Self {
            control_url: control_url.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            http,
        }
    }

    async fn pending(&self) -> anyhow::Result<Vec<Uuid>> {
        let pending: Pending = self
            .http
            .get(format!(
                "{}/internal/tenant-purges?component={COMPONENT}",
                self.control_url
            ))
            .header("X-Internal-Token", &self.token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(pending.tenants)
    }

    async fn ack(&self, tenant_id: Uuid) -> anyhow::Result<()> {
        self.http
            .post(format!(
                "{}/internal/tenant-purges/{tenant_id}/acks",
                self.control_url
            ))
            .header("X-Internal-Token", &self.token)
            .json(&serde_json::json!({ "component": COMPONENT }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Purges every pending tenant and acknowledges each only after its transaction committed,
/// so a failure leaves the request pending for the next run. Returns the tenants purged.
pub async fn purge_pending(db: &PgPool, feed: &PurgeFeed) -> anyhow::Result<Vec<Uuid>> {
    let mut purged = Vec::new();
    for tenant_id in feed.pending().await? {
        purge_tenant(db, tenant_id).await?;
        feed.ack(tenant_id).await?;
        tracing::info!(%tenant_id, "tenant purged");
        purged.push(tenant_id);
    }
    Ok(purged)
}

/// Background worker: polls the feed every `interval`.
pub async fn start_purge_worker(db: PgPool, feed: PurgeFeed, interval: std::time::Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if let Err(e) = purge_pending(&db, &feed).await {
            tracing::warn!(error = %e, "tenant purge feed skipped");
        }
    }
}
