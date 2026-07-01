use query_api::alerts::list_alert_rules;
use uuid::Uuid;

#[tokio::test]
async fn list_rules_returns_seeded_dev_rule() {
    let pool = test_support::postgres::shared_pool().await;
    let dev_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let rules = list_alert_rules(&pool, dev_tenant).await.unwrap();

    assert!(
        !rules.is_empty(),
        "dev tenant must have at least one seeded rule"
    );
    assert!(
        rules.iter().any(|r| r.name == "High error rate"),
        "seeded 'High error rate' rule must be present"
    );
    let seeded = rules.iter().find(|r| r.name == "High error rate").unwrap();
    assert_eq!(seeded.metric_name, "error_rate");
    assert_eq!(seeded.operator, "gt");
    assert!(!seeded.silenced);
    assert!(!seeded.firing);
    assert_eq!(seeded.state, "ok");
}

#[tokio::test]
async fn list_rules_does_not_return_other_tenant_rules() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let condition = serde_json::json!({
        "metric_name": "test_metric",
        "operator": "gt",
        "threshold": 10.0
    });
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, $3, 'threshold', 'warning', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_a)
    .bind("test rule")
    .bind(condition)
    .execute(&pool)
    .await
    .unwrap();

    let tenant_b_rules = list_alert_rules(&pool, tenant_b).await.unwrap();
    assert!(
        tenant_b_rules.is_empty(),
        "tenant B must not see tenant A's rules"
    );
}

#[tokio::test]
async fn list_rules_reports_pending_active_resolved_and_silenced_states() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let pending_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Pending rule', 'threshold', 'warning', $3)",
    )
    .bind(pending_id)
    .bind(tenant)
    .bind(serde_json::json!({"metric_name":"pending_metric","operator":"gt","threshold":1.0}))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'pending', 2.0)",
    )
    .bind(pending_id)
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let active_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Active rule', 'threshold', 'warning', $3)",
    )
    .bind(active_id)
    .bind(tenant)
    .bind(serde_json::json!({"metric_name":"active_metric","operator":"gt","threshold":1.0}))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'active', 2.0)",
    )
    .bind(active_id)
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let resolved_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Resolved rule', 'threshold', 'warning', $3)",
    )
    .bind(resolved_id)
    .bind(tenant)
    .bind(serde_json::json!({"metric_name":"resolved_metric","operator":"gt","threshold":1.0}))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value, resolved_at) \
         VALUES ($1, $2, 'resolved', 0.5, NOW())",
    )
    .bind(resolved_id)
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let silenced_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'Silenced rule', 'threshold', 'warning', $3)",
    )
    .bind(silenced_id)
    .bind(tenant)
    .bind(serde_json::json!({"metric_name":"silenced_metric","operator":"gt","threshold":1.0}))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE alert_rules SET silenced = true WHERE rule_id = $1")
        .bind(silenced_id)
        .execute(&pool)
        .await
        .unwrap();

    let rules = list_alert_rules(&pool, tenant).await.unwrap();

    let state_for = |name: &str| {
        rules
            .iter()
            .find(|rule| rule.name == name)
            .map(|rule| rule.state.as_str())
            .unwrap()
    };
    assert_eq!(state_for("Pending rule"), "pending");
    assert_eq!(state_for("Active rule"), "active");
    assert_eq!(state_for("Resolved rule"), "resolved");
    assert_eq!(state_for("Silenced rule"), "silenced");
}
