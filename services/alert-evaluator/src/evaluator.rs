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
        "SELECT rule_id, tenant_id, name, condition \
         FROM alert_rules WHERE alert_type = 'threshold'",
    )
    .fetch_all(db)
    .await?;

    tracing::debug!(count = rules.len(), "evaluating threshold rules");

    for rule in rules {
        let cond: ThresholdCondition = match serde_json::from_value(rule.condition) {
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

        if evaluate_threshold(value, &cond) == EvalResult::Firing {
            tracing::warn!(
                rule_id = %rule.rule_id,
                tenant_id = %rule.tenant_id,
                rule_name = %rule.name,
                metric_name = %cond.metric_name,
                value = value,
                threshold = cond.threshold,
                "alert firing: threshold exceeded"
            );
            if let Err(e) = sqlx::query(
                "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
                 VALUES ($1, $2, 'active', $3)",
            )
            .bind(rule.rule_id)
            .bind(rule.tenant_id)
            .bind(value)
            .execute(db)
            .await
            {
                tracing::warn!(rule_id = %rule.rule_id, error = %e, "failed to record alert firing");
            }
        }
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
}
