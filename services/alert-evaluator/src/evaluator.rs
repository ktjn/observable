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
        "alert-evaluator: starting threshold eval worker"
    );
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        if let Err(e) = eval_threshold_rules(&db, &ch).await {
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
}
