use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SloDefinitionItem {
    pub slo_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub sli_type: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: String,
    pub firing: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSloRequest {
    pub service_name: String,
    pub environment: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
    pub description: Option<String>,
}

pub async fn list_slos(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<SloDefinitionItem>, sqlx::Error> {
    sqlx::query_as::<_, SloDefinitionItem>(
        "SELECT slo_id, service_name, environment, sli_type, target, window_days, \
         burn_rate_fast_threshold, burn_rate_slow_threshold, description, \
         EXISTS( \
             SELECT 1 FROM alert_rules ar \
             JOIN alert_firings af ON af.rule_id = ar.rule_id \
             WHERE ar.tenant_id = slo_definitions.tenant_id \
               AND ar.alert_type = 'slo_burn_rate' \
               AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
               AND af.state = 'active' \
         ) AS firing, \
         (SELECT MAX(af.occurred_at) FROM alert_rules ar \
          JOIN alert_firings af ON af.rule_id = ar.rule_id \
          WHERE ar.tenant_id = slo_definitions.tenant_id \
            AND ar.alert_type = 'slo_burn_rate' \
            AND ar.condition->>'slo_id' = slo_definitions.slo_id::text \
            AND af.state = 'active') AS last_fired_at, \
         created_at, updated_at \
         FROM slo_definitions WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db)
    .await
}
