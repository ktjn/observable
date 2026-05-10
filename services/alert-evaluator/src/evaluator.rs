use serde::Deserialize;
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

#[derive(sqlx::FromRow)]
pub struct AlertRuleRow {
    pub rule_id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub condition: serde_json::Value,
    pub for_duration_secs: Option<i64>,
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
        "SELECT rule_id, tenant_id, name, condition, for_duration_secs \
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
                if let Err(e) = resolve_open_firing(db, rule.rule_id, rule.tenant_id, value).await {
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
        "SELECT rule_id, tenant_id, name, condition, for_duration_secs \
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
                if let Err(e) =
                    resolve_open_firing(db, rule.rule_id, rule.tenant_id, fast_burn_rate).await
                {
                    tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to resolve SLO alert firing");
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
    Ok(())
}

async fn record_firing(db: &sqlx::PgPool, rule: &AlertRuleRow, value: f64) -> anyhow::Result<()> {
    let for_duration_secs = rule.for_duration_secs.unwrap_or(0).max(0);
    let initial_state = if for_duration_secs == 0 {
        "active"
    } else {
        "pending"
    };

    sqlx::query(
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
             RETURNING firing_id \
         ) \
         INSERT INTO alert_firings (rule_id, tenant_id, state, value, firing_start) \
         SELECT $1, $2, $5, $3, NOW() \
         WHERE NOT EXISTS (SELECT 1 FROM updated)",
    )
    .bind(rule.rule_id)
    .bind(rule.tenant_id)
    .bind(value)
    .bind(for_duration_secs)
    .bind(initial_state)
    .execute(db)
    .await?;

    Ok(())
}

async fn resolve_open_firing(
    db: &sqlx::PgPool,
    rule_id: Uuid,
    tenant_id: Uuid,
    value: f64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE alert_firings \
         SET state = 'resolved', \
             resolved_at = NOW(), \
             occurred_at = NOW(), \
             value = $3 \
         WHERE rule_id = $1 \
           AND tenant_id = $2 \
           AND state IN ('pending', 'active')",
    )
    .bind(rule_id)
    .bind(tenant_id)
    .bind(value)
    .execute(db)
    .await?;

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
}
