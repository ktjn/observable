use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdOperator {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ThresholdCondition {
    pub metric_name: String,
    pub operator: ThresholdOperator,
    pub threshold: f64,
}

#[derive(Debug, PartialEq)]
pub enum EvalResult {
    Firing,
    Ok,
}

pub fn evaluate_threshold(value: f64, condition: &ThresholdCondition) -> EvalResult {
    let fires = match condition.operator {
        ThresholdOperator::Gt => value > condition.threshold,
        ThresholdOperator::Gte => value >= condition.threshold,
        ThresholdOperator::Lt => value < condition.threshold,
        ThresholdOperator::Lte => value <= condition.threshold,
        ThresholdOperator::Eq => (value - condition.threshold).abs() < f64::EPSILON,
    };
    if fires {
        EvalResult::Firing
    } else {
        EvalResult::Ok
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SloBurnRateCondition {
    pub slo_id: Uuid,
    pub fast_window_minutes: u64,
    pub slow_window_minutes: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CompositeRuleCondition {
    pub left_rule_id: Uuid,
    pub right_rule_id: Uuid,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeadmanCondition {
    pub service_name: String,
    pub window_secs: i64,
}

pub fn evaluate_deadman(
    last_seen_secs_ago: Option<i64>,
    condition: &DeadmanCondition,
) -> EvalResult {
    match last_seen_secs_ago {
        None => EvalResult::Firing,
        Some(elapsed) if elapsed >= condition.window_secs => EvalResult::Firing,
        Some(_) => EvalResult::Ok,
    }
}

pub fn calculate_burn_rate(
    bad_events: u64,
    total_events: u64,
    target: f64,
    _window_minutes: u64,
    _slo_window_days: i32,
) -> f64 {
    if bad_events == 0 || total_events == 0 {
        return 0.0;
    }
    let error_budget_fraction = 1.0 - target;
    let observed_error_fraction = bad_events as f64 / total_events as f64;
    observed_error_fraction / error_budget_fraction
}

pub fn evaluate_burn_rate(
    fast_burn_rate: f64,
    slow_burn_rate: f64,
    fast_threshold: f64,
    slow_threshold: f64,
    _condition: &SloBurnRateCondition,
) -> EvalResult {
    if fast_burn_rate >= fast_threshold && slow_burn_rate >= slow_threshold {
        EvalResult::Firing
    } else {
        EvalResult::Ok
    }
}

#[derive(sqlx::FromRow, Clone)]
pub struct AlertRuleRow {
    pub rule_id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub condition: serde_json::Value,
    pub severity: String,
    pub for_duration_secs: Option<i64>,
    pub notification_channels: Vec<Uuid>,
    pub auto_trigger_incident: bool,
    pub auto_trigger_delay_secs: Option<i64>,
    pub runbook_url: Option<String>,
}

#[derive(clickhouse::Row, serde::Deserialize)]
pub struct LatestPointRow {
    pub value_double: Option<f64>,
    pub value_int: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub struct SloDefinitionRow {
    pub slo_id: Uuid,
    pub tenant_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub target: f64,
    pub window_days: i32,
    pub burn_rate_fast_threshold: f64,
    pub burn_rate_slow_threshold: f64,
}

#[derive(clickhouse::Row, serde::Deserialize)]
pub struct SpanCountRow {
    pub total_count: u64,
    pub bad_count: u64,
}

pub async fn eval_threshold_rules(
    db: &sqlx::PgPool,
    ch: &clickhouse::Client,
) -> anyhow::Result<()> {
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
         auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
         FROM alert_rules WHERE alert_type = 'threshold' AND silenced = false",
    )
    .fetch_all(db)
    .await?;

    tracing::debug!(count = rules.len(), "evaluating threshold rules");

    for rule in rules {
        let cond: ThresholdCondition = match serde_json::from_value(rule.condition.clone()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    error = %e,
                    "skipping rule: condition parse failed"
                );
                continue;
            }
        };

        let mut cursor = ch
            .query(
                "SELECT value_double, value_int \
                 FROM observable.metric_points \
                 WHERE tenant_id = ? AND metric_name = ? \
                 ORDER BY time_unix_nano DESC \
                 LIMIT 1",
            )
            .bind(rule.tenant_id)
            .bind(&cond.metric_name)
            .fetch::<LatestPointRow>()
            .map_err(|e| anyhow::anyhow!("clickhouse query error: {e}"))?;

        let point = match cursor.next().await {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    error = %e,
                    "skipping rule: metric fetch failed"
                );
                continue;
            }
        };

        let value = match (point.value_double, point.value_int) {
            (Some(v), _) => v,
            (None, Some(v)) => v as f64,
            (None, None) => continue,
        };

        match evaluate_threshold(value, &cond) {
            EvalResult::Firing => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    tenant_id = %rule.tenant_id,
                    rule_name = %rule.name,
                    metric_name = %cond.metric_name,
                    value = value,
                    threshold = cond.threshold,
                    "alert firing: threshold exceeded"
                );
                if let Err(e) = record_firing(db, &rule, value).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to record alert firing");
                }
            }
            EvalResult::Ok => {
                if let Err(e) = resolve_open_firing(db, &rule, value).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to resolve alert firing");
                }
            }
        }
    }
    Ok(())
}

pub async fn eval_slo_burn_rate_rules(
    db: &sqlx::PgPool,
    ch: &clickhouse::Client,
) -> anyhow::Result<()> {
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
         auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
         FROM alert_rules WHERE alert_type = 'slo_burn_rate' AND silenced = false",
    )
    .fetch_all(db)
    .await?;

    tracing::debug!(count = rules.len(), "evaluating SLO burn-rate rules");

    for rule in rules {
        let cond: SloBurnRateCondition = match serde_json::from_value(rule.condition.clone()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    error = %e,
                    "skipping SLO rule: condition parse failed"
                );
                continue;
            }
        };

        let slo: Option<SloDefinitionRow> = sqlx::query_as(
            "SELECT slo_id, tenant_id, service_name, environment, target, window_days, \
             burn_rate_fast_threshold, burn_rate_slow_threshold \
             FROM slo_definitions WHERE slo_id = $1 AND tenant_id = $2",
        )
        .bind(cond.slo_id)
        .bind(rule.tenant_id)
        .fetch_optional(db)
        .await?;

        let Some(slo) = slo else {
            tracing::warn!(
                rule_id = %rule.rule_id,
                slo_id = %cond.slo_id,
                "skipping SLO rule: SLO definition not found"
            );
            continue;
        };

        let fast_counts = match fetch_span_counts(
            ch,
            slo.tenant_id,
            &slo.service_name,
            &slo.environment,
            cond.fast_window_minutes,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(rule_id = %rule.rule_id, error = %e, "skipping SLO rule: fast-window span fetch failed");
                continue;
            }
        };

        let slow_counts = match fetch_span_counts(
            ch,
            slo.tenant_id,
            &slo.service_name,
            &slo.environment,
            cond.slow_window_minutes,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(rule_id = %rule.rule_id, error = %e, "skipping SLO rule: slow-window span fetch failed");
                continue;
            }
        };

        let fast_burn_rate = calculate_burn_rate(
            fast_counts.bad_count,
            fast_counts.total_count,
            slo.target,
            cond.fast_window_minutes,
            slo.window_days,
        );
        let slow_burn_rate = calculate_burn_rate(
            slow_counts.bad_count,
            slow_counts.total_count,
            slo.target,
            cond.slow_window_minutes,
            slo.window_days,
        );

        match evaluate_burn_rate(
            fast_burn_rate,
            slow_burn_rate,
            slo.burn_rate_fast_threshold,
            slo.burn_rate_slow_threshold,
            &cond,
        ) {
            EvalResult::Firing => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    slo_id = %slo.slo_id,
                    tenant_id = %slo.tenant_id,
                    service_name = %slo.service_name,
                    environment = %slo.environment,
                    fast_burn_rate,
                    slow_burn_rate,
                    fast_threshold = slo.burn_rate_fast_threshold,
                    slow_threshold = slo.burn_rate_slow_threshold,
                    "alert firing: SLO burn rate exceeded"
                );
                if let Err(e) = record_firing(db, &rule, fast_burn_rate).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to record SLO alert firing");
                }
            }
            EvalResult::Ok => {
                if let Err(e) = resolve_open_firing(db, &rule, fast_burn_rate).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to resolve SLO alert firing");
                }
            }
        }
    }

    Ok(())
}

pub async fn eval_composite_rules(
    db: &sqlx::PgPool,
    _ch: &clickhouse::Client,
) -> anyhow::Result<()> {
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
         auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
         FROM alert_rules WHERE alert_type = 'composite' AND silenced = false",
    )
    .fetch_all(db)
    .await?;

    tracing::debug!(count = rules.len(), "evaluating composite rules");

    for rule in rules {
        let cond: CompositeRuleCondition = match serde_json::from_value(rule.condition.clone()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    error = %e,
                    "skipping composite rule: condition parse failed"
                );
                continue;
            }
        };

        let left_active: Option<Uuid> = sqlx::query_scalar(
            "SELECT firing_id FROM alert_firings \
             WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active' \
             LIMIT 1",
        )
        .bind(cond.left_rule_id)
        .bind(rule.tenant_id)
        .fetch_optional(db)
        .await?;

        let right_active: Option<Uuid> = sqlx::query_scalar(
            "SELECT firing_id FROM alert_firings \
             WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active' \
             LIMIT 1",
        )
        .bind(cond.right_rule_id)
        .bind(rule.tenant_id)
        .fetch_optional(db)
        .await?;

        match (left_active, right_active) {
            (Some(_), Some(_)) => {
                if let Err(e) = record_firing(db, &rule, 1.0).await {
                    tracing::warn!(
                        rule_id = %rule.rule_id,
                        error = %e,
                        "failed to record composite alert firing"
                    );
                }
            }
            _ => {
                if let Err(e) = resolve_open_firing(db, &rule, 0.0).await {
                    tracing::warn!(
                        rule_id = %rule.rule_id,
                        error = %e,
                        "failed to resolve composite alert firing"
                    );
                }
            }
        }
    }

    Ok(())
}

async fn fetch_span_counts(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    service_name: &str,
    environment: &str,
    window_minutes: u64,
) -> anyhow::Result<SpanCountRow> {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos() as u64;
    let cutoff_ns = now_ns.saturating_sub(window_minutes * 60 * 1_000_000_000);
    let mut cursor = ch
        .query(
            "SELECT count() AS total_count, countIf(status_code = 'ERROR') AS bad_count \
             FROM observable.spans \
             WHERE tenant_id = ? \
               AND service_name = ? \
               AND environment = ? \
               AND start_time_unix_nano >= ?",
        )
        .bind(tenant_id)
        .bind(service_name)
        .bind(environment)
        .bind(cutoff_ns)
        .fetch::<SpanCountRow>()
        .map_err(|e| anyhow::anyhow!("clickhouse query error: {e}"))?;

    match cursor.next().await {
        Ok(Some(row)) => Ok(row),
        Ok(None) => Ok(SpanCountRow {
            total_count: 0,
            bad_count: 0,
        }),
        Err(e) => Err(anyhow::anyhow!("clickhouse query error: {e}")),
    }
}

pub async fn eval_alert_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()> {
    eval_threshold_rules(db, ch).await?;
    eval_slo_burn_rate_rules(db, ch).await?;
    eval_composite_rules(db, ch).await?;
    Ok(())
}

async fn record_firing(db: &sqlx::PgPool, rule: &AlertRuleRow, value: f64) -> anyhow::Result<()> {
    let for_duration_secs = rule.for_duration_secs.unwrap_or(0).max(0);
    let initial_state = if for_duration_secs == 0 {
        "active"
    } else {
        "pending"
    };

    let res: Option<(Uuid, String)> = sqlx::query_as(
        "WITH updated AS ( \
             UPDATE alert_firings \
             SET value = $3, \
                 occurred_at = NOW(), \
                 state = CASE \
                     WHEN state = 'pending' \
                      AND NOW() >= firing_start + ($4::BIGINT * INTERVAL '1 second') \
                     THEN 'active' \
                     ELSE state \
                 END \
             WHERE firing_id = ( \
                 SELECT firing_id FROM alert_firings \
                 WHERE rule_id = $1 \
                   AND tenant_id = $2 \
                   AND state IN ('pending', 'active') \
                 ORDER BY occurred_at DESC \
                 LIMIT 1 \
             ) \
             RETURNING firing_id, state \
         ), \
         inserted AS ( \
             INSERT INTO alert_firings (rule_id, tenant_id, state, value, firing_start) \
             SELECT $1, $2, $5, $3, NOW() \
             WHERE NOT EXISTS (SELECT 1 FROM updated) \
             RETURNING firing_id, state \
         ) \
         SELECT firing_id, state FROM updated \
         UNION ALL \
         SELECT firing_id, state FROM inserted",
    )
    .bind(rule.rule_id)
    .bind(rule.tenant_id)
    .bind(value)
    .bind(for_duration_secs)
    .bind(initial_state)
    .fetch_optional(db)
    .await?;

    if let Some((firing_id, state)) = res
        && state == "active"
    {
        enqueue_notifications(
            db,
            rule.tenant_id,
            firing_id,
            &rule.notification_channels,
            "active",
        )
        .await?;

        if rule.auto_trigger_incident {
            let dedup_key = rule.rule_id.to_string();
            if let Err(e) =
                upsert_incident_from_firing(db, rule, firing_id, &dedup_key, value).await
            {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    firing_id = %firing_id,
                    error = %e,
                    "failed to upsert incident from firing"
                );
            }
        }
    }

    Ok(())
}

async fn resolve_open_firing(
    db: &sqlx::PgPool,
    rule: &AlertRuleRow,
    value: f64,
) -> anyhow::Result<()> {
    let firings: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE alert_firings \
         SET state = 'resolved', \
             resolved_at = NOW(), \
             occurred_at = NOW(), \
             value = $3 \
         WHERE rule_id = $1 \
           AND tenant_id = $2 \
           AND state IN ('pending', 'active') \
         RETURNING firing_id",
    )
    .bind(rule.rule_id)
    .bind(rule.tenant_id)
    .bind(value)
    .fetch_all(db)
    .await?;

    for firing_id in &firings {
        enqueue_notifications(
            db,
            rule.tenant_id,
            *firing_id,
            &rule.notification_channels,
            "resolved",
        )
        .await?;
    }

    if rule.auto_trigger_incident {
        for firing_id in &firings {
            if let Err(e) = resolve_incident_for_firing(db, rule, *firing_id, value).await {
                tracing::warn!(
                    rule_id = %rule.rule_id,
                    firing_id = %firing_id,
                    error = %e,
                    "failed to resolve incident for firing"
                );
            }
        }
    }

    Ok(())
}

async fn upsert_incident_from_firing(
    db: &sqlx::PgPool,
    rule: &AlertRuleRow,
    _firing_id: Uuid,
    dedup_key: &str,
    value: f64,
) -> anyhow::Result<()> {
    let incident_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT incident_id FROM incidents \
         WHERE tenant_id = $1 \
           AND dedup_key = $2 \
           AND status NOT IN ('resolved', 'post_mortem') \
         LIMIT 1",
    )
    .bind(rule.tenant_id)
    .bind(dedup_key)
    .fetch_optional(db)
    .await?;

    let incident_id = match incident_id {
        Some(id) => id,
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO incidents \
                 (tenant_id, title, severity, status, dedup_key, triggered_by_rule_id, runbook_url) \
                 VALUES ($1, $2, $3, 'triggered', $4, $5, $6) \
                 RETURNING incident_id",
            )
            .bind(rule.tenant_id)
            .bind(&rule.name)
            .bind(&rule.severity)
            .bind(dedup_key)
            .bind(rule.rule_id)
            .bind(rule.runbook_url.as_deref())
            .fetch_one(db)
            .await?;

            sqlx::query(
                "INSERT INTO incident_events (incident_id, event_type, actor, message) \
                 VALUES ($1, 'triggered', 'system', 'Alert rule transitioned to active')",
            )
            .bind(id)
            .execute(db)
            .await?;

            id
        }
    };

    sqlx::query(
        "INSERT INTO incident_events (incident_id, event_type, actor, message) \
         VALUES ($1, 'alert_fired', 'system', $2)",
    )
    .bind(incident_id)
    .bind(format!("{} fired: value={:.2}", rule.name, value))
    .execute(db)
    .await?;

    Ok(())
}

async fn resolve_incident_for_firing(
    db: &sqlx::PgPool,
    rule: &AlertRuleRow,
    _firing_id: Uuid,
    value: f64,
) -> anyhow::Result<()> {
    let incident_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT incident_id FROM incidents \
         WHERE tenant_id = $1 \
           AND dedup_key = $2 \
           AND status NOT IN ('resolved', 'post_mortem') \
         LIMIT 1",
    )
    .bind(rule.tenant_id)
    .bind(rule.rule_id.to_string())
    .fetch_optional(db)
    .await?;

    if let Some(incident_id) = incident_id {
        sqlx::query(
            "UPDATE incidents \
             SET status = 'resolved', resolved_at = NOW(), updated_at = NOW() \
             WHERE incident_id = $1",
        )
        .bind(incident_id)
        .execute(db)
        .await?;

        sqlx::query(
            "INSERT INTO incident_events (incident_id, event_type, actor, message) \
             VALUES ($1, 'alert_resolved', 'system', $2)",
        )
        .bind(incident_id)
        .bind(format!("{} resolved: value={:.2}", rule.name, value))
        .execute(db)
        .await?;
    }

    Ok(())
}

async fn enqueue_notifications(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    firing_id: Uuid,
    channels: &[Uuid],
    trigger_state: &str,
) -> anyhow::Result<()> {
    for &channel_id in channels {
        sqlx::query(
            "INSERT INTO notification_audit_log (tenant_id, firing_id, channel_id, trigger_state) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (firing_id, channel_id, trigger_state) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(firing_id)
        .bind(channel_id)
        .bind(trigger_state)
        .execute(db)
        .await?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct WebhookPayload {
    pub version: String,
    pub firing_id: Uuid,
    pub rule_id: Uuid,
    pub rule_name: String,
    pub tenant_id: Uuid,
    pub severity: String,
    pub state: String,
    pub value: f64,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct PendingNotificationRow {
    audit_id: Uuid,
    tenant_id: Uuid,
    firing_id: Uuid,
    trigger_state: String,
    retry_count: i32,
    rule_id: Uuid,
    value: Option<f64>,
    occurred_at: chrono::DateTime<chrono::Utc>,
    rule_name: String,
    severity: String,
    channel_config: serde_json::Value,
}

pub async fn notification_worker(db: sqlx::PgPool) {
    tracing::info!("alert-evaluator: starting notification worker");
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
    let http_client = reqwest::Client::new();

    loop {
        ticker.tick().await;
        if let Err(e) = process_pending_notifications(&db, &http_client).await {
            tracing::error!(error = %e, "notification worker cycle failed");
        }
    }
}

async fn process_pending_notifications(
    db: &sqlx::PgPool,
    http_client: &reqwest::Client,
) -> anyhow::Result<()> {
    let pending = sqlx::query_as::<_, PendingNotificationRow>(
        "SELECT a.audit_id, a.tenant_id, a.firing_id, a.channel_id, a.trigger_state, a.retry_count, \
                f.rule_id, f.value, f.occurred_at, r.name as rule_name, r.severity, \
                c.config as channel_config \
         FROM notification_audit_log a \
         JOIN alert_firings f ON a.firing_id = f.firing_id \
         JOIN alert_rules r ON f.rule_id = r.rule_id \
         JOIN notification_channels c ON a.channel_id = c.channel_id \
         WHERE a.state = 'pending' \
           AND (a.last_attempt_at IS NULL OR a.last_attempt_at < NOW() - (POWER(2, a.retry_count) * INTERVAL '10 seconds')) \
         LIMIT 50"
    )
    .fetch_all(db)
    .await?;

    for record in pending {
        let payload = WebhookPayload {
            version: "1".into(),
            firing_id: record.firing_id,
            rule_id: record.rule_id,
            rule_name: record.rule_name,
            tenant_id: record.tenant_id,
            severity: record.severity,
            state: record.trigger_state.clone(),
            value: record.value.unwrap_or(0.0),
            occurred_at: record.occurred_at,
        };

        let config: serde_json::Value = record.channel_config;
        let url = config["url"].as_str().unwrap_or("");

        if url.is_empty() {
            sqlx::query(
                "UPDATE notification_audit_log SET state = 'failed', error_message = 'missing url' WHERE audit_id = $1"
            )
            .bind(record.audit_id)
            .execute(db).await?;
            continue;
        }

        match http_client.post(url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                sqlx::query(
                    "UPDATE notification_audit_log SET state = 'sent', last_attempt_at = NOW() WHERE audit_id = $1"
                )
                .bind(record.audit_id)
                .execute(db).await?;
            }
            Ok(resp) => {
                let status = resp.status();
                let error = format!("HTTP {}", status);
                handle_notification_failure(db, record.audit_id, record.retry_count, error).await?;
            }
            Err(e) => {
                handle_notification_failure(db, record.audit_id, record.retry_count, e.to_string())
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_notification_failure(
    db: &sqlx::PgPool,
    audit_id: Uuid,
    retry_count: i32,
    error: String,
) -> anyhow::Result<()> {
    if retry_count >= 10 {
        sqlx::query(
            "UPDATE notification_audit_log SET state = 'failed', error_message = $2, last_attempt_at = NOW() WHERE audit_id = $1"
        )
        .bind(audit_id)
        .bind(error)
        .execute(db).await?;
    } else {
        sqlx::query(
            "UPDATE notification_audit_log SET retry_count = retry_count + 1, error_message = $2, last_attempt_at = NOW() WHERE audit_id = $1"
        )
        .bind(audit_id)
        .bind(error)
        .execute(db).await?;
    }
    Ok(())
}

pub async fn start_eval_worker(
    db: sqlx::PgPool,
    ch: clickhouse::Client,
    interval: std::time::Duration,
) {
    tracing::info!(
        interval_secs = interval.as_secs(),
        "alert-evaluator: starting alert eval worker"
    );
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        if let Err(e) = eval_alert_rules(&db, &ch).await {
            tracing::warn!(error = %e, "alert-evaluator: eval cycle failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cond(operator: ThresholdOperator, threshold: f64) -> ThresholdCondition {
        ThresholdCondition {
            metric_name: "m".into(),
            operator,
            threshold,
        }
    }

    #[test]
    fn gt_fires_when_above() {
        assert_eq!(
            evaluate_threshold(1.1, &cond(ThresholdOperator::Gt, 1.0)),
            EvalResult::Firing
        );
    }

    #[test]
    fn gt_ok_when_equal() {
        assert_eq!(
            evaluate_threshold(1.0, &cond(ThresholdOperator::Gt, 1.0)),
            EvalResult::Ok
        );
    }

    #[test]
    fn gte_fires_when_equal() {
        assert_eq!(
            evaluate_threshold(1.0, &cond(ThresholdOperator::Gte, 1.0)),
            EvalResult::Firing
        );
    }

    #[test]
    fn lt_fires_when_below() {
        assert_eq!(
            evaluate_threshold(0.9, &cond(ThresholdOperator::Lt, 1.0)),
            EvalResult::Firing
        );
    }

    #[test]
    fn lt_ok_when_equal() {
        assert_eq!(
            evaluate_threshold(1.0, &cond(ThresholdOperator::Lt, 1.0)),
            EvalResult::Ok
        );
    }

    #[test]
    fn lte_fires_when_equal() {
        assert_eq!(
            evaluate_threshold(1.0, &cond(ThresholdOperator::Lte, 1.0)),
            EvalResult::Firing
        );
    }

    #[test]
    fn eq_fires_when_equal() {
        assert_eq!(
            evaluate_threshold(5.0, &cond(ThresholdOperator::Eq, 5.0)),
            EvalResult::Firing
        );
    }

    #[test]
    fn eq_ok_when_different() {
        assert_eq!(
            evaluate_threshold(5.1, &cond(ThresholdOperator::Eq, 5.0)),
            EvalResult::Ok
        );
    }

    #[test]
    fn threshold_condition_parses_all_operators() {
        // Smoke-test all five operators round-trip through the evaluator.
        // The silenced-rule SQL filter (AND silenced = false) is enforced at the
        // query level in eval_threshold_rules; verified by the postgres integration test.
        for (op, value, threshold, expected) in [
            (ThresholdOperator::Gt, 1.1, 1.0, EvalResult::Firing),
            (ThresholdOperator::Gte, 1.0, 1.0, EvalResult::Firing),
            (ThresholdOperator::Lt, 0.9, 1.0, EvalResult::Firing),
            (ThresholdOperator::Lte, 1.0, 1.0, EvalResult::Firing),
            (ThresholdOperator::Eq, 5.0, 5.0, EvalResult::Firing),
        ] {
            assert_eq!(evaluate_threshold(value, &cond(op, threshold)), expected);
        }
    }

    #[test]
    fn burn_rate_uses_error_rate_over_error_budget() {
        let burn = calculate_burn_rate(10, 1_000, 0.999, 60, 30);
        assert!((burn - 10.0).abs() < 0.001);
    }

    #[test]
    fn multi_window_requires_fast_and_slow_to_fire() {
        let cond = SloBurnRateCondition {
            slo_id: Uuid::nil(),
            fast_window_minutes: 60,
            slow_window_minutes: 360,
        };
        assert_eq!(
            evaluate_burn_rate(15.0, 2.0, 14.4, 1.0, &cond),
            EvalResult::Firing
        );
        assert_eq!(
            evaluate_burn_rate(15.0, 0.5, 14.4, 1.0, &cond),
            EvalResult::Ok
        );
    }

    fn deadman_cond(window_secs: i64) -> DeadmanCondition {
        DeadmanCondition {
            service_name: "checkout".into(),
            window_secs,
        }
    }

    #[test]
    fn deadman_fires_when_never_seen() {
        assert_eq!(
            evaluate_deadman(None, &deadman_cond(300)),
            EvalResult::Firing
        );
    }

    #[test]
    fn deadman_fires_when_stale() {
        assert_eq!(
            evaluate_deadman(Some(301), &deadman_cond(300)),
            EvalResult::Firing
        );
    }

    #[test]
    fn deadman_fires_exactly_at_boundary() {
        assert_eq!(
            evaluate_deadman(Some(300), &deadman_cond(300)),
            EvalResult::Firing
        );
    }

    #[test]
    fn deadman_ok_when_fresh() {
        assert_eq!(
            evaluate_deadman(Some(10), &deadman_cond(300)),
            EvalResult::Ok
        );
    }
}
